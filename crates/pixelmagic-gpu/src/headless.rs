//! A headless OpenGL context, for tests and command-line rendering.
//!
//! Shader bugs are miserable to find by eye. A shader that fails to compile at
//! least says so; one that compiles and produces subtly wrong colour does not,
//! and "does `Multiply` actually multiply" is not a question that should be
//! answered by squinting at a screenshot. This module brings up a real GL
//! context with no window and no display server — via EGL's surfaceless
//! platform, which Mesa supports including on its software rasteriser — so the
//! renderer can be tested the way any other code is.
//!
//! It is also what `pixelmagic --render` uses for batch work.

use std::ffi::c_void;
use std::rc::Rc;

use crate::{GpuError, Result};

// EGL constants. Spelled out rather than pulled from a binding crate so this
// module has no build-time dependency on EGL headers being installed.
const EGL_PLATFORM_SURFACELESS_MESA: u32 = 0x31DD;
const EGL_OPENGL_API: u32 = 0x30A2;
const EGL_OPENGL_ES_API: u32 = 0x30A0;
const EGL_OPENGL_ES3_BIT: i32 = 0x00000040;
const EGL_NO_DISPLAY: *mut c_void = std::ptr::null_mut();
const EGL_NO_CONTEXT: *mut c_void = std::ptr::null_mut();
const EGL_NO_SURFACE: *mut c_void = std::ptr::null_mut();

const EGL_SURFACE_TYPE: i32 = 0x3033;
const EGL_RENDERABLE_TYPE: i32 = 0x3040;
const EGL_OPENGL_BIT: i32 = 0x0008;
const EGL_RED_SIZE: i32 = 0x3024;
const EGL_GREEN_SIZE: i32 = 0x3023;
const EGL_BLUE_SIZE: i32 = 0x3022;
const EGL_ALPHA_SIZE: i32 = 0x3021;
const EGL_NONE: i32 = 0x3038;
const EGL_CONTEXT_MAJOR_VERSION: i32 = 0x3098;
const EGL_CONTEXT_MINOR_VERSION: i32 = 0x30FB;
const EGL_CONTEXT_OPENGL_PROFILE_MASK: i32 = 0x30FD;
const EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT: i32 = 0x00000001;

type EglGetProcAddress = unsafe extern "C" fn(*const i8) -> *const c_void;
type EglGetPlatformDisplay =
    unsafe extern "C" fn(u32, *mut c_void, *const isize) -> *mut c_void;
type EglGetDisplay = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
type EglInitialize = unsafe extern "C" fn(*mut c_void, *mut i32, *mut i32) -> u32;
type EglBindApi = unsafe extern "C" fn(u32) -> u32;
type EglChooseConfig =
    unsafe extern "C" fn(*mut c_void, *const i32, *mut *mut c_void, i32, *mut i32) -> u32;
type EglCreateContext =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const i32) -> *mut c_void;
type EglMakeCurrent =
    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32;
type EglTerminate = unsafe extern "C" fn(*mut c_void) -> u32;
type EglGetError = unsafe extern "C" fn() -> i32;
type EglDestroyContext = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;

/// An owned headless GL context. Dropping it tears the context down.
pub struct HeadlessContext {
    lib: libloading::Library,
    display: *mut c_void,
    context: *mut c_void,
    pub gl: Rc<glow::Context>,
    pub flavor: crate::GlFlavor,
}

impl HeadlessContext {
    /// Bring up a surfaceless context.
    ///
    /// Desktop GL 3.3 core by default. `PIXELMAGIC_TEST_GLES=1` asks for GLES
    /// instead, which matters because GLES is what the application actually
    /// runs on: GTK hands `GtkGLArea` a GLES context on a great many systems,
    /// and a suite that only exercises desktop GL passes happily while the app
    /// aborts on a desktop-only entry point. That is not hypothetical — it is
    /// how `glGetBufferSubData` got into the histogram readback.
    ///
    /// **Caveat:** the GLES path does not currently come up on Mesa's software
    /// rasteriser (`eglCreateContext` returns `EGL_BAD_ATTRIBUTE` regardless of
    /// the config chosen), so it is unverified there. Real GLES coverage comes
    /// from `scripts/smoke-test.sh`, which drives the actual application
    /// against the context GTK creates. If you are on hardware where this
    /// works, running the suite both ways is worth it.
    ///
    /// Returns [`GpuError::NoContext`] rather than panicking when EGL is
    /// missing, so tests can skip cleanly on machines without it instead of
    /// reporting a spurious failure.
    pub fn new() -> Result<Self> {
        let want_es = std::env::var("PIXELMAGIC_TEST_GLES")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);
        Self::with_api(want_es)
    }

    pub fn new_desktop() -> Result<Self> {
        Self::with_api(false)
    }

    pub fn new_es() -> Result<Self> {
        Self::with_api(true)
    }

    fn with_api(want_es: bool) -> Result<Self> {
        // Serialise context creation. `eglBindAPI` is per-thread but the
        // display and its config list are shared, and bringing up thirty
        // contexts concurrently — which is exactly what a parallel test run
        // does — makes Mesa intermittently return EGL_BAD_ATTRIBUTE. Creation
        // happens a handful of times per process, so a mutex costs nothing.
        static EGL_INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = EGL_INIT.lock().unwrap_or_else(|e| e.into_inner());

        unsafe {
            let lib = libloading::Library::new("libEGL.so.1")
                .or_else(|_| libloading::Library::new("libEGL.so"))
                .map_err(|e| GpuError::NoContext(format!("libEGL not available: {e}")))?;

            let get_proc: libloading::Symbol<EglGetProcAddress> = lib
                .get(b"eglGetProcAddress\0")
                .map_err(|e| GpuError::NoContext(e.to_string()))?;
            let get_proc = *get_proc;

            // Surfaceless is the clean path; fall back to the default display
            // for older drivers that lack the extension.
            let display = match lib.get::<EglGetPlatformDisplay>(b"eglGetPlatformDisplay\0") {
                Ok(f) => {
                    f(EGL_PLATFORM_SURFACELESS_MESA, std::ptr::null_mut(), std::ptr::null())
                }
                Err(_) => {
                    let f: libloading::Symbol<EglGetDisplay> = lib
                        .get(b"eglGetDisplay\0")
                        .map_err(|e| GpuError::NoContext(e.to_string()))?;
                    f(std::ptr::null_mut())
                }
            };
            if display == EGL_NO_DISPLAY {
                return Err(GpuError::NoContext("eglGetDisplay returned none".into()));
            }

            let initialize: libloading::Symbol<EglInitialize> =
                lib.get(b"eglInitialize\0").map_err(|e| GpuError::NoContext(e.to_string()))?;
            let (mut major, mut minor) = (0i32, 0i32);
            if initialize(display, &mut major, &mut minor) == 0 {
                return Err(GpuError::NoContext("eglInitialize failed".into()));
            }

            let bind_api: libloading::Symbol<EglBindApi> =
                lib.get(b"eglBindAPI\0").map_err(|e| GpuError::NoContext(e.to_string()))?;
            let api = if want_es { EGL_OPENGL_ES_API } else { EGL_OPENGL_API };
            if bind_api(api) == 0 {
                return Err(GpuError::NoContext(format!(
                    "this EGL cannot bind {}",
                    if want_es { "GLES" } else { "desktop GL" }
                )));
            }

            let choose: libloading::Symbol<EglChooseConfig> = lib
                .get(b"eglChooseConfig\0")
                .map_err(|e| GpuError::NoContext(e.to_string()))?;
            // EGL_SURFACE_TYPE is explicitly zero, and both halves of that
            // matter. Asking for EGL_PBUFFER_BIT matches nothing for GLES on
            // the surfaceless platform; *omitting* the attribute is worse,
            // because the spec's default is EGL_WINDOW_BIT, which matches
            // nothing here either. Zero means "no surface capability required",
            // which is the truth: this context renders only into framebuffer
            // objects and never creates a surface at all.
            let attrs = [
                EGL_SURFACE_TYPE,
                0,
                EGL_RENDERABLE_TYPE,
                if want_es { EGL_OPENGL_ES3_BIT } else { EGL_OPENGL_BIT },
                EGL_RED_SIZE,
                8,
                EGL_GREEN_SIZE,
                8,
                EGL_BLUE_SIZE,
                8,
                EGL_ALPHA_SIZE,
                8,
                EGL_NONE,
            ];
            let mut config: *mut c_void = std::ptr::null_mut();
            let mut count = 0i32;
            if choose(display, attrs.as_ptr(), &mut config, 1, &mut count) == 0 || count == 0 {
                return Err(GpuError::NoContext("no suitable EGL config".into()));
            }

            let create: libloading::Symbol<EglCreateContext> =
                lib.get(b"eglCreateContext\0")
                    .map_err(|e| GpuError::NoContext(e.to_string()))?;
            let ctx_attrs = [
                EGL_CONTEXT_MAJOR_VERSION,
                3,
                EGL_CONTEXT_MINOR_VERSION,
                3,
                EGL_CONTEXT_OPENGL_PROFILE_MASK,
                EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT,
                EGL_NONE,
            ];
            let context = create(display, config, EGL_NO_CONTEXT, ctx_attrs.as_ptr());
            if context == EGL_NO_CONTEXT {
                let code = lib.get::<EglGetError>(b"eglGetError\0").map(|f| f()).unwrap_or(0);
                return Err(GpuError::NoContext(format!(
                    "eglCreateContext failed (EGL error 0x{code:x})"
                )));
            }

            let make_current: libloading::Symbol<EglMakeCurrent> =
                lib.get(b"eglMakeCurrent\0").map_err(|e| GpuError::NoContext(e.to_string()))?;
            if make_current(display, EGL_NO_SURFACE, EGL_NO_SURFACE, context) == 0 {
                return Err(GpuError::NoContext(
                    "eglMakeCurrent failed (no surfaceless context support)".into(),
                ));
            }

            let gl = glow::Context::from_loader_function(|name| {
                let c = std::ffi::CString::new(name).unwrap();
                get_proc(c.as_ptr()) as *const _
            });
            let flavor = crate::detect_flavor(&gl);

            Ok(Self { lib, display, context, gl: Rc::new(gl), flavor })
        }
    }

    /// Human-readable renderer string, useful in test output when a result
    /// differs between llvmpipe and real hardware.
    pub fn describe(&self) -> String {
        use glow::HasContext;
        unsafe {
            format!(
                "{} / {}",
                self.gl.get_parameter_string(glow::RENDERER),
                self.gl.get_parameter_string(glow::VERSION)
            )
        }
    }
}

impl Drop for HeadlessContext {
    fn drop(&mut self) {
        unsafe {
            if let Ok(f) = self.lib.get::<EglDestroyContext>(b"eglDestroyContext\0") {
                f(self.display, self.context);
            }
            if let Ok(f) = self.lib.get::<EglTerminate>(b"eglTerminate\0") {
                f(self.display);
            }
        }
    }
}
