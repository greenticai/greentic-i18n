# i18n Translator

`greentic-i18n-translator` manages translation diffs, validation, generation, and staleness checks for `i18n/*.json`.

## One-time local setup

Browser login (recommended for local):

```bash
codex login
```

## Run locally

Translate all language files:

```bash
cargo run -p greentic-i18n-translator -- \
  translate --langs all --en i18n/en.json
```

Validate placeholder/backtick/newline rules:

```bash
cargo run -p greentic-i18n-translator -- \
  validate --langs all --en i18n/en.json
```

Check staleness and missing keys:

```bash
cargo run -p greentic-i18n-translator -- \
  status --langs all --en i18n/en.json
```

Use localized runtime output:

```bash
cargo run -p greentic-i18n-translator -- \
  --locale nl status --langs all --en i18n/en.json
```

Note: clap-generated `--help` output is currently English. `--locale` applies to runtime command output and errors.

## If login does not work

Use API key mode:

```bash
export OPENAI_API_KEY=...
cargo run -p greentic-i18n-translator -- \
  translate --langs all --en i18n/en.json --auth-mode api-key
```

## Add a new language

Create `i18n/<lang>.json` and commit it (it can start empty `{}`), then run:

```bash
cargo run -p greentic-i18n-translator -- \
  translate --langs <lang> --en i18n/en.json
```

## Staleness policy

`status` compares each language file to `.i18n/translator-state.json` and reports:

- keys missing from `<lang>.json`
- keys that need update because English changed since the last bot translation

CI uses `status` to fail when `i18n/en.json` changed but translations were not regenerated.

## State and cache

- state file: `.i18n/translator-state.json` (committed)
- cache dir: OS-local cache (not committed), override with `--cache-dir`
- translator short-circuits provider calls when state proves a key is already up to date, even if local cache is empty

## Manual overrides

Manual edits are preserved by default when English text is unchanged.

Use `--overwrite-manual` to force regeneration.

## Cost-aware usage

For day-to-day development, prefer checks first and selective translation:

```bash
cargo run -p greentic-i18n-translator -- \
  status --langs all --en i18n/en.json
cargo run -p greentic-i18n-translator -- \
  validate --langs all --en i18n/en.json
```

Translate only when status reports missing/stale keys, and scope languages when possible:

```bash
cargo run -p greentic-i18n-translator -- \
  translate --langs ar,ja,en-GB --en i18n/en.json --max-retries 0
```
