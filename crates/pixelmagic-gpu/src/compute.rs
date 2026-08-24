//! Compute shaders: capability detection, programs, and storage buffers.
//!
//! ## Why compute at all
//!
//! Every pass in the fragment pipeline reads through the texture unit and
//! writes one value per invocation, with no way for neighbouring invocations to
//! share work. That is fine for a point operation like `Invert`, and wasteful
//! for anything with a kernel: a 32-pixel-radius blur reads each pixel 65 times
//! per axis, and every one of those reads goes through the texture cache.
//!
//! A compute shader can stage a tile of the image into **workgroup shared
//! memory** once, then have all 128 threads in the group read their windows out
//! of it. Same arithmetic, roughly 1/60th of the memory traffic.
//!
//! Compute also makes possible things the fragment pipeline simply cannot
//! express — a histogram needs scattered atomic writes, which fragment shaders
//! have no way to perform. That is what unlocks Levels, Curves, Auto Contrast
//! and Auto Color.
//!
//! ## Why this is optional
//!
//! Compute needs GL 4.3 or GLES 3.1. That covers everything Mesa has shipped
//! for a decade, but not literally everything, so [`Capabilities`] is detected
//! at start-up and every compute path has a fragment fallback that produces the
//! same image. The renderer never fails because compute is missing; it just
//! runs the slower route.

use glow::HasContext;
use std::rc::Rc;

use crate::program::GlFlavor;
use crate::{GpuError, Result};

/// What the current context can actually do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub version: (u32, u32),
    pub is_es: bool,
    /// Compute shaders and image load/store are available.
    pub compute: bool,
    pub max_workgroup_invocations: u32,
    pub max_shared_memory: u32,
}

impl Capabilities {
    pub fn detect(gl: &glow::Context, flavor: GlFlavor) -> Self {
        unsafe {
            let version_string = gl.get_parameter_string(glow::VERSION);
            let is_es = flavor == GlFlavor::Es || version_string.contains("OpenGL ES");
            let major = gl.get_parameter_i32(glow::MAJOR_VERSION).max(0) as u32;
            let minor = gl.get_parameter_i32(glow::MINOR_VERSION).max(0) as u32;

            // Compute landed in GL 4.3 and GLES 3.1.
            let version_ok = if is_es {
                major > 3 || (major == 3 && minor >= 1)
            } else {
                major > 4 || (major == 4 && minor >= 3)
            };

            // Trust the version, but verify: some drivers report a version they
            // do not fully implement, and querying a compute-only limit is a
            // cheap way to find out before we compile anything.
            let invocations = if version_ok {
                let v = gl.get_parameter_i32(glow::MAX_COMPUTE_WORK_GROUP_INVOCATIONS);
                gl.get_error(); // clear the error the query raises if unsupported
                v.max(0) as u32
            } else {
                0
            };
            let shared = if version_ok {
                let v = gl.get_parameter_i32(glow::MAX_COMPUTE_SHARED_MEMORY_SIZE);
                gl.get_error();
                v.max(0) as u32
            } else {
                0
            };

            // The shaders below need 128 threads and ~4 KB of shared memory;
            // the spec floors are 128 and 16 KB, so anything reporting less is
            // lying and should not be trusted with a dispatch.
            let compute = version_ok && invocations >= 128 && shared >= 16384;

            Self {
                version: (major, minor),
                is_es,
                compute,
                max_workgroup_invocations: invocations,
                max_shared_memory: shared,
            }
        }
    }

    pub fn describe(&self) -> String {
        format!(
            "{}{}.{} — compute {} ({} invocations/group, {} KB shared)",
            if self.is_es { "GLES " } else { "GL " },
            self.version.0,
            self.version.1,
            if self.compute { "yes" } else { "NO — using fragment fallbacks" },
            self.max_workgroup_invocations,
            self.max_shared_memory / 1024,
        )
    }
}

/// Every compute shader, embedded at build time.
pub static COMPUTE_SOURCES: &[(&str, &str)] = &[
    ("blur_separable", include_str!("../shaders/compute/blur_separable.comp")),
    ("histogram", include_str!("../shaders/compute/histogram.comp")),
];

/// A linked compute program.
pub struct ComputeProgram {
    gl: Rc<glow::Context>,
    program: glow::Program,
    uniforms: std::collections::HashMap<String, Option<glow::UniformLocation>>,
}

impl ComputeProgram {
    pub fn new(
        gl: Rc<glow::Context>,
        flavor: GlFlavor,
        name: &str,
        source: &str,
        common: &str,
    ) -> Result<Self> {
        unsafe {
            let header = if flavor == GlFlavor::Es {
                "#version 310 es\nprecision highp float;\nprecision highp int;\n\
                 precision highp image2D;\nprecision highp sampler2D;\n"
            } else {
                "#version 430 core\n"
            };
            let full = format!("{header}{common}\n{source}");

            let shader = gl
                .create_shader(glow::COMPUTE_SHADER)
                .map_err(|e| GpuError::Shader(format!("{name}: {e}")))?;
            gl.shader_source(shader, &full);
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                let log = gl.get_shader_info_log(shader);
                gl.delete_shader(shader);
                let listing: String = full
                    .lines()
                    .enumerate()
                    .map(|(i, l)| format!("{:4} | {l}\n", i + 1))
                    .collect();
                return Err(GpuError::Shader(format!(
                    "{name} (compute) failed to compile:\n{log}\n--- source ---\n{listing}"
                )));
            }

            let program =
                gl.create_program().map_err(|e| GpuError::Shader(format!("{name}: {e}")))?;
            gl.attach_shader(program, shader);
            gl.link_program(program);
            gl.delete_shader(shader);

            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(GpuError::Shader(format!("{name}: link failed: {log}")));
            }

            Ok(Self { gl, program, uniforms: Default::default() })
        }
    }

    pub fn bind(&self) {
        unsafe { self.gl.use_program(Some(self.program)) };
    }

    fn location(&mut self, name: &str) -> Option<glow::UniformLocation> {
        if let Some(l) = self.uniforms.get(name) {
            return *l;
        }
        let l = unsafe { self.gl.get_uniform_location(self.program, name) };
        self.uniforms.insert(name.to_string(), l);
        l
    }

    pub fn set_i32(&mut self, name: &str, v: i32) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_1_i32(Some(&l), v) };
        }
    }

    pub fn set_f32(&mut self, name: &str, v: f32) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_1_f32(Some(&l), v) };
        }
    }

    pub fn set_ivec2(&mut self, name: &str, v: [i32; 2]) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_2_i32(Some(&l), v[0], v[1]) };
        }
    }

    /// Bind a texture for sampling.
    pub fn set_texture(&mut self, name: &str, unit: u32, texture: glow::Texture) {
        unsafe {
            self.gl.active_texture(glow::TEXTURE0 + unit);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        }
        self.set_i32(name, unit as i32);
    }

    /// Bind a texture as a writable image.
    ///
    /// `format` must match the shader's layout qualifier exactly — the spec
    /// makes a mismatch undefined behaviour, and in practice it silently writes
    /// garbage rather than raising an error.
    pub fn bind_image(&self, unit: u32, texture: glow::Texture, format: u32, write: bool) {
        unsafe {
            self.gl.bind_image_texture(
                unit,
                Some(texture),
                0,
                false,
                0,
                if write { glow::WRITE_ONLY } else { glow::READ_ONLY },
                format,
            );
        }
    }

    /// Dispatch enough workgroups to cover `width` × `height` items.
    pub fn dispatch_covering(&self, width: u32, height: u32, group: (u32, u32)) {
        let gx = width.div_ceil(group.0.max(1)).max(1);
        let gy = height.div_ceil(group.1.max(1)).max(1);
        unsafe { self.gl.dispatch_compute(gx, gy, 1) };
    }

    /// Wait for image writes to become visible to later texture reads.
    ///
    /// Without this the next pass may sample a texture the GPU has not finished
    /// writing. It is not a stall — it is a visibility barrier — but forgetting
    /// it produces tearing and half-blurred tiles that look like a shader bug.
    pub fn barrier_image_to_texture(&self) {
        unsafe {
            self.gl.memory_barrier(
                glow::TEXTURE_FETCH_BARRIER_BIT | glow::SHADER_IMAGE_ACCESS_BARRIER_BIT,
            );
        }
    }

    pub fn barrier_storage(&self) {
        unsafe { self.gl.memory_barrier(glow::SHADER_STORAGE_BARRIER_BIT) };
    }
}

impl Drop for ComputeProgram {
    fn drop(&mut self) {
        unsafe { self.gl.delete_program(self.program) };
    }
}

/// A shader storage buffer, for compute output that is not an image.
pub struct StorageBuffer {
    gl: Rc<glow::Context>,
    buffer: glow::Buffer,
    len_bytes: usize,
}

impl StorageBuffer {
    pub fn new(gl: Rc<glow::Context>, len_bytes: usize) -> Result<Self> {
        unsafe {
            let buffer = gl.create_buffer().map_err(GpuError::Gl)?;
            gl.bind_buffer(glow::SHADER_STORAGE_BUFFER, Some(buffer));
            gl.buffer_data_size(
                glow::SHADER_STORAGE_BUFFER,
                len_bytes as i32,
                glow::DYNAMIC_READ,
            );
            gl.bind_buffer(glow::SHADER_STORAGE_BUFFER, None);
            Ok(Self { gl, buffer, len_bytes })
        }
    }

    pub fn bind(&self, binding: u32) {
        unsafe {
            self.gl.bind_buffer_base(glow::SHADER_STORAGE_BUFFER, binding, Some(self.buffer));
        }
    }

    pub fn clear_to_zero(&self) {
        unsafe {
            let zeros = vec![0u8; self.len_bytes];
            self.gl.bind_buffer(glow::SHADER_STORAGE_BUFFER, Some(self.buffer));
            self.gl.buffer_sub_data_u8_slice(glow::SHADER_STORAGE_BUFFER, 0, &zeros);
            self.gl.bind_buffer(glow::SHADER_STORAGE_BUFFER, None);
        }
    }

    /// Read the buffer back as `u32`s.
    ///
    /// Uses `glMapBufferRange` rather than `glGetBufferSubData`: the latter is
    /// desktop-GL only, and GTK hands us a GLES context on plenty of systems —
    /// where calling it does not fail gracefully but aborts the process.
    pub fn read_u32(&self) -> Vec<u32> {
        let count = self.len_bytes / 4;
        let mut out = vec![0u32; count];
        unsafe {
            self.gl.bind_buffer(glow::SHADER_STORAGE_BUFFER, Some(self.buffer));
            let ptr = self.gl.map_buffer_range(
                glow::SHADER_STORAGE_BUFFER,
                0,
                self.len_bytes as i32,
                glow::MAP_READ_BIT,
            );
            if !ptr.is_null() {
                std::ptr::copy_nonoverlapping(
                    ptr as *const u8,
                    out.as_mut_ptr() as *mut u8,
                    self.len_bytes,
                );
                self.gl.unmap_buffer(glow::SHADER_STORAGE_BUFFER);
            }
            self.gl.bind_buffer(glow::SHADER_STORAGE_BUFFER, None);
        }
        out
    }
}

impl Drop for StorageBuffer {
    fn drop(&mut self) {
        unsafe { self.gl.delete_buffer(self.buffer) };
    }
}

/// Lazily-compiled cache of compute programs, mirroring `ShaderLibrary`.
pub struct ComputeLibrary {
    gl: Rc<glow::Context>,
    flavor: GlFlavor,
    programs: std::collections::HashMap<&'static str, ComputeProgram>,
}

impl ComputeLibrary {
    pub fn new(gl: Rc<glow::Context>, flavor: GlFlavor) -> Self {
        Self { gl, flavor, programs: Default::default() }
    }

    pub fn get(&mut self, key: &str) -> Result<&mut ComputeProgram> {
        if !self.programs.contains_key(key) {
            let (name, src) = COMPUTE_SOURCES
                .iter()
                .find(|(k, _)| *k == key)
                .ok_or_else(|| GpuError::Shader(format!("no compute shader named `{key}`")))?;
            let program = ComputeProgram::new(
                self.gl.clone(),
                self.flavor,
                name,
                src,
                crate::program::COMMON_SOURCE,
            )?;
            self.programs.insert(name, program);
        }
        Ok(self.programs.get_mut(key).expect("just inserted"))
    }

    pub fn compile_all(&mut self) -> Result<usize> {
        let keys: Vec<&str> = COMPUTE_SOURCES.iter().map(|(k, _)| *k).collect();
        let mut errors = Vec::new();
        for key in &keys {
            if let Err(e) = self.get(key) {
                errors.push(e.to_string());
            }
        }
        if errors.is_empty() {
            Ok(keys.len())
        } else {
            Err(GpuError::Shader(errors.join("\n\n")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_reject_versions_without_compute() {
        // Constructed directly rather than detected, to check the reporting.
        let caps = Capabilities {
            version: (3, 3),
            is_es: false,
            compute: false,
            max_workgroup_invocations: 0,
            max_shared_memory: 0,
        };
        assert!(caps.describe().contains("fragment fallbacks"));
    }

    #[test]
    fn capabilities_describe_a_working_context() {
        let caps = Capabilities {
            version: (4, 5),
            is_es: false,
            compute: true,
            max_workgroup_invocations: 1024,
            max_shared_memory: 32768,
        };
        let d = caps.describe();
        assert!(d.contains("GL 4.5"));
        assert!(d.contains("32 KB"));
        assert!(!d.contains("NO"));
    }
}
