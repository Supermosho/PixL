//! The `.pxm` document container.
//!
//! A `.pxm` is a Zip archive:
//!
//! ```text
//! mimetype            stored uncompressed, first entry: "application/x-pixelmagic"
//! document.json       the full document model, minus pixel data
//! layers/<n>.png      one PNG per pixel layer, in depth-first order
//! masks/<n>.png       one greyscale PNG per bitmap mask
//! ```
//!
//! Zip-of-parts rather than one big serialised blob, for three reasons. Pixel
//! data stays in a format anything can read, so a document is recoverable with
//! `unzip` even if this code is gone. PNG's own compression beats anything a
//! generic serialiser would do with image bytes. And `document.json` stays
//! small and human-readable, which makes debugging a broken file tractable.
//!
//! The uncompressed leading `mimetype` entry is the ODF/EPUB convention: it
//! lets `file(1)` and other sniffers identify the format from the first few
//! bytes without decompressing anything.

use pixelmagic_core::buffer::{MaskBuffer, PixelBuffer};
use pixelmagic_core::document::Document;
use pixelmagic_core::layer::{LayerId, LayerKind, Mask};
use std::io::{Read, Seek, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;

use crate::{IoError, Result};

pub const PXM_EXTENSION: &str = "pxm";
pub const PXM_MIMETYPE: &str = "application/x-pixelmagic";
/// Bumped when the on-disk layout changes incompatibly.
pub const PXM_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    version: u32,
    generator: String,
    document: Document,
}

/// Write a document to `path`.
pub fn save_document(doc: &Document, path: &Path) -> Result<()> {
    let file = std::fs::File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);

    zip.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )?;
    zip.write_all(PXM_MIMETYPE.as_bytes())?;

    // Strip the pixel data out of the copy that gets serialised, writing each
    // buffer to its own PNG as we go.
    let mut stripped = doc.clone();
    let order: Vec<LayerId> =
        doc.layers.iter_depth_first().into_iter().map(|(id, _)| id).collect();

    let deflated =
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    for (index, id) in order.iter().enumerate() {
        let Some(layer) = doc.layers.get(*id) else { continue };

        if let LayerKind::Pixel { buffer } = &layer.kind {
            if !buffer.is_empty() {
                zip.start_file(format!("layers/{index}.png"), deflated)?;
                let png = crate::image_io::encode_image(
                    buffer,
                    crate::image_io::ExportOptions::default(),
                )?;
                zip.write_all(&png)?;
            }
            if let Some(s) = stripped.layers.get_mut(*id) {
                s.kind = LayerKind::Pixel { buffer: PixelBuffer::new(0, 0) };
            }
        }

        if let Some(Mask::Bitmap { buffer, .. }) = &layer.mask {
            zip.start_file(format!("masks/{index}.png"), deflated)?;
            let gray = image::GrayImage::from_raw(
                buffer.width(),
                buffer.height(),
                buffer.data().to_vec(),
            )
            .ok_or_else(|| IoError::Format("mask buffer has the wrong length".into()))?;
            let mut out = std::io::Cursor::new(Vec::new());
            gray.write_to(&mut out, image::ImageFormat::Png)?;
            zip.write_all(&out.into_inner())?;

            if let Some(s) = stripped.layers.get_mut(*id) {
                if let Some(Mask::Bitmap { buffer, .. }) = &mut s.mask {
                    *buffer = MaskBuffer::new(0, 0);
                }
            }
        }
    }

    let manifest = Manifest {
        version: PXM_VERSION,
        generator: format!("Pixelmagic {}", env!("CARGO_PKG_VERSION")),
        document: stripped,
    };
    zip.start_file("document.json", deflated)?;
    let json =
        serde_json::to_vec_pretty(&manifest).map_err(|e| IoError::Format(e.to_string()))?;
    zip.write_all(&json)?;

    zip.finish()?;
    Ok(())
}

/// Read a document from `path`.
pub fn load_document(path: &Path) -> Result<Document> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let manifest: Manifest = {
        let mut entry = zip.by_name("document.json").map_err(|_| {
            IoError::Format("not a Pixelmagic document: no document.json".into())
        })?;
        let mut buf = String::new();
        entry.read_to_string(&mut buf)?;
        serde_json::from_str(&buf).map_err(|e| IoError::Format(e.to_string()))?
    };

    if manifest.version > PXM_VERSION {
        return Err(IoError::Format(format!(
            "this document was written by a newer version of Pixelmagic \
             (format {}, this build reads up to {PXM_VERSION})",
            manifest.version
        )));
    }

    let mut doc = manifest.document;
    let order: Vec<LayerId> =
        doc.layers.iter_depth_first().into_iter().map(|(id, _)| id).collect();

    for (index, id) in order.iter().enumerate() {
        if let Some(buffer) = read_optional_png(&mut zip, &format!("layers/{index}.png"))? {
            if let Some(layer) = doc.layers.get_mut(*id) {
                if matches!(layer.kind, LayerKind::Pixel { .. }) {
                    layer.kind = LayerKind::Pixel { buffer };
                }
            }
        }
        if let Some(mask) = read_optional_mask(&mut zip, &format!("masks/{index}.png"))? {
            if let Some(layer) = doc.layers.get_mut(*id) {
                if let Some(Mask::Bitmap { buffer, .. }) = &mut layer.mask {
                    *buffer = mask;
                }
            }
        }
    }

    doc.path = Some(path.to_path_buf());
    doc.dirty = false;
    Ok(doc)
}

fn read_optional_png<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Option<PixelBuffer>> {
    let mut bytes = Vec::new();
    match zip.by_name(name) {
        Ok(mut e) => e.read_to_end(&mut bytes)?,
        // A missing entry is normal: empty layers write no PNG at all.
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(Some(crate::image_io::decode_image(&bytes)?))
}

fn read_optional_mask<R: Read + Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Option<MaskBuffer>> {
    let mut bytes = Vec::new();
    match zip.by_name(name) {
        Ok(mut e) => e.read_to_end(&mut bytes)?,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let img = image::load_from_memory(&bytes)?.to_luma8();
    let (w, h) = img.dimensions();
    Ok(MaskBuffer::from_raw(w, h, img.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixelmagic_core::blend::BlendMode;
    use pixelmagic_core::color::Rgba;

    fn temp(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("pixelmagic-container-{name}.pxm"));
        p
    }

    fn sample() -> Document {
        let mut doc = Document::empty(32, 24);
        doc.name = "Sample".into();
        doc.resolution = 144.0;

        let base = doc.layers.insert(
            "Base",
            LayerKind::Pixel {
                buffer: PixelBuffer::filled(32, 24, Rgba::from_u8(10, 20, 30, 255)),
            },
            None,
        );
        let group = doc.layers.insert("Group", LayerKind::Group, None);
        let child = doc.layers.insert(
            "Child",
            LayerKind::Pixel {
                buffer: PixelBuffer::filled(8, 8, Rgba::from_u8(200, 0, 0, 128)),
            },
            Some(group),
        );

        doc.layers.get_mut(child).unwrap().blend_mode = BlendMode::Overlay;
        doc.layers.get_mut(child).unwrap().opacity = 0.6;
        doc.layers.get_mut(base).unwrap().mask = Some(Mask::bitmap(32, 24));
        doc.active = vec![child];
        doc
    }

    #[test]
    fn documents_round_trip() {
        let doc = sample();
        let path = temp("roundtrip");
        save_document(&doc, &path).unwrap();
        let back = load_document(&path).unwrap();

        assert_eq!(back.name, "Sample");
        assert_eq!((back.width, back.height), (32, 24));
        assert_eq!(back.resolution, 144.0);
        assert_eq!(back.layers.len(), doc.layers.len());
        assert!(!back.dirty);
        assert_eq!(back.path.as_deref(), Some(path.as_path()));

        // Structure and per-layer settings survived.
        assert_eq!(back.layers, doc.layers);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pixel_data_survives() {
        let doc = sample();
        let path = temp("pixels");
        save_document(&doc, &path).unwrap();
        let back = load_document(&path).unwrap();

        let order: Vec<_> = back.layers.iter_depth_first();
        let mut found = 0;
        for (id, _) in order {
            if let Some(LayerKind::Pixel { buffer }) = back.layers.get(id).map(|l| &l.kind) {
                if buffer.width() == 32 {
                    assert_eq!(buffer.get(5, 5).unwrap().to_u8(), [10, 20, 30, 255]);
                    found += 1;
                } else if buffer.width() == 8 {
                    assert_eq!(buffer.get(1, 1).unwrap().to_u8(), [200, 0, 0, 128]);
                    found += 1;
                }
            }
        }
        assert_eq!(found, 2, "both pixel layers should have come back");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn masks_survive() {
        let mut doc = sample();
        let base = doc.layers.roots()[1];
        if let Some(Mask::Bitmap { buffer, .. }) = &mut doc.layers.get_mut(base).unwrap().mask {
            buffer.set(3, 4, 77);
        }
        let path = temp("mask");
        save_document(&doc, &path).unwrap();
        let back = load_document(&path).unwrap();

        let mask = back
            .layers
            .iter_depth_first()
            .into_iter()
            .filter_map(|(id, _)| back.layers.get(id))
            .find_map(|l| l.mask.clone())
            .expect("mask should have survived");
        match mask {
            Mask::Bitmap { buffer, .. } => {
                assert_eq!(buffer.width(), 32);
                assert_eq!(buffer.get(3, 4), 77);
                assert_eq!(buffer.get(0, 0), 255);
            }
            other => panic!("expected a bitmap mask, got {other:?}"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn archive_starts_with_a_readable_mimetype() {
        let path = temp("mimetype");
        save_document(&sample(), &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        // Zip local header is 30 bytes plus the name; "mimetype" is 8 chars.
        let head = String::from_utf8_lossy(&bytes[..128]);
        assert!(head.contains("mimetype"), "mimetype should be the first entry");
        assert!(head.contains(PXM_MIMETYPE), "mimetype should be stored uncompressed");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_layers_do_not_write_a_png() {
        let mut doc = Document::empty(16, 16);
        doc.layers.insert("Empty", LayerKind::Pixel { buffer: PixelBuffer::new(0, 0) }, None);
        let path = temp("empty");
        save_document(&doc, &path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let zip = zip::ZipArchive::new(file).unwrap();
        assert!(
            !zip.file_names().any(|n| n.starts_with("layers/")),
            "an empty layer should not produce a PNG"
        );

        let back = load_document(&path).unwrap();
        assert_eq!(back.layers.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_newer_format_version_is_refused_clearly() {
        let path = temp("future");
        save_document(&sample(), &path).unwrap();

        // Rewrite the manifest claiming a far-future version.
        let file = std::fs::File::open(&path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut json = String::new();
        zip.by_name("document.json").unwrap().read_to_string(&mut json).unwrap();
        let bumped = json.replacen("\"version\": 1", "\"version\": 999", 1);

        let out = std::fs::File::create(&path).unwrap();
        let mut w = zip::ZipWriter::new(out);
        w.start_file("document.json", SimpleFileOptions::default()).unwrap();
        w.write_all(bumped.as_bytes()).unwrap();
        w.finish().unwrap();

        let err = load_document(&path).unwrap_err();
        assert!(
            err.to_string().contains("newer version"),
            "expected a clear version message, got: {err}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_non_pxm_zip_is_refused() {
        let path = temp("bogus");
        let out = std::fs::File::create(&path).unwrap();
        let mut w = zip::ZipWriter::new(out);
        w.start_file("hello.txt", SimpleFileOptions::default()).unwrap();
        w.write_all(b"not a document").unwrap();
        w.finish().unwrap();

        let err = load_document(&path).unwrap_err();
        assert!(err.to_string().contains("not a Pixelmagic document"));
        let _ = std::fs::remove_file(path);
    }
}
