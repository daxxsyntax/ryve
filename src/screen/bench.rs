// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Bench panel — tabbed workspace for terminal sessions and coding agents.

use iced::widget::{Space, button, column, container, row, text};
use iced::{Element, Length};

use crate::coding_agents::CodingAgent;

/// A tab in the bench — either a plain terminal or a coding agent session.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: u64,
    pub title: String,
    pub kind: TabKind,
}

#[derive(Debug, Clone)]
pub enum TabKind {
    Terminal,
    CodingAgent(CodingAgent),
}

/// State for the bench panel.
pub struct BenchState {
    pub tabs: Vec<Tab>,
    pub active_tab: Option<u64>,
    pub dropdown_open: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    SelectTab(u64),
    CloseTab(u64),
    ToggleDropdown,
    NewTerminal,
    NewCodingAgent(CodingAgent),
    TerminalEvent(iced_term::Event),
}

impl BenchState {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab: None,
            dropdown_open: false,
        }
    }

    /// Create a tab with an externally-assigned ID.
    pub fn create_tab(&mut self, id: u64, title: String, kind: TabKind) {
        self.tabs.push(Tab { id, title, kind });
        self.active_tab = Some(id);
        self.dropdown_open = false;
    }

    pub fn close_tab(&mut self, id: u64) {
        self.tabs.retain(|t| t.id != id);
        if self.active_tab == Some(id) {
            self.active_tab = self.tabs.last().map(|t| t.id);
        }
    }

    /// Render the tab bar (tabs + "+" dropdown).
    pub fn view_tab_bar<'a>(&'a self, available_agents: &'a [CodingAgent]) -> Element<'a, Message> {
        let mut tab_row = row![].spacing(2);

        for tab in &self.tabs {
            let is_active = self.active_tab == Some(tab.id);
            let style = if is_active {
                button::primary
            } else {
                button::secondary
            };

            let tab_btn = button(text(&tab.title).size(13))
                .style(style)
                .padding([4, 10])
                .on_press(Message::SelectTab(tab.id));

            let close_btn = button(text("x").size(11))
                .style(button::text)
                .padding([4, 6])
                .on_press(Message::CloseTab(tab.id));

            tab_row = tab_row.push(row![tab_btn, close_btn].spacing(0));
        }

        let new_btn = button(text("+  \u{25BE}").size(13))
            .style(button::secondary)
            .padding([4, 10])
            .on_press(Message::ToggleDropdown);

        tab_row = tab_row.push(Space::new().width(Length::Fill)).push(new_btn);

        let mut bar = column![tab_row].spacing(0);

        if self.dropdown_open {
            let mut menu = column![].spacing(2).padding(4);

            menu = menu.push(
                button(text("New Terminal...").size(13))
                    .style(button::text)
                    .width(Length::Fill)
                    .on_press(Message::NewTerminal),
            );

            for agent in available_agents {
                let label = format!("New {}...", agent.display_name);
                menu = menu.push(
                    button(text(label).size(13))
                        .style(button::text)
                        .width(Length::Fill)
                        .on_press(Message::NewCodingAgent(agent.clone())),
                );
            }

            let dropdown = container(menu).style(container::bordered_box).width(200);

            bar = bar.push(row![Space::new().width(Length::Fill), dropdown].width(Length::Fill));
        }

        bar.into()
    }
}
