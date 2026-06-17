# AGENTS.md

This file is for contributors and coding agents working inside this repository. User-facing product overview belongs in `README.md`.

## Project Shape

- Desktop shell: `Tauri`
- Core pipeline: `Rust` in `src-tauri/`
- Frontend shell: `Vue 3 + Vite + Tailwind CSS + PrimeVue`
- Frontend state/routing: `Pinia + vue-router`

## Ownership Boundary

- Rust owns the product transaction:
  - scan game
  - extract JSON and plugin strings
  - persist catalog
  - call OpenAI
  - validate output
  - export translated copy
- Vue should stay thin:
  - collect config
  - trigger commands
  - show logs, progress, preview, and summaries

Do not move extraction, validation, translation, or writeback business rules into frontend code.

## Frontend Structure

- `src/App.vue`: route outlet only
- `src/router.ts`: route definitions
- `src/pages/`: page-level composition
- `src/components/`: reusable UI panels
- `src/stores/`: Pinia stores
- `src/api/`: typed Tauri invoke wrappers
- `src/types/`: shared frontend-only TS types

If a Vue file is starting to become a workflow hub or long form, split it before adding more logic.

## Rust Structure

Keep modules cohesive and small. Current module intent:

- `scanner`: detect MV/MZ project structure
- `extractor_json`: JSON extraction and JSON writeback
- `extractor_js`: conservative JS string extraction and JS span replacement
- `catalog`: build/load/persist translation catalog
- `prompting`: translation prompt assembly and response parsing
- `translation_io`: glossary / do-not-translate file loading
- `translator`: OpenAI batching, retry, timeout, catalog updates
- `validator`: token/control-code/output validation
- `applier`: export translated copy without touching source
- `commands`: Tauri invoke entrypoints
- `domain`: shared Rust data structures

Before adding a new Rust module, check whether the responsibility fits an existing one.

## Dependency Notes

- Do not stage or commit `Cargo.lock` or `bun.lock` unless explicitly requested by the user.
- `async-openai` requires explicit feature enablement for chat-completion APIs.

## Validation Expectations

- Prefer `cargo test` as the Rust verification baseline.
- If frontend dependencies are changed, also run the relevant Bun verification path when permissions allow.
- Do not claim completion for translation integration until Rust compile/test evidence is current.
