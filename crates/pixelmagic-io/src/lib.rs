//! # pixelmagic-io
//!
//! Reading and writing images, and the native `.pxm` document container.
//!
//! ## On `.pxd`
//!
//! Pixelmator Pro's own format is undocumented (see `docs/SPEC.md` §6.5), and
//! reading it would mean reverse-engineering a proprietary container. That is
//! legitimate work for interoperability, but it is also open-ended, and getting
//! it half-right is worse than not doing it — a reader that silently drops
//! adjustments produces files that look fine and are not. So `.pxm` is a new,
//! documented format, and `.pxd` support is a separate project that should
//! start from real sample files.

pub mod container;
pub mod image_io;

pub use container::{load_document, save_document, PXM_EXTENSION};
pub use image_io::{
    decode_image, encode_image, export_document, load_image, save_image, ExportFormat,
    ExportOptions,
};

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("document format error: {0}")]
    Format(String),
    #[error("unsupported file type: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, IoError>;

/// Extensions Pixelmagic can open, for the file chooser's filter.
///
/// This is what we can actually decode today, not the aspirational list —
/// offering a format in the picker and then failing to open it is worse than
/// not offering it.
pub const OPENABLE_EXTENSIONS: &[&str] =
    &["pxm", "png", "jpg", "jpeg", "tif", "tiff", "webp", "bmp", "gif"];

pub fn is_openable(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| OPENABLE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn extension_matching_is_case_insensitive() {
        assert!(is_openable(Path::new("a.PNG")));
        assert!(is_openable(Path::new("a.jpeg")));
        assert!(is_openable(Path::new("/x/y/z.pxm")));
        assert!(!is_openable(Path::new("a.psd")));
        assert!(!is_openable(Path::new("noextension")));
    }
}
