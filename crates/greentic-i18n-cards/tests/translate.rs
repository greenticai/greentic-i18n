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
    write_exec(
        &p,
        "#!/bin/sh\nlang=\"\"; en=\"\"\nwhile [ $# -gt 0 ]; do case \"$1\" in --langs) lang=\"$2\"; shift 2;; --en) en=\"$2\"; shift 2;; *) shift;; esac; done\ncp \"$en\" \"$(dirname \"$en\")/$lang.json\"\n",
    );
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
    unsafe {
        std::env::set_var("GREENTIC_I18N_TRANSLATOR_BIN", &stub);
    }
    let resolved = resolve_translator();
    let available = is_translator_available();
    unsafe {
        std::env::remove_var("GREENTIC_I18N_TRANSLATOR_BIN");
    }
    assert_eq!(resolved.as_deref(), Some(stub.as_path()));
    assert!(available);
}

#[test]
fn write_manifest_is_sorted_deduped_and_includes_en() {
    let tmp = tempfile::tempdir().unwrap();
    write_manifest(
        tmp.path(),
        &["ja".to_string(), "id".to_string(), "id".to_string()],
    )
    .unwrap();
    let got: Vec<String> =
        serde_json::from_str(&fs::read_to_string(tmp.path().join("_manifest.json")).unwrap())
            .unwrap();
    assert_eq!(
        got,
        vec!["en".to_string(), "id".to_string(), "ja".to_string()]
    );
}
