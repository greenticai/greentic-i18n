use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
