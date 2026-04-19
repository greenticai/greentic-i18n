use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslatorState {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub langs: BTreeMap<String, LangState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LangState {
    #[serde(default)]
    pub keys: BTreeMap<String, KeyState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyState {
    pub last_english_hash: String,
    pub last_bot_translation_hash: String,
    pub engine: String,
    pub timestamp_epoch_secs: u64,
}

fn default_version() -> u32 {
    1
}

impl Default for TranslatorState {
    fn default() -> Self {
        Self {
            version: default_version(),
            langs: BTreeMap::new(),
        }
    }
}

pub fn hash_text(value: &str) -> String {
    blake3::hash(value.as_bytes()).to_hex().to_string()
}

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl TranslatorState {
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".i18n").join("translator-state.json")
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path)
            .map_err(|err| format!("failed reading state file {}: {err}", path.display()))?;
        serde_json::from_str(&raw)
            .map_err(|err| format!("failed parsing state file {}: {err}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed creating state directory {}: {err}",
                    parent.display()
                )
            })?;
        }
        let mut raw = serde_json::to_string_pretty(self)
            .map_err(|err| format!("failed serializing translator state: {err}"))?;
        raw.push('\n');
        fs::write(path, raw)
            .map_err(|err| format!("failed writing state file {}: {err}", path.display()))
    }

    pub fn key_state(&self, lang: &str, key: &str) -> Option<&KeyState> {
        self.langs
            .get(lang)
            .and_then(|lang_state| lang_state.keys.get(key))
    }

    pub fn set_key_state(
        &mut self,
        lang: &str,
        key: &str,
        last_english_hash: String,
        last_bot_translation_hash: String,
        engine: &str,
    ) {
        let lang_state = self.langs.entry(lang.to_string()).or_default();
        lang_state.keys.insert(
            key.to_string(),
            KeyState {
                last_english_hash,
                last_bot_translation_hash,
                engine: engine.to_string(),
                timestamp_epoch_secs: now_epoch_secs(),
            },
        );
    }

    pub fn backfill_missing_keys_from_maps(
        &mut self,
        lang: &str,
        en_map: &BTreeMap<String, String>,
        tr_map: &BTreeMap<String, String>,
        engine: &str,
    ) -> usize {
        let lang_state = self.langs.entry(lang.to_string()).or_default();
        let before = lang_state.keys.len();
        let timestamp_epoch_secs = now_epoch_secs();

        for (key, en_text) in en_map {
            if lang_state.keys.contains_key(key) {
                continue;
            }
            let Some(translated_text) = tr_map.get(key) else {
                continue;
            };
            lang_state.keys.insert(
                key.clone(),
                KeyState {
                    last_english_hash: hash_text(en_text),
                    last_bot_translation_hash: hash_text(translated_text),
                    engine: engine.to_string(),
                    timestamp_epoch_secs,
                },
            );
        }

        lang_state.keys.len().saturating_sub(before)
    }
}

#[cfg(test)]
mod tests {
    use super::{TranslatorState, hash_text};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("gt-i18n-state-{name}-{stamp}"))
    }

    #[test]
    fn default_path_points_to_repo_i18n_state_file() {
        assert_eq!(
            TranslatorState::default_path(Path::new("/repo")),
            Path::new("/repo/.i18n/translator-state.json")
        );
    }

    #[test]
    fn load_missing_file_returns_default_state() {
        let path = unique_temp_dir("missing").join("missing.json");
        let state = TranslatorState::load(&path).expect("missing file should yield default state");
        assert_eq!(state.version, 1);
        assert!(state.langs.is_empty());
    }

    #[test]
    fn save_and_load_round_trip_state() {
        let dir = unique_temp_dir("roundtrip");
        let path = dir.join("translator-state.json");
        let mut state = TranslatorState::default();
        state.set_key_state(
            "fr",
            "hello",
            hash_text("Hello"),
            hash_text("Bonjour"),
            "codex-cli",
        );

        state.save(&path).expect("state should save");
        let loaded = TranslatorState::load(&path).expect("state should load");
        let key_state = loaded
            .key_state("fr", "hello")
            .expect("saved key state should exist");
        assert_eq!(key_state.engine, "codex-cli");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn backfill_only_adds_keys_that_exist_in_translation_map() {
        let mut state = TranslatorState::default();
        let mut en_map = BTreeMap::new();
        en_map.insert("hello".to_string(), "Hello".to_string());
        en_map.insert("bye".to_string(), "Bye".to_string());
        let mut tr_map = BTreeMap::new();
        tr_map.insert("hello".to_string(), "Bonjour".to_string());

        let added = state.backfill_missing_keys_from_maps("fr", &en_map, &tr_map, "codex-cli");
        assert_eq!(added, 1);
        assert!(state.key_state("fr", "hello").is_some());
        assert!(state.key_state("fr", "bye").is_none());
    }
}
