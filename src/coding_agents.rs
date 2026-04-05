// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Auto-detection of CLI coding agents available on the system.

use std::fmt;

/// A coding agent that can be launched from the bench.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodingAgent {
    /// Display name shown in the dropdown (e.g. "Claude Code")
    pub display_name: String,
    /// CLI command to run (e.g. "claude")
    pub command: String,
    /// Default arguments
    pub args: Vec<String>,
}

impl fmt::Display for CodingAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name)
    }
}

/// Known coding agents and their CLI commands.
const KNOWN_AGENTS: &[(&str, &str, &[&str])] = &[
    ("Claude Code", "claude", &[]),
    ("Codex", "codex", &[]),
    ("Aider", "aider", &[]),
    ("Goose", "goose", &["session"]),
    ("Cline", "cline", &[]),
    ("Continue", "continue", &[]),
];

/// Detect which coding agents are available on PATH.
pub fn detect_available() -> Vec<CodingAgent> {
    KNOWN_AGENTS
        .iter()
        .filter(|(_, cmd, _)| which(cmd))
        .map(|(name, cmd, args)| CodingAgent {
            display_name: name.to_string(),
            command: cmd.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
        })
        .collect()
}

/// Check if a command exists on PATH.
fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
