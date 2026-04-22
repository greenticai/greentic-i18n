use std::path::{Path, PathBuf};

pub fn default_i18n_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("i18n")
}

pub fn en_json(repo_root: &Path) -> PathBuf {
    default_i18n_dir(repo_root).join("en.json")
}

pub fn lang_json(repo_root: &Path, lang: &str) -> PathBuf {
    default_i18n_dir(repo_root).join(format!("{lang}.json"))
}

#[cfg(test)]
mod tests {
    use super::{default_i18n_dir, en_json, lang_json};
    use std::path::Path;

    #[test]
    fn path_helpers_use_repo_root_i18n_directory() {
        let root = Path::new("/repo");
        assert_eq!(default_i18n_dir(root), Path::new("/repo/i18n"));
        assert_eq!(en_json(root), Path::new("/repo/i18n/en.json"));
        assert_eq!(lang_json(root, "fr"), Path::new("/repo/i18n/fr.json"));
    }
}
