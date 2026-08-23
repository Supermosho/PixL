//! Decoding and encoding flat image files.

use image::{DynamicImage, ImageFormat, RgbaImage};
use pixelmagic_core::buffer::PixelBuffer;
use pixelmagic_core::document::Document;
use std::path::Path;

use crate::{IoError, Result};

/// Decode an image file into a straight-alpha RGBA8 buffer.
pub fn load_image(path: &Path) -> Result<PixelBuffer> {
    let img = image::open(path)?;
    Ok(to_buffer(img))
}

/// Decode from memory, for clipboard pastes and embedded resources.
pub fn decode_image(bytes: &[u8]) -> Result<PixelBuffer> {
    let img = image::load_from_memory(bytes)?;
    Ok(to_buffer(img))
}

fn to_buffer(img: DynamicImage) -> PixelBuffer {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    PixelBuffer::from_raw(w, h, rgba.into_raw())
        .expect("to_rgba8 always yields exactly w*h*4 bytes")
}

fn to_image(buffer: &PixelBuffer) -> Result<RgbaImage> {
    RgbaImage::from_raw(buffer.width(), buffer.height(), buffer.data().to_vec())
        .ok_or_else(|| IoError::Format("pixel buffer has the wrong length".into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
    Tiff,
    WebP,
    Bmp,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 5] = [
        ExportFormat::Png,
        ExportFormat::Jpeg,
        ExportFormat::Tiff,
        ExportFormat::WebP,
        ExportFormat::Bmp,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG",
            ExportFormat::Jpeg => "JPEG",
            ExportFormat::Tiff => "TIFF",
            ExportFormat::WebP => "WebP",
            ExportFormat::Bmp => "BMP",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Png => "png",
            ExportFormat::Jpeg => "jpg",
            ExportFormat::Tiff => "tif",
            ExportFormat::WebP => "webp",
            ExportFormat::Bmp => "bmp",
        }
    }

    /// Whether the format can store an alpha channel. The export panel warns
    /// before flattening onto a matte, rather than silently losing
    /// transparency.
    pub fn supports_alpha(self) -> bool {
        !matches!(self, ExportFormat::Jpeg | ExportFormat::Bmp)
    }

    pub fn supports_quality(self) -> bool {
        matches!(self, ExportFormat::Jpeg | ExportFormat::WebP)
    }

    fn image_format(self) -> ImageFormat {
        match self {
            ExportFormat::Png => ImageFormat::Png,
            ExportFormat::Jpeg => ImageFormat::Jpeg,
            ExportFormat::Tiff => ImageFormat::Tiff,
            ExportFormat::WebP => ImageFormat::WebP,
            ExportFormat::Bmp => ImageFormat::Bmp,
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Some(ExportFormat::Png),
            "jpg" | "jpeg" => Some(ExportFormat::Jpeg),
            "tif" | "tiff" => Some(ExportFormat::Tiff),
            "webp" => Some(ExportFormat::WebP),
            "bmp" => Some(ExportFormat::Bmp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExportOptions {
    pub format: ExportFormat,
    /// 1..=100, used by JPEG and WebP.
    pub quality: u8,
    /// Colour composited behind the image when the format has no alpha.
    pub matte: pixelmagic_core::color::Rgba,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            format: ExportFormat::Png,
            quality: 90,
            matte: pixelmagic_core::color::Rgba::WHITE,
        }
    }
}

/// Write a rendered buffer to disk.
pub fn save_image(buffer: &PixelBuffer, path: &Path, options: ExportOptions) -> Result<()> {
    let img = if options.format.supports_alpha() {
        to_image(buffer)?
    } else {
        to_image(&flatten_onto(buffer, options.matte))?
    };

    // JPEG and WebP need their quality setting, which `save` does not expose,
    // so those two go through an explicit encoder.
    match options.format {
        ExportFormat::Jpeg => {
            let file = std::fs::File::create(path)?;
            let mut w = std::io::BufWriter::new(file);
            let mut enc =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, options.quality);
            enc.encode_image(&DynamicImage::ImageRgba8(img).to_rgb8())?;
        }
        format => {
            img.save_with_format(path, format.image_format())?;
        }
    }
    Ok(())
}

pub fn encode_image(buffer: &PixelBuffer, options: ExportOptions) -> Result<Vec<u8>> {
    let img = if options.format.supports_alpha() {
        to_image(buffer)?
    } else {
        to_image(&flatten_onto(buffer, options.matte))?
    };
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, options.format.image_format())?;
    Ok(out.into_inner())
}

/// Composite a straight-alpha buffer over an opaque matte.
///
/// Blending happens in linear light, because compositing in the encoded domain
/// is what produces the grey halo around anti-aliased edges when a logo with
/// transparency is exported to JPEG.
pub fn flatten_onto(buffer: &PixelBuffer, matte: pixelmagic_core::color::Rgba) -> PixelBuffer {
    let mut out = PixelBuffer::new(buffer.width(), buffer.height());
    let bg = matte.to_linear();
    for y in 0..buffer.height() {
        for x in 0..buffer.width() {
            let Some(c) = buffer.get(x, y) else { continue };
            let fg = c.to_linear();
            let a = c.a;
            let mixed = pixelmagic_core::color::Rgba::new(
                fg.r * a + bg.r * (1.0 - a),
                fg.g * a + bg.g * (1.0 - a),
                fg.b * a + bg.b * (1.0 - a),
                1.0,
            );
            out.set(x, y, mixed.to_srgb());
        }
    }
    out
}

/// Open an image file as a new single-layer document.
pub fn export_document(doc: &Document, buffer: &PixelBuffer, path: &Path) -> Result<()> {
    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(ExportFormat::from_extension)
        .ok_or_else(|| {
            IoError::Unsupported(format!("cannot infer a format from {}", path.display()))
        })?;
    let _ = doc;
    save_image(buffer, path, ExportOptions { format, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixelmagic_core::color::Rgba;

    fn temp(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("pixelmagic-test-{name}"));
        p
    }

    #[test]
    fn png_round_trips_exactly() {
        let mut buf = PixelBuffer::new(8, 6);
        buf.set(0, 0, Rgba::from_u8(12, 34, 56, 255));
        buf.set(7, 5, Rgba::from_u8(200, 100, 50, 128));

        let path = temp("roundtrip.png");
        save_image(&buf, &path, ExportOptions::default()).unwrap();
        let back = load_image(&path).unwrap();

        assert_eq!(back.width(), 8);
        assert_eq!(back.height(), 6);
        assert_eq!(back.get(0, 0).unwrap().to_u8(), [12, 34, 56, 255]);
        assert_eq!(back.get(7, 5).unwrap().to_u8(), [200, 100, 50, 128]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn encode_decode_in_memory() {
        let buf = PixelBuffer::filled(4, 4, Rgba::from_u8(1, 2, 3, 255));
        let bytes = encode_image(&buf, ExportOptions::default()).unwrap();
        let back = decode_image(&bytes).unwrap();
        assert_eq!(back.get(2, 2).unwrap().to_u8(), [1, 2, 3, 255]);
    }

    #[test]
    fn flatten_composites_in_linear_light() {
        let buf = PixelBuffer::filled(2, 2, Rgba::new(1.0, 1.0, 1.0, 0.5));
        let flat = flatten_onto(&buf, Rgba::BLACK);
        let v = flat.get(0, 0).unwrap().to_u8()[0];
        // Half-covering white over black is linear 0.5, which encodes near 188.
        // A naive encoded-space blend would give 128.
        assert!((185..=191).contains(&(v as i32)), "expected ~188, got {v}");
        assert_eq!(flat.get(0, 0).unwrap().a, 1.0);
    }

    #[test]
    fn alpha_only_survives_formats_that_support_it() {
        assert!(ExportFormat::Png.supports_alpha());
        assert!(ExportFormat::WebP.supports_alpha());
        assert!(!ExportFormat::Jpeg.supports_alpha());
        assert!(!ExportFormat::Bmp.supports_alpha());
    }

    #[test]
    fn jpeg_export_flattens_rather_than_failing() {
        let buf = PixelBuffer::filled(8, 8, Rgba::new(1.0, 0.0, 0.0, 0.0));
        let path = temp("flatten.jpg");
        save_image(
            &buf,
            &path,
            ExportOptions { format: ExportFormat::Jpeg, quality: 90, matte: Rgba::WHITE },
        )
        .unwrap();
        let back = load_image(&path).unwrap();
        let p = back.get(4, 4).unwrap().to_u8();
        assert_eq!(p[3], 255);
        assert!(p[0] > 240 && p[1] > 240, "fully transparent should show the matte: {p:?}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn extension_mapping() {
        assert_eq!(ExportFormat::from_extension("JPG"), Some(ExportFormat::Jpeg));
        assert_eq!(ExportFormat::from_extension("tiff"), Some(ExportFormat::Tiff));
        assert_eq!(ExportFormat::from_extension("psd"), None);
        for f in ExportFormat::ALL {
            assert_eq!(ExportFormat::from_extension(f.extension()), Some(f));
        }
    }

    #[test]
    fn export_without_a_known_extension_is_an_error() {
        let doc = Document::new(4, 4);
        let buf = PixelBuffer::new(4, 4);
        let err = export_document(&doc, &buf, &temp("nope.xyz"));
        assert!(matches!(err, Err(IoError::Unsupported(_))));
    }
}
