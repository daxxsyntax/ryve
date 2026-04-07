// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Spark picker modal — shown when spawning a Hand so the user can assign
//! the agent to a specific spark before the terminal launches.

use data::sparks::types::Spark;
use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Element, Length, Theme};

use crate::style::{self, Palette, FONT_BODY, FONT_HEADER, FONT_ICON_SM, FONT_LABEL, FONT_SMALL};

// ── Messages ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    /// User selected a spark to assign to the new Hand.
    SelectSpark(String),
    /// Cancel — close the picker without spawning.
    Cancel,
}

// ── View ────────────────────────────────────────────────

pub fn view<'a>(sparks: &'a [Spark], pal: &Palette) -> Element<'a, Message> {
    let pal = *pal;

    let title = text("Assign Spark").size(FONT_HEADER).color(pal.text_primary);
    let close_btn = button(text("\u{00D7}").size(FONT_HEADER).color(pal.text_secondary))
        .style(button::text)
        .on_press(Message::Cancel);

    let header =
        row![title, Space::new().width(Length::Fill), close_btn].align_y(iced::Alignment::Center);

    let subtitle = text("Select a spark for this Hand to work on. Spark assignment is required.")
        .size(FONT_SMALL)
        .color(pal.text_secondary);

    // Filter to actionable sparks (open, in_progress, blocked)
    let actionable: Vec<&Spark> = sparks
        .iter()
        .filter(|s| matches!(s.status.as_str(), "open" | "in_progress" | "blocked"))
        .collect();

    let mut list = column![].spacing(2);

    if actionable.is_empty() {
        list = list.push(
            text("No open sparks")
                .size(FONT_BODY)
                .color(pal.text_tertiary),
        );
    } else {
        for spark in actionable {
            list = list.push(view_spark_row(spark, &pal));
        }
    }

    let scrollable_list = scrollable(list).height(Length::Fill);

    // Action buttons at the bottom — only Cancel; spark selection is required
    let actions = row![
        Space::new().width(Length::Fill),
        button(text("Cancel").size(FONT_LABEL).color(pal.text_tertiary))
            .style(button::text)
            .padding([6, 12])
            .on_press(Message::Cancel),
    ]
    .align_y(iced::Alignment::Center);

    let content = column![header, subtitle, scrollable_list, actions]
        .spacing(12)
        .padding(20)
        .width(420)
        .height(400);

    let inner = container(content).style(move |_theme: &Theme| style::modal(&pal));

    // Center the modal with backdrop overlay
    container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_theme: &Theme| style::modal_backdrop(&pal))
        .into()
}

fn view_spark_row<'a>(spark: &'a Spark, pal: &Palette) -> Element<'a, Message> {
    let pal = *pal;
    let status_indicator: &str = match spark.status.as_str() {
        "open" => "\u{25CB}",        // ○
        "in_progress" => "\u{25D4}", // ◔
        "blocked" => "\u{25A0}",     // ■
        _ => "\u{25CB}",
    };

    let priority_label = format!("P{}", spark.priority);
    let id = spark.id.clone();

    let row_content = row![
        text(status_indicator)
            .size(FONT_ICON_SM)
            .color(pal.text_secondary),
        text(priority_label)
            .size(FONT_LABEL)
            .color(pal.text_tertiary),
        text(&spark.title).size(FONT_BODY).color(pal.text_primary),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    button(row_content)
        .style(button::text)
        .width(Length::Fill)
        .padding([6, 8])
        .on_press(Message::SelectSpark(id))
        .into()
}
