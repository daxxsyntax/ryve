// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

mod coding_agents;
mod screen;
mod widget;
mod workshop;

use std::path::PathBuf;

use iced::widget::{button, column, container, row, text, Space};
use iced::{Element, Length, Subscription, Task, Theme};
use uuid::Uuid;

use coding_agents::CodingAgent;
use screen::agents::AgentSession;
use workshop::Workshop;

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title("Forge")
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size((1400.0, 900.0))
        .run()
}

struct App {
    /// Available coding agents detected on PATH
    available_agents: Vec<CodingAgent>,
    /// All open workshops
    workshops: Vec<Workshop>,
    /// Index of the active workshop in `workshops`
    active_workshop: Option<usize>,
    /// Global terminal ID counter (unique across all workshops)
    next_terminal_id: u64,
}

#[derive(Debug, Clone)]
enum Message {
    /// Workshop-level tab bar
    SelectWorkshop(usize),
    CloseWorkshop(usize),
    NewWorkshopDialog,
    WorkshopDirPicked(Option<PathBuf>),

    /// Forwarded to the active workshop
    FileExplorer(screen::file_explorer::Message),
    Agents(screen::agents::Message),
    Bench(screen::bench::Message),
}

impl App {
    fn boot() -> (Self, Task<Message>) {
        let available_agents = coding_agents::detect_available();

        (
            Self {
                available_agents,
                workshops: Vec::new(),
                active_workshop: None,
                next_terminal_id: 1,
            },
            Task::none(),
        )
    }

    fn active_workshop(&self) -> Option<&Workshop> {
        self.active_workshop
            .and_then(|i| self.workshops.get(i))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // -- Workshop tab bar --
            Message::SelectWorkshop(idx) => {
                if idx < self.workshops.len() {
                    self.active_workshop = Some(idx);
                }
                Task::none()
            }
            Message::CloseWorkshop(idx) => {
                if idx < self.workshops.len() {
                    self.workshops.remove(idx);
                    // Adjust active index
                    if self.workshops.is_empty() {
                        self.active_workshop = None;
                    } else if let Some(active) = self.active_workshop {
                        if active >= self.workshops.len() {
                            self.active_workshop =
                                Some(self.workshops.len() - 1);
                        } else if active > idx {
                            self.active_workshop = Some(active - 1);
                        }
                    }
                }
                Task::none()
            }
            Message::NewWorkshopDialog => {
                Task::perform(pick_workshop_directory(), |path| {
                    Message::WorkshopDirPicked(path)
                })
            }
            Message::WorkshopDirPicked(Some(path)) => {
                let workshop = Workshop::new(path);
                self.workshops.push(workshop);
                self.active_workshop = Some(self.workshops.len() - 1);
                Task::none()
            }
            Message::WorkshopDirPicked(None) => Task::none(),

            // -- Forward to active workshop --
            Message::FileExplorer(_msg) => Task::none(),
            Message::Agents(msg) => {
                if let Some(idx) = self.active_workshop {
                    let ws = &mut self.workshops[idx];
                    match msg {
                        screen::agents::Message::SelectAgent(id) => {
                            if let Some(session) = ws
                                .agent_sessions
                                .iter()
                                .find(|s| s.id == id)
                            {
                                ws.bench.active_tab =
                                    Some(session.tab_id);
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::Bench(msg) => self.handle_bench_message(msg),
        }
    }

    fn handle_bench_message(
        &mut self,
        msg: screen::bench::Message,
    ) -> Task<Message> {
        // Terminal events can come from any workshop, so we need to
        // find the right one by terminal ID for terminal events.
        if let screen::bench::Message::TerminalEvent(
            iced_term::Event::BackendCall(id, ref cmd),
        ) = msg
        {
            // Find which workshop owns this terminal
            let ws_idx = self
                .workshops
                .iter()
                .position(|ws| ws.terminals.contains_key(&id));

            if let Some(idx) = ws_idx {
                let ws = &mut self.workshops[idx];
                if let Some(term) = ws.terminals.get_mut(&id) {
                    let action = term.handle(
                        iced_term::Command::ProxyToBackend(cmd.clone()),
                    );
                    ws.handle_terminal_action(id, action);
                }
            }
            return Task::none();
        }

        // All other bench messages go to the active workshop
        let Some(idx) = self.active_workshop else {
            return Task::none();
        };

        match msg {
            screen::bench::Message::SelectTab(id) => {
                self.workshops[idx].bench.active_tab = Some(id);
            }
            screen::bench::Message::CloseTab(id) => {
                let ws = &mut self.workshops[idx];
                ws.terminals.remove(&id);
                ws.agent_sessions.retain(|s| s.tab_id != id);
                ws.bench.close_tab(id);
            }
            screen::bench::Message::ToggleDropdown => {
                self.workshops[idx].bench.dropdown_open =
                    !self.workshops[idx].bench.dropdown_open;
            }
            screen::bench::Message::NewTerminal => {
                let next_id = &mut self.next_terminal_id;
                self.workshops[idx].spawn_terminal(
                    "Terminal".to_string(),
                    None,
                    next_id,
                );
            }
            screen::bench::Message::NewCodingAgent(agent) => {
                let title = agent.display_name.clone();
                let next_id = &mut self.next_terminal_id;
                let tab_id = self.workshops[idx].spawn_terminal(
                    title.clone(),
                    Some(&agent),
                    next_id,
                );
                self.workshops[idx].agent_sessions.push(AgentSession {
                    id: Uuid::new_v4(),
                    name: title,
                    tab_id,
                });
            }
            // TerminalEvent handled above
            screen::bench::Message::TerminalEvent(_) => {}
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let term_subs: Vec<_> = self
            .workshops
            .iter()
            .flat_map(|ws| ws.terminals.values())
            .map(|term| {
                term.subscription().map(|e| {
                    Message::Bench(screen::bench::Message::TerminalEvent(e))
                })
            })
            .collect();

        Subscription::batch(term_subs)
    }

    fn view(&self) -> Element<'_, Message> {
        let workshop_bar = self.view_workshop_bar();

        let content = if let Some(ws) = self.active_workshop() {
            self.view_workshop(ws)
        } else {
            self.view_welcome()
        };

        column![workshop_bar, content]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    /// Top-level tab bar for workshops.
    fn view_workshop_bar(&self) -> Element<'_, Message> {
        let mut tab_row = row![].spacing(2).padding([4, 4]);

        for (idx, ws) in self.workshops.iter().enumerate() {
            let is_active = self.active_workshop == Some(idx);
            let style = if is_active {
                button::primary
            } else {
                button::secondary
            };

            let tab_btn = button(text(ws.name()).size(13))
                .style(style)
                .padding([4, 12])
                .on_press(Message::SelectWorkshop(idx));

            let close_btn = button(text("\u{00D7}").size(13))
                .style(button::text)
                .padding([4, 6])
                .on_press(Message::CloseWorkshop(idx));

            tab_row = tab_row.push(row![tab_btn, close_btn].spacing(0));
        }

        let new_btn = button(text("+ New Workshop").size(13))
            .style(button::secondary)
            .padding([4, 12])
            .on_press(Message::NewWorkshopDialog);

        tab_row = tab_row
            .push(Space::new().width(Length::Fill))
            .push(new_btn);

        container(tab_row)
            .width(Length::Fill)
            .style(container::bordered_box)
            .into()
    }

    /// Welcome screen when no workshops are open.
    fn view_welcome(&self) -> Element<'_, Message> {
        container(
            column![
                text("Forge").size(40),
                text("Open a workshop to get started").size(16),
                button(text("Open Workshop...").size(14))
                    .style(button::primary)
                    .padding([8, 20])
                    .on_press(Message::NewWorkshopDialog),
            ]
            .spacing(16)
            .align_x(iced::Alignment::Center),
        )
        .center(Length::Fill)
        .into()
    }

    /// Full workshop view (sidebar + bench).
    fn view_workshop<'a>(
        &'a self,
        ws: &'a Workshop,
    ) -> Element<'a, Message> {
        // -- Left sidebar: files (top) + agents (bottom) --
        let files_panel = container(
            column![
                text("Files").size(14),
                text(ws.directory.display().to_string()).size(11),
            ]
            .spacing(4)
            .padding(10),
        )
        .width(Length::Fill)
        .height(Length::FillPortion(
            (ws.sidebar_split * 100.0) as u16,
        ))
        .style(container::bordered_box);

        let agents_panel =
            container(self.view_agents(ws))
                .width(Length::Fill)
                .height(Length::FillPortion(
                    ((1.0 - ws.sidebar_split) * 100.0) as u16,
                ))
                .style(container::bordered_box);

        let sidebar = column![files_panel, agents_panel]
            .width(250)
            .height(Length::Fill);

        // -- Right: bench (tabbed area) --
        let bench = self.view_bench(ws);

        row![sidebar, bench].height(Length::Fill).into()
    }

    fn view_agents<'a>(&'a self, ws: &'a Workshop) -> Element<'a, Message> {
        let mut content = column![text("Agents").size(14)]
            .spacing(4)
            .padding(10);

        if ws.agent_sessions.is_empty() {
            content =
                content.push(text("No active agents").size(12));
        } else {
            for session in &ws.agent_sessions {
                let btn = button(text(&session.name).size(12))
                    .style(button::text)
                    .on_press(Message::Agents(
                        screen::agents::Message::SelectAgent(session.id),
                    ));
                content = content.push(btn);
            }
        }

        content.into()
    }

    fn view_bench<'a>(&'a self, ws: &'a Workshop) -> Element<'a, Message> {
        let tab_bar = ws
            .bench
            .view_tab_bar(&self.available_agents)
            .map(Message::Bench);

        let content: Element<'a, Message> =
            if let Some(active_id) = ws.bench.active_tab {
                if let Some(term) = ws.terminals.get(&active_id) {
                    iced_term::TerminalView::show(term).map(|e| {
                        Message::Bench(
                            screen::bench::Message::TerminalEvent(e),
                        )
                    })
                } else {
                    container(text("Loading...").size(14))
                        .center(Length::Fill)
                        .into()
                }
            } else {
                container(
                    column![
                        text("Forge").size(32),
                        text(
                            "Press + to open a terminal or coding agent",
                        )
                        .size(14),
                    ]
                    .spacing(8)
                    .align_x(iced::Alignment::Center),
                )
                .center(Length::Fill)
                .into()
            };

        column![tab_bar, content]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

/// Open a native directory picker dialog.
async fn pick_workshop_directory() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title("Select Workshop Directory")
        .pick_folder()
        .await
        .map(|handle| handle.path().to_path_buf())
}
