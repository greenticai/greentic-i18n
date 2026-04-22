pub mod cache;
pub mod cli;
pub mod cli_i18n;
pub mod git_diff;
pub mod json_map;
pub mod paths;
pub mod provider;
pub mod state;
pub mod validate;

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}
