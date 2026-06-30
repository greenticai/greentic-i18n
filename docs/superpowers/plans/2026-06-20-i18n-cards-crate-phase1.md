# greentic-i18n-cards crate (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the published `greentic-i18n-cards` crate holding the shared Adaptive-Card i18n primitives (string extraction + single-language translator invocation + manifest writing), so `greentic-pack` and `greentic-cards2pack` can later drop their duplicated copies.

**Architecture:** A new library crate in the `greentic-i18n` workspace with three modules: `extract` (recursive AC string extractor), `bundle` (directory scan + en.json/bundle IO + card-id), `translate` (translator-binary resolution + per-language invocation + `_manifest.json`). The `extract`/`bundle` modules are copied verbatim from greentic-pack's already-clean, already-tested `i18n_build` module; `translate` is the primitive subset plus an optional glossary parameter. No high-level orchestration (that stays in each consumer).

**Tech Stack:** Rust 2024 (workspace edition), `serde_json`, `walkdir`, `anyhow`, `tempfile`. External binary `greentic-i18n-translator` (invoked, not depended on).

## Global Constraints

- Workspace: `greentic-i18n`, on branch `feat/i18n-cards-crate` (worktree off `origin/develop`). Workspace version `1.1.0-dev.0`; new crate uses `version.workspace = true`, `edition.workspace = true`.
- `#![forbid(unsafe_code)]` as line 1 of every `src/*.rs` file in the new crate.
- The crate is **primitives-only**: NO `run_auto_translate`, parallel chunking, glossary orchestration, `merge_en_sources`, auto-install (`cargo binstall`), or extraction report. Those stay in cards2pack.
- Translator env var names owned here: `GREENTIC_I18N_TRANSLATOR_BIN`, `GREENTIC_I18N_TRANSLATOR_DEV_BIN`.
- Translator invocation contract: `<translator> translate --langs <lang> --en <abs en.json> [--glossary <abs>] --auth-mode auto`, run in a unique per-language temp cwd (`tempfile::tempdir()`, `git init --quiet`); the translator writes `<lang>.json` next to `--en`.
- Manifest: a sorted, deduped JSON array of locale codes, always including `"en"`.
- env-mutating / unix-shell-stub tests live in integration test files (`tests/*.rs`, gated `#![cfg(unix)]`), never inline under `#![forbid(unsafe_code)]`.
- Source of truth for `extract`/`bundle`: the greentic-pack worktree at `/Users/bimapangestu/Desktop/Works/personal/greentic/.worktrees-greentic-pack/pack-i18n-build/crates/packc/src/i18n_build/{extract.rs,bundle.rs}` — copy verbatim.
- Conventional commits; NO Claude/AI co-author attribution.

---

### Task 1: Scaffold the crate with the extraction modules

**Files:**
- Create: `crates/greentic-i18n-cards/Cargo.toml`
- Create: `crates/greentic-i18n-cards/src/lib.rs`
- Create: `crates/greentic-i18n-cards/src/extract.rs`
- Create: `crates/greentic-i18n-cards/src/bundle.rs`
- Modify: workspace root `Cargo.toml` (`members` + `[workspace.dependencies]`)

**Interfaces:**
- Produces (re-exported from crate root): `ExtractedString { key, value, source_file, json_path }`, `ExtractConfig { cards_dir, output, prefix, skip_i18n_patterns }`, `extract_from_value(...) -> Vec<ExtractedString>`, `extract_from_directory(&ExtractConfig) -> anyhow::Result<Vec<ExtractedString>>`, `to_json_bundle(&[ExtractedString]) -> serde_json::Value`, `write_bundle(&[ExtractedString], &Path) -> anyhow::Result<()>`.

- [ ] **Step 1: Add workspace deps + member**

In the root `Cargo.toml`, add to `members` (keep sorted-ish):
```toml
    "crates/greentic-i18n-cards",
```
In `[workspace.dependencies]`, add (next to the existing `serde_json = "1"`):
```toml
walkdir = "2"
anyhow = "1"
tempfile = "3"
```

- [ ] **Step 2: Create `crates/greentic-i18n-cards/Cargo.toml`**

```toml
[package]
name = "greentic-i18n-cards"
version.workspace = true
edition.workspace = true
license = "MIT"
description = "Adaptive Card i18n primitives for Greentic: extract translatable strings and drive greentic-i18n-translator to produce locale bundles."
repository = "https://github.com/greentic-ng/greentic-i18n"
homepage = "https://github.com/greentic-ng/greentic-i18n"
documentation = "https://docs.rs/greentic-i18n-cards"
keywords = ["i18n", "localization", "greentic", "adaptive-cards"]
readme = "README.md"

[dependencies]
serde_json = { workspace = true }
walkdir = { workspace = true }
anyhow = { workspace = true }
tempfile = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 3: Create the extraction modules by copying greentic-pack's clean versions**

Copy these two files VERBATIM (they already begin with `#![forbid(unsafe_code)]`, use Rust 2024 let-chains, and carry their full unit-test modules):
- `…/pack-i18n-build/crates/packc/src/i18n_build/extract.rs` → `crates/greentic-i18n-cards/src/extract.rs`
- `…/pack-i18n-build/crates/packc/src/i18n_build/bundle.rs` → `crates/greentic-i18n-cards/src/bundle.rs`

(Source paths are under `/Users/bimapangestu/Desktop/Works/personal/greentic/.worktrees-greentic-pack/pack-i18n-build/`.) Change nothing — `bundle.rs` already does `use super::extract::{ExtractedString, extract_from_value};` which resolves the same way here.

- [ ] **Step 4: Create `crates/greentic-i18n-cards/src/lib.rs`**

```rust
#![forbid(unsafe_code)]
//! Adaptive Card i18n primitives: extract translatable strings from cards and
//! drive `greentic-i18n-translator` to produce per-locale bundles. Each
//! consumer keeps its own high-level orchestration on top of these primitives.

mod bundle;
mod extract;

pub use bundle::{ExtractConfig, extract_from_directory, to_json_bundle, write_bundle};
pub use extract::{ExtractedString, extract_from_value};
```

- [ ] **Step 5: Add a minimal README (needed for crates.io publish)**

Create `crates/greentic-i18n-cards/README.md`:
```markdown
# greentic-i18n-cards

Adaptive Card i18n primitives for the Greentic platform: extract translatable
strings from Adaptive Card JSON and drive `greentic-i18n-translator` to produce
per-locale bundles plus a `_manifest.json`.

This crate provides **primitives only** — string extraction, single-language
translation, and manifest writing. High-level orchestration (parallelism,
glossaries, result reporting, auto-install) lives in the consuming tools
(`greentic-pack`, `greentic-cards2pack`).
```

- [ ] **Step 6: Build + run the ported extractor tests**

Run: `cargo test -p greentic-i18n-cards --locked`
Expected: PASS — all copied extractor + bundle tests (e.g. `extract::tests::test_extract_from_simple_card`, `bundle::tests::test_write_bundle_creates_parent_dirs`).

- [ ] **Step 7: Lint**

Run: `cargo clippy -p greentic-i18n-cards --all-targets --locked -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/greentic-i18n-cards/
git commit -m "feat(i18n-cards): scaffold crate with adaptive-card string extractor"
```

---

### Task 2: Translator primitives module

**Files:**
- Create: `crates/greentic-i18n-cards/src/translate.rs`
- Modify: `crates/greentic-i18n-cards/src/lib.rs` (add `mod translate;` + re-exports)
- Test: `crates/greentic-i18n-cards/tests/translate.rs` (integration, `#![cfg(unix)]`)

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces (re-exported from crate root):
  - `pub const TRANSLATOR_BIN_ENV: &str` / `pub const TRANSLATOR_DEV_BIN_ENV: &str`
  - `resolve_translator() -> Option<PathBuf>`
  - `is_translator_available() -> bool`
  - `translate_to_language(translator: &Path, lang: &str, en_bundle: &Path, glossary: Option<&Path>) -> anyhow::Result<()>`
  - `write_manifest(i18n_dir: &Path, locales: &[String]) -> anyhow::Result<()>`

- [ ] **Step 1: Write the failing integration test**

Create `crates/greentic-i18n-cards/tests/translate.rs`:
```rust
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use greentic_i18n_cards::{
    is_translator_available, resolve_translator, translate_to_language, write_manifest,
};

fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perms = fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).unwrap();
}

// Stub translator: copies the --en bundle to <lang>.json next to it.
fn ok_stub(dir: &Path) -> std::path::PathBuf {
    let p = dir.join("ok-translator.sh");
    write_exec(&p, "#!/bin/sh\nlang=\"\"; en=\"\"\nwhile [ $# -gt 0 ]; do case \"$1\" in --langs) lang=\"$2\"; shift 2;; --en) en=\"$2\"; shift 2;; *) shift;; esac; done\ncp \"$en\" \"$(dirname \"$en\")/$lang.json\"\n");
    p
}

fn fail_stub(dir: &Path) -> std::path::PathBuf {
    let p = dir.join("fail-translator.sh");
    write_exec(&p, "#!/bin/sh\nexit 1\n");
    p
}

#[test]
fn translate_to_language_produces_locale_file() {
    let tmp = tempfile::tempdir().unwrap();
    let i18n = tmp.path().join("i18n");
    fs::create_dir_all(&i18n).unwrap();
    let en = i18n.join("en.json");
    fs::write(&en, r#"{"card.text":"Hello"}"#).unwrap();
    let stub = ok_stub(tmp.path());

    translate_to_language(&stub, "id", &en, None).unwrap();
    assert!(i18n.join("id.json").is_file());
}

#[test]
fn translate_to_language_errors_on_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let i18n = tmp.path().join("i18n");
    fs::create_dir_all(&i18n).unwrap();
    let en = i18n.join("en.json");
    fs::write(&en, r#"{"card.text":"Hello"}"#).unwrap();
    let stub = fail_stub(tmp.path());

    assert!(translate_to_language(&stub, "id", &en, None).is_err());
    assert!(!i18n.join("id.json").exists());
}

#[test]
fn resolve_and_available_honour_env_override() {
    let tmp = tempfile::tempdir().unwrap();
    let stub = ok_stub(tmp.path());
    // SAFETY: integration test; var removed before assertions.
    unsafe { std::env::set_var("GREENTIC_I18N_TRANSLATOR_BIN", &stub); }
    let resolved = resolve_translator();
    let available = is_translator_available();
    unsafe { std::env::remove_var("GREENTIC_I18N_TRANSLATOR_BIN"); }
    assert_eq!(resolved.as_deref(), Some(stub.as_path()));
    assert!(available);
}

#[test]
fn write_manifest_is_sorted_deduped_and_includes_en() {
    let tmp = tempfile::tempdir().unwrap();
    write_manifest(tmp.path(), &["ja".to_string(), "id".to_string(), "id".to_string()]).unwrap();
    let got: Vec<String> =
        serde_json::from_str(&fs::read_to_string(tmp.path().join("_manifest.json")).unwrap()).unwrap();
    assert_eq!(got, vec!["en".to_string(), "id".to_string(), "ja".to_string()]);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p greentic-i18n-cards --locked --test translate -- --nocapture`
Expected: FAIL — `greentic_i18n_cards::{resolve_translator,…}` are unresolved (module not created yet).

- [ ] **Step 3: Create `crates/greentic-i18n-cards/src/translate.rs`**

```rust
#![forbid(unsafe_code)]
//! Translator-binary resolution and single-language invocation primitives.

use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Explicit path override for the translator binary.
pub const TRANSLATOR_BIN_ENV: &str = "GREENTIC_I18N_TRANSLATOR_BIN";
/// Dev override for the translator binary.
pub const TRANSLATOR_DEV_BIN_ENV: &str = "GREENTIC_I18N_TRANSLATOR_DEV_BIN";

const TRANSLATOR_DEFAULT_BIN: &str = "greentic-i18n-translator";

/// Locate the translator: env override (`GREENTIC_I18N_TRANSLATOR_BIN` then
/// `_DEV_BIN`) if the pointed file exists, otherwise search `PATH`.
pub fn resolve_translator() -> Option<PathBuf> {
    for key in [TRANSLATOR_BIN_ENV, TRANSLATOR_DEV_BIN_ENV] {
        if let Some(value) = env::var_os(key) {
            let candidate = PathBuf::from(value);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(TRANSLATOR_DEFAULT_BIN);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Whether a translator binary can be located.
pub fn is_translator_available() -> bool {
    resolve_translator().is_some()
}

/// Translate one language. Runs
/// `<translator> translate --langs <lang> --en <abs en> [--glossary <abs>] --auth-mode auto`
/// in a unique temp cwd (`git init` so codex-cli trusts it). The translator
/// writes `<lang>.json` next to `--en`. Returns `Err` on spawn failure or
/// non-zero exit.
pub fn translate_to_language(
    translator: &Path,
    lang: &str,
    en_bundle: &Path,
    glossary: Option<&Path>,
) -> Result<()> {
    let work_dir = tempfile::tempdir().context("create translator work dir")?;
    let work_path = work_dir.path();
    let _ = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .current_dir(work_path)
        .output();

    let en_abs = std::fs::canonicalize(en_bundle).unwrap_or_else(|_| en_bundle.to_path_buf());
    let mut cmd = Command::new(translator);
    cmd.current_dir(work_path)
        .arg("translate")
        .arg("--langs")
        .arg(lang)
        .arg("--en")
        .arg(&en_abs);
    if let Some(glossary) = glossary {
        let glossary_abs =
            std::fs::canonicalize(glossary).unwrap_or_else(|_| glossary.to_path_buf());
        cmd.arg("--glossary").arg(glossary_abs);
    }
    cmd.arg("--auth-mode").arg("auto");

    let output = cmd
        .output()
        .context("failed to execute greentic-i18n-translator")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("translator exited non-zero for {lang}: {}", stderr.trim_end());
    }
    Ok(())
}

/// Write `_manifest.json` (sorted, deduped locale array, always incl. `"en"`).
pub fn write_manifest(i18n_dir: &Path, locales: &[String]) -> Result<()> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    set.insert("en".to_string());
    for code in locales {
        set.insert(code.clone());
    }
    let codes: Vec<&String> = set.iter().collect();
    let json = serde_json::to_string_pretty(&codes).context("serialise i18n manifest")?;
    std::fs::write(i18n_dir.join("_manifest.json"), json).context("write _manifest.json")?;
    Ok(())
}
```

- [ ] **Step 4: Wire the module into `lib.rs`**

Edit `crates/greentic-i18n-cards/src/lib.rs` to add the module and re-exports:
```rust
mod bundle;
mod extract;
mod translate;

pub use bundle::{ExtractConfig, extract_from_directory, to_json_bundle, write_bundle};
pub use extract::{ExtractedString, extract_from_value};
pub use translate::{
    TRANSLATOR_BIN_ENV, TRANSLATOR_DEV_BIN_ENV, is_translator_available, resolve_translator,
    translate_to_language, write_manifest,
};
```

- [ ] **Step 5: Run the integration test to verify it passes**

Run: `cargo test -p greentic-i18n-cards --locked --test translate -- --nocapture`
Expected: PASS — all four tests (`translate_to_language_produces_locale_file`, `translate_to_language_errors_on_failure`, `resolve_and_available_honour_env_override`, `write_manifest_is_sorted_deduped_and_includes_en`).

- [ ] **Step 6: Full crate test + lint**

Run: `cargo test -p greentic-i18n-cards --locked && cargo clippy -p greentic-i18n-cards --all-targets --locked -- -D warnings`
Expected: all tests pass; no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/greentic-i18n-cards/src/translate.rs crates/greentic-i18n-cards/src/lib.rs crates/greentic-i18n-cards/tests/translate.rs
git commit -m "feat(i18n-cards): translator resolution, single-language translate, manifest"
```

---

### Task 3: Publish-readiness gate

**Files:** none (verification + metadata only).

- [ ] **Step 1: Confirm the crate packages cleanly for crates.io**

Run: `cargo publish -p greentic-i18n-cards --dry-run --allow-dirty`
Expected: packages without error — all required metadata (`description`, `license`, `repository`, `readme`) present; no missing-file errors. If it complains the README path is wrong, fix the `readme` field in `Cargo.toml` to `"README.md"` (the file created in Task 1) and re-run.

- [ ] **Step 2: Confirm the workspace still builds as a whole**

Run: `cargo build --workspace --locked`
Expected: success — the new member does not break the workspace.

- [ ] **Step 3: Commit any metadata fix (only if Step 1 required one)**

```bash
git add crates/greentic-i18n-cards/Cargo.toml
git commit -m "chore(i18n-cards): fix crate metadata for crates.io publish"
```
(Skip this commit if Step 1 passed with no change.)

---

## Follow-up plans (NOT in this plan — gated on this crate landing + publishing)

- **Phase 2 — cards2pack adoption:** depend on `greentic-i18n-cards`, delete `src/i18n_extract/{extractor.rs, mod.rs extraction parts}` + the extraction/`translate_to_language` bodies in `src/translate.rs`, rewire `run_auto_translate`/`workspace.rs` onto the crate's primitives (passing the existing glossary), keep `run_auto_translate` + `report.rs` + auto-install. Acceptance gate: cards2pack's existing suite green.
- **Phase 3 — greentic-pack adoption (after greentic-pack#179 merges):** depend on `greentic-i18n-cards`, delete `crates/packc/src/i18n_build/{extract.rs, bundle.rs}`, rewire `materialize_i18n` onto the crate, remove the `greentic-i18n-translator` arm from `external_tools.rs`. Acceptance gate: `i18n_build_materialize` + `i18n_build_pipeline` stay green.

These are written as separate plans once `greentic-i18n-cards` is on develop and a publishable/path-dep version is available.

## Self-Review

**Spec coverage (Phase 1 scope):**
- New crate in greentic-i18n workspace, version.workspace, publish metadata → Task 1 + Task 3. ✓
- Primitives-only API (extract + translate-one + manifest + resolution), no orchestration → Tasks 1-2; orchestration explicitly excluded. ✓
- `translate_to_language` gains `glossary: Option<&Path>` → Task 2 Step 3. ✓
- env var names owned + re-exported as consts → Task 2 Step 3/4. ✓
- Manifest sorted/deduped/always-en, returns Result → Task 2 Step 3 (`write_manifest`). ✓
- forbid(unsafe_code) every src file; env/unix-stub tests in `tests/` `#![cfg(unix)]` → Tasks 1-2. ✓
- extract/bundle copied verbatim from greentic-pack clean source → Task 1 Step 3. ✓
- Consumer adoption (phases 2-3) → explicitly deferred to follow-up plans. ✓ (spec sequencing honoured)

**Placeholder scan:** none — every code step shows full content; Task 1 Step 3 names exact source files to copy.

**Type consistency:** `resolve_translator() -> Option<PathBuf>`, `translate_to_language(&Path,&str,&Path,Option<&Path>) -> Result<()>`, `write_manifest(&Path,&[String]) -> Result<()>` are identical in the Interfaces blocks, the implementation (Task 2 Step 3), the re-exports (Step 4), and the tests (Step 1). `ExtractConfig`/`ExtractedString`/`extract_from_directory`/`write_bundle` match the greentic-pack source being copied.
