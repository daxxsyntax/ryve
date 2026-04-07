// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Workgraph panel — displays and manages sparks for the active workshop.

use data::sparks::types::Spark;
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Theme};

use crate::style::{
    self, FONT_BODY, FONT_HEADER, FONT_ICON, FONT_ICON_SM, FONT_LABEL, FONT_SMALL, Palette,
};

// ── State ────────────────────────────────────────────

/// Inline create form state, held on the Workshop.
#[derive(Debug, Clone, Default)]
pub struct CreateForm {
    pub title: String,
    pub visible: bool,
}

// ── Messages ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    SelectSpark(String),
    Refresh,
    ShowCreateForm,
    CreateFormTitleChanged(String),
    SubmitNewSpark,
    CancelCreate,
    /// Quick status cycle: open → in_progress → closed
    CycleStatus(String, String),
}

// ── View ─────────────────────────────────────────────

pub fn view<'a>(
    sparks: &'a [Spark],
    pal: &Palette,
    has_bg: bool,
    create_form: &'a CreateForm,
) -> Element<'a, Message> {
    let pal = *pal;

    let header = row![
        text("Workgraph").size(FONT_HEADER).color(pal.text_primary),
        Space::new().width(Length::Fill),
        button(text("+").size(FONT_ICON).color(pal.accent))
            .style(button::text)
            .padding([2, 6])
            .on_press(Message::ShowCreateForm),
        button(text("\u{21BB}").size(FONT_ICON).color(pal.text_secondary))
            .style(button::text)
            .padding([2, 6])
            .on_press(Message::Refresh),
    ]
    .spacing(4)
    .padding([8, 10]);

    let mut list = column![].spacing(2).padding([0, 10]);

    // Inline create form
    if create_form.visible {
        let form = column![
            text_input("Spark title...", &create_form.title)
                .size(FONT_BODY)
                .padding([6, 8])
                .on_input(Message::CreateFormTitleChanged)
                .on_submit(Message::SubmitNewSpark),
            row![
                button(text("Create").size(FONT_LABEL).color(pal.accent))
                    .style(button::text)
                    .padding([3, 8])
                    .on_press(Message::SubmitNewSpark),
                button(text("Cancel").size(FONT_LABEL).color(pal.text_tertiary))
                    .style(button::text)
                    .padding([3, 8])
                    .on_press(Message::CancelCreate),
            ]
            .spacing(8),
        ]
        .spacing(4)
        .padding([4, 0]);
        list = list.push(form);
    }

    if sparks.is_empty() && !create_form.visible {
        list = list.push(
            text("No sparks yet")
                .size(FONT_BODY)
                .color(pal.text_tertiary),
        );
    } else {
        for spark in sparks {
            list = list.push(view_spark_row(spark, &pal));
        }
    }

    let content = column![header, scrollable(list).height(Length::Fill)]
        .width(Length::Fill)
        .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| style::glass_panel(&pal, has_bg))
        .into()
}

fn view_spark_row<'a>(spark: &'a Spark, pal: &Palette) -> Element<'a, Message> {
    let pal = *pal;
    let status_indicator: &str = match spark.status.as_str() {
        "open" => "\u{25CB}",        // ○
        "in_progress" => "\u{25D4}", // ◔
        "blocked" => "\u{25A0}",     // ■
        "deferred" => "\u{25CC}",    // ◌
        "closed" => "\u{25CF}",      // ●
        _ => "\u{25CB}",
    };

    let next_status = next_status_str(&spark.status);
    let priority_label = format!("P{}", spark.priority);
    let id = spark.id.clone();

    let status_btn = button(
        text(status_indicator)
            .size(FONT_ICON_SM)
            .color(pal.text_secondary),
    )
    .style(button::text)
    .padding([2, 4])
    .on_press(Message::CycleStatus(id.clone(), next_status.to_string()));

    row![
        status_btn,
        button(
            row![
                text(priority_label)
                    .size(FONT_LABEL)
                    .color(pal.text_tertiary),
                text(&spark.title).size(FONT_BODY).color(pal.text_primary),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center),
        )
        .style(button::text)
        .width(Length::Fill)
        .padding([5, 6])
        .on_press(Message::SelectSpark(id))
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center)
    .into()
}

/// Cycle: open → in_progress → closed → open
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
