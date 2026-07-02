//! Shared registry for supported table formats.

mod csv;
mod detect;
mod format;
mod infer;
mod json;
mod json_value;
mod jsonl;
mod md;
mod nose;
mod parse;
mod registry;
mod sqlite;
mod xlsx;
mod yml;

pub(crate) use detect::{detect_by_path, detect_by_sample};
pub use format::Format;
pub(crate) use registry::find;
