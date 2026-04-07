// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Spark detail view — shown when a spark is selected in the workgraph panel.

use data::sparks::types::Spark;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Element, Length, Theme};

use crate::style::{self, Palette, FONT_BODY, FONT_HEADER, FONT_ICON, FONT_LABEL, FONT_SMALL};

// ── Messages ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    Back,
    /// Quick status cycle: pass (spark_id, new_status)
    CycleStatus(String, String),
}

// ── View ─────────────────────────────────────────────

pub fn view<'a>(spark: &'a Spark, pal: &Palette, has_bg: bool) -> Element<'a, Message> {
    let pal = *pal;

    // Back button + header row
    let back_btn = button(
        row![
            text("\u{2190}").size(FONT_ICON).color(pal.accent),
            text("Back").size(FONT_LABEL).color(pal.accent),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .style(button::text)
    .padding([4, 8])
    .on_press(Message::Back);

    let header = row![back_btn, Space::new().width(Length::Fill)]
        .spacing(4)
        .padding([8, 10]);

    // Title
    let title = text(&spark.title)
        .size(FONT_HEADER + 4.0)
        .color(pal.text_primary);

    let title_row = container(title).padding([4, 10]);

    // Status / Priority / Type badges
    let status_indicator = status_symbol(&spark.status);
    let status_color = status_color(&spark.status, &pal);
    let next = next_status_str(&spark.status);

    let status_pill = button(
        row![
            text(status_indicator).size(FONT_LABEL).color(status_color),
            text(format_status(&spark.status))
                .size(FONT_LABEL)
                .color(status_color),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .style(button::text)
    .padding([3, 8])
    .on_press(Message::CycleStatus(
        spark.id.clone(),
        next.to_string(),
    ));

    let priority_color = priority_color(spark.priority, &pal);
    let priority_pill = container(
        text(format!("P{}", spark.priority))
            .size(FONT_LABEL)
            .color(priority_color),
    )
    .padding([3, 8]);

    let type_pill = container(
        text(&spark.spark_type)
            .size(FONT_LABEL)
            .color(pal.text_secondary),
    )
    .padding([3, 8]);

    let badges = row![status_pill, priority_pill, type_pill]
        .spacing(6)
        .padding([4, 10])
        .align_y(iced::Alignment::Center);

    // Separator
    let sep = container(Space::new().height(1))
        .width(Length::Fill)
        .padding([0, 10])
        .style(move |_theme: &Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(pal.separator)),
            ..Default::default()
        });

    // Description
    let mut body = column![].spacing(12).padding([8, 10]);

    if !spark.description.is_empty() {
        body = body.push(
            column![
                text("Description")
                    .size(FONT_LABEL)
                    .color(pal.text_tertiary),
                text(&spark.description)
                    .size(FONT_BODY)
                    .color(pal.text_primary),
            ]
            .spacing(4),
        );
    }

    // Intent section — intent() returns an owned struct, so we extract
    // owned strings to avoid lifetime issues with the view tree.
    let intent = spark.intent();

    if let Some(problem) = intent.problem_statement {
        if !problem.is_empty() {
            body = body.push(
                column![
                    text("Problem Statement")
                        .size(FONT_LABEL)
                        .color(pal.text_tertiary),
                    text(problem).size(FONT_BODY).color(pal.text_primary),
                ]
                .spacing(4),
            );
        }
    }

    if !intent.invariants.is_empty() {
        let mut items = column![
            text("Invariants")
                .size(FONT_LABEL)
                .color(pal.text_tertiary),
        ]
        .spacing(2);
        for inv in intent.invariants {
            items = items.push(
                text(format!("\u{2022} {inv}"))
                    .size(FONT_BODY)
                    .color(pal.text_primary),
            );
        }
        body = body.push(items);
    }

    if !intent.non_goals.is_empty() {
        let mut items = column![
            text("Non-Goals")
                .size(FONT_LABEL)
                .color(pal.text_tertiary),
        ]
        .spacing(2);
        for ng in intent.non_goals {
            items = items.push(
                text(format!("\u{2022} {ng}"))
                    .size(FONT_BODY)
                    .color(pal.text_primary),
            );
        }
        body = body.push(items);
    }

    if !intent.acceptance_criteria.is_empty() {
        let mut items = column![
            text("Acceptance Criteria")
                .size(FONT_LABEL)
                .color(pal.text_tertiary),
        ]
        .spacing(2);
        for ac in intent.acceptance_criteria {
            items = items.push(
                text(format!("\u{2022} {ac}"))
                    .size(FONT_BODY)
                    .color(pal.text_primary),
            );
        }
        body = body.push(items);
    }

    // Metadata row: assignee, owner, dates
    let mut meta = column![].spacing(4).padding([8, 0]);

    if let Some(ref assignee) = spark.assignee {
        meta = meta.push(
            row![
                text("Assignee").size(FONT_SMALL).color(pal.text_tertiary),
                text(assignee).size(FONT_SMALL).color(pal.text_secondary),
            ]
            .spacing(8),
        );
    }

    if let Some(ref owner) = spark.owner {
        meta = meta.push(
            row![
                text("Owner").size(FONT_SMALL).color(pal.text_tertiary),
                text(owner).size(FONT_SMALL).color(pal.text_secondary),
            ]
            .spacing(8),
        );
    }

    meta = meta.push(
        row![
            text("Created").size(FONT_SMALL).color(pal.text_tertiary),
            text(&spark.created_at)
                .size(FONT_SMALL)
                .color(pal.text_secondary),
        ]
        .spacing(8),
    );

    meta = meta.push(
        row![
            text("Updated").size(FONT_SMALL).color(pal.text_tertiary),
            text(&spark.updated_at)
                .size(FONT_SMALL)
                .color(pal.text_secondary),
        ]
        .spacing(8),
    );

    body = body.push(meta);

    let content = column![
        header,
        title_row,
        badges,
        sep,
        scrollable(body).height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| style::glass_panel(&pal, has_bg))
        .into()
}

// ── Helpers ──────────────────────────────────────────

fn status_symbol(status: &str) -> &'static str {
    match status {
        "open" => "\u{25CB}",        // ○
        "in_progress" => "\u{25D4}", // ◔
        "blocked" => "\u{25A0}",     // ■
        "deferred" => "\u{25CC}",    // ◌
        "closed" => "\u{25CF}",      // ●
        _ => "\u{25CB}",
    }
}

fn status_color(status: &str, pal: &Palette) -> iced::Color {
    match status {
        "open" => pal.text_secondary,
        "in_progress" => pal.accent,
        "blocked" => pal.danger,
        "deferred" => pal.text_tertiary,
        "closed" => pal.text_tertiary,
        _ => pal.text_secondary,
    }
}

fn format_status(status: &str) -> &'static str {
    match status {
        "open" => "Open",
        "in_progress" => "In Progress",
        "blocked" => "Blocked",
        "deferred" => "Deferred",
        "closed" => "Closed",
        _ => "Unknown",
    }
}

fn priority_color(priority: i32, pal: &Palette) -> iced::Color {
    match priority {
        0 => pal.danger,        // P0 — critical
        1 => iced::Color {      // P1 — orange-ish
            r: 1.0,
            g: 0.6,
            b: 0.0,
            a: 1.0,
        },
        2 => pal.accent,       // P2 — normal
        3 => pal.text_secondary, // P3 — low
        _ => pal.text_tertiary,  // P4+ — minimal
    }
}

/// Cycle: open -> in_progress -> closed -> open
fn next_status_str(current: &str) -> &'static str {
    match current {
        "open" => "in_progress",
        "in_progress" => "closed",
        "closed" => "open",
        "blocked" => "open",
        "deferred" => "open",
        _ => "open",
    }
}
