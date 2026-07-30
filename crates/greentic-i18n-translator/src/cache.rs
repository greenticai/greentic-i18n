use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const CACHE_FILE_SUFFIX: &str = ".json";

#[derive(Debug, Clone)]
pub struct CacheStore {
    dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    translation: String,
}

impl CacheStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn default_dir() -> PathBuf {
        #[cfg(target_os = "windows")]
        {
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                return Path::new(&local).join("greentic").join("i18n-translator");
            }
        }
        #[cfg(target_os = "macos")]
        {
            if let Ok(home) = std::env::var("HOME") {
                return Path::new(&home)
                    .join("Library")
                    .join("Caches")
                    .join("greentic")
                    .join("i18n-translator");
            }
        }

        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            return Path::new(&xdg).join("greentic").join("i18n-translator");
        }
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home)
                .join(".cache")
                .join("greentic")
                .join("i18n-translator");
        }
        PathBuf::from(".i18n/cache")
    }

    pub fn cache_key(
        lang: &str,
        english_text: &str,
        glossary_version: &str,
        rules_version: &str,
    ) -> String {
        let seed = format!("{lang}\n{english_text}\n{glossary_version}\n{rules_version}");
        blake3::hash(seed.as_bytes()).to_hex().to_string()
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, String> {
        let path = self.entry_path(key);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .map_err(|err| format!("failed reading cache entry {}: {err}", path.display()))?;
        let entry: CacheEntry = serde_json::from_str(&raw)
            .map_err(|err| format!("invalid cache entry {}: {err}", path.display()))?;
        Ok(Some(entry.translation))
    }

    pub fn put(&self, key: &str, translation: &str) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|err| {
            format!(
                "failed creating cache directory {}: {err}",
                self.dir.display()
            )
        })?;
        let path = self.entry_path(key);
        let raw = serde_json::to_string(&CacheEntry {
            translation: translation.to_string(),
        })
        .map_err(|err| format!("failed serializing cache entry: {err}"))?;
        fs::write(&path, raw)
            .map_err(|err| format!("failed writing cache entry {}: {err}", path.display()))
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}{CACHE_FILE_SUFFIX}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{CACHE_FILE_SUFFIX, CacheStore};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("gt-i18n-cache-{name}-{stamp}"))
    }

    #[test]
    fn get_returns_none_when_entry_missing() {
        let dir = unique_temp_dir("missing");
        let store = CacheStore::new(dir.clone());
        assert_eq!(
            store
                .get("does-not-exist")
                .expect("missing is not an error"),
            None
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn put_then_get_round_trips_translation() {
        let dir = unique_temp_dir("roundtrip");
        let store = CacheStore::new(dir.clone());
        let key = CacheStore::cache_key("fr", "Hello", "g1", "r1");

        store.put(&key, "Bonjour").expect("put should succeed");
        assert_eq!(
            store.get(&key).expect("get should succeed"),
            Some("Bonjour".to_string())
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn put_creates_missing_cache_directory() {
        let dir = unique_temp_dir("nested").join("a").join("b");
        assert!(!dir.exists());
        let store = CacheStore::new(dir.clone());
        let key = CacheStore::cache_key("de", "Hi", "g1", "r1");

        store
            .put(&key, "Hallo")
            .expect("put should create the directory tree");
        assert!(dir.exists());

        let _ = fs::remove_dir_all(dir.parent().and_then(|p| p.parent()).unwrap_or(&dir));
    }

    #[test]
    fn cache_key_is_deterministic_and_input_sensitive() {
        let base = CacheStore::cache_key("fr", "Hello", "g1", "r1");
        assert_eq!(base, CacheStore::cache_key("fr", "Hello", "g1", "r1"));

        assert_ne!(base, CacheStore::cache_key("es", "Hello", "g1", "r1"));
        assert_ne!(base, CacheStore::cache_key("fr", "Goodbye", "g1", "r1"));
        assert_ne!(base, CacheStore::cache_key("fr", "Hello", "g2", "r1"));
        assert_ne!(base, CacheStore::cache_key("fr", "Hello", "g1", "r2"));
    }

    #[test]
    fn get_reports_error_for_corrupt_entry() {
        let dir = unique_temp_dir("corrupt");
        fs::create_dir_all(&dir).expect("create cache dir");
        let key = "deadbeef";
        fs::write(
            dir.join(format!("{key}{CACHE_FILE_SUFFIX}")),
            "not valid json",
        )
        .expect("write corrupt entry");

        let store = CacheStore::new(dir.clone());
        let err = store.get(key).expect_err("corrupt entry should error");
        assert!(
            err.contains("invalid cache entry"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn default_dir_returns_a_translator_scoped_path() {
        let dir = CacheStore::default_dir();
        assert!(
            dir.ends_with("i18n-translator"),
            "expected translator-scoped cache dir, got {}",
            dir.display()
        );
    }
}
