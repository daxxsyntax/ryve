// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! File explorer panel — displays project tree with git/worktree awareness.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use data::git::FileStatus;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Color, Element, Length};

// ── Messages ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    SelectFile(PathBuf),
    ToggleDirectory(PathBuf),
    Refresh,
    TreeLoaded(Vec<FileNode>, HashMap<PathBuf, FileStatus>, Option<String>),
    LinkSpark(PathBuf),
}

// ── File tree types ───────────────────────────────────

#[derive(Debug, Clone)]
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub kind: NodeKind,
    pub children: Vec<FileNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Directory,
}

// ── State ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FileExplorerState {
    /// Root nodes of the file tree.
    pub tree: Vec<FileNode>,
    /// Directories currently expanded.
    pub expanded: HashSet<PathBuf>,
    /// Git status per file (repo-relative paths).
    pub git_statuses: HashMap<PathBuf, FileStatus>,
    /// Current git branch name.
    pub branch: Option<String>,
    /// Currently selected file path.
    pub selected: Option<PathBuf>,
}

impl FileExplorerState {
    pub fn new() -> Self {
        Self {
            tree: Vec::new(),
            expanded: HashSet::new(),
            git_statuses: HashMap::new(),
            branch: None,
            selected: None,
        }
    }
}

// ── Async scanning ────────────────────────────────────

/// Scan a directory and build a file tree + git statuses.
/// Call this via `Task::perform`.
pub async fn scan_directory(
    root: PathBuf,
    ignore: Vec<String>,
) -> (Vec<FileNode>, HashMap<PathBuf, FileStatus>, Option<String>) {
    let ignore = std::sync::Arc::new(ignore);
    let tree = build_tree(root.clone(), 0, ignore).await;

    let repo = data::git::Repository::new(&root);
    let is_repo = data::git::Repository::is_repo(&root).await;

    let (statuses, branch) = if is_repo {
        let statuses = repo.file_statuses().await.unwrap_or_default();
        let branch = repo.current_branch().await.ok();
        (statuses, branch)
    } else {
        (HashMap::new(), None)
    };

    (tree, statuses, branch)
}

/// Collected entry info for sorting before recursion.
struct RawEntry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

/// Recursively build the file tree, skipping ignored entries.
fn build_tree(
    path: PathBuf,
    depth: u32,
    ignore: std::sync::Arc<Vec<String>>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<FileNode>> + Send>> {
    Box::pin(async move {
        let mut nodes = Vec::new();

        if depth > 12 {
            return nodes;
        }

        let mut entries = match tokio::fs::read_dir(&path).await {
            Ok(rd) => rd,
            Err(_) => return nodes,
        };

        // Collect entries with their metadata
        let mut raw: Vec<RawEntry> = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(".#") || ignore.iter().any(|p| p == &name) {
                continue;
            }
            let is_dir = entry
                .file_type()
                .await
                .map(|ft| ft.is_dir())
                .unwrap_or(false);
            raw.push(RawEntry {
                path: entry.path(),
                name,
                is_dir,
            });
        }

        // Sort: directories first, then alphabetical (case-insensitive)
        raw.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase()),
        });

        for entry in raw {
            if entry.is_dir {
                let children = build_tree(entry.path.clone(), depth + 1, ignore.clone()).await;
                nodes.push(FileNode {
                    path: entry.path,
                    name: entry.name,
                    kind: NodeKind::Directory,
                    children,
                });
            } else {
                nodes.push(FileNode {
                    path: entry.path,
                    name: entry.name,
                    kind: NodeKind::File,
                    children: Vec::new(),
                });
            }
        }

        nodes
    })
}

// ── View ──────────────────────────────────────────────

/// Render the file explorer panel.
pub fn view<'a>(state: &'a FileExplorerState, root: &'a Path) -> Element<'a, Message> {
    let branch_label = state.branch.as_deref().unwrap_or("no branch");

    let header = row![
        text("Files").size(14),
        Space::new().width(Length::Fill),
        text(branch_label)
            .size(10)
            .color(Color::from_rgb(0.6, 0.6, 0.7)),
        button(text("\u{21BB}").size(13))
            .style(button::text)
            .padding([2, 6])
            .on_press(Message::Refresh),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center)
    .padding([8, 10]);

    let mut items: Vec<Element<'a, Message>> = Vec::new();

    if state.tree.is_empty() {
        items.push(
            container(text("Scanning...").size(11))
                .padding([8, 10])
                .into(),
        );
    } else {
        for node in &state.tree {
            collect_nodes(node, root, state, 0, &mut items);
        }
    }

    let list = iced::widget::Column::with_children(items)
        .spacing(0)
        .padding([0, 10]);

    let content = column![
        header,
        scrollable(list)
            .height(Length::Fill)
            .style(scrollable::default)
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    content.into()
}

fn collect_nodes<'a>(
    node: &'a FileNode,
    root: &'a Path,
    state: &'a FileExplorerState,
    depth: u16,
    items: &mut Vec<Element<'a, Message>>,
) {
    let indent = (depth as f32) * 16.0;
    let is_expanded = state.expanded.contains(&node.path);
    let is_selected = state.selected.as_ref() == Some(&node.path);

    // Determine git status color for this entry
    let rel_path = node.path.strip_prefix(root).unwrap_or(&node.path);
    let git_color = file_git_color(rel_path, &node.kind, &state.git_statuses);

    let icon = file_icon(&node.name, &node.kind, is_expanded);

    let label = row![
        Space::new().width(indent),
        text(icon).size(13).color(git_color),
        text(&node.name).size(12).color(git_color),
        Space::new().width(Length::Fill),
        // Spark link button
        button(text("\u{26A1}").size(9))
            .style(button::text)
            .padding([1, 3])
            .on_press(Message::LinkSpark(node.path.clone())),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let msg = match node.kind {
        NodeKind::Directory => Message::ToggleDirectory(node.path.clone()),
        NodeKind::File => Message::SelectFile(node.path.clone()),
    };

    let style = if is_selected {
        button::primary
    } else {
        button::text
    };

    let btn = button(label)
        .style(style)
        .width(Length::Fill)
        .padding([2, 4])
        .on_press(msg);

    items.push(btn.into());

    // Render children if directory is expanded
    if node.kind == NodeKind::Directory && is_expanded {
        for child in &node.children {
            collect_nodes(child, root, state, depth + 1, items);
        }
    }
}

// ── Git status colors ─────────────────────────────────

/// Determine the display color for a file/directory based on git status.
/// Directories inherit the "most important" status of their children.
fn file_git_color(
    rel_path: &Path,
    kind: &NodeKind,
    statuses: &HashMap<PathBuf, FileStatus>,
) -> Color {
    if *kind == NodeKind::File {
        if let Some(status) = statuses.get(rel_path) {
            return status_color(*status);
        }
        // Default: normal text
        return Color::from_rgb(0.78, 0.78, 0.78);
    }

    // Directory: check if any child file has a git status
    let dir_prefix = rel_path.to_string_lossy();
    let mut most_important: Option<FileStatus> = None;

    for (path, status) in statuses {
        let path_str = path.to_string_lossy();
        if path_str.starts_with(dir_prefix.as_ref()) && path_str.len() > dir_prefix.len() {
            most_important = Some(match most_important {
                None => *status,
                Some(prev) => higher_priority_status(prev, *status),
            });
        }
    }

    match most_important {
        Some(status) => status_color(status),
        None => Color::from_rgb(0.78, 0.78, 0.78),
    }
}

fn status_color(status: FileStatus) -> Color {
    match status {
        FileStatus::Modified => Color::from_rgb(0.90, 0.80, 0.30), // yellow
        FileStatus::Added => Color::from_rgb(0.40, 0.85, 0.45),    // green
        FileStatus::Deleted => Color::from_rgb(0.90, 0.35, 0.35),  // red
        FileStatus::Renamed => Color::from_rgb(0.55, 0.75, 0.95),  // blue
        FileStatus::Copied => Color::from_rgb(0.55, 0.75, 0.95),   // blue
        FileStatus::Untracked => Color::from_rgb(0.55, 0.55, 0.55), // dim grey
        FileStatus::Ignored => Color::from_rgb(0.40, 0.40, 0.40),  // darker grey
        FileStatus::Conflicted => Color::from_rgb(1.0, 0.45, 0.20), // orange
    }
}

fn higher_priority_status(a: FileStatus, b: FileStatus) -> FileStatus {
    fn rank(s: FileStatus) -> u8 {
        match s {
            FileStatus::Conflicted => 7,
            FileStatus::Deleted => 6,
            FileStatus::Added => 5,
            FileStatus::Modified => 4,
            FileStatus::Renamed => 3,
            FileStatus::Copied => 2,
            FileStatus::Untracked => 1,
            FileStatus::Ignored => 0,
        }
    }
    if rank(b) > rank(a) { b } else { a }
}

// ── File icons ────────────────────────────────────────

/// Return an icon string for a file based on its name/extension.
/// Uses Unicode symbols for a clean, monospace-friendly look.
/// Branded icons for well-known file types.
fn file_icon(name: &str, kind: &NodeKind, is_expanded: bool) -> &'static str {
    if *kind == NodeKind::Directory {
        return if is_expanded {
            "\u{25BE} \u{1F4C2}" // ▾ 📂
        } else {
            "\u{25B8} \u{1F4C1}" // ▸ 📁
        };
    }

    let lower = name.to_ascii_lowercase();

    // Exact filename matches (branded / special)
    match lower.as_str() {
        "cargo.toml" | "cargo.lock" => return "\u{1F980}", // 🦀 Rust/Cargo
        "package.json" | "package-lock.json" => return "\u{1F4E6}", // 📦 npm
        "tsconfig.json" => return "\u{1F535}",             // 🔵 TypeScript config
        "dockerfile" | "docker-compose.yml" | "docker-compose.yaml" => return "\u{1F433}", // 🐳 Docker
        ".gitignore" | ".gitattributes" | ".gitmodules" => return "\u{1F500}",             // 🔀 Git
        "makefile" | "justfile" => return "\u{2699}\u{FE0F}", // ⚙️ Build
        "license" | "license.md" | "license.txt" => return "\u{1F4DC}", // 📜 License
        "readme.md" | "readme.txt" | "readme" => return "\u{1F4D6}", // 📖 Readme
        ".env" | ".env.local" | ".env.production" => return "\u{1F510}", // 🔐 Env/secrets
        "flake.nix" | "flake.lock" => return "\u{2744}\u{FE0F}", // ❄️ Nix
        _ => {}
    }

    // Extension-based icons
    let ext = lower.rsplit('.').next().unwrap_or("");
    match ext {
        // Rust
        "rs" => "\u{1F980}", // 🦀
        // TypeScript / JavaScript
        "ts" | "tsx" => "\u{1F535}", // 🔵
        "js" | "jsx" => "\u{1F7E1}", // 🟡
        // Python
        "py" => "\u{1F40D}", // 🐍
        // Go
        "go" => "\u{1F439}", // 🐹
        // Swift
        "swift" => "\u{1F426}", // 🐦
        // Ruby
        "rb" => "\u{1F48E}", // 💎
        // Shell
        "sh" | "bash" | "zsh" | "fish" => "\u{1F41A}", // 🐚
        // Markdown
        "md" | "mdx" => "\u{1F4DD}", // 📝
        // Config / data
        "toml" | "yaml" | "yml" | "json" | "json5" | "jsonc" => "\u{2699}\u{FE0F}", // ⚙️
        // HTML / CSS
        "html" | "htm" => "\u{1F310}",                   // 🌐
        "css" | "scss" | "sass" | "less" => "\u{1F3A8}", // 🎨
        // SQL
        "sql" => "\u{1F5C3}\u{FE0F}", // 🗃️
        // Images
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "webp" => "\u{1F5BC}\u{FE0F}", // 🖼️
        // Fonts
        "ttf" | "otf" | "woff" | "woff2" => "\u{1F524}", // 🔤
        // Archives
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" => "\u{1F4E6}", // 📦
        // Lock files
        "lock" => "\u{1F512}", // 🔒
        // C / C++
        "c" | "h" => "\u{1F1E8}",                   // 🇨
        "cpp" | "cxx" | "cc" | "hpp" => "\u{2795}", // ➕
        // Java / Kotlin
        "java" => "\u{2615}",        // ☕
        "kt" | "kts" => "\u{1F1F0}", // 🇰
        // XML
        "xml" | "xsl" | "xslt" => "\u{1F4C4}", // 📄
        // Text
        "txt" | "text" | "log" => "\u{1F4C3}", // 📃
        // PDF
        "pdf" => "\u{1F4D5}", // 📕
        // Lua
        "lua" => "\u{1F319}", // 🌙
        // Zig
        "zig" => "\u{26A1}", // ⚡
        // Elixir
        "ex" | "exs" => "\u{1F52E}", // 🔮
        // Default
        _ => "\u{1F4C4}", // 📄
    }
}
