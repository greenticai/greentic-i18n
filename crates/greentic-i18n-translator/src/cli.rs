use clap::error::ErrorKind;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::CacheStore;
use crate::cli_i18n::CliI18n;
use crate::git_diff;
use crate::json_map::{JsonMap, read_json_map, write_json_map};
use crate::provider::TranslatorProvider;
use crate::provider::codex_cli::{AuthMode as ProviderAuthMode, CodexCliConfig, CodexCliProvider};
use crate::state::{TranslatorState, hash_text};
use crate::validate::{ValidationIssue, validate_lang_map, validate_translation};

const RULES_VERSION: &str = "v1-placeholder-backtick-newline-exact";
const ENGINE_TAG: &str = "codex-cli";

#[derive(Debug, Parser)]
#[command(name = "greentic-i18n-translator")]
#[command(about = "Translation workflow CLI for Greentic i18n JSON files")]
pub struct Cli {
    #[arg(long, global = true)]
    pub locale: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Diff keys between base/head refs for the English source map")]
    Diff {
        #[arg(long)]
        base: String,
        #[arg(long)]
        head: String,
        #[arg(long, default_value = "i18n/en.json")]
        en: PathBuf,
    },
    #[command(about = "Validate translated language files against English format invariants")]
    Validate {
        #[arg(long)]
        langs: String,
        #[arg(long, default_value = "i18n/en.json")]
        en: PathBuf,
    },
    #[command(about = "Report missing or stale translations using translator state metadata")]
    Status {
        #[arg(long)]
        langs: String,
        #[arg(long, default_value = "i18n/en.json")]
        en: PathBuf,
    },
    #[command(
        about = "Generate/update translations via provider integration with cache/state safety"
    )]
    Translate {
        #[arg(long)]
        langs: String,
        #[arg(long, default_value = "i18n/en.json")]
        en: PathBuf,
        #[arg(long, value_enum, default_value_t = CliAuthMode::Auto)]
        auth_mode: CliAuthMode,
        #[arg(long)]
        codex_home: Option<PathBuf>,
        #[arg(long, default_value_t = 50)]
        batch_size: usize,
        #[arg(long, default_value_t = 2)]
        max_retries: usize,
        #[arg(long)]
        glossary: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        api_key_stdin: bool,
        #[arg(long, default_value_t = false)]
        overwrite_manual: bool,
        #[arg(long)]
        cache_dir: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliAuthMode {
    Auto,
    Browser,
    Device,
    ApiKey,
}

impl From<CliAuthMode> for ProviderAuthMode {
    fn from(value: CliAuthMode) -> Self {
        match value {
            CliAuthMode::Auto => ProviderAuthMode::Auto,
            CliAuthMode::Browser => ProviderAuthMode::Browser,
            CliAuthMode::Device => ProviderAuthMode::Device,
            CliAuthMode::ApiKey => ProviderAuthMode::ApiKey,
        }
    }
}

#[derive(Debug, Default)]
struct TranslateOutcome {
    translated: usize,
    cache_hits: usize,
    manual_preserved: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct StatusOutcome {
    missing_keys: Vec<String>,
    stale_keys: Vec<String>,
}

struct RunTranslateArgs<'a> {
    langs: &'a str,
    en_path: &'a Path,
    auth_mode: CliAuthMode,
    codex_home: Option<&'a PathBuf>,
    batch_size: usize,
    max_retries: usize,
    glossary_path: Option<&'a PathBuf>,
    api_key_stdin: bool,
    overwrite_manual: bool,
    cache_dir: Option<&'a PathBuf>,
}

pub fn run() -> Result<(), String> {
    let raw_args = env::args().collect::<Vec<_>>();
    match Cli::try_parse_from(raw_args.clone()) {
        Ok(cli) => {
            let i18n = CliI18n::from_request(cli.locale.as_deref())?;
            run_with(cli, &i18n)
        }
        Err(err) => match err.kind() {
            ErrorKind::DisplayHelp => {
                let requested_locale = requested_locale_from_args(&raw_args);
                let i18n = CliI18n::from_request(requested_locale.as_deref())?;
                print_help(&raw_args, &i18n);
                Ok(())
            }
            ErrorKind::DisplayVersion => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            _ => Err(err.to_string()),
        },
    }
}

pub fn run_with(cli: Cli, i18n: &CliI18n) -> Result<(), String> {
    match cli.command {
        Command::Diff { base, head, en } => {
            let repo_root =
                env::current_dir().map_err(|err| format!("failed to read current dir: {err}"))?;
            let report = git_diff::diff_en_at_refs(&repo_root, &base, &head, &en)?;
            let payload = json!({
                "added": report.added,
                "removed": report.removed,
                "updated": report.updated,
            });
            let output = serde_json::to_string_pretty(&payload)
                .map_err(|err| format!("failed to format diff JSON: {err}"))?;
            println!("{output}");
            Ok(())
        }
        Command::Validate { langs, en } => run_validate(&langs, &en, i18n),
        Command::Status { langs, en } => run_status(&langs, &en, i18n),
        Command::Translate {
            langs,
            en,
            auth_mode,
            codex_home,
            batch_size,
            max_retries,
            glossary,
            api_key_stdin,
            overwrite_manual,
            cache_dir,
        } => run_translate(
            RunTranslateArgs {
                langs: &langs,
                en_path: &en,
                auth_mode,
                codex_home: codex_home.as_ref(),
                batch_size,
                max_retries,
                glossary_path: glossary.as_ref(),
                api_key_stdin,
                overwrite_manual,
                cache_dir: cache_dir.as_ref(),
            },
            i18n,
        ),
    }
}

fn run_validate(langs: &str, en_path: &Path, i18n: &CliI18n) -> Result<(), String> {
    let en_map = read_json_map(en_path)?;
    let langs = resolve_langs(langs, en_path)?;
    if langs.is_empty() {
        return Err(i18n.t("cli.error.no_target_languages"));
    }

    println!(
        "{}",
        i18n.tf("cli.validate.header", &[&langs.len().to_string()])
    );
    let mut failing_langs = 0usize;
    for lang in langs {
        let lang_path = lang_path_for(en_path, &lang)?;
        match read_json_map(&lang_path) {
            Ok(tr_map) => {
                let issues = validate_lang_map(&en_map, &tr_map);
                if issues.is_empty() {
                    println!("{}", i18n.tf("cli.lang.ok", &[&lang]));
                } else {
                    failing_langs += 1;
                    print_lang_issues(&lang, &issues, i18n);
                }
            }
            Err(err) => {
                failing_langs += 1;
                println!("{}", i18n.tf("cli.lang.failed_load", &[&lang, &err]));
            }
        }
    }

    if failing_langs > 0 {
        return Err(i18n.tf(
            "cli.validate.error.failed_langs",
            &[&failing_langs.to_string()],
        ));
    }
    Ok(())
}

fn run_status(langs: &str, en_path: &Path, i18n: &CliI18n) -> Result<(), String> {
    let en_map = read_json_map(en_path)?;
    let langs = resolve_langs(langs, en_path)?;
    if langs.is_empty() {
        return Err(i18n.t("cli.error.no_target_languages"));
    }

    let repo_root =
        env::current_dir().map_err(|err| format!("failed to read current working dir: {err}"))?;
    let state_path = TranslatorState::default_path(&repo_root);
    let state = TranslatorState::load(&state_path)?;
    println!(
        "{}",
        i18n.tf("cli.status.header", &[&langs.len().to_string()])
    );

    let mut failing_langs = 0usize;
    for lang in langs {
        let lang_path = lang_path_for(en_path, &lang)?;
        let tr_map = if lang_path.exists() {
            read_json_map(&lang_path)?
        } else {
            JsonMap::new()
        };
        let outcome = status_for_lang(&lang, &en_map, &tr_map, &state);
        if outcome.missing_keys.is_empty() && outcome.stale_keys.is_empty() {
            println!("{}", i18n.tf("cli.lang.ok", &[&lang]));
        } else {
            failing_langs += 1;
            println!(
                "{}",
                i18n.tf(
                    "cli.status.lang.summary",
                    &[
                        &lang,
                        &outcome.missing_keys.len().to_string(),
                        &outcome.stale_keys.len().to_string(),
                    ],
                )
            );
            for key in outcome.missing_keys {
                println!("{}", i18n.tf("cli.status.lang.missing", &[&key]));
            }
            for key in outcome.stale_keys {
                println!("{}", i18n.tf("cli.status.lang.stale", &[&key]));
            }
        }
    }

    if failing_langs > 0 {
        return Err(i18n.tf(
            "cli.status.error.failed_langs",
            &[&failing_langs.to_string()],
        ));
    }
    Ok(())
}

fn run_translate(args: RunTranslateArgs<'_>, i18n: &CliI18n) -> Result<(), String> {
    if args.batch_size == 0 {
        return Err(i18n.t("cli.translate.error.batch_size_gt_zero"));
    }

    let en_map = read_json_map(args.en_path)?;
    let langs = resolve_langs(args.langs, args.en_path)?;
    if langs.is_empty() {
        return Err(i18n.t("cli.error.no_target_languages"));
    }

    let glossary = match args.glossary_path {
        Some(path) => Some(read_json_map(path)?),
        None => None,
    };
    let glossary_version = glossary_version(glossary.as_ref())?;

    let provider = CodexCliProvider::new(CodexCliConfig {
        auth_mode: args.auth_mode.into(),
        codex_home: args.codex_home.cloned(),
        api_key_stdin: args.api_key_stdin,
    });
    provider.ensure_auth()?;

    let repo_root =
        env::current_dir().map_err(|err| format!("failed to read current working dir: {err}"))?;
    let state_path = TranslatorState::default_path(&repo_root);
    let mut state = TranslatorState::load(&state_path)?;
    let cache = CacheStore::new(
        args.cache_dir
            .cloned()
            .unwrap_or_else(CacheStore::default_dir),
    );

    for lang in langs {
        let lang_path = lang_path_for(args.en_path, &lang)?;
        let existing_map = if lang_path.exists() {
            read_json_map(&lang_path)?
        } else {
            JsonMap::new()
        };

        let (out_map, outcome) = translate_map_with_provider(
            &provider,
            &lang,
            &en_map,
            existing_map,
            glossary.as_ref(),
            &glossary_version,
            args.batch_size,
            args.max_retries,
            &cache,
            &mut state,
            args.overwrite_manual,
        )?;
        write_json_map(&lang_path, &out_map)?;
        println!(
            "{}",
            i18n.tf(
                "cli.translate.lang.summary",
                &[
                    &lang,
                    &outcome.translated.to_string(),
                    &outcome.cache_hits.to_string(),
                    &outcome.manual_preserved.to_string(),
                    &lang_path.display().to_string(),
                ],
            )
        );
    }

    state.save(&state_path)?;
    println!(
        "{}",
        i18n.tf(
            "cli.translate.state.path",
            &[&state_path.display().to_string()]
        )
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn translate_map_with_provider(
    provider: &dyn TranslatorProvider,
    lang: &str,
    en_map: &JsonMap,
    mut out_map: JsonMap,
    glossary: Option<&JsonMap>,
    glossary_version: &str,
    batch_size: usize,
    max_retries: usize,
    cache: &CacheStore,
    state: &mut TranslatorState,
    overwrite_manual: bool,
) -> Result<(JsonMap, TranslateOutcome), String> {
    let mut outcome = TranslateOutcome::default();
    let mut pending = Vec::<(String, String)>::new();

    for (key, en_text) in en_map {
        let en_hash = hash_text(en_text);

        if !overwrite_manual && has_manual_override(lang, key, en_text, &out_map, state) {
            outcome.manual_preserved += 1;
            continue;
        }

        // If translator state says this key was bot-translated from the same
        // English text and the current output still matches, keep it as-is.
        // This avoids provider calls when local cache is cold/ephemeral.
        if has_fresh_bot_translation(lang, key, en_text, &out_map, state) {
            outcome.cache_hits += 1;
            continue;
        }

        let cache_key = CacheStore::cache_key(lang, en_text, glossary_version, RULES_VERSION);
        if let Some(cached) = cache.get(&cache_key)?
            && validate_translation(en_text, &cached).is_ok()
        {
            out_map.insert(key.clone(), cached.clone());
            state.set_key_state(lang, key, en_hash, hash_text(&cached), ENGINE_TAG);
            outcome.cache_hits += 1;
            continue;
        }

        pending.push((key.clone(), en_text.clone()));
    }

    for chunk in pending.chunks(batch_size) {
        let mut feedback: Option<String> = None;
        let mut response: Option<JsonMap> = None;
        for attempt in 0..=max_retries {
            let attempt_response =
                provider.translate_batch(lang, chunk, glossary, feedback.as_deref())?;
            match validate_batch(chunk, &attempt_response) {
                Ok(()) => {
                    response = Some(attempt_response);
                    break;
                }
                Err(message) => {
                    if attempt == max_retries {
                        return Err(format!(
                            "translation validation failed for language `{lang}` after {} attempts: {message}",
                            max_retries + 1
                        ));
                    }
                    feedback = Some(format!(
                        "You broke placeholders/backticks/newlines. Fix and return valid JSON only.\n{message}"
                    ));
                }
            }
        }
        let response = response.ok_or_else(|| {
            format!("internal error: missing translated response for language `{lang}`")
        })?;

        for (key, en_text) in chunk {
            let translated = response
                .get(key)
                .cloned()
                .ok_or_else(|| format!("provider output missing key `{key}`"))?;
            out_map.insert(key.clone(), translated.clone());
            let en_hash = hash_text(en_text);
            let tr_hash = hash_text(&translated);
            state.set_key_state(lang, key, en_hash, tr_hash, ENGINE_TAG);
            let cache_key = CacheStore::cache_key(lang, en_text, glossary_version, RULES_VERSION);
            cache.put(&cache_key, &translated)?;
            outcome.translated += 1;
        }
    }

    Ok((out_map, outcome))
}

fn has_manual_override(
    lang: &str,
    key: &str,
    english_text: &str,
    out_map: &JsonMap,
    state: &TranslatorState,
) -> bool {
    let Some(existing_translation) = out_map.get(key) else {
        return false;
    };
    let Some(key_state) = state.key_state(lang, key) else {
        return false;
    };
    if key_state.last_english_hash != hash_text(english_text) {
        return false;
    }
    hash_text(existing_translation) != key_state.last_bot_translation_hash
}

fn has_fresh_bot_translation(
    lang: &str,
    key: &str,
    english_text: &str,
    out_map: &JsonMap,
    state: &TranslatorState,
) -> bool {
    let Some(existing_translation) = out_map.get(key) else {
        return false;
    };
    let Some(key_state) = state.key_state(lang, key) else {
        return false;
    };
    key_state.last_english_hash == hash_text(english_text)
        && key_state.last_bot_translation_hash == hash_text(existing_translation)
}

fn status_for_lang(
    lang: &str,
    en_map: &JsonMap,
    tr_map: &JsonMap,
    state: &TranslatorState,
) -> StatusOutcome {
    let mut outcome = StatusOutcome::default();
    for (key, en_text) in en_map {
        if !tr_map.contains_key(key) {
            outcome.missing_keys.push(key.clone());
            continue;
        }
        let key_state = state.key_state(lang, key);
        let en_hash = hash_text(en_text);
        let is_stale = match key_state {
            Some(s) => s.last_english_hash != en_hash,
            None => true,
        };
        if is_stale {
            outcome.stale_keys.push(key.clone());
        }
    }
    outcome
}

fn print_lang_issues(lang: &str, issues: &[ValidationIssue], i18n: &CliI18n) {
    println!(
        "{}",
        i18n.tf("cli.lang.issues.header", &[lang, &issues.len().to_string()])
    );
    for issue in issues {
        println!(
            "{}",
            i18n.tf(
                "cli.lang.issues.item",
                &[&issue.key, &issue.error.message()]
            )
        );
    }
}

fn resolve_langs(langs: &str, en_path: &Path) -> Result<Vec<String>, String> {
    if langs.trim() == "all" {
        let i18n_dir = en_path
            .parent()
            .ok_or_else(|| format!("`{}` has no parent directory", en_path.display()))?;
        let mut result = Vec::new();
        let entries = fs::read_dir(i18n_dir).map_err(|err| {
            format!(
                "failed reading i18n directory {}: {err}",
                i18n_dir.display()
            )
        })?;

        for entry in entries {
            let entry =
                entry.map_err(|err| format!("failed reading i18n directory entry: {err}"))?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem != "en" {
                result.push(stem.to_string());
            }
        }
        result.sort();
        result.dedup();
        return Ok(result);
    }

    let parsed = langs
        .split(',')
        .map(str::trim)
        .filter(|lang| !lang.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    Ok(parsed)
}

fn lang_path_for(en_path: &Path, lang: &str) -> Result<PathBuf, String> {
    let parent = en_path
        .parent()
        .ok_or_else(|| format!("`{}` has no parent directory", en_path.display()))?;
    Ok(parent.join(format!("{lang}.json")))
}

fn glossary_version(glossary: Option<&JsonMap>) -> Result<String, String> {
    let text = match glossary {
        Some(map) => serde_json::to_string(map)
            .map_err(|err| format!("failed serializing glossary for cache key: {err}"))?,
        None => String::new(),
    };
    Ok(hash_text(&text))
}

fn validate_batch(items: &[(String, String)], translated: &JsonMap) -> Result<(), String> {
    let mut errors = Vec::new();
    for (key, en_text) in items {
        let Some(tr_text) = translated.get(key) else {
            errors.push(format!("missing key `{key}` in provider output"));
            continue;
        };
        if let Err(err) = validate_translation(en_text, tr_text) {
            errors.push(format!("key `{key}`: {}", err.message()));
        }
    }

    for key in translated.keys() {
        if !items.iter().any(|(expected, _)| expected == key) {
            errors.push(format!("unexpected key `{key}` in provider output"));
        }
    }

    if errors.is_empty() {
        return Ok(());
    }
    Err(errors.join("; "))
}

#[derive(Debug, Clone, Copy)]
enum HelpTarget {
    TopLevel,
    Diff,
    Validate,
    Status,
    Translate,
}

fn requested_locale_from_args(args: &[String]) -> Option<String> {
    let mut idx = 1usize;
    while idx < args.len() {
        let token = &args[idx];
        if token == "--locale" {
            return args.get(idx + 1).cloned();
        }
        if let Some(value) = token.strip_prefix("--locale=") {
            return Some(value.to_string());
        }
        idx += 1;
    }
    None
}

fn help_target_from_args(args: &[String]) -> HelpTarget {
    let mut idx = 1usize;
    while idx < args.len() {
        let token = &args[idx];
        if token == "--locale" {
            idx += 2;
            continue;
        }
        if token.starts_with("--locale=") || token == "-h" || token == "--help" {
            idx += 1;
            continue;
        }
        if token.starts_with('-') {
            idx += 1;
            continue;
        }
        return match token.as_str() {
            "diff" => HelpTarget::Diff,
            "validate" => HelpTarget::Validate,
            "status" => HelpTarget::Status,
            "translate" => HelpTarget::Translate,
            _ => HelpTarget::TopLevel,
        };
    }
    HelpTarget::TopLevel
}

fn print_help(args: &[String], i18n: &CliI18n) {
    match help_target_from_args(args) {
        HelpTarget::TopLevel => print_top_level_help(i18n),
        HelpTarget::Diff => print_diff_help(i18n),
        HelpTarget::Validate => print_validate_help(i18n),
        HelpTarget::Status => print_status_help(i18n),
        HelpTarget::Translate => print_translate_help(i18n),
    }
}

fn print_top_level_help(i18n: &CliI18n) {
    println!("{}", i18n.t("cli.help.top.title"));
    println!();
    println!("{}", i18n.t("cli.help.top.usage"));
    println!();
    println!("{}", i18n.t("cli.help.top.commands"));
    println!("  diff       {}", i18n.t("cli.help.command.diff"));
    println!("  validate   {}", i18n.t("cli.help.command.validate"));
    println!("  status     {}", i18n.t("cli.help.command.status"));
    println!("  translate  {}", i18n.t("cli.help.command.translate"));
    println!("  help       {}", i18n.t("cli.help.command.help"));
    println!();
    println!("{}", i18n.t("cli.help.top.options"));
    println!(
        "      --locale <LOCALE>  {}",
        i18n.t("cli.help.option.locale")
    );
    println!(
        "  -h, --help             {}",
        i18n.t("cli.help.option.help")
    );
}

fn print_diff_help(i18n: &CliI18n) {
    println!("{}", i18n.t("cli.help.command.diff"));
    println!();
    println!("{}", i18n.t("cli.help.diff.usage"));
    println!();
    println!("{}", i18n.t("cli.help.diff.options"));
    println!(
        "      --base <BASE>      {}",
        i18n.t("cli.help.diff.option.base")
    );
    println!(
        "      --head <HEAD>      {}",
        i18n.t("cli.help.diff.option.head")
    );
    println!(
        "      --en <EN>          {}",
        i18n.t("cli.help.diff.option.en")
    );
    println!(
        "      --locale <LOCALE>  {}",
        i18n.t("cli.help.option.locale")
    );
    println!(
        "  -h, --help             {}",
        i18n.t("cli.help.option.help")
    );
}

fn print_validate_help(i18n: &CliI18n) {
    println!("{}", i18n.t("cli.help.command.validate"));
    println!();
    println!("{}", i18n.t("cli.help.validate.usage"));
    println!();
    println!("{}", i18n.t("cli.help.validate.options"));
    println!(
        "      --langs <LANGS>    {}",
        i18n.t("cli.help.validate.option.langs")
    );
    println!(
        "      --en <EN>          {}",
        i18n.t("cli.help.validate.option.en")
    );
    println!(
        "      --locale <LOCALE>  {}",
        i18n.t("cli.help.option.locale")
    );
    println!(
        "  -h, --help             {}",
        i18n.t("cli.help.option.help")
    );
}

fn print_status_help(i18n: &CliI18n) {
    println!("{}", i18n.t("cli.help.command.status"));
    println!();
    println!("{}", i18n.t("cli.help.status.usage"));
    println!();
    println!("{}", i18n.t("cli.help.status.options"));
    println!(
        "      --langs <LANGS>    {}",
        i18n.t("cli.help.status.option.langs")
    );
    println!(
        "      --en <EN>          {}",
        i18n.t("cli.help.status.option.en")
    );
    println!(
        "      --locale <LOCALE>  {}",
        i18n.t("cli.help.option.locale")
    );
    println!(
        "  -h, --help             {}",
        i18n.t("cli.help.option.help")
    );
}

fn print_translate_help(i18n: &CliI18n) {
    println!("{}", i18n.t("cli.help.command.translate"));
    println!();
    println!("{}", i18n.t("cli.help.translate.usage"));
    println!();
    println!("{}", i18n.t("cli.help.translate.options"));
    println!(
        "      --langs <LANGS>            {}",
        i18n.t("cli.help.translate.option.langs")
    );
    println!(
        "      --en <EN>                  {}",
        i18n.t("cli.help.translate.option.en")
    );
    println!(
        "      --auth-mode <AUTH_MODE>    {}",
        i18n.t("cli.help.translate.option.auth_mode")
    );
    println!(
        "      --codex-home <CODEX_HOME>  {}",
        i18n.t("cli.help.translate.option.codex_home")
    );
    println!(
        "      --batch-size <BATCH_SIZE>  {}",
        i18n.t("cli.help.translate.option.batch_size")
    );
    println!(
        "      --max-retries <MAX_RETRIES> {}",
        i18n.t("cli.help.translate.option.max_retries")
    );
    println!(
        "      --glossary <GLOSSARY>      {}",
        i18n.t("cli.help.translate.option.glossary")
    );
    println!(
        "      --api-key-stdin            {}",
        i18n.t("cli.help.translate.option.api_key_stdin")
    );
    println!(
        "      --overwrite-manual         {}",
        i18n.t("cli.help.translate.option.overwrite_manual")
    );
    println!(
        "      --cache-dir <CACHE_DIR>    {}",
        i18n.t("cli.help.translate.option.cache_dir")
    );
    println!(
        "      --locale <LOCALE>          {}",
        i18n.t("cli.help.option.locale")
    );
    println!(
        "  -h, --help                     {}",
        i18n.t("cli.help.option.help")
    );
}

#[cfg(test)]
mod tests {
    use super::{glossary_version, status_for_lang, translate_map_with_provider};
    use crate::cache::CacheStore;
    use crate::json_map::JsonMap;
    use crate::provider::TranslatorProvider;
    use crate::state::{TranslatorState, hash_text};
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct MockProvider {
        calls: Cell<usize>,
    }

    impl MockProvider {
        fn new() -> Self {
            Self {
                calls: Cell::new(0),
            }
        }
        fn call_count(&self) -> usize {
            self.calls.get()
        }
    }

    impl TranslatorProvider for MockProvider {
        fn ensure_auth(&self) -> Result<(), String> {
            Ok(())
        }

        fn translate_batch(
            &self,
            _lang: &str,
            items: &[(String, String)],
            _glossary: Option<&JsonMap>,
            _retry_feedback: Option<&str>,
        ) -> Result<JsonMap, String> {
            self.calls.set(self.calls.get() + 1);
            let mut out = JsonMap::new();
            for (key, _) in items {
                out.insert(key.clone(), format!("translated:{key}"));
            }
            Ok(out)
        }
    }

    fn temp_cache_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid system clock")
            .as_nanos();
        std::env::temp_dir().join(format!("gt-i18n-cache-{name}-{stamp}"))
    }

    #[test]
    fn cache_hit_skips_provider_call() {
        let provider = MockProvider::new();
        let mut en = JsonMap::new();
        en.insert("hello".to_string(), "Hello {}".to_string());
        let out = JsonMap::new();
        let mut state = TranslatorState::default();
        let cache_dir = temp_cache_dir("hit");
        let cache = CacheStore::new(cache_dir.clone());
        let gv = glossary_version(None).expect("glossary version");
        let key = CacheStore::cache_key(
            "fr",
            "Hello {}",
            &gv,
            "v1-placeholder-backtick-newline-exact",
        );
        cache.put(&key, "Bonjour {}").expect("cache write");

        let (result, outcome) = translate_map_with_provider(
            &provider, "fr", &en, out, None, &gv, 10, 2, &cache, &mut state, false,
        )
        .expect("translate should succeed");

        assert_eq!(provider.call_count(), 0);
        assert_eq!(outcome.cache_hits, 1);
        assert_eq!(outcome.translated, 0);
        assert_eq!(result.get("hello"), Some(&"Bonjour {}".to_string()));
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn manual_override_is_preserved_when_english_unchanged() {
        let provider = MockProvider::new();
        let mut en = JsonMap::new();
        en.insert("k".to_string(), "Hello".to_string());
        let mut out = JsonMap::new();
        out.insert("k".to_string(), "Bonjour manuel".to_string());

        let mut state = TranslatorState::default();
        state.set_key_state(
            "fr",
            "k",
            hash_text("Hello"),
            hash_text("Bonjour bot"),
            "codex-cli",
        );
        let cache_dir = temp_cache_dir("manual");
        let cache = CacheStore::new(cache_dir.clone());
        let gv = glossary_version(None).expect("glossary version");

        let (result, outcome) = translate_map_with_provider(
            &provider, "fr", &en, out, None, &gv, 10, 2, &cache, &mut state, false,
        )
        .expect("translate should succeed");

        assert_eq!(provider.call_count(), 0);
        assert_eq!(outcome.manual_preserved, 1);
        assert_eq!(result.get("k"), Some(&"Bonjour manuel".to_string()));
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn fresh_state_skips_provider_call_without_cache() {
        let provider = MockProvider::new();
        let mut en = JsonMap::new();
        en.insert("k".to_string(), "Hello".to_string());

        let mut out = JsonMap::new();
        out.insert("k".to_string(), "Bonjour".to_string());

        let mut state = TranslatorState::default();
        state.set_key_state(
            "fr",
            "k",
            hash_text("Hello"),
            hash_text("Bonjour"),
            "codex-cli",
        );

        let cache_dir = temp_cache_dir("fresh-state");
        let cache = CacheStore::new(cache_dir.clone());
        let gv = glossary_version(None).expect("glossary version");

        let (result, outcome) = translate_map_with_provider(
            &provider, "fr", &en, out, None, &gv, 10, 2, &cache, &mut state, false,
        )
        .expect("translate should succeed");

        assert_eq!(provider.call_count(), 0);
        assert_eq!(outcome.translated, 0);
        assert_eq!(result.get("k"), Some(&"Bonjour".to_string()));
        let _ = fs::remove_dir_all(cache_dir);
    }

    #[test]
    fn status_reports_missing_and_stale_keys() {
        let mut en = JsonMap::new();
        en.insert("k1".to_string(), "Hello".to_string());
        en.insert("k2".to_string(), "Bye".to_string());

        let mut tr = JsonMap::new();
        tr.insert("k1".to_string(), "Bonjour".to_string());

        let mut state = TranslatorState::default();
        state.set_key_state(
            "fr",
            "k1",
            hash_text("Old English"),
            hash_text("Bonjour"),
            "codex-cli",
        );

        let outcome = status_for_lang("fr", &en, &tr, &state);
        assert_eq!(outcome.missing_keys, vec!["k2".to_string()]);
        assert_eq!(outcome.stale_keys, vec!["k1".to_string()]);
    }

    #[test]
    fn status_reports_ok_when_hashes_match_and_keys_exist() {
        let mut en = JsonMap::new();
        en.insert("k1".to_string(), "Hello".to_string());

        let mut tr = JsonMap::new();
        tr.insert("k1".to_string(), "Bonjour".to_string());

        let mut state = TranslatorState::default();
        state.set_key_state(
            "fr",
            "k1",
            hash_text("Hello"),
            hash_text("Bonjour"),
            "codex-cli",
        );

        let outcome = status_for_lang("fr", &en, &tr, &state);
        assert!(outcome.missing_keys.is_empty());
        assert!(outcome.stale_keys.is_empty());
    }
}
