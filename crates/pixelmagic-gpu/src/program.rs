//! Shader compilation, caching and uniform binding.

use glow::HasContext;
use std::collections::HashMap;
use std::rc::Rc;

use crate::{GpuError, Result};

/// Which GLSL dialect the current context wants.
///
/// GTK's `GLArea` hands us desktop GL on most Linux systems but GLES on some
/// drivers and inside containers, and the two need different `#version` lines
/// and precision qualifiers. Getting this wrong is not a subtle bug — nothing
/// compiles at all — so it is detected once and threaded through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlFlavor {
    /// Desktop OpenGL 3.3+ core profile.
    Core,
    /// OpenGL ES 3.0+.
    Es,
}

impl GlFlavor {
    fn header(self) -> &'static str {
        match self {
            GlFlavor::Core => "#version 330 core\n",
            // GLES has no default float precision in fragment shaders, so one
            // must be declared or every shader fails to compile.
            GlFlavor::Es => {
                "#version 300 es\nprecision highp float;\nprecision highp sampler2D;\n"
            }
        }
    }
}

/// A linked shader program plus its uniform-location cache.
pub struct Program {
    gl: Rc<glow::Context>,
    program: glow::Program,
    uniforms: HashMap<String, Option<glow::UniformLocation>>,
}

impl Program {
    pub fn new(
        gl: Rc<glow::Context>,
        flavor: GlFlavor,
        name: &str,
        vertex_src: &str,
        fragment_src: &str,
        common: &str,
    ) -> Result<Self> {
        unsafe {
            let vs = compile(&gl, flavor, glow::VERTEX_SHADER, name, vertex_src, common)?;
            let fs = compile(&gl, flavor, glow::FRAGMENT_SHADER, name, fragment_src, common)?;

            let program =
                gl.create_program().map_err(|e| GpuError::Shader(format!("{name}: {e}")))?;
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);

            // Shaders can be deleted once linked; the program keeps what it
            // needs.
            gl.delete_shader(vs);
            gl.delete_shader(fs);

            if !gl.get_program_link_status(program) {
                let log = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(GpuError::Shader(format!("{name}: link failed: {log}")));
            }

            Ok(Self { gl, program, uniforms: HashMap::new() })
        }
    }

    pub fn bind(&self) {
        unsafe { self.gl.use_program(Some(self.program)) };
    }

    fn location(&mut self, name: &str) -> Option<glow::UniformLocation> {
        // Looking a uniform up is a driver round-trip, and these are set every
        // frame, so cache — including the negative result, since an optimised-
        // out uniform would otherwise be looked up forever.
        if let Some(loc) = self.uniforms.get(name) {
            return *loc;
        }
        let loc = unsafe { self.gl.get_uniform_location(self.program, name) };
        self.uniforms.insert(name.to_string(), loc);
        loc
    }

    pub fn set_f32(&mut self, name: &str, v: f32) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_1_f32(Some(&l), v) };
        }
    }

    pub fn set_i32(&mut self, name: &str, v: i32) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_1_i32(Some(&l), v) };
        }
    }

    pub fn set_bool(&mut self, name: &str, v: bool) {
        self.set_i32(name, if v { 1 } else { 0 });
    }

    pub fn set_vec2(&mut self, name: &str, v: [f32; 2]) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_2_f32(Some(&l), v[0], v[1]) };
        }
    }

    pub fn set_vec3(&mut self, name: &str, v: [f32; 3]) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_3_f32(Some(&l), v[0], v[1], v[2]) };
        }
    }

    pub fn set_vec4(&mut self, name: &str, v: [f32; 4]) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_4_f32(Some(&l), v[0], v[1], v[2], v[3]) };
        }
    }

    pub fn set_vec3_array(&mut self, name: &str, v: &[f32]) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_3_f32_slice(Some(&l), v) };
        }
    }

    pub fn set_vec4_array(&mut self, name: &str, v: &[f32]) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_4_f32_slice(Some(&l), v) };
        }
    }

    pub fn set_f32_array(&mut self, name: &str, v: &[f32]) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_1_f32_slice(Some(&l), v) };
        }
    }

    pub fn set_mat3(&mut self, name: &str, v: &[f32; 9]) {
        if let Some(l) = self.location(name) {
            unsafe { self.gl.uniform_matrix_3_f32_slice(Some(&l), false, v) };
        }
    }

    /// Bind a texture to a unit and point the sampler uniform at it.
    pub fn set_texture(&mut self, name: &str, unit: u32, texture: glow::Texture) {
        unsafe {
            self.gl.active_texture(glow::TEXTURE0 + unit);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(texture));
        }
        self.set_i32(name, unit as i32);
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        unsafe { self.gl.delete_program(self.program) };
    }
}

unsafe fn compile(
    gl: &glow::Context,
    flavor: GlFlavor,
    kind: u32,
    name: &str,
    src: &str,
    common: &str,
) -> Result<glow::Shader> {
    let full = format!("{}{}\n{}", flavor.header(), common, src);
    let shader =
        gl.create_shader(kind).map_err(|e| GpuError::Shader(format!("{name}: {e}")))?;
    gl.shader_source(shader, &full);
    gl.compile_shader(shader);

    if !gl.get_shader_compile_status(shader) {
        let log = gl.get_shader_info_log(shader);
        gl.delete_shader(shader);
        let stage = if kind == glow::VERTEX_SHADER { "vertex" } else { "fragment" };
        // Numbering the source makes the driver's "0:73: error" actually
        // actionable, which matters when the shader the driver sees is the
        // concatenation of three files.
        let listing: String =
            full.lines().enumerate().map(|(i, l)| format!("{:4} | {l}\n", i + 1)).collect();
        return Err(GpuError::Shader(format!(
            "{name} ({stage}) failed to compile:\n{log}\n--- source ---\n{listing}"
        )));
    }
    Ok(shader)
}

// ---------------------------------------------------------------------------
// The library
// ---------------------------------------------------------------------------

/// Every shader in the crate, embedded at build time.
///
/// Embedding rather than loading from disk means a built binary has no runtime
/// asset dependency, and a shader that fails to exist is a compile error rather
/// than a crash on first use.
pub struct ShaderLibrary {
    gl: Rc<glow::Context>,
    flavor: GlFlavor,
    programs: HashMap<&'static str, Program>,
}

macro_rules! shader_sources {
    ($($key:literal => $path:literal),* $(,)?) => {
        pub static FRAGMENT_SOURCES: &[(&str, &str)] = &[
            $(($key, include_str!(concat!("../shaders/", $path)))),*
        ];
    };
}

shader_sources! {
    "composite" => "composite.frag",
    "present" => "present.frag",
    "backdrop" => "backdrop.frag",
    "selection_overlay" => "selection_overlay.frag",
    "encode_srgb" => "encode_srgb.frag",
    "place" => "place.frag",
    "mask_apply" => "mask_apply.frag",

    "adjust.basic" => "adjust/basic.frag",
    "adjust.white_balance" => "adjust/white_balance.frag",
    "adjust.hue_saturation" => "adjust/hue_saturation.frag",
    "adjust.levels" => "adjust/levels.frag",
    "adjust.curves" => "adjust/curves.frag",
    "adjust.black_white" => "adjust/black_white.frag",
    "adjust.invert" => "adjust/invert.frag",
    "adjust.channel_mixer" => "adjust/channel_mixer.frag",
    "adjust.vignette" => "adjust/vignette.frag",
    "adjust.grain" => "adjust/grain.frag",
    "adjust.sharpen" => "adjust/sharpen.frag",
    "adjust.color_balance" => "adjust/color_balance.frag",
    "adjust.selective_color" => "adjust/selective_color.frag",
    "adjust.replace_color" => "adjust/replace_color.frag",

    "effect.blur_separable" => "effect/blur_separable.frag",
    "effect.blur_disc" => "effect/blur_disc.frag",
    "effect.blur_radial" => "effect/blur_radial.frag",
    "effect.blur_mask" => "effect/blur_mask.frag",
    "effect.distort" => "effect/distort.frag",
    "effect.kaleidoscope" => "effect/kaleidoscope.frag",
    "effect.pixelate" => "effect/pixelate.frag",
    "effect.crystallize" => "effect/crystallize.frag",
    "effect.posterize" => "effect/posterize.frag",
    "effect.threshold" => "effect/threshold.frag",
    "effect.tint" => "effect/tint.frag",
    "effect.gradient_map" => "effect/gradient_map.frag",
    "effect.exposure" => "effect/exposure.frag",
    "effect.color_controls" => "effect/color_controls.frag",
    "effect.hue_adjust" => "effect/hue_adjust.frag",
    "effect.noise" => "effect/noise.frag",
    "effect.bloom" => "effect/bloom.frag",
    "effect.halftone" => "effect/halftone.frag",
    "effect.checkerboard" => "effect/checkerboard.frag",
    "effect.clouds" => "effect/clouds.frag",
    "effect.fill_color" => "effect/fill_color.frag",
    "effect.frequency" => "effect/frequency.frag",
    "effect.mask_to_alpha" => "effect/mask_to_alpha.frag",
}

pub const VERTEX_SOURCE: &str = include_str!("../shaders/quad.vert");
pub const COMMON_SOURCE: &str = include_str!("../shaders/common.glsl");

impl ShaderLibrary {
    pub fn new(gl: Rc<glow::Context>, flavor: GlFlavor) -> Self {
        Self { gl, flavor, programs: HashMap::new() }
    }

    /// Fetch a program, compiling it the first time it is asked for.
    ///
    /// Lazy compilation keeps start-up quick: a session that never opens the
    /// Halftone panel never pays for those four shaders.
    pub fn get(&mut self, key: &str) -> Result<&mut Program> {
        if !self.programs.contains_key(key) {
            let (name, src) = FRAGMENT_SOURCES
                .iter()
                .find(|(k, _)| *k == key)
                .ok_or_else(|| GpuError::Shader(format!("no shader named `{key}`")))?;
            let program = Program::new(
                self.gl.clone(),
                self.flavor,
                name,
                VERTEX_SOURCE,
                src,
                COMMON_SOURCE,
            )?;
            self.programs.insert(name, program);
        }
        Ok(self.programs.get_mut(key).expect("just inserted"))
    }

    /// Compile everything. Used by the test suite and by `--check-shaders`, so
    /// a broken shader is caught at build time rather than when a user first
    /// reaches for that effect.
    pub fn compile_all(&mut self) -> Result<usize> {
        let keys: Vec<&str> = FRAGMENT_SOURCES.iter().map(|(k, _)| *k).collect();
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

    pub fn compiled_count(&self) -> usize {
        self.programs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shader_key_is_unique() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in FRAGMENT_SOURCES {
            assert!(seen.insert(*k), "duplicate shader key {k}");
        }
    }

    #[test]
    fn sources_are_non_empty_and_declare_an_entry_point() {
        for (k, src) in FRAGMENT_SOURCES {
            assert!(!src.trim().is_empty(), "{k} is empty");
            assert!(src.contains("void main()"), "{k} has no main()");
            assert!(src.contains("frag_color"), "{k} never writes an output");
        }
        assert!(VERTEX_SOURCE.contains("void main()"));
        assert!(COMMON_SOURCE.contains("PIXELMAGIC_COMMON"));
    }

    #[test]
    fn no_shader_declares_its_own_version() {
        // The header is prepended, so a `#version` in the body would land in
        // the middle of the file and fail to compile.
        for (k, src) in FRAGMENT_SOURCES {
            assert!(!src.contains("#version"), "{k} should not set #version");
        }
        assert!(!VERTEX_SOURCE.contains("#version"));
    }

    #[test]
    fn flavor_headers_differ_where_it_matters() {
        assert!(GlFlavor::Core.header().starts_with("#version 330 core"));
        assert!(GlFlavor::Es.header().contains("precision highp float"));
    }
}
