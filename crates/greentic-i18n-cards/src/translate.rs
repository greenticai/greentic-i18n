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
