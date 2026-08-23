//! # pixelmagic-gpu
//!
//! The rendering half of Pixelmagic: an OpenGL shader library and the render
//! graph that drives it.
//!
//! ## Why OpenGL and not Vulkan or wgpu
//!
//! The renderer has to hand finished frames to GTK. GTK4's `GLArea` gives us a
//! GL context that it already owns and already composites from, so rendering
//! straight into it is zero-copy. Reaching that same point from wgpu means
//! either reading frames back to the CPU every draw — 12 MB per frame at 1080p,
//! which is enough to feel — or exporting Vulkan memory as a dma-buf and
//! importing it as a `GdkDmabufTexture`, which works beautifully on Mesa and
//! then falls over on the next driver. GL via `GLArea` runs everywhere GTK
//! runs, including software rasterisation, and that portability is worth more
//! right now than WGSL's nicer ergonomics.
//!
//! The [`Renderer`] owns no GTK types, so swapping the backend later means
//! reimplementing this crate, not touching the app or the model.
//!
//! ## Conventions
//!
//! - Intermediates are RGBA16F, premultiplied, linear light.
//! - Every pass is a fullscreen triangle; geometry is done by inverse mapping
//!   in the fragment shader.
//! - Passes never read and write the same target; the [`TargetPool`] supplies
//!   ping-pong buffers.

pub mod headless;
pub mod program;
pub mod renderer;
pub mod texture;

pub use program::{GlFlavor, Program, ShaderLibrary};
pub use renderer::{FrameStats, Renderer};
pub use texture::{Filter, Format, RenderTarget, TargetPool, Texture, Wrap};

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("shader error: {0}")]
    Shader(String),
    #[error("OpenGL error: {0}")]
    Gl(String),
    #[error("{0}")]
    Invalid(String),
    #[error("no OpenGL context available: {0}")]
    NoContext(String),
}

pub type Result<T> = std::result::Result<T, GpuError>;

/// Load GL entry points from libepoxy, which is what GTK itself links against.
///
/// Going through epoxy rather than dlopening `libGL` directly matters: epoxy
/// dispatches to whatever the current context actually is, so this works
/// identically on GLX, EGL and GLES without the caller having to know which one
/// is in play. libepoxy exports every GL entry point as a real symbol, so a
/// plain `dlsym` is all the resolution we need — no separate loader crate.
///
/// # Safety
///
/// The caller must have made a GL context current on this thread, and must keep
/// it current for as long as the returned context is used.
pub unsafe fn context_from_epoxy() -> Result<glow::Context> {
    use libloading::os::unix::{Library, Symbol, RTLD_GLOBAL, RTLD_LAZY};
    use std::ffi::c_void;

    let lib = Library::open(Some("libepoxy.so.0"), RTLD_LAZY | RTLD_GLOBAL)
        .or_else(|_| Library::open(Some("libepoxy.so"), RTLD_LAZY | RTLD_GLOBAL))
        .map_err(|e| GpuError::NoContext(format!("libepoxy not found: {e}")))?;

    // libepoxy does not export `glGetString`. It exports `epoxy_glGetString`,
    // and — this is the part that is easy to get wrong — as a *data* symbol
    // holding a function pointer, not as a function. Reading it as a function
    // symbol yields the address of the pointer variable rather than the
    // function, so every call lands in the middle of some unrelated data and
    // glow reports the entry point as "not loaded".
    //
    // The pointer starts out as epoxy's resolver stub and rewrites itself on
    // first call, which is why it is safe to read all of these up front,
    // before any GL work has happened.
    let context = glow::Context::from_loader_function(|name| {
        let mut prefixed = Vec::with_capacity(name.len() + 8);
        prefixed.extend_from_slice(b"epoxy_");
        prefixed.extend_from_slice(name.as_bytes());
        prefixed.push(0);

        if let Ok(sym) = lib.get::<*const c_void>(&prefixed) {
            // Careful: dereferencing a `libloading` symbol yields the symbol's
            // *address*, not the value stored there. For a function symbol
            // those are the same thing, which is why the mistake is easy to
            // make — but `epoxy_glGetString` is a pointer variable, so one more
            // dereference is needed to get the function it points at. Without
            // it, every GL call jumps into `.data` and segfaults.
            let sym: Symbol<*const c_void> = sym;
            let address: *const c_void = *sym;
            if !address.is_null() {
                let target = *(address as *const *const c_void);
                if !target.is_null() {
                    return target;
                }
            }
        }

        // Some builds do export the bare name; try it as a function symbol.
        let mut bare = name.as_bytes().to_vec();
        bare.push(0);
        match lib.get::<unsafe extern "C" fn()>(&bare) {
            Ok(f) => {
                let f: Symbol<unsafe extern "C" fn()> = f;
                *f as usize as *const c_void
            }
            Err(_) => std::ptr::null(),
        }
    });

    // The library must outlive every GL call made through it.
    std::mem::forget(lib);
    Ok(context)
}

/// Detect whether the current context is desktop GL or GLES.
pub fn detect_flavor(gl: &glow::Context) -> GlFlavor {
    use glow::HasContext;
    let version = unsafe { gl.get_parameter_string(glow::VERSION) };
    if version.contains("OpenGL ES") {
        GlFlavor::Es
    } else {
        GlFlavor::Core
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_render_usefully() {
        let e = GpuError::Shader("bad".into());
        assert!(e.to_string().contains("shader error"));
        let e = GpuError::NoContext("nope".into());
        assert!(e.to_string().contains("no OpenGL context"));
    }
}
