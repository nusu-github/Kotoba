# Agent Guide for Kotoba

## Purpose
- This file is for coding agents working in this repository.
- Use it as the default guide for build/test workflows and code style.
- If repository instructions conflict with this file, repository instructions win.

## Project Snapshot
- Language: Rust (`edition = "2024"`).
- Crate name: `kotoba` (binary + library in one crate).
- Main domains:
  - `src/frontend`: lexer/parser/AST/tokenization.
  - `src/sema`: semantic analysis and diagnostics.
  - `src/backend`: codegen, VM, values, builtins.
  - `src/module`: module loading and dependency resolution.
  - `src/diag`: diagnostic rendering.
  - `tests/unit`, `tests/e2e`, `tests/conformance`.
- DSL and diagnostics are Japanese-heavy; preserve that behavior.

## Toolchain and Setup
- Rust toolchain must support edition 2024.
- Standard workflow uses Cargo only (no Makefile/justfile found).
- Build artifacts and temp files should stay under `target/`.

## Build Commands
- Fast type/build check:
  - `cargo check`
- Debug build:
  - `cargo build`
- Release build:
  - `cargo build --release`
- Run CLI static check:
  - `cargo run -- check examples/hello.kb`
- Run CLI execution:
  - `cargo run -- run examples/hello.kb`

## Lint and Format Commands
- Format code in-place:
  - `cargo fmt --all`
- Verify formatting only (CI-style):
  - `cargo fmt --all -- --check`
- Run Clippy (recommended baseline):
  - `cargo clippy --all-targets --all-features`
- Run strict Clippy:
  - `cargo clippy --all-targets --all-features -- -D warnings`
- Notes from current repo state:
  - `cargo test` passes.
  - Strict Clippy currently reports existing warnings in repository code.
  - Formatting check may fail unless `cargo fmt --all` is applied.

## Test Commands
- Run all tests:
  - `cargo test`
- Run integration suite group:
  - `cargo test --test unit`
  - `cargo test --test e2e`
  - `cargo test --test conformance`
- List all test names (useful before targeting one):
  - `cargo test -- --list`

## Running a Single Test (Important)
- Exact single unit test:
  - `cargo test --test unit parser::parser_accepts_proc_def -- --exact`
- Exact single e2e test:
  - `cargo test --test e2e cli_test::cli_test_manifest -- --exact`
- Exact single conformance integration test harness:
  - `cargo test --test conformance runner::conformance_smoke -- --exact`
- Name-filtered single test (substring match):
  - `cargo test parser_accepts_proc_def`
- Show stdout/stderr for one test while debugging:
  - `cargo test --test unit parser::parser_accepts_proc_def -- --exact --nocapture`

## Conformance and CLI Test Cases
- Run conformance cases via CLI manifest:
  - `cargo run -- test`
- Run one conformance case by ID filter:
  - `cargo run -- test --filter RUN-ACCEPT-001`
- Use a custom manifest path:
  - `KOTOBA_TEST_MANIFEST=tests/conformance/manifest.yaml cargo run -- test`

## Useful Environment Variables
- `KOTOBA_TEST_MANIFEST`
  - Path to conformance manifest for `kotoba test`.
- `KOTOBA_PARSE_STEP_LIMIT`
  - Parser safety limit override (positive integer).
- `KOTOBA_ANALYZE_STEP_LIMIT`
  - Semantic analyzer safety limit override (positive integer).

## Code Style Guidelines

### Formatting
- Follow `rustfmt` output; do not hand-format against it.
- Keep functions and matches readable over dense one-liners.
- Use section comments like `// === ... ===` only for major logical blocks.

### Imports
- Keep imports grouped in this order:
  1. `std` imports.
  2. Third-party crate imports.
  3. `crate::...` imports.
- Keep one blank line between groups.
- Prefer explicit imports; keep wildcard imports only where already established.
- Let `rustfmt` normalize ordering inside groups.

### Types and Data Modeling
- Prefer expressive enums/structs over loosely typed tuples.
- Keep source positions (`Span`) attached to syntax/diagnostic data where possible.
- Use `BigInt`-based numeric behavior consistently with existing `Value` semantics.
- Use `Option`/`Result` for absence and failures; avoid sentinel values.

### Naming Conventions
- Rust naming:
  - `snake_case` for functions/variables/modules.
  - `PascalCase` for structs/enums/traits.
  - `UPPER_SNAKE_CASE` for constants.
- Test names should describe behavior, e.g. `parser_accepts_proc_def`.
- Token/keyword variant names follow existing romanized Japanese conventions (`Moshi`, `Koukai`, etc.); keep consistent with lexer/parser.

### Error Handling and Diagnostics
- In library/core modules, return structured errors/diagnostics instead of exiting.
- `process::exit(...)` is acceptable in CLI command handlers in `src/main.rs`.
- Prefer actionable diagnostics:
  - Set `DiagnosticKind` correctly.
  - Attach `.with_span(...)` when location is known.
  - Attach `.with_hint(...)` when a concrete next action exists.
- Keep user-facing diagnostic text consistent with existing Japanese messaging.
- Avoid `panic!` in non-test code except for clear internal invariant violations.

### Parser/Semantic Rules
- Preserve parser/analyzer forward-progress safeguards and step-limit checks.
- If adding syntax/semantic rules, update both implementation and tests.
- Keep diagnostic code patterns (e.g., `DGN-00x`) stable and searchable.

### Module and Public API Changes
- If public module surfaces change, update exports in `src/lib.rs` deliberately.
- Preserve compatibility re-exports unless there is an explicit migration decision.

### Testing Practices
- Put parser/lexer/sema/vm behavior tests under `tests/unit/*`.
- Put binary/CLI behavior tests under `tests/e2e/*`.
- Keep conformance coverage in `tests/conformance/manifest.yaml` and runner tests.
- Use `env!("CARGO_BIN_EXE_kotoba")` for CLI integration tests.
- Prefer deterministic assertions; include stderr/stdout in failure messages.
- Use temp paths in `target/` or `std::env::temp_dir()` for generated test files.

## Docs and Specs
- Language/spec references live under `docs/`.
- If behavior changes, update relevant spec docs and tests together.

## Cursor/Copilot Rule Files
- Checked paths:
  - `.cursor/rules/`
  - `.cursorrules`
  - `.github/copilot-instructions.md`
- No Cursor/Copilot instruction files were found in this repository at authoring time.
- If these files are added later, treat them as mandatory instructions and update this guide.

## Suggested Pre-PR Validation
- `cargo fmt --all`
- `cargo test`
- `cargo clippy --all-targets --all-features`
- For language changes: `cargo run -- test --filter <CASE_ID>` on affected conformance IDs.
