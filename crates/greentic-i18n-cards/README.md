# greentic-i18n-cards

Adaptive Card i18n primitives for the Greentic platform: extract translatable
strings from Adaptive Card JSON and drive `greentic-i18n-translator` to produce
per-locale bundles plus a `_manifest.json`.

This crate provides **primitives only** — string extraction, single-language
translation, and manifest writing. High-level orchestration (parallelism,
glossaries, result reporting, auto-install) lives in the consuming tools
(`greentic-pack`, `greentic-cards2pack`).
