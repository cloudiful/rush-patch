# Rush Patch

`Rush Patch` is a desktop tool for translating `RPG Maker MV/MZ` games into Chinese.

## What It Does

V1 focuses on one workflow:

1. Scan an `RPG Maker MV/MZ` game directory
2. Extract text from `www/data/*.json`
3. Extract safe static strings from `www/js/plugins/*.js`
4. Translate through the OpenAI API
5. Validate placeholders, control codes, and structure
6. Back up files into `.rush-patch/backups`
7. Patch the game files in place

Use the restore action to copy the backups back over the patched files.
If you rerun translation later, Rush Patch reuses the original backups as the extraction and patch baseline, so you can try another model or prompt without restoring first.
Translation cache data lives under `.rush-patch/catalog.sqlite` as a per-project SQLite catalog. The main workflow reuses the existing catalog by default and only rebuilds it when the cache is missing or unreadable. Rebuilding the cache reuses completed translations from matching units when the source text is unchanged.
RPG Maker event command lists are extracted in command order: consecutive dialogue lines are grouped into one translation unit, scroll text is grouped, and nearby previous / next lines are carried into the prompt context.

## Scope

V1 supports:

- `RPG Maker MV/MZ`
- OpenAI API as the translation provider
- JSON data files
- Static plugin JS string literals
- In-place patch workflow with restore from `.rush-patch/backups`
- Repeatable re-translation from original backups without a mandatory restore step

V1 does not include:

- OCR or image text replacement
- Binary script formats
- Unity, Ren'Py, RPG Maker XP/VX/VXAce, or other engines
- Multi-project management
- Web deployment

## Stack

- Desktop shell: `Tauri`
- Core pipeline: `Rust`
- Frontend: `Vue 3 + Tailwind CSS + PrimeVue`

## Configuration

Rush Patch persists ordinary settings in the Tauri app config directory as `settings.json`.
The OpenAI API key is stored separately through the OS credential store. On Windows this uses Windows Credential Manager via the Rust `keyring` crate.
The OpenAI endpoint defaults to the Responses API. Switch to Chat Completions only for compatible legacy providers.
OpenAI connection settings and language selection live on the Settings page. The translation workbench keeps run-specific options such as model, input token budget, and prompt files. The token budget uses `tiktoken-rs` counting so large dialogue blocks are split before requests are sent.
The UI supports Chinese and English. The language preference is stored in browser `localStorage`.

## Current Status

The repository is under active implementation.

Implemented or in progress:

- MV/MZ project scanning
- JSON extraction and grouped context
- Conservative plugin JS string extraction
- SQLite catalog persistence with reusable completed translations
- App config persistence with OS credential-store API key storage
- In-place writeback with per-file backup and restore
- Rust-side OpenAI translation pipeline with Responses API by default, optional Chat Completions endpoint, token-budget batching, adjacent small database-segment merging to avoid tiny requests, retry, timeout, configurable concurrency, and cooperative cancel
- Vue workbench UI with Chinese / English localization and a Settings page

## Developer Notes

Rust SQL lives in `src-tauri/sql/**/*.sql` and is compiled through `sqlx::query_file!`-style macros.
For local schema validation, run `cargo run --bin db_init` from `src-tauri/`. The repo keeps a small dev SQLite file at `src-tauri/dev/catalog-dev.sqlite`, and `build.rs` wires `DATABASE_URL` to it automatically unless you override the variable yourself.

Verification status changes over time. Check recent commit history or local test results for the latest build state.

## Releases

GitHub Actions builds the Windows x86_64 executable on pushes to `main` and `v*` tags.

- `main` publishes a moving `latest` prerelease.
- `v*` tags publish formal releases.
- Current release asset: `Rush-Patch-windows-x86_64.exe`.

## Plan

The implementation baseline lives in [docs/implementation-plan.md](docs/implementation-plan.md).

## Acknowledgements

This project references ideas and practical lessons from open-source work beyond direct runtime dependencies.

- [neavo/LinguaGacha](https://github.com/neavo/LinguaGacha): referenced for RPG Maker translation workflow lessons, especially around preserving context, avoiding over-fragmented extraction, using name fields as translation hints, and protecting formatting/control tokens.
- [jsonptr](https://github.com/chanced/jsonptr): adopted for safer JSON Pointer-based writeback.
- [oxc](https://github.com/oxc-project/oxc): adopted for conservative JS tokenization during plugin string extraction.
