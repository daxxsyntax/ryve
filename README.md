<p align="center">
  <img src="./assets/logo.svg" alt="Ryve logo" width="140" />
</p>

<h1 align="center">Ryve</h1>

<p align="center">
  <strong>A native desktop IDE for multi-hand development.</strong>
</p>

<p align="center">
  Ryve coordinates terminals, coding tools, and structured work so parallel development can move fast without pinching, blocking, or kicking back.
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-stable-black?logo=rust" />
  <img alt="License" src="https://img.shields.io/badge/license-AGPL--3.0--or--later-blue" />
  <img alt="Platform" src="https://img.shields.io/badge/platform-desktop-6f42c1" />
</p>

---

## What is Ryve?

**Ryve** is a desktop IDE for managing development work through autonomous coding processes, embedded terminals, and a structured workgraph.

It is named after the **riving knife** on a table saw: the safety device that keeps the cut open and prevents dangerous kickback. In the same way, Ryve keeps parallel development work moving safely by reducing collisions, ambiguity, and coordination failure.

Ryve is built for people who want:

- a native desktop environment
- multiple coding tools running side by side
- a terminal-first workflow
- structured work tracking inside the editor
- better coordination between active workers and the code they touch

---

## Core concepts

| Concept | Meaning |
|---|---|
| **Workshop** | A project workspace opened in Ryve |
| **Bench** | The tabbed work surface for terminals and tool sessions |
| **Hand** | An active worker process operating inside a workshop |
| **Crew** | An optional grouping of multiple Hands |
| **Spark** | A unit of work in the workgraph |
| **Bond** | A dependency or relationship between Sparks |
| **Ember** | A short-lived signal emitted during work |
| **Engraving** | Persistent workshop knowledge |
| **Alloy** | A coordination pattern for grouped work |

---

## Interface

Ryve combines a file explorer, a tabbed terminal bench, and an embedded workgraph.

```text
┌──────────────────────────────────────────────────────────────────┐
│ Workshop Tabs                                   [+ New Workshop] │
├──────────────────────────────────────────────────────────────────┤
│ File Explorer     │ Bench (tabbed terminals)   │ Workgraph       │
│                   │                             │                 │
│ > src/            │ [Terminal] [Claude] [+]    │ SP-001  P0      │
│   main.rs         │                             │ SP-002  P1      │
│   workshop.rs     │ $ claude --chat            │ SP-003  P2      │
│ > data/           │ > working on feature...    │                 │
│                   │                             │                 │
│ Active Hands      │                             │                 │
│ Claude Code       │                             │                 │
│ Aider             │                             │                 │
└──────────────────────────────────────────────────────────────────┘
```

<p align="center">
  <img src="./assets/example.png" alt="Ryve desktop screenshot" width="800" />
</p>

| Area | Purpose |
|------|---------|
| File Explorer | Project tree with git-aware status display |
| Bench | Tabbed terminal and tool sessions |
| Active Hands | Running worker sessions inside the workshop |
| Workgraph | Sparks, Bonds, and coordination state |

---

## Features

### Native desktop UI

Ryve is built with Iced for a fast, cross-platform Rust desktop experience. Supports both dark and light mode based on system appearance.

### Embedded terminals

Each workshop can run multiple terminal-backed sessions using alacritty-terminal through a patched iced_term integration.

### Multi-tool workflow

Ryve detects supported coding tools on your PATH and can launch them directly into Bench tabs.

Supported tools (must support system prompt injection):

- **Claude Code** (`--system-prompt`)
- **Codex** (`--instructions`)
- **Aider** (`--read`)
- **OpenCode** (`--prompt`)

Only agents that accept system prompt injection are supported. Ryve requires control over each Hand's instructions to enforce workgraph coordination rules.

### Worktree isolation

Every Hand spawns in its own git worktree (`.ryve/worktrees/<session>/`), preventing merge conflicts between concurrent agents working on the same project.

### Workgraph-driven coordination

The workgraph is the nervous system of Ryve. Every Hand reads `.ryve/WORKSHOP.md` (injected via system prompt) which contains active sparks, architectural constraints, failing contracts, and coordination rules. The workgraph database is the source of truth; WORKSHOP.md is a generated projection.

The workgraph includes:

- **Sparks** — work items with structured intent (problem, invariants, risk, scope)
- **Bonds** — dependency graph with cycle detection
- **Contracts** — machine-checkable verification criteria (required/advisory)
- **Embers** — ephemeral inter-Hand signals with TTL
- **Engravings** — persistent knowledge and architectural constraints
- **Alloys** — coordination patterns (scatter/watch/chain)
- **Hand Assignments** — liveness-aware claims with heartbeat and handoff
- **Commit Links** — git commits linked to sparks via `[sp-xxxx]` references

### Real-time synchronisation

The sparks panel auto-refreshes every 3 seconds, detecting changes made by agents directly in the database. WORKSHOP.md is regenerated on every spark mutation so all Hands see current state.

### Workshop-local state

Each workshop gets its own `.ryve/` directory for config, data, worktrees, and context.

---

## Architecture

Ryve is a Rust workspace made up of focused crates:

| Crate | Purpose |
|-------|---------|
| `src/` | Main desktop application |
| `data/` | SQLite persistence, config, git, workgraph, integrations |
| `llm/` | Multi-provider LLM integration |
| `ipc/` | Single-instance and local coordination support |
| `vendor/iced_term/` | Vendored terminal widget integration |

Built with:

- [Iced](https://iced.rs) — native Rust GUI
- [alacritty-terminal](https://github.com/alacritty/alacritty) — terminal backend
- [sqlx](https://github.com/launchbadge/sqlx) — SQLite access and migrations
- [genai](https://github.com/nickel-org/genai) — multi-provider LLM support
- [tokio](https://tokio.rs) — async runtime

---

## Project layout

```
ryve/
├── Cargo.toml
├── src/                  # desktop app
├── data/                 # persistence, git, sparks, config
├── llm/                  # LLM client + protocol types
├── ipc/                  # local process coordination
├── vendor/
│   └── iced_term/        # patched embedded terminal widget
├── assets/
│   └── logo.svg
└── docs/
```

---

## Requirements

- Rust stable
- A desktop OS supported by Iced
- Git installed and available in shell

Recommended:

- Latest stable Rust toolchain
- One or more supported coding CLIs for Hand sessions

---

## Getting started

```sh
git clone https://github.com/loomantix/ryve.git
cd ryve
cargo run
```

### Build

```sh
cargo build
```

### Run checks

```sh
cargo check
cargo test
cargo clippy --all-targets --all-features
```

---

## Status

Ryve is in active development.

The project is currently focused on:

- Core desktop UX and light/dark mode support
- Terminal and tool session management with worktree isolation
- Workshop structure and background customisation
- Workgraph: intent tracking, verification contracts, provenance, constraints
- Multi-Hand coordination: assignments, heartbeat, handoff, auto-refresh
- Agent context injection via system prompt flags and boot file pointers

Expect rapid iteration.

---

## Design goals

Ryve is being built around a few simple principles:

- **Native first** — not a web app wrapped in a shell
- **Terminal centered** — terminals are first-class, not bolted on
- **Structured coordination** — work should be visible and traceable
- **Tool agnostic** — Hands can be powered by different engines
- **Local ownership** — workshop state lives with the project

---

## Contributing

Ryve is open source and still early. The best way to contribute right now is to:

- Open issues
- Suggest UX improvements
- Test workflows on real projects
- Contribute focused PRs once the architecture stabilizes

A fuller contributor guide can be added as the project matures.

---

## License

Licensed under AGPL-3.0-or-later. See [LICENSE](LICENSE).

Copyright 2026 Xerxes Noble