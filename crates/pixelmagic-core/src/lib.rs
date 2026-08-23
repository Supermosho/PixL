//! # pixelmagic-core
//!
//! The document model: layers, adjustments, effects, masks, selections,
//! history. Everything here is pure data and pure functions — no GTK, no
//! OpenGL, no I/O. That boundary is deliberate: it keeps the model testable
//! without a display server, and it means the renderer and the UI can be
//! replaced independently.
//!
//! ## Orientation
//!
//! - [`document`] — the top-level [`Document`], canvas, and layer tree
//! - [`layer`] — [`Layer`] and the layer kinds
//! - [`adjust`] / [`effect`] — non-destructive image operations
//! - [`blend`] — the 26 blend modes
//! - [`style`] — fill, stroke and shadow layer styles
//! - [`mask`] / [`selection`] — masking and selection state
//! - [`history`] — undo/redo
//! - [`tool`] — the tool roster and per-tool options
//! - [`param`] — the reflection layer the UI builds panels from

pub mod adjust;
pub mod blend;
pub mod buffer;
pub mod color;
pub mod curve;
pub mod document;
pub mod effect;
pub mod geom;
pub mod history;
pub mod layer;
pub mod macros;
pub mod param;
pub mod selection;
pub mod style;
pub mod text;
pub mod tool;
pub mod vector;

pub use blend::{BlendGroup, BlendMode};
pub use color::{BitDepth, ColorSpace, Rgba};
pub use curve::{Curve, CurvePoint};
pub use document::Document;
pub use geom::{Rect, Size, Transform};
pub use layer::{Layer, LayerId, LayerKind};
pub use param::{ParamKind, ParamSpec, ParamValue, Parameterized};

/// Errors raised by the document model.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("no layer with id {0:?}")]
    NoSuchLayer(LayerId),
    #[error("cannot move a layer into itself or one of its descendants")]
    CyclicReparent,
    #[error("layer is locked")]
    LayerLocked,
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
