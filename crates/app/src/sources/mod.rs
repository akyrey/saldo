//! Companion sources: Amazon and PayPal exports whose rows explain bank
//! lines (Phase 13, Tracks C and D).

pub mod parse;
pub(crate) mod repo;
mod service;
mod views;

pub use service::SourceService;
pub use views::{DocumentView, RowFilter, SourceRowView, SourceUpload};
