//! Textures, render targets and the target pool.

use glow::HasContext;
use std::rc::Rc;

use crate::{GpuError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// The working format: 16-bit float per channel, linear light.
    ///
    /// 8-bit intermediates would band visibly after a few adjustment passes —
    /// each one quantises again — and 32-bit doubles the bandwidth for
    /// precision no one can see. Half-float is the standard compromise, and it
    /// also leaves headroom above 1.0 for highlights that a later pass may pull
    /// back down.
    Rgba16f,
    /// 8-bit, for uploading source images and reading back results.
    Rgba8,
    /// 8-bit sRGB-encoded with linear alpha.
    ///
    /// Source images arrive gamma-encoded. Handing them to the GPU as this
    /// format makes every `texture()` fetch return linear light for free, in
    /// the texture unit's fixed-function hardware — which is both faster and
    /// less error-prone than remembering to call a decode function in each of
    /// forty shaders. Alpha is passed through untouched, which is correct:
    /// alpha was never gamma-encoded.
    Srgb8,
    /// Single-channel 8-bit: masks and selections.
    R8,
    /// Single-channel float: baked curve and level lookup tables.
    R32f,
}

impl Format {
    fn internal(self) -> u32 {
        match self {
            Format::Rgba16f => glow::RGBA16F,
            Format::Rgba8 => glow::RGBA8,
            Format::Srgb8 => glow::SRGB8_ALPHA8,
            Format::R8 => glow::R8,
            Format::R32f => glow::R32F,
        }
    }

    fn layout(self) -> u32 {
        match self {
            Format::Rgba16f | Format::Rgba8 | Format::Srgb8 => glow::RGBA,
            Format::R8 | Format::R32f => glow::RED,
        }
    }

    fn component(self) -> u32 {
        match self {
            Format::Rgba16f | Format::R32f => glow::FLOAT,
            Format::Rgba8 | Format::Srgb8 | Format::R8 => glow::UNSIGNED_BYTE,
        }
    }

    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Format::Rgba16f => 8,
            Format::Rgba8 | Format::Srgb8 => 4,
            Format::R8 => 1,
            Format::R32f => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrap {
    /// Clamp to edge. The right default for image passes: repeating would wrap
    /// a blur's tail around to the opposite edge.
    Clamp,
    Repeat,
}

pub struct Texture {
    gl: Rc<glow::Context>,
    handle: glow::Texture,
    pub width: u32,
    pub height: u32,
    pub format: Format,
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texture")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .finish()
    }
}

impl Texture {
    pub fn new(
        gl: Rc<glow::Context>,
        width: u32,
        height: u32,
        format: Format,
        filter: Filter,
        wrap: Wrap,
    ) -> Result<Self> {
        let width = width.max(1);
        let height = height.max(1);
        unsafe {
            let handle = gl.create_texture().map_err(GpuError::Gl)?;
            gl.bind_texture(glow::TEXTURE_2D, Some(handle));
            // Immutable storage rather than `tex_image_2d`. Compute shaders
            // bind these as images via `glBindImageTexture`, which requires an
            // immutable format — a mutable texture is rejected outright on
            // strict drivers and silently misbehaves on lenient ones.
            gl.tex_storage_2d(
                glow::TEXTURE_2D,
                1,
                format.internal(),
                width as i32,
                height as i32,
            );
            let f = match filter {
                Filter::Nearest => glow::NEAREST,
                Filter::Linear => glow::LINEAR,
            } as i32;
            let w = match wrap {
                Wrap::Clamp => glow::CLAMP_TO_EDGE,
                Wrap::Repeat => glow::REPEAT,
            } as i32;
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, f);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, f);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, w);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, w);
            gl.bind_texture(glow::TEXTURE_2D, None);
            Ok(Self { gl, handle, width, height, format })
        }
    }

    pub fn handle(&self) -> glow::Texture {
        self.handle
    }

    /// Upload straight-alpha, sRGB-encoded RGBA8 pixels exactly as they are.
    ///
    /// Deliberately *not* premultiplied here. Premultiplying encoded values is
    /// wrong — `encode(c) * a` is not `encode(c * a)` — so the multiply has to
    /// happen after the texture unit has decoded to linear, which is what
    /// `place.frag` does.
    pub fn upload_srgb8_straight(&self, data: &[u8]) -> Result<()> {
        let expected = self.width as usize * self.height as usize * 4;
        if data.len() != expected {
            return Err(GpuError::Invalid(format!(
                "upload of {} bytes into a {}x{} texture expecting {expected}",
                data.len(),
                self.width,
                self.height
            )));
        }
        self.upload_raw(data)
    }

    pub fn upload_raw(&self, data: &[u8]) -> Result<()> {
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.handle));
            // Rows are tightly packed; the default of 4 would corrupt any R8
            // texture whose width is not a multiple of four.
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                self.width as i32,
                self.height as i32,
                self.format.layout(),
                self.format.component(),
                glow::PixelUnpackData::Slice(Some(data)),
            );
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
        Ok(())
    }

    pub fn upload_f32(&self, data: &[f32]) -> Result<()> {
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.handle));
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                self.width as i32,
                self.height as i32,
                self.format.layout(),
                glow::FLOAT,
                glow::PixelUnpackData::Slice(Some(bytemuck::cast_slice(data))),
            );
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
        Ok(())
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe { self.gl.delete_texture(self.handle) };
    }
}

/// A texture with a framebuffer attached, so passes can render into it.
pub struct RenderTarget {
    gl: Rc<glow::Context>,
    fbo: glow::Framebuffer,
    pub texture: Texture,
}

impl std::fmt::Debug for RenderTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderTarget").field("texture", &self.texture).finish()
    }
}

impl RenderTarget {
    pub fn new(gl: Rc<glow::Context>, width: u32, height: u32, format: Format) -> Result<Self> {
        let texture =
            Texture::new(gl.clone(), width, height, format, Filter::Linear, Wrap::Clamp)?;
        unsafe {
            let fbo = gl.create_framebuffer().map_err(GpuError::Gl)?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture.handle()),
                0,
            );
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            if status != glow::FRAMEBUFFER_COMPLETE {
                gl.delete_framebuffer(fbo);
                return Err(GpuError::Invalid(format!(
                    "framebuffer incomplete (0x{status:x}) for {width}x{height} {format:?}"
                )));
            }
            Ok(Self { gl, fbo, texture })
        }
    }

    pub fn width(&self) -> u32 {
        self.texture.width
    }

    pub fn height(&self) -> u32 {
        self.texture.height
    }

    /// Make this the active render target and set the viewport to match.
    pub fn bind(&self) {
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            self.gl.viewport(0, 0, self.width() as i32, self.height() as i32);
        }
    }

    pub fn clear(&self) {
        self.bind();
        unsafe {
            self.gl.clear_color(0.0, 0.0, 0.0, 0.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    /// Read an 8-bit target back verbatim.
    ///
    /// `GL_RGBA` + `GL_UNSIGNED_BYTE` is the one combination every GL and GLES
    /// implementation is required to support, so this works everywhere. Use
    /// [`crate::Renderer::read_image`] for a linear-light target — it encodes
    /// through a shader first, because reading floats back is not portable.
    ///
    /// No vertical flip: the renderer's UV convention already puts document
    /// row 0 at framebuffer row 0 (see `quad.vert`).
    pub fn read_rgba8(&self) -> Result<Vec<u8>> {
        if self.texture.format != Format::Rgba8 && self.texture.format != Format::Srgb8 {
            return Err(GpuError::Invalid(format!(
                "read_rgba8 needs an 8-bit target, got {:?}",
                self.texture.format
            )));
        }
        let (w, h) = (self.width() as usize, self.height() as usize);
        let mut buf = vec![0u8; w * h * 4];
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            self.gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            self.gl.read_pixels(
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut buf)),
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        Ok(buf)
    }
}

impl Drop for RenderTarget {
    fn drop(&mut self) {
        unsafe { self.gl.delete_framebuffer(self.fbo) };
    }
}

/// Recycles render targets between passes.
///
/// A layer stack with adjustments and effects needs a handful of intermediates
/// per layer, all the same size. Allocating and freeing those every frame would
/// thrash the driver's allocator and stall on the GPU; keeping a small pool of
/// same-shaped targets makes a redraw allocation-free in the steady state.
pub struct TargetPool {
    gl: Rc<glow::Context>,
    free: Vec<RenderTarget>,
    /// Targets whose shape no longer matches are dropped rather than kept
    /// forever; this caps how many we hold on to.
    capacity: usize,
}

impl TargetPool {
    pub fn new(gl: Rc<glow::Context>) -> Self {
        Self { gl, free: Vec::new(), capacity: 12 }
    }

    pub fn acquire(&mut self, width: u32, height: u32, format: Format) -> Result<RenderTarget> {
        let width = width.max(1);
        let height = height.max(1);
        if let Some(i) = self.free.iter().position(|t| {
            t.width() == width && t.height() == height && t.texture.format == format
        }) {
            let t = self.free.swap_remove(i);
            t.clear();
            return Ok(t);
        }
        RenderTarget::new(self.gl.clone(), width, height, format)
    }

    pub fn release(&mut self, target: RenderTarget) {
        if self.free.len() < self.capacity {
            self.free.push(target);
        }
    }

    /// Drop every pooled target. Called when the canvas size changes, since
    /// none of the old shapes will be asked for again.
    pub fn clear(&mut self) {
        self.free.clear();
    }

    pub fn pooled(&self) -> usize {
        self.free.len()
    }

    /// Approximate VRAM held by the pool, for the diagnostics panel.
    pub fn bytes(&self) -> usize {
        self.free
            .iter()
            .map(|t| {
                t.width() as usize * t.height() as usize * t.texture.format.bytes_per_pixel()
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_sizes() {
        assert_eq!(Format::Rgba16f.bytes_per_pixel(), 8);
        assert_eq!(Format::Rgba8.bytes_per_pixel(), 4);
        assert_eq!(Format::R8.bytes_per_pixel(), 1);
        assert_eq!(Format::R32f.bytes_per_pixel(), 4);
    }

    #[test]
    fn formats_pick_matching_layouts() {
        assert_eq!(Format::Rgba16f.layout(), glow::RGBA);
        assert_eq!(Format::R8.layout(), glow::RED);
        assert_eq!(Format::R32f.component(), glow::FLOAT);
        assert_eq!(Format::Rgba8.component(), glow::UNSIGNED_BYTE);
    }
}
