# Rush Patch V1 Implementation Plan

This document is the execution baseline for the current repository goal. Implementation work should follow this plan unless a later decision explicitly updates it.

## 1. Product Definition

### 1.1 Goal

Build a `Tauri + Rust` desktop tool for translating `RPG Maker MV/MZ` games into Chinese.

V1 success means:

- The app detects a valid `RPG Maker MV/MZ` game directory.
- The app extracts supported text from `www/data/*.json` and `www/js/plugins/*.js`.
- The app translates through the OpenAI API.
- The app validates the results before writeback.
- The app backs up files into `.rush-patch/backups` before writeback.
- The app patches the game files in place and can restore the backed-up originals.

### 1.2 Naming

- Chinese display name: `Rush Patch`
- English repo/app identifier: `Rush Patch`
- Rust package name: `rush-patch`

The Chinese name can stay playful. The English identifier should stay short, public-safe, and technically usable in package names, window titles, and repository paths.

### 1.3 Non-Goals for V1

- No support for non-`MV/MZ` engines
- No OCR or image text replacement
- No binary script extraction
- No web app
- No CLI-first release
- No destructive writeback without a restorable backup
- No advanced project management or collaborative glossary features

## 2. Stack and Project Shape

### 2.1 Desktop Stack

- Desktop shell: `Tauri`
- Backend/core: `Rust`
- Frontend: `Vue 3 + Vite`
- Styling: `Tailwind CSS`
- UI components: `PrimeVue`

This follows the chosen frontend direction and keeps the frontend focused on configuration, progress, logs, and result preview.

### 2.2 Rust Module Layout

Keep the Rust code cohesive and single-purpose. Initial modules should be:

- `domain`
- `scanner`
- `extractor_json`
- `extractor_js`
- `catalog`
- `translator`
- `validator`
- `applier`
- `app_state`
- `commands`

The frontend should stay thin. Core translation logic belongs in Rust.

## 3. Execution Phases

### Phase 0: Scaffold

Deliverables:

- `Tauri + Vue 3 + Tailwind CSS + PrimeVue` project scaffold
- Basic window shell and layout
- Shared config types and command wiring

Notes:

- Start simple with one Rust crate and one frontend app.
- Avoid premature workspace splitting.

### Phase 1: Scan and Extract JSON

Deliverables:

- Detect valid `RPG Maker MV/MZ` structure
- Require `www/data`
- Optionally enable plugin scan when `www/js/plugins` exists
- Traverse `www/data/*.json`
- Extract stable text units with location metadata

Acceptance:

- Standard `Map*.json`, `System.json`, `CommonEvents.json`, and similar files produce stable extraction output.

### Phase 2: Catalog and Safe Patch

Deliverables:

- Unified catalog model and cache file
- Deterministic IDs
- Hidden work directory at `.rush-patch`
- Per-file backup before first in-place patch
- Restore flow that copies backups over patched files

Acceptance:

- Patched files can be restored from `.rush-patch/backups`.
- Existing original backups are not overwritten by later patch runs.

### Phase 3: OpenAI Translation

Deliverables:

- `async-openai` provider implementation
- Batch translation queue
- Concurrency limit
- Timeout, retry, and cancel support
- Prompt envelope with glossary and protected token handling

Acceptance:

- Failed requests are surfaced in UI and do not corrupt prior results.

### Phase 4: JS Plugin Extraction

Deliverables:

- Parse plugin JS safely
- Extract only static string literals
- Skip template strings, dynamic concatenation, regex literals, and obvious code constants
- Replace strings by source span, not by full-file reformatting

Acceptance:

- Rewritten JS keeps original formatting as much as possible.
- Basic syntax validation passes after writeback.

### Phase 5: UI and Workflow

Deliverables:

- Game directory picker
- Persist ordinary settings in the Tauri app config directory
- Persist API key in the OS credential store
- OpenAI API key input
- Model selector
- Glossary and do-not-translate import
- Start/cancel buttons
- Progress and logs
- Result preview, patch entrypoint, and restore entrypoint

Acceptance:

- A user can complete the V1 flow without touching config files manually.

## 4. Data Model

The original `TranslationItem` idea is useful, but V1 should separate translation units from writeback spans.

### 4.1 ProjectConfig

- `game_root`
- `model`
- `api_key`
- `base_url`
- `system_prompt`
- `glossary_path`
- `do_not_translate_path`
- `batch_size`
- `max_concurrency`
- `request_timeout_secs`
- `source_lang`
- `target_lang`

The API key is intentionally excluded from the app config JSON file. It is loaded from and saved to the OS credential store.

### 4.2 TranslationSpan

Represents one concrete writeback target.

- `id`
- `file`
- `source_kind` as `json | js`
- `locator`
- `source_text`
- `protected_tokens`
- `flags`

### 4.3 TranslationUnit

Represents the semantic unit sent to the model.

- `id`
- `group_id`
- `semantic_kind`
- `context`
- `source_text`
- `translated_text`
- `status`
- `span_ids`

### 4.4 ContextEnvelope

Carries context that improves translation quality.

- `file`
- `json_path`
- `map_id`
- `event_id`
- `page_id`
- `command_index`
- `speaker_name`
- `prev_texts`
- `next_texts`
- `block_text`
- `glossary_hits`
- `notes`

### 4.5 ValidationReport

- `unit_id`
- `status`
- `errors`
- `warnings`
- `token_diff`

## 5. Context Strategy

This is the main place where V1 should improve on naive extract-and-translate tools.

### 5.1 Translate Semantic Blocks, Not Just Isolated Strings

Do not send every tiny string node to the model independently when the game structure provides better grouping.

Preferred examples:

- Consecutive dialogue lines should form one `dialogue_block`.
- Choice lists should preserve list grouping.
- Event messages should retain nearby speaker and event metadata.

### 5.2 Preserve Speaker and Neighbor Context

When available, send:

- Current speaker name
- Previous nearby lines
- Next nearby lines when cheap to collect
- Semantic hints such as `dialogue`, `choice`, `item description`, or `system label`

### 5.3 Use Name Fields as Hints

Character names, speaker labels, and similar fields should be captured both as translation targets and as consistency hints for nearby lines.

### 5.4 Do Not Globally Reuse by Raw Source Text Alone

Cache hits may help, but reuse must remain context-aware. The same Japanese string can require different Chinese output in different locations.

## 6. Extraction Rules

### 6.1 JSON

Initial extraction focus:

- Dialogue/event text
- Choice text
- Character, item, skill, state, class, enemy, and weapon names where applicable
- Descriptions and help text
- Other clear player-facing strings

Avoid over-aggressive extraction when a field looks machine-oriented or engine-internal.

### 6.2 JS Plugins

Only extract when all of the following are true:

- The node is a static string literal
- The string appears player-facing
- Replacing it will not alter code shape

Skip:

- Template literals
- Dynamic concatenations
- Regex content
- Import/export specifiers
- Obvious engine constants, internal keys, or code-only identifiers

## 7. Validation Rules

These rules are mandatory for V1 writeback.

- Control codes must be preserved, including forms like `\\N[1]`, `\\V[2]`, and `\\C[3]`.
- Placeholders such as `%s`, `%d`, `{name}`, and similar tokens must be preserved.
- Empty translations must never overwrite source text.
- Suspiciously short or long outputs should raise warnings.
- JSON output must parse successfully after writeback.
- JS output must pass basic syntax validation after writeback.

Failed validation means:

- Mark the unit failed
- Keep the original source text for that span
- Surface the failure in logs and UI

## 8. Test Plan

### 8.1 Functional Samples

Primary sample:

- A local RPG Maker MV/MZ sample project with `www/data` and optional `www/js/plugins`.

Secondary sample:

- One additional standard `RPG Maker MV/MZ` sample project

### 8.2 Verification Targets

- Valid MV/MZ directory detection
- Stable extraction from common JSON files
- Safe JS string extraction without dynamic-expression damage
- Retry behavior for OpenAI failures
- Token preservation
- In-place translated files
- Restore from `.rush-patch/backups`

## 9. Documentation and Attribution

When external open-source projects influence architecture or heuristics without being direct `Cargo.toml` dependencies, they should be acknowledged in `README.md`.

Current documented reference:

- `neavo/LinguaGacha`

Reference rationale:

- Preserving context instead of over-fragmenting text
- Using name fields as translation hints
- Protecting formatting and control sequences in game localization workflows

## 10. Immediate Next Steps

1. Expand JSON extraction from basic field capture into richer grouped dialogue context and validation-aware catalog entries.
2. Implement JS plugin extraction using static string literal detection and span-based replacement.
3. Add validator rules for control codes, placeholders, empty outputs, and suspicious translation lengths.
4. Improve translation progress reporting beyond command-level summaries.
5. Add cancellation tests around partially completed translation catalogs.
