# Agent Guide: Fettlr

This file is the authoritative agent and AI guide for this repo. CLAUDE.md imports it via `@AGENTS.md`.

## What this project is

A cross-platform desktop app for managing Claude Code's skills, plugins, marketplaces, and MCP
servers: browse, install, update, remove, and enable or disable them, and track what needs
updating, at user level and per project.

This is a Rust and Tauri rewrite of a finished, frozen Swift/macOS app. All domain logic lives in a
Rust core, fully tested and independent of Tauri; the app shell is a thin typed IPC layer, and the
frontend is view-only React.

## Repo layout

```
fettlr/
├── Cargo.toml               Cargo workspace root
├── crates/
│   └── fettlr-core/        all domain logic (CLI bridge, discovery, stores, updates, watcher);
│                             no Tauri dependency, fully tested
├── app/                      Tauri app (added in a later task)
│   ├── src-tauri/            thin shell crate: typed commands, events, the updater
│   └── src/                  React + TypeScript frontend, view-only
├── packages/
│   └── design-system/        tokens, Tailwind theme, restyled shadcn components (added later)
├── site/                      Astro download site, imports the design system (added later)
├── scripts/                   icon generation, release helpers, localisation sync/check
├── pnpm-workspace.yaml        pnpm workspace: app, packages/*, site
└── LICENSE, README.md
```

`docs/` and every dot-directory except `.github/` and `.well-known/` are git-ignored: local working
notes, never commit them.

## Build, test, and lint commands

### Rust (from the repo root)

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

All three must pass before any commit.

### JavaScript and TypeScript (from the repo root, once packages exist)

```sh
pnpm -r typecheck   # tsc --noEmit across every package
pnpm -r lint
```

### Running the app (from `app/`, once it exists)

```sh
pnpm tauri dev
pnpm tauri build
```

## Conventions

- **Comments: zero as the norm.** Add one only when omitting it would let a future edit silently
  break a non-obvious correctness or security invariant. Never narrate what code does. Never
  reference ticket ids. Never add section-divider comments. Doc comments on public core items are
  fine; SCSS and CSS never carry comments, no exceptions.
- **Localisation.** Every user-facing string goes through an i18next key from day one. Development
  language is `en` with US spellings; `en-GB` and `fr` are added later. A hardcoded UI string
  authored once localisation exists is a bug. The rule governs copy the frontend writes. Diagnostic
  text the shell and the core produce stays English on purpose: `UiError`'s message and detail, and
  the core warnings the onboarding screen lists, quote command lines, paths and CLI stderr, so a
  translated wrapper around untranslated evidence would help nobody and would pin a catalogue key to
  every error the CLI can invent. Anything the window itself words, including the sentences it
  builds around structured failures, goes through a key.
- **Commits.** Conventional commit subjects (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`,
  `test:`). No `Co-Authored-By` trailer and no generated-by or attribution lines of any kind. Stage
  specific files by name; never `git add -A` or `git add .`, which sweeps up ignored-in-spirit
  scratch and local planning docs.
- **Secrets.** Values such as MCP environment variables and header values never leave the Tauri
  shell unmasked and never appear in logs; secret-bearing MCP args, URL passwords and URL query
  values are masked the same way. The core crate has no notion of masking; that is a shell concern.
- **YAGNI.** No abstraction beyond what the current task requires.
- **Paths.** The core crate never resolves `~`, and every path that depends on the reader's machine
  arrives injected via a `Paths` struct built once in the shell, so tests can point the whole crate
  at a temporary directory. Four absolute paths are documented platform constants rather than
  discovered state, and they are the only ones: `/opt/homebrew/bin/claude` and
  `/usr/local/bin/claude`, the two ruled rungs of Claude Code discovery in `cli.rs`; `/bin/zsh`, the
  login shell `cli.rs` probes when those rungs miss; and `/usr/bin/git`, the binary `updates/git.rs`
  runs. They hold until the portability milestone, which is where per-platform lookup belongs. Do
  not add a fifth without a ruling.

## Reference implementation

`/Users/thomasguillot/Sites/natalie-swift` is the frozen Swift/macOS implementation this project
rebuilds. Its spec defines the behavioural contract and its test suite is the coverage checklist.
When a task says to match the Swift behaviour, read the named Swift source file and port the
behaviour, not the code.
