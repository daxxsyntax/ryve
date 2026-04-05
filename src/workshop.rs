// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! A Workshop is a self-contained workspace bound to a directory.
//! Each workshop has its own file explorer, agents, and bench.

use std::collections::HashMap;
use std::path::PathBuf;

use uuid::Uuid;

use crate::coding_agents::CodingAgent;
use crate::screen::agents::AgentSession;
use crate::screen::bench::{BenchState, TabKind};

pub struct Workshop {
    pub id: Uuid,
    pub directory: PathBuf,
    pub bench: BenchState,
    pub terminals: HashMap<u64, iced_term::Terminal>,
    pub agent_sessions: Vec<AgentSession>,
    pub sidebar_split: f32,
}

impl Workshop {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            directory,
            bench: BenchState::new(),
            terminals: HashMap::new(),
            agent_sessions: Vec::new(),
            sidebar_split: 0.65,
        }
    }

    /// Human-readable name (last path component).
    pub fn name(&self) -> &str {
        self.directory
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workshop")
    }

    /// Spawn a terminal tab, optionally running a coding agent command.
    /// Uses the global next_terminal_id counter for unique IDs.
    pub fn spawn_terminal(
        &mut self,
        title: String,
        agent: Option<&CodingAgent>,
        next_terminal_id: &mut u64,
    ) -> u64 {
        let kind = match agent {
            Some(a) => TabKind::CodingAgent(a.clone()),
            None => TabKind::Terminal,
        };

        let tab_id = *next_terminal_id;
        *next_terminal_id += 1;
        self.bench.create_tab(tab_id, title, kind);

        let mut settings = iced_term::settings::Settings::default();
        settings.font.size = 14.0;
        settings.backend.working_directory = Some(self.directory.clone());

        if let Some(agent) = agent {
            settings.backend.program = agent.command.clone();
            settings.backend.args = agent.args.clone();
        } else {
            let shell = std::env::var("SHELL")
                .unwrap_or_else(|_| "/bin/bash".to_string());
            settings.backend.program = shell;
        }

        if let Ok(term) = iced_term::Terminal::new(tab_id, settings) {
            self.terminals.insert(tab_id, term);
        }

        tab_id
    }

    /// Handle terminal shutdown/title-change for a given terminal id.
    pub fn handle_terminal_action(
        &mut self,
        id: u64,
        action: iced_term::actions::Action,
    ) {
        match action {
            iced_term::actions::Action::Shutdown => {
                self.terminals.remove(&id);
                self.agent_sessions.retain(|s| s.tab_id != id);
                self.bench.close_tab(id);
            }
            iced_term::actions::Action::ChangeTitle(title) => {
                if let Some(tab) =
                    self.bench.tabs.iter_mut().find(|t| t.id == id)
                {
                    tab.title = title;
                }
            }
            iced_term::actions::Action::Ignore => {}
        }
    }
}
