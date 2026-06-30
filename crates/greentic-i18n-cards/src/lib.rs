#![forbid(unsafe_code)]
//! Adaptive Card i18n primitives: extract translatable strings from cards and
//! drive `greentic-i18n-translator` to produce per-locale bundles. Each
//! consumer keeps its own high-level orchestration on top of these primitives.

mod bundle;
mod extract;
mod translate;

pub use bundle::{ExtractConfig, extract_from_directory, to_json_bundle, write_bundle};
pub use extract::{ExtractedString, extract_from_value};
pub use translate::{
    TRANSLATOR_BIN_ENV, TRANSLATOR_DEV_BIN_ENV, is_translator_available, resolve_translator,
    translate_to_language, write_manifest,
};
