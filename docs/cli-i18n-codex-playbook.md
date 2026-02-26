# Codex Playbook: English-Only CLI to Fully Internationalized CLI

This playbook is for migrating `greentic-i18n` CLI output from hardcoded English strings to i18n keys and translated language packs.

## Scope and target languages

Current target language set (66 files, including English):

- `ar`
- `ar-AE`
- `ar-DZ`
- `ar-EG`
- `ar-IQ`
- `ar-MA`
- `ar-SA`
- `ar-SD`
- `ar-SY`
- `ar-TN`
- `ay`
- `bg`
- `bn`
- `cs`
- `da`
- `de`
- `el`
- `en`
- `en-GB`
- `es`
- `et`
- `fa`
- `fi`
- `fr`
- `gn`
- `gu`
- `hi`
- `hr`
- `ht`
- `hu`
- `id`
- `it`
- `ja`
- `km`
- `kn`
- `ko`
- `lo`
- `lt`
- `lv`
- `ml`
- `mr`
- `ms`
- `my`
- `nah`
- `ne`
- `nl`
- `no`
- `pa`
- `pl`
- `pt`
- `qu`
- `ro`
- `ru`
- `si`
- `sk`
- `sr`
- `sv`
- `ta`
- `te`
- `th`
- `tl`
- `tr`
- `uk`
- `ur`
- `vi`
- `zh`

Keep this list in-repo and update this document when languages are added.

## Migration outcomes

1. All user-visible CLI runtime text is key-based (no hardcoded English in command output/errors; clap-generated help remains English unless explicitly reworked).
2. English source strings are centralized in one `en.json`.
3. Non-English language files exist for every target language from `operator_cli`.
4. Translation safety rules are enforced (`{}` placeholders, newline counts, backtick spans).
5. CI fails on broken/stale translations.

## Phase 1: Inventory all translatable CLI text

### Codex instructions

1. Find all user-visible strings:
   - `println!`, `eprintln!`, `format!` with user-facing text
   - clap `about`, `help`, `long_about`, `after_help`
   - error messages returned to CLI users
2. Exclude internal-only logs/debug traces not shown to end users.
3. Produce an inventory table: `file`, `line`, `literal`, `context`.
4. Group by domain:
   - command help/usage
   - runtime status messages
   - validation errors
   - interactive prompts

### Suggested commands

```bash
rg -n 'println!|eprintln!|format!\(|about\s*=|help\s*=|long_about|after_help' crates/greentic-i18n crates/greentic-i18n-translator
```

## Phase 2: Introduce i18n key architecture

### Codex instructions

1. Create a key naming convention:
   - `cli.<command>.<message>`
   - `cli.common.<message>`
2. Add an i18n lookup API used by CLI rendering code, e.g.:
   - `t(key)` for plain strings
   - `tf(key, args)` for formatted strings
3. Add fallback behavior:
   - selected locale -> default locale (`en`) -> key echo as last resort
4. Add a global locale selector:
   - `--locale <tag>` flag and/or `LANG`/`LC_ALL` support.
5. Replace all inventoried literals with key lookups.

### Locale detection reference implementation (reuse across repos)

Use this exact precedence so CLI behavior is deterministic:

1. `--locale <tag>` if passed
2. Environment (`LC_ALL`, `LC_MESSAGES`, `LANG`)
3. OS locale from `sys-locale`
4. Fallback to `"en"`

Add dependencies:

```toml
sys-locale = "0.3"
unic-langid = "0.9"
```

Reference implementation:

```rust
use std::env;
use unic_langid::LanguageIdentifier;

fn detect_env_locale() -> Option<String> {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(val) = env::var(key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn detect_system_locale() -> Option<String> {
    sys_locale::get_locale()
}

fn normalize_locale(raw: &str) -> Option<String> {
    // Handle common forms like en_US.UTF-8 or de_DE@euro before parsing.
    let mut cleaned = raw.trim();
    if cleaned.is_empty() {
        return None;
    }
    if let Some((head, _)) = cleaned.split_once('.') {
        cleaned = head;
    }
    if let Some((head, _)) = cleaned.split_once('@') {
        cleaned = head;
    }
    let cleaned = cleaned.replace('_', "-");
    cleaned
        .parse::<LanguageIdentifier>()
        .ok()
        .map(|lid| lid.to_string())
}

fn base_language(tag: &str) -> Option<String> {
    tag.split('-').next().map(|s| s.to_ascii_lowercase())
}

fn select_locale(cli_locale: Option<String>, supported: &[&str]) -> String {
    fn resolve(candidate: &str, supported: &[&str]) -> Option<String> {
        let norm = normalize_locale(candidate)?;
        if supported.iter().any(|s| *s == norm) {
            return Some(norm);
        }
        let base = base_language(&norm)?;
        if supported.iter().any(|s| *s == base) {
            return Some(base);
        }
        None
    }

    if let Some(cli) = cli_locale.as_deref() {
        if let Some(found) = resolve(cli, supported) {
            return found;
        }
    }

    if let Some(env_loc) = detect_env_locale() {
        if let Some(found) = resolve(&env_loc, supported) {
            return found;
        }
    }

    if let Some(sys_loc) = detect_system_locale() {
        if let Some(found) = resolve(&sys_loc, supported) {
            return found;
        }
    }

    "en".to_string()
}
```

Notes:

- Example raw inputs that should normalize: `en_US.UTF-8`, `en_US`, `en-US`, `de_DE@euro`.
- Never add OS-specific locale API calls manually; `sys-locale` already handles platform differences.
- Keep `supported` in one source of truth (same language list used by translation validation).

## Phase 3: Build English source map

### Codex instructions

1. Create `i18n/en.json` for this CLI surface.
2. Add one key per user-visible message.
3. Preserve format tokens exactly:
   - `{}` counts
   - `\n` structure
   - `` `...` `` spans
4. Keep values stable and deterministic (sorted keys, no duplicate keys).

## Phase 4: Sync language set from in-repo list

### Codex instructions

1. Enumerate languages from the list in this document.
2. Ensure matching files exist in this repo’s i18n directory.
3. For missing language files:
   - create `<lang>.json` with `{}` initially, or let translator generate.

### Suggested commands

```bash
# create missing language files from this playbook's list (example loop)
for lang in ar ar-AE ar-DZ ar-EG ar-IQ ar-MA ar-SA ar-SD ar-SY ar-TN ay bg bn cs da de el en-GB es et fa fi fr gn gu hi hr ht hu id it ja km kn ko lo lt lv ml mr ms my nah ne nl no pa pl pt qu ro ru si sk sr sv ta te th tl tr uk ur vi zh; do
  test -f "crates/greentic-i18n-translator/i18n/$lang.json" || printf "{\n}\n" > "crates/greentic-i18n-translator/i18n/$lang.json"
done
```

If you do not have a helper script, create missing files manually as `{}` and let translator fill them.

## Phase 5: Generate translations with translator crate

### Codex instructions

1. Run translate for all target languages.
2. Use glossary if domain terms must stay fixed.
3. Keep manual overrides by default; only force when intentional.

### Suggested commands

```bash
tools/i18n.sh all
```

### Process notes from actual implementation

1. `codex exec` compatibility:
   - current Codex CLI expects `codex exec <PROMPT>` (not `codex exec - --quiet`).
2. Auth is required for translation generation:
   - local browser login, or
   - `OPENAI_API_KEY` with `--auth-mode api-key`.
3. In sandboxed/non-auth environments, you can still:
   - seed all target language files from `en.json`,
   - run `validate` successfully,
   - defer true translation generation until auth is available.
4. Operational shortcut:
   - run `tools/i18n.sh all` whenever English strings change.
5. `--locale` scope:
   - today it applies to runtime messages emitted by command handlers.
   - clap-generated `--help`/usage text is still English with the current derive-based clap setup.
   - if fully localized help is required, migrate to a runtime-built clap command tree with localized `about/help` strings.

## Phase 6: CI and quality gates

### Required gates

1. `validate` must pass for all touched language files.
2. `status` must pass when `en.json` or translator state changes.
3. `cargo clippy --workspace --all-targets -- -D warnings` must pass.
4. `ci/local_check.sh` must pass.

## Phase 7: Human QA pass

### Codex instructions

1. Run sample commands in at least:
   - English (`en`)
   - one RTL language (`ar`)
   - one CJK language (`ja` or `zh`)
   - one language with region tag (`en-GB`, `ar-SA`)
2. Verify:
   - no broken placeholders
   - no translated code/backtick content
   - multiline output shape unchanged
   - runtime command output reflects locale (for example: `status`, `validate`, `translate`)
3. Note:
   - `--help` remains English unless clap help localization is implemented separately.

## Definition of done

1. No remaining hardcoded English user messages in CLI execution paths.
2. `i18n/en.json` complete and key-stable.
3. All target language files exist and validate.
4. `status` reports no stale/missing keys after translation run.
5. CI workflows enforce validation/staleness continuously.

## Codex execution prompt (copy/paste)

```text
Task: Internationalize greentic-i18n CLI fully.

Requirements:
1) Inventory all user-visible strings in crates/greentic-i18n and related CLI paths.
2) Introduce i18n key lookup layer with locale selection and English fallback.
3) Replace literals with i18n keys.
4) Build i18n/en.json from extracted strings.
5) Use the in-repo language list in this playbook as source-of-truth language set.
6) Ensure language files exist for that set, then run translator translate/validate/status.
7) Keep placeholder/newline/backtick invariants intact.
8) Run clippy/tests/local_check and fix all issues.
9) Summarize changed files, key naming scheme, and any unresolved strings.
```
