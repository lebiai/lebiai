# CLAUDE.md

This file defines the coding behavior expected from Claude when working in this repository.

The goal is not to produce more code.

The goal is to produce the smallest correct change that satisfies the user's request and can be verified.

---

## Project Context

Tech stack:

```text
Rust 2021 (rustc 1.91+), Cargo workspace, tokio async runtime.
Crates: hermes-core / -llm / -mcp / -store / -skills / -memory / -reflect / -cli / -tui / -gui
LLM: Anthropic Messages API (also DeepSeek's anthropic-compat endpoint).
MCP: rmcp 1.6 (stdio + Streamable HTTP).
TUI: ratatui 0.30 + crossterm 0.29.
Storage: plain markdown + YAML frontmatter under ~/.small-rust-hermes/.
```

Common commands:

```bash
# install (no install step — cargo fetches on build)
cargo fetch

# test
cargo test --workspace

# typecheck (fast iteration)
cargo check --workspace

# lint
cargo clippy --workspace --all-targets -- -D warnings

# build (release binaries)
cargo build --workspace --release

# run the CLI against the configured provider
cargo run -p hermes-cli -- ask "your prompt"
```

Config lives at `~/.small-rust-hermes/config.toml` (mode 600 — contains API keys).

---

## 1. Think Before Coding

Before editing code, explain briefly:

- What you think the user is asking for.
- Which files or modules are likely involved.
- What assumptions you are making.
- What risks or ambiguities exist.

If the request is ambiguous, ask for clarification before making changes. Do not silently guess important requirements.

---

## 2. Simplicity First

Prefer the simplest solution that solves the current problem. Do not add abstractions unless they are clearly required by the current task.

Avoid: new frameworks, new layers, new generic utilities, premature configuration, "future-proof" designs that are not needed now.

A small direct fix is usually better than a clever generalized design.

---

## 3. Surgical Changes Only

Only modify files that are necessary for the task. Every changed line should be traceable to the user's request.

Do not rewrite unrelated code, reformat unrelated files, rename existing APIs unless explicitly asked, clean up old code you did not touch, or change behavior outside the requested scope.

If you discover unrelated problems, report them separately instead of fixing them silently.

---

## 4. Goal-Driven Execution

Turn the user's request into verifiable goals. Whenever possible:

- Write or identify a failing test first.
- Make the smallest change to pass the test.
- Run relevant checks.
- Report what was verified.

Do not claim success without verification.

---

## 5. Context Discipline

Do not load unnecessary files. Prefer reading the smallest set of files needed to understand the task. When the task becomes too large, suggest splitting it into smaller steps.

---

## 6. Human Review Awareness

Your summary is not a substitute for human review. For high-risk changes, explicitly warn the user.

High-risk areas in this project:

```text
- crates/hermes-llm/src/anthropic.rs — provider HTTP path; never log api_key,
  never print full request bodies (system prompts may include user secrets).
- ~/.small-rust-hermes/config.toml — contains API keys. Mode 600. Never commit.
- Skill / memory file writes (hermes-store) — writes go to user's home directory;
  validate paths to prevent escape (no `..` in skill names).
- Reflection auto-write — must always require user approval before persisting
  candidates. Never auto-accept.
```

Generic high-risk categories: authentication, authorization, payment, data migration, security, privacy, production release, public API behavior, large refactors.

---

## 7. Communication Style

Be concise, concrete, and honest. Do not hide uncertainty. Do not say the task is complete if verification was not performed.

End every task with:

```text
Changed:
- ...

Verified:
- ...

Not verified:
- ...

Risks:
- ...
```
