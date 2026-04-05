// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Agents panel — lists active coding agent sessions.

use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Message {
    SelectAgent(Uuid),
}

/// An active coding agent session shown in the agents panel.
#[derive(Debug, Clone)]
pub struct AgentSession {
    pub id: Uuid,
    pub name: String,
    pub tab_id: u64,
}
