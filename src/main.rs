// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

mod coding_agents;
mod screen;
mod widget;
mod workshop;

use std::path::PathBuf;

use data::sparks::types::Spark;
use iced::widget::{Space, button, column, container, row, stack, text};
use iced::{Color, Element, Length, Subscription, Task, Theme};
use uuid::Uuid;

use coding_agents::CodingAgent;
use screen::agents::AgentSession;
use screen::file_explorer;
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

#[derive(Clone)]
enum Message {
    /// Workshop-level tab bar
    SelectWorkshop(usize),
    CloseWorkshop(usize),
    NewWorkshopDialog,
    WorkshopDirPicked(Option<PathBuf>),

    /// Workshop .forge/ initialized
    WorkshopReady {
        idx: usize,
        pool: sqlx::SqlitePool,
        config: data::forge_dir::WorkshopConfig,
        custom_agents: Vec<data::forge_dir::AgentDef>,
        agent_context: Option<String>,
    },
    /// Sparks loaded from DB
    SparksLoaded(usize, Vec<Spark>),
    /// File tree scanned for a workshop
    FilesScanned(usize, file_explorer::Message),

    /// Forwarded to the active workshop
    FileExplorer(screen::file_explorer::Message),
    Agents(screen::agents::Message),
    Bench(screen::bench::Message),
    Sparks(screen::sparks::Message),
    Background(screen::background_picker::Message),
    OpenBackgroundPicker,

    /// Background image loaded from disk
    BackgroundLoaded(usize, Option<Vec<u8>>),
    /// Unsplash thumbnail bytes loaded
    UnsplashThumbnailLoaded(String, Vec<u8>),
    /// Unsplash photo downloaded to disk
    UnsplashDownloaded {
        filename: String,
        photographer: String,
        photographer_url: String,
    },
    /// Local file copied to backgrounds dir
    LocalFileCopied(String),
    /// Background config saved
    BackgroundConfigSaved,
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelectWorkshop(i) => write!(f, "SelectWorkshop({i})"),
            Self::CloseWorkshop(i) => write!(f, "CloseWorkshop({i})"),
            Self::NewWorkshopDialog => write!(f, "NewWorkshopDialog"),
            Self::WorkshopDirPicked(p) => write!(f, "WorkshopDirPicked({p:?})"),
            Self::WorkshopReady { idx, .. } => write!(f, "WorkshopReady({idx})"),
            Self::SparksLoaded(i, s) => write!(f, "SparksLoaded({i}, {} sparks)", s.len()),
            Self::FilesScanned(i, _) => write!(f, "FilesScanned({i})"),
            Self::FileExplorer(m) => write!(f, "FileExplorer({m:?})"),
            Self::Agents(m) => write!(f, "Agents({m:?})"),
            Self::Bench(m) => write!(f, "Bench({m:?})"),
            Self::Sparks(m) => write!(f, "Sparks({m:?})"),
            Self::Background(m) => write!(f, "Background({m:?})"),
            Self::OpenBackgroundPicker => write!(f, "OpenBackgroundPicker"),
            Self::BackgroundLoaded(i, _) => write!(f, "BackgroundLoaded({i})"),
            Self::UnsplashThumbnailLoaded(id, _) => {
                write!(f, "UnsplashThumbnailLoaded({id})")
            }
            Self::UnsplashDownloaded { filename, .. } => {
                write!(f, "UnsplashDownloaded({filename})")
            }
            Self::LocalFileCopied(name) => write!(f, "LocalFileCopied({name})"),
            Self::BackgroundConfigSaved => write!(f, "BackgroundConfigSaved"),
        }
    }
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
        self.active_workshop.and_then(|i| self.workshops.get(i))
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
                            self.active_workshop = Some(self.workshops.len() - 1);
                        } else if active > idx {
                            self.active_workshop = Some(active - 1);
                        }
                    }
                }
                Task::none()
            }
            Message::NewWorkshopDialog => Task::perform(pick_workshop_directory(), |path| {
                Message::WorkshopDirPicked(path)
            }),
            Message::WorkshopDirPicked(Some(path)) => {
                let workshop = Workshop::new(path.clone());
                self.workshops.push(workshop);
                let idx = self.workshops.len() - 1;
                self.active_workshop = Some(idx);

                // Async: init .forge/ dir, DB, config, agents, context
                Task::perform(workshop::init_workshop(path), move |result| match result {
                    Ok(init) => Message::WorkshopReady {
                        idx,
                        pool: init.pool,
                        config: init.config,
                        custom_agents: init.custom_agents,
                        agent_context: init.agent_context,
                    },
                    Err(e) => {
                        log::error!("Failed to init workshop: {e}");
                        Message::WorkshopDirPicked(None)
                    }
                })
            }
            Message::WorkshopDirPicked(None) => Task::none(),

            Message::WorkshopReady {
                idx,
                pool,
                config,
                custom_agents,
                agent_context,
            } => {
                if let Some(ws) = self.workshops.get_mut(idx) {
                    ws.sparks_db = Some(pool.clone());
                    ws.config = config;
                    ws.custom_agents = custom_agents;
                    ws.agent_context = agent_context;

                    // Load sparks + scan file tree in parallel
                    let ws_id = ws.id.to_string();
                    let dir = ws.directory.clone();
                    let sparks_task = Task::perform(load_sparks(pool, ws_id), move |sparks| {
                        Message::SparksLoaded(idx, sparks)
                    });
                    let ignore = ws.config.explorer.ignore.clone();
                    let scan_task = Task::perform(
                        file_explorer::scan_directory(dir, ignore),
                        move |(tree, statuses, branch)| {
                            Message::FilesScanned(
                                idx,
                                file_explorer::Message::TreeLoaded(tree, statuses, branch),
                            )
                        },
                    );
                    // Optionally load background image
                    let bg_task = if let Some(ref filename) = ws.config.background.image {
                        let path = ws.forge_dir.backgrounds_dir().join(filename);
                        Task::perform(
                            async move { tokio::fs::read(&path).await.ok() },
                            move |bytes| Message::BackgroundLoaded(idx, bytes),
                        )
                    } else {
                        Task::none()
                    };

                    return Task::batch([sparks_task, scan_task, bg_task]);
                }
                Task::none()
            }
            Message::SparksLoaded(idx, sparks) => {
                if let Some(ws) = self.workshops.get_mut(idx) {
                    ws.sparks = sparks;
                }
                Task::none()
            }

            Message::FilesScanned(idx, msg) => {
                if let Some(ws) = self.workshops.get_mut(idx) {
                    if let file_explorer::Message::TreeLoaded(tree, statuses, branch) = msg {
                        ws.file_explorer.tree = tree;
                        ws.file_explorer.git_statuses = statuses;
                        ws.file_explorer.branch = branch;
                        // Start collapsed — user expands directories on demand
                    }
                }
                Task::none()
            }

            // -- Forward to active workshop --
            Message::FileExplorer(msg) => {
                let Some(idx) = self.active_workshop else {
                    return Task::none();
                };
                let ws = &mut self.workshops[idx];
                match msg {
                    file_explorer::Message::SelectFile(ref path) => {
                        ws.file_explorer.selected = Some(path.clone());
                    }
                    file_explorer::Message::ToggleDirectory(ref path) => {
                        if ws.file_explorer.expanded.contains(path) {
                            ws.file_explorer.expanded.remove(path);
                        } else {
                            ws.file_explorer.expanded.insert(path.clone());
                        }
                    }
                    file_explorer::Message::Refresh => {
                        let dir = ws.directory.clone();
                        let ignore = ws.config.explorer.ignore.clone();
                        return Task::perform(
                            file_explorer::scan_directory(dir, ignore),
                            move |(tree, statuses, branch)| {
                                Message::FilesScanned(
                                    idx,
                                    file_explorer::Message::TreeLoaded(tree, statuses, branch),
                                )
                            },
                        );
                    }
                    file_explorer::Message::TreeLoaded(..) => {
                        // Handled via FilesScanned
                    }
                    file_explorer::Message::LinkSpark(ref _path) => {
                        // TODO: open spark link dialog for this path
                    }
                }
                Task::none()
            }
            Message::Agents(msg) => {
                if let Some(idx) = self.active_workshop {
                    let ws = &mut self.workshops[idx];
                    match msg {
                        screen::agents::Message::SelectAgent(id) => {
                            if let Some(session) = ws.agent_sessions.iter().find(|s| s.id == id) {
                                ws.bench.active_tab = Some(session.tab_id);
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::Bench(msg) => self.handle_bench_message(msg),
            Message::Sparks(msg) => {
                match msg {
                    screen::sparks::Message::Refresh => {
                        if let Some(idx) = self.active_workshop {
                            if let Some(ws) = self.workshops.get(idx) {
                                if let Some(ref pool) = ws.sparks_db {
                                    let pool = pool.clone();
                                    let ws_id = ws.id.to_string();
                                    return Task::perform(
                                        load_sparks(pool, ws_id),
                                        move |sparks| Message::SparksLoaded(idx, sparks),
                                    );
                                }
                            }
                        }
                    }
                    screen::sparks::Message::SelectSpark(_id) => {
                        // TODO: open spark detail view
                    }
                }
                Task::none()
            }

            // ── Background ───────────────────────────────
            Message::OpenBackgroundPicker => {
                if let Some(idx) = self.active_workshop {
                    self.workshops[idx].background_picker.open = true;
                }
                Task::none()
            }
            Message::Background(msg) => self.handle_background_message(msg),
            Message::BackgroundLoaded(idx, Some(bytes)) => {
                if let Some(ws) = self.workshops.get_mut(idx) {
                    ws.background_handle =
                        Some(iced::widget::image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
            Message::BackgroundLoaded(_, None) => Task::none(),
            Message::UnsplashThumbnailLoaded(id, bytes) => {
                if let Some(idx) = self.active_workshop {
                    let ws = &mut self.workshops[idx];
                    ws.background_picker
                        .thumbnails
                        .insert(id, iced::widget::image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
            Message::UnsplashDownloaded {
                filename,
                photographer,
                photographer_url,
            } => {
                let Some(idx) = self.active_workshop else {
                    return Task::none();
                };
                let ws = &mut self.workshops[idx];
                ws.config.background.image = Some(filename.clone());
                ws.config.background.unsplash_photographer = Some(photographer);
                ws.config.background.unsplash_photographer_url = Some(photographer_url);
                ws.background_picker.open = false;
                ws.background_picker.loading = false;

                // Load the image + save config
                let bg_dir = ws.forge_dir.backgrounds_dir();
                let path = bg_dir.join(&filename);
                let forge_dir = ws.forge_dir.clone();
                let config = ws.config.clone();
                Task::batch([
                    Task::perform(
                        async move { tokio::fs::read(&path).await.ok() },
                        move |bytes| Message::BackgroundLoaded(idx, bytes),
                    ),
                    Task::perform(
                        async move { data::forge_dir::save_config(&forge_dir, &config).await.ok(); },
                        |_| Message::BackgroundConfigSaved,
                    ),
                ])
            }
            Message::LocalFileCopied(filename) => {
                let Some(idx) = self.active_workshop else {
                    return Task::none();
                };
                let ws = &mut self.workshops[idx];
                ws.config.background.image = Some(filename.clone());
                ws.config.background.unsplash_photographer = None;
                ws.config.background.unsplash_photographer_url = None;
                ws.background_picker.open = false;

                let bg_dir = ws.forge_dir.backgrounds_dir();
                let path = bg_dir.join(&filename);
                let forge_dir = ws.forge_dir.clone();
                let config = ws.config.clone();
                Task::batch([
                    Task::perform(
                        async move { tokio::fs::read(&path).await.ok() },
                        move |bytes| Message::BackgroundLoaded(idx, bytes),
                    ),
                    Task::perform(
                        async move { data::forge_dir::save_config(&forge_dir, &config).await.ok(); },
                        |_| Message::BackgroundConfigSaved,
                    ),
                ])
            }
            Message::BackgroundConfigSaved => Task::none(),
        }
    }

    fn handle_bench_message(&mut self, msg: screen::bench::Message) -> Task<Message> {
        // Terminal events can come from any workshop, so we need to
        // find the right one by terminal ID for terminal events.
        if let screen::bench::Message::TerminalEvent(iced_term::Event::BackendCall(id, ref cmd)) =
            msg
        {
            // Find which workshop owns this terminal
            let ws_idx = self
                .workshops
                .iter()
                .position(|ws| ws.terminals.contains_key(&id));

            if let Some(idx) = ws_idx {
                let ws = &mut self.workshops[idx];
                if let Some(term) = ws.terminals.get_mut(&id) {
                    let action = term.handle(iced_term::Command::ProxyToBackend(cmd.clone()));
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
                self.workshops[idx].bench.dropdown_open = !self.workshops[idx].bench.dropdown_open;
            }
            screen::bench::Message::NewTerminal => {
                let next_id = &mut self.next_terminal_id;
                self.workshops[idx].spawn_terminal("Terminal".to_string(), None, next_id);
            }
            screen::bench::Message::NewCodingAgent(agent) => {
                let title = agent.display_name.clone();
                let next_id = &mut self.next_terminal_id;
                let tab_id =
                    self.workshops[idx].spawn_terminal(title.clone(), Some(&agent), next_id);
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

    fn handle_background_message(
        &mut self,
        msg: screen::background_picker::Message,
    ) -> Task<Message> {
        let Some(idx) = self.active_workshop else {
            return Task::none();
        };
        let ws = &mut self.workshops[idx];

        match msg {
            screen::background_picker::Message::Close => {
                ws.background_picker.open = false;
                Task::none()
            }
            screen::background_picker::Message::PickLocalFile => {
                let bg_dir = ws.forge_dir.backgrounds_dir();
                Task::perform(
                    async move {
                        let file = rfd::AsyncFileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
                            .pick_file()
                            .await?;
                        let bytes = file.read().await;
                        let name = file.file_name();
                        let dest = bg_dir.join(&name);
                        tokio::fs::write(&dest, &bytes).await.ok()?;
                        Some(name)
                    },
                    |name| match name {
                        Some(name) => Message::LocalFileCopied(name),
                        None => Message::BackgroundConfigSaved, // no-op
                    },
                )
            }
            screen::background_picker::Message::QueryChanged(q) => {
                ws.background_picker.query = q;
                Task::none()
            }
            screen::background_picker::Message::Search => {
                let query = ws.background_picker.query.clone();
                if query.is_empty() {
                    return Task::none();
                }
                ws.background_picker.loading = true;
                ws.background_picker.results.clear();
                ws.background_picker.thumbnails.clear();

                let api_key = std::env::var("UNSPLASH_ACCESS_KEY").unwrap_or_default();
                Task::perform(
                    async move { data::unsplash::search(&api_key, &query, 1).await },
                    |result| match result {
                        Ok(sr) => Message::Background(screen::background_picker::Message::SearchResults(sr.photos)),
                        Err(e) => {
                            log::error!("Unsplash search failed: {e}");
                            Message::Background(screen::background_picker::Message::SearchResults(Vec::new()))
                        }
                    },
                )
            }
            screen::background_picker::Message::SearchResults(photos) => {
                ws.background_picker.loading = false;
                ws.background_picker.results = photos.clone();

                // Kick off thumbnail downloads
                let tasks: Vec<_> = photos
                    .into_iter()
                    .map(|photo| {
                        let id = photo.id.clone();
                        let url = photo.thumb_url.clone();
                        Task::perform(
                            async move { data::unsplash::fetch_thumbnail_bytes(&url).await },
                            move |result| match result {
                                Ok(bytes) => Message::UnsplashThumbnailLoaded(id.clone(), bytes),
                                Err(_) => Message::BackgroundConfigSaved, // no-op
                            },
                        )
                    })
                    .collect();

                Task::batch(tasks)
            }
            screen::background_picker::Message::ThumbnailLoaded(_, _) => {
                // Handled via UnsplashThumbnailLoaded at the top level
                Task::none()
            }
            screen::background_picker::Message::SelectPhoto(photo) => {
                ws.background_picker.loading = true;
                let api_key = std::env::var("UNSPLASH_ACCESS_KEY").unwrap_or_default();
                let bg_dir = ws.forge_dir.backgrounds_dir();
                let photographer = photo.photographer.clone();
                let photographer_url = photo.photographer_url.clone();

                Task::perform(
                    async move { data::unsplash::download(&api_key, &photo, &bg_dir).await },
                    move |result| match result {
                        Ok(filename) => Message::UnsplashDownloaded {
                            filename,
                            photographer: photographer.clone(),
                            photographer_url: photographer_url.clone(),
                        },
                        Err(e) => {
                            log::error!("Unsplash download failed: {e}");
                            Message::BackgroundConfigSaved // no-op
                        }
                    },
                )
            }
            screen::background_picker::Message::RemoveBackground => {
                ws.config.background.image = None;
                ws.config.background.unsplash_photographer = None;
                ws.config.background.unsplash_photographer_url = None;
                ws.background_handle = None;
                ws.background_picker.open = false;

                let forge_dir = ws.forge_dir.clone();
                let config = ws.config.clone();
                Task::perform(
                    async move { data::forge_dir::save_config(&forge_dir, &config).await.ok(); },
                    |_| Message::BackgroundConfigSaved,
                )
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let term_subs: Vec<_> = self
            .workshops
            .iter()
            .flat_map(|ws| ws.terminals.values())
            .map(|term| {
                term.subscription()
                    .map(|e| Message::Bench(screen::bench::Message::TerminalEvent(e)))
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

        tab_row = tab_row.push(Space::new().width(Length::Fill));

        // Background picker button (only when a workshop is active)
        if self.active_workshop.is_some() {
            tab_row = tab_row.push(
                button(text("\u{1F5BC}").size(13))
                    .style(button::text)
                    .padding([4, 8])
                    .on_press(Message::OpenBackgroundPicker),
            );
        }

        tab_row = tab_row.push(new_btn);

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

    /// Full workshop view (sidebar + bench), with optional background image.
    fn view_workshop<'a>(&'a self, ws: &'a Workshop) -> Element<'a, Message> {
        let has_bg = ws.background_handle.is_some();

        // Panel style: transparent when background is set, bordered_box otherwise
        let panel_style: fn(&Theme) -> container::Style = if has_bg {
            |_theme: &Theme| container::Style {
                background: None,
                border: iced::Border {
                    color: Color::from_rgba(1.0, 1.0, 1.0, 0.1),
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        } else {
            container::bordered_box
        };

        // -- Left sidebar: files (top) + agents (bottom) --
        let files_view =
            file_explorer::view(&ws.file_explorer, &ws.directory).map(Message::FileExplorer);

        let files_panel = container(files_view)
            .width(Length::Fill)
            .height(Length::FillPortion((ws.sidebar_split() * 100.0) as u16))
            .style(panel_style);

        let agents_panel = container(self.view_agents(ws))
            .width(Length::Fill)
            .height(Length::FillPortion(
                ((1.0 - ws.sidebar_split()) * 100.0) as u16,
            ))
            .style(panel_style);

        let sidebar = column![files_panel, agents_panel]
            .width(ws.sidebar_width())
            .height(Length::Fill);

        // -- Center: bench (tabbed area) --
        let bench = self.view_bench(ws);

        // -- Right: sparks panel --
        let sparks_panel = screen::sparks::view(&ws.sparks).map(Message::Sparks);

        let sparks_col = container(sparks_panel)
            .width(ws.sparks_width())
            .height(Length::Fill);

        let workshop_content: Element<'a, Message> =
            row![sidebar, bench, sparks_col].height(Length::Fill).into();

        // Layer background image behind content
        let mut layers: Vec<Element<'a, Message>> = Vec::new();

        if let Some(ref handle) = ws.background_handle {
            layers.push(
                iced::widget::image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(iced::ContentFit::Cover)
                    .into(),
            );

            // Dim overlay so UI stays readable
            let opacity = ws.config.background.dim_opacity;
            layers.push(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_theme: &Theme| container::Style {
                        background: Some(iced::Background::Color(Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: opacity,
                        })),
                        ..Default::default()
                    })
                    .into(),
            );
        }

        layers.push(workshop_content);

        // Background picker modal overlay
        if ws.background_picker.open {
            let has_bg = ws.config.background.image.is_some();
            layers.push(
                screen::background_picker::view(&ws.background_picker, has_bg)
                    .map(Message::Background),
            );
        }

        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_agents<'a>(&'a self, ws: &'a Workshop) -> Element<'a, Message> {
        let mut content = column![text("Agents").size(14)].spacing(4).padding(10);

        if ws.agent_sessions.is_empty() {
            content = content.push(text("No active agents").size(12));
        } else {
            for session in &ws.agent_sessions {
                let btn = button(text(&session.name).size(12))
                    .style(button::text)
                    .on_press(Message::Agents(screen::agents::Message::SelectAgent(
                        session.id,
                    )));
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

        let content: Element<'a, Message> = if let Some(active_id) = ws.bench.active_tab {
            if let Some(term) = ws.terminals.get(&active_id) {
                iced_term::TerminalView::show(term)
                    .map(|e| Message::Bench(screen::bench::Message::TerminalEvent(e)))
                    .into()
            } else {
                container(text("Loading...").size(14))
                    .center(Length::Fill)
                    .into()
            }
        } else {
            container(
                column![
                    text("Forge").size(32),
                    text("Press + to open a terminal or coding agent",).size(14),
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

/// Load all sparks for a workshop from the database.
async fn load_sparks(pool: sqlx::SqlitePool, workshop_id: String) -> Vec<Spark> {
    data::sparks::spark_repo::list(
        &pool,
        data::sparks::types::SparkFilter {
            workshop_id: Some(workshop_id),
            ..Default::default()
        },
    )
    .await
    .unwrap_or_default()
}
