# greentic-i18n-cards shared crate — design

- **Date:** 2026-06-20
- **Status:** Approved (design)
- **Primary repo:** `greentic-i18n` (new crate `crates/greentic-i18n-cards`)
- **Consumer repos:** `greentic-pack`, `greentic-cards2pack`

## Problem

The Adaptive-Card i18n authoring logic — extract translatable strings from card
JSON, then drive the `greentic-i18n-translator` binary per language to produce
locale bundles — exists as two near-identical copies:

- `greentic-cards2pack/src/i18n_extract/` (extractor.rs ~438, mod.rs ~240,
  report.rs ~37) + `src/translate.rs` (~473) — the original.
- `greentic-pack/crates/packc/src/i18n_build/extract.rs` + `bundle.rs` — a
  faithful port added by the pack-i18n-build feature (greentic-pack#179),
  deliberately copied to avoid a heavy cross-crate dependency.

Two copies of ~700 lines of extractor + the translator-invocation drift apart
over time. This is tracked as a follow-up to greentic-pack#179.

## Decision

Extract the shared **primitives** into a new published crate
`greentic-i18n-cards` in the `greentic-i18n` workspace. Both consumers depend on
it and delete their copies. Each consumer keeps its own thin, behaviour-specific
orchestration on top of the shared primitives — the crate does NOT impose a
single high-level pipeline.

### Why primitives-only (not a shared `run_auto_translate`)

The extractor and the single-language translator invocation are byte-identical
across both copies and are the bulk of the duplication. The high-level
orchestration genuinely differs and should stay per-consumer:

- **greentic-pack** `materialize_i18n`: non-fatal (never errors), pack-dir
  oriented, reports to stderr, carry-over of pre-existing locale files, NO
  auto-install.
- **cards2pack** `run_auto_translate`: returns a `TranslateResult`, supports a
  glossary and `merge_en_sources`, parallel chunked translation, auto-installs
  the translator via `cargo binstall`, emits an extraction report.

Forcing these to converge would change behaviour and add risk for no benefit.
Primitives-only dedups the shared mass while leaving each orchestration intact.

## The crate: `greentic-i18n-cards`

- Location: `greentic-i18n/crates/greentic-i18n-cards`.
- `version.workspace = true` (currently `1.1.0-dev.0` on develop), edition 2024,
  `#![forbid(unsafe_code)]`, published to crates.io alongside `greentic-i18n-lib`
  and `greentic-i18n-translator`.
- Dependencies: `serde_json`, `walkdir`, `anyhow` (and `tempfile` for the
  per-language work dir used by `translate_to_language`). No dependency on
  greentic-pack or cards2pack.

### Public API (primitives)

Extraction module (canonical source = cards2pack `i18n_extract`):

```rust
pub struct ExtractedString { pub key: String, pub value: String, pub source_file: PathBuf, pub json_path: String }
pub struct ExtractConfig { pub cards_dir: PathBuf, pub output: PathBuf, pub prefix: String, pub skip_i18n_patterns: bool }

pub fn extract_from_value(value: &serde_json::Value, prefix: &str, path: &str, source_file: &Path, skip_i18n_patterns: bool) -> Vec<ExtractedString>;
pub fn extract_from_directory(config: &ExtractConfig) -> anyhow::Result<Vec<ExtractedString>>;
pub fn to_json_bundle(strings: &[ExtractedString]) -> serde_json::Value;
pub fn write_bundle(strings: &[ExtractedString], output: &Path) -> anyhow::Result<()>;
```

Translator module (canonical source = cards2pack `translate.rs`, primitive parts):

```rust
/// Locate the translator binary: env override (GREENTIC_I18N_TRANSLATOR_BIN /
/// GREENTIC_I18N_TRANSLATOR_DEV_BIN) then PATH. Returns the resolved path.
pub fn resolve_translator() -> Option<PathBuf>;
pub fn is_translator_available() -> bool;

/// Translate one language: run `<translator> translate --langs <lang> --en <abs en>
/// [--glossary <abs>] --auth-mode auto` in a unique per-language temp cwd
/// (`tempfile::tempdir()`, `git init --quiet`). The translator writes
/// `<lang>.json` next to `--en`. Returns Err on non-zero exit / spawn failure.
pub fn translate_to_language(translator: &Path, lang: &str, en_bundle: &Path, glossary: Option<&Path>) -> anyhow::Result<()>;

/// Write `_manifest.json` (sorted, deduped JSON array of locale codes,
/// always including "en") into `i18n_dir`.
pub fn write_manifest(i18n_dir: &Path, locales: &[String]) -> anyhow::Result<()>;
```

Notes:
- `translate_to_language` gains an explicit `glossary: Option<&Path>` so
  cards2pack can pass its glossary and greentic-pack passes `None`.
- The env-var names are owned by this crate and re-exported as `pub const`
  (`TRANSLATOR_BIN_ENV`, `TRANSLATOR_DEV_BIN_ENV`) so consumers reference one
  source of truth.
- NOT in the crate: `run_auto_translate`, parallel chunking, glossary
  orchestration, `merge_en_sources`, auto-install (`cargo binstall`), extraction
  report. Those remain in cards2pack.

### Tests (in the crate)

- All ported extractor unit tests (field sets, key format, `$t()`/`{{}}` skips,
  factset, choiceset, nested columns, show-card, sanitisation, bundle IO) — the
  16 tests already shared by both copies.
- `write_manifest`: sorted + deduped + always-`en` output.
- `translate_to_language` + `is_translator_available`: a stub translator script
  (unix, `#![cfg(unix)]`) that copies `en.json` → `<lang>.json`, plus a failing
  stub (exits 1) asserting an `Err` and no output file. Deterministic regardless
  of PATH (point `GREENTIC_I18N_TRANSLATOR_BIN` at an existing stub).

## Consumer changes

### cards2pack

- Delete `src/i18n_extract/extractor.rs` and the extraction/types parts of
  `src/i18n_extract/mod.rs`; delete the extraction + `translate_to_language`
  bodies in `src/translate.rs`.
- Add `greentic-i18n-cards` dependency. Re-point `src/workspace.rs` and
  `src/translate.rs` to the crate's `extract_from_directory`, `write_bundle`,
  `resolve_translator`/`is_translator_available`, `translate_to_language`
  (passing the existing glossary), and `write_manifest`.
- KEEP cards2pack's `run_auto_translate` orchestration (parallel chunking,
  `merge_en_sources`, `TranslateResult`, `ensure_translator_available`
  auto-install, `format_translation_summary`). KEEP `i18n_extract/report.rs`
  (`extract-i18n` CLI report) — consumer-specific.
- `extract-i18n` CLI command keeps working via the crate's extractor.

### greentic-pack (lands AFTER greentic-pack#179 merges)

- Delete `crates/packc/src/i18n_build/extract.rs` and `bundle.rs`.
- Add `greentic-i18n-cards` dependency. `i18n_build/mod.rs` keeps
  `materialize_i18n` (non-fatal, pack-dir, stderr, carry-over) but its internals
  call the crate: `extract_from_directory` + `write_bundle` for en.json,
  `resolve_translator` (its own "absent → warn + skip" branch),
  `translate_to_language(.., None)` per lang, `write_manifest`. Remove the
  `"greentic-i18n-translator"` arm from `crates/packc/src/external_tools.rs`
  (it was added by greentic-pack#179 solely for i18n; the crate now owns
  translator resolution, so the arm is dead). Leave the `greentic-flow` /
  `greentic-component` arms untouched.
- greentic-pack's existing i18n integration tests
  (`i18n_build_materialize`, `i18n_build_pipeline`) stay and must remain green —
  they are the regression net proving the swap is behaviour-preserving.

## Sequencing (operational crux)

Three phases across three repos; each is independently testable:

1. **greentic-i18n** — create + test `greentic-i18n-cards`; merge to develop;
   it publishes to crates.io via the workspace's normal release flow.
2. **cards2pack** — depend on `greentic-i18n-cards`, delete duplicates, rewire,
   tests green.
3. **greentic-pack** — depend on `greentic-i18n-cards`, delete
   `extract.rs`/`bundle.rs`, rewire `materialize_i18n`, tests green. Sequenced
   after greentic-pack#179.

During development, consumers depend on the new crate via a path or git
dependency (or `[patch.crates-io]`); the crates.io cutover follows
greentic-i18n's publish. Each consumer's existing test suite is the acceptance
gate for its phase — behaviour must be identical before/after.

## Out of scope

- Converging the two orchestrations (`run_auto_translate` vs `materialize_i18n`)
  into one — deliberately kept separate (see "Why primitives-only").
- Moving cards2pack's extraction report into the crate.
- Any change to `greentic-i18n-lib` (locale-tag/format runtime — unrelated).

## Risks

- **Behaviour drift on extraction:** mitigated by porting the full unit-test set
  into the crate and keeping each consumer's tests as the regression gate.
- **Publish/version coordination:** the crate must be published before the
  crates.io cutover; dev proceeds on path/git dep. Phase 1 must land + publish
  before phases 2-3 can finalise their `Cargo.toml` to a crates.io version.
- **greentic-pack#179 ordering:** phase 3 deletes code that #179 just added;
  doing it before #179 merges would create churn — explicitly sequenced after.
