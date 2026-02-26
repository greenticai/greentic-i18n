# Greentic i18n Workspace

This workspace contains the Greentic localization tooling:

- `crates/greentic-i18n-lib`: canonical resolver, formatter facade, and the I18nId v1 generator packed into a single deterministic library.
- `crates/greentic-i18n`: CLI surface that normalizes tags, resolves profiles, emits JSON, and supports debugging.

The repo also documents the canonical CBOR schema, resolver contract, and CLI JSON schema so downstream services can rely on stable IDs and formatting outputs.

## Install

Install the translator CLI via `cargo-binstall`:

```bash
cargo binstall greentic-i18n-translator
```
