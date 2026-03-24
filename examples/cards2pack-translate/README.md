# cards2pack Translation Example

Translate an English bundle extracted by `greentic-cards2pack` into multiple languages.

## Usage

```bash
# Translate to French and German
greentic-i18n-translator translate \
  --langs fr,de \
  --en en.json \
  --glossary glossary.json

# Validate output
greentic-i18n-translator validate \
  --langs fr,de \
  --en en.json
```

## Files

- `en.json` — English bundle extracted via `greentic-cards2pack extract-i18n`
- `glossary.json` — Terms to keep untranslated (brand names, technical terms)

## Key format

Keys follow the `{prefix}.{cardId}.{json_path}.{field}` pattern:

```
card.welcome.body_0.text      → first body element's text
card.form.body_2_choices_1.title → second choice option's title
```

## Generating the English bundle

```bash
greentic-cards2pack extract-i18n \
  --input ./cards \
  --output en.json
```

Or let `generate` do it all:

```bash
greentic-cards2pack generate \
  --cards ./cards --out ./pack --name demo \
  --auto-translate --langs fr,de \
  --glossary glossary.json
```
