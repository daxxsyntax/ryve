// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Spark detail view — shown when a spark is selected in the workgraph panel.

use data::sparks::types::{Bond, Contract, ContractEnforcement, ContractKind, Spark};
use iced::widget::{Id, Space, button, column, container, row, scrollable, text, text_input};
use iced::{Element, Length, Theme};

/// Prefix used to build stable widget `Id`s for acceptance criterion rows.
/// Building the id from the row index lets the update handler issue the
/// focus operation to move focus to a freshly inserted row (used by the
/// "+ Add criterion" button to drop the caret straight into the new row).
const ACCEPTANCE_ROW_ID_PREFIX: &str = "ac-row-";

/// Build a stable widget `Id` for the Nth acceptance criterion row.
pub fn acceptance_row_id(index: usize) -> Id {
    Id::from(format!("{ACCEPTANCE_ROW_ID_PREFIX}{index}"))
}

use crate::screen::delegation_trace::DelegationTrace;
use crate::style::{self, FONT_BODY, FONT_HEADER, FONT_ICON, FONT_LABEL, FONT_SMALL, Palette};

// ── Form state ───────────────────────────────────────

/// Inline create-contract form state. Held on the Workshop so it
/// survives cross-message updates and follows the spark_create_form
/// pattern used elsewhere in the workgraph panel.
#[derive(Debug, Clone)]
pub struct ContractCreateForm {
    pub visible: bool,
    pub kind: ContractKind,
    pub description: String,
    pub check_command: String,
    pub enforcement: ContractEnforcement,
}

impl Default for ContractCreateForm {
    fn default() -> Self {
        Self {
            visible: false,
            kind: ContractKind::CustomCommand,
            description: String::new(),
            check_command: String::new(),
            enforcement: ContractEnforcement::Required,
        }
    }
}

impl ContractCreateForm {
    pub fn reset(&mut self) {
        self.visible = false;
        self.kind = ContractKind::CustomCommand;
        self.description.clear();
        self.check_command.clear();
        self.enforcement = ContractEnforcement::Required;
    }
}

/// Cycle through ContractKind variants for the form picker.
pub fn next_contract_kind(k: ContractKind) -> ContractKind {
    match k {
        ContractKind::CustomCommand => ContractKind::TestPass,
        ContractKind::TestPass => ContractKind::NoApiBreak,
        ContractKind::NoApiBreak => ContractKind::GrepAbsent,
        ContractKind::GrepAbsent => ContractKind::GrepPresent,
        ContractKind::GrepPresent => ContractKind::CustomCommand,
    }
}

/// Toggle enforcement.
pub fn toggle_enforcement(e: ContractEnforcement) -> ContractEnforcement {
    match e {
        ContractEnforcement::Required => ContractEnforcement::Advisory,
        ContractEnforcement::Advisory => ContractEnforcement::Required,
    }
}

fn contract_kind_label(k: ContractKind) -> &'static str {
    match k {
        ContractKind::CustomCommand => "custom_command",
        ContractKind::TestPass => "test_pass",
        ContractKind::NoApiBreak => "no_api_break",
        ContractKind::GrepAbsent => "grep_absent",
        ContractKind::GrepPresent => "grep_present",
    }
}

fn enforcement_label(e: ContractEnforcement) -> &'static str {
    match e {
        ContractEnforcement::Required => "required",
        ContractEnforcement::Advisory => "advisory",
    }
}

// ── Acceptance criteria edit state ───────────────────

/// In-memory draft of a spark's acceptance criteria while the user is
/// editing them in the detail view. Held on the Workshop so edits and
/// the one-level undo buffer survive cross-message updates.
///
/// Invariant for the caller: after a successful save, `items` must equal
/// the `metadata.intent.acceptance_criteria` on the reloaded spark. The
/// easiest way to honour that is to call [`AcceptanceCriteriaEdit::load`]
/// whenever a fresh spark comes back from the DB.
#[derive(Debug, Clone, Default)]
pub struct AcceptanceCriteriaEdit {
    /// Spark id this editor is bound to — used to guard against stale state
    /// when the selection changes.
    pub spark_id: String,
    /// Current draft rows, one entry per criterion.
    pub items: Vec<String>,
    /// One-level undo buffer for the most recent deletion: `(index, text)`.
    /// Populated by [`delete_criterion`] and consumed by [`undo_delete`].
    pub last_deleted: Option<(usize, String)>,
}

impl AcceptanceCriteriaEdit {
    /// Seed the editor from the spark's current intent. Call this whenever
    /// the selection changes or a save reloads the spark — it keeps the
    /// in-memory vec in sync with persisted state.
    pub fn load(spark: &Spark) -> Self {
        Self {
            spark_id: spark.id.clone(),
            items: spark.intent().acceptance_criteria,
            last_deleted: None,
        }
    }

    /// Returns true if this editor is bound to the given spark id.
    pub fn is_for(&self, spark_id: &str) -> bool {
        self.spark_id == spark_id
    }
}

/// Merge a new acceptance-criteria vec into a spark's existing metadata
/// JSON, preserving every other field under `intent` and every sibling of
/// `intent`. Returns the serialized JSON string ready for
/// `UpdateSpark { metadata: Some(..) }`.
///
/// If the existing metadata is missing or malformed we fall back to a
/// freshly constructed `{ "intent": { "acceptance_criteria": [...] } }`.
pub fn merge_acceptance_criteria_into_metadata(
    existing_metadata: &str,
    new_criteria: &[String],
) -> String {
    let mut root: serde_json::Value = serde_json::from_str(existing_metadata)
        .unwrap_or_else(|_| serde_json::json!({}));
    if !root.is_object() {
        root = serde_json::json!({});
    }
    let obj = root.as_object_mut().expect("root is object");
    let intent_entry = obj
        .entry("intent".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !intent_entry.is_object() {
        *intent_entry = serde_json::json!({});
    }
    let intent_obj = intent_entry.as_object_mut().expect("intent is object");
    intent_obj.insert(
        "acceptance_criteria".to_string(),
        serde_json::Value::Array(
            new_criteria
                .iter()
                .map(|s| serde_json::Value::String(s.clone()))
                .collect(),
        ),
    );
    serde_json::to_string(&root).unwrap_or_else(|_| existing_metadata.to_string())
}

/// Append a new empty row and return its index so the caller can focus it.
pub fn add_criterion(edit: &mut AcceptanceCriteriaEdit) -> usize {
    edit.items.push(String::new());
    edit.last_deleted = None;
    edit.items.len() - 1
}

/// Delete the row at `index`, stashing it in the undo buffer. Returns
/// `true` if the row existed.
pub fn delete_criterion(edit: &mut AcceptanceCriteriaEdit, index: usize) -> bool {
    if index >= edit.items.len() {
        return false;
    }
    let text = edit.items.remove(index);
    edit.last_deleted = Some((index, text));
    true
}

/// Restore the last deleted row if one is in the undo buffer. Returns
/// `true` if anything was restored.
pub fn undo_delete(edit: &mut AcceptanceCriteriaEdit) -> bool {
    let Some((index, text)) = edit.last_deleted.take() else {
        return false;
    };
    let clamped = index.min(edit.items.len());
    edit.items.insert(clamped, text);
    true
}

/// Move the row at `index` one slot up. No-op at position 0.
pub fn move_up(edit: &mut AcceptanceCriteriaEdit, index: usize) -> bool {
    if index == 0 || index >= edit.items.len() {
        return false;
    }
    edit.items.swap(index, index - 1);
    edit.last_deleted = None;
    true
}

/// Move the row at `index` one slot down. No-op at the last position.
pub fn move_down(edit: &mut AcceptanceCriteriaEdit, index: usize) -> bool {
    if index + 1 >= edit.items.len() {
        return false;
    }
    edit.items.swap(index, index + 1);
    edit.last_deleted = None;
    true
}

/// On-blur commit for the row at `index`. If the row is blank we drop it
/// so stray empties never land on disk (matches the "empty row on blur is
/// auto-deleted" acceptance criterion). Returns `true` if a row was
/// removed, so the caller can skip focusing into a gone-away index.
pub fn trim_blank_on_blur(edit: &mut AcceptanceCriteriaEdit, index: usize) -> bool {
    if index >= edit.items.len() {
        return false;
    }
    if edit.items[index].trim().is_empty() {
        edit.items.remove(index);
        // A blank row removal isn't undo-worthy — the user never typed anything.
        return true;
    }
    false
}

// ── Messages ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    Back,
    /// Quick status cycle: pass (spark_id, new_status)
    CycleStatus(String, String),
    /// Show inline create-contract form for the given spark.
    ShowCreateContract,
    /// Cancel the create-contract form.
    CancelCreateContract,
    /// Cycle the kind in the create form.
    CycleContractKind,
    /// Toggle enforcement in the create form.
    ToggleContractEnforcement,
    ContractDescriptionChanged(String),
    ContractCheckCommandChanged(String),
    /// Submit the create form for the given spark id.
    SubmitContract(String),
    /// Run the check command for a contract id (and spark id, for refresh).
    RunContract {
        spark_id: String,
        contract_id: i64,
    },
    /// Delete a contract by id (and spark id, for refresh).
    DeleteContract {
        spark_id: String,
        contract_id: i64,
    },

    // ── Acceptance criteria editing ──────────────────
    /// Text in the Nth row changed (keystroke). Draft-only — no DB write.
    AcceptanceCriterionChanged(usize, String),
    /// Nth row was submitted / blurred. If blank, auto-delete it.
    /// Otherwise persist the full vec through `update_spark`.
    AcceptanceCriterionSubmit(usize),
    /// "+ Add criterion" pressed: append an empty row and focus it.
    AcceptanceCriterionAdd,
    /// Delete the Nth row, persist, and show an undo toast.
    AcceptanceCriterionDelete(usize),
    /// Restore the most recently deleted row and persist.
    AcceptanceCriterionUndoDelete,
    /// Reorder: move the Nth row up by one (persist on change).
    AcceptanceCriterionMoveUp(usize),
    /// Reorder: move the Nth row down by one (persist on change).
    AcceptanceCriterionMoveDown(usize),
}

// ── View ─────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    spark: &'a Spark,
    contracts: &'a [Contract],
    bonds: &'a [Bond],
    all_sparks: &'a [Spark],
    delegation: &DelegationTrace,
    create_form: &'a ContractCreateForm,
    acceptance_edit: &'a AcceptanceCriteriaEdit,
    pal: &Palette,
    has_bg: bool,
) -> Element<'a, Message> {
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
    .on_press(Message::CycleStatus(spark.id.clone(), next.to_string()));

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

    if let Some(problem) = intent.problem_statement
        && !problem.is_empty()
    {
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

    if !intent.invariants.is_empty() {
        let mut items =
            column![text("Invariants").size(FONT_LABEL).color(pal.text_tertiary),].spacing(2);
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
        let mut items =
            column![text("Non-Goals").size(FONT_LABEL).color(pal.text_tertiary),].spacing(2);
        for ng in intent.non_goals {
            items = items.push(
                text(format!("\u{2022} {ng}"))
                    .size(FONT_BODY)
                    .color(pal.text_primary),
            );
        }
        body = body.push(items);
    }

    // Acceptance criteria are always editable — even when empty, we
    // render the header and the "+ Add criterion" button so users can
    // seed a brand-new list. The editor reads from `acceptance_edit`
    // (the draft vec) rather than `intent.acceptance_criteria` so
    // in-flight keystrokes are visible before a save round-trips.
    body = body.push(view_acceptance_criteria_section(acceptance_edit, &pal));

    // ── Delegation trace section ─────────────────────
    // Surface the Atlas → Head → Hand chain so users can see who is
    // working on this spark and where the request originated. See
    // ryve-8fadd6ab.
    body = body.push(crate::screen::delegation_trace::view(delegation, &pal));

    // ── Dependencies section ─────────────────────────
    // Surface bonds in both directions so a Hand can immediately see what
    // a spark blocks (downstream work) and what's blocking it (upstream
    // work that must close first). Without this, bonds were invisible to
    // anyone not running raw SQL — see sp-ux0006.
    body = body.push(view_bonds_section(spark, bonds, all_sparks, &pal));

    // ── Contracts section ────────────────────────────
    body = body.push(view_contracts_section(
        &spark.id,
        contracts,
        create_form,
        &pal,
    ));

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

// ── Bonds section ────────────────────────────────────

/// Classify bonds for the spark into three groups for display:
/// downstream blocking ("Blocks"), upstream blocking ("Blocked by"),
/// and everything else ("Related"). Returns owned data so the view tree
/// can borrow without lifetime gymnastics.
#[allow(clippy::type_complexity)]
fn classify_bonds<'a>(
    spark: &'a Spark,
    bonds: &'a [Bond],
    all_sparks: &'a [Spark],
) -> (
    Vec<(&'a Spark, &'a Bond)>,                      // blocks (downstream)
    Vec<(&'a Spark, &'a Bond)>,                      // blocked by (upstream)
    Vec<(&'a Spark, &'a Bond, bool /* outgoing */)>, // related/other
) {
    let lookup = |id: &str| all_sparks.iter().find(|s| s.id == id);
    let mut blocks = Vec::new();
    let mut blocked_by = Vec::new();
    let mut other = Vec::new();
    for b in bonds {
        let blocking = matches!(b.bond_type.as_str(), "blocks" | "conditional_blocks");
        if b.from_id == spark.id {
            // outgoing
            if let Some(other_s) = lookup(&b.to_id) {
                if blocking {
                    blocks.push((other_s, b));
                } else {
                    other.push((other_s, b, true));
                }
            }
        } else if b.to_id == spark.id {
            // incoming
            if let Some(other_s) = lookup(&b.from_id) {
                if blocking {
                    blocked_by.push((other_s, b));
                } else {
                    other.push((other_s, b, false));
                }
            }
        }
    }
    (blocks, blocked_by, other)
}

fn view_bonds_section<'a>(
    spark: &'a Spark,
    bonds: &'a [Bond],
    all_sparks: &'a [Spark],
    pal: &Palette,
) -> Element<'a, Message> {
    let pal = *pal;
    let (blocks, blocked_by, other) = classify_bonds(spark, bonds, all_sparks);

    if blocks.is_empty() && blocked_by.is_empty() && other.is_empty() {
        return column![
            text("Dependencies")
                .size(FONT_LABEL)
                .color(pal.text_tertiary),
            text("No bonds").size(FONT_SMALL).color(pal.text_tertiary),
        ]
        .spacing(2)
        .into();
    }

    let mut col = column![
        text("Dependencies")
            .size(FONT_LABEL)
            .color(pal.text_tertiary),
    ]
    .spacing(4);

    if !blocked_by.is_empty() {
        // Highlight if any blocker is still open — that's the "you can't
        // work on this yet" signal the spark says was missing.
        let any_open = blocked_by.iter().any(|(s, _)| s.status != "closed");
        let header_color = if any_open {
            pal.danger
        } else {
            pal.text_secondary
        };
        col = col.push(
            text(format!("Blocked by ({})", blocked_by.len()))
                .size(FONT_SMALL)
                .color(header_color),
        );
        for (s, _b) in &blocked_by {
            col = col.push(view_bond_row(s, &pal));
        }
    }

    if !blocks.is_empty() {
        col = col.push(
            text(format!("Blocks ({})", blocks.len()))
                .size(FONT_SMALL)
                .color(pal.text_secondary),
        );
        for (s, _b) in &blocks {
            col = col.push(view_bond_row(s, &pal));
        }
    }

    if !other.is_empty() {
        col = col.push(
            text(format!("Related ({})", other.len()))
                .size(FONT_SMALL)
                .color(pal.text_tertiary),
        );
        for (s, b, outgoing) in &other {
            col = col.push(view_bond_row_typed(s, &b.bond_type, *outgoing, &pal));
        }
    }

    col.into()
}

fn bond_status_symbol(status: &str) -> &'static str {
    status_symbol(status)
}

fn view_bond_row<'a>(s: &'a Spark, pal: &Palette) -> Element<'a, Message> {
    let pal = *pal;
    let icon = bond_status_symbol(&s.status);
    let color = status_color(&s.status, &pal);
    row![
        text(icon).size(FONT_SMALL).color(color),
        text(format!("P{}", s.priority))
            .size(FONT_SMALL)
            .color(pal.text_tertiary),
        text(s.id.as_str())
            .size(FONT_SMALL)
            .color(pal.text_tertiary),
        text(s.title.as_str())
            .size(FONT_BODY)
            .color(pal.text_primary),
    ]
    .spacing(6)
    .padding([2, 8])
    .align_y(iced::Alignment::Center)
    .into()
}

fn view_bond_row_typed<'a>(
    s: &'a Spark,
    bond_type: &'a str,
    outgoing: bool,
    pal: &Palette,
) -> Element<'a, Message> {
    let pal = *pal;
    let icon = bond_status_symbol(&s.status);
    let color = status_color(&s.status, &pal);
    let arrow = if outgoing { "\u{2192}" } else { "\u{2190}" };
    row![
        text(icon).size(FONT_SMALL).color(color),
        text(arrow).size(FONT_SMALL).color(pal.text_tertiary),
        text(bond_type).size(FONT_SMALL).color(pal.text_tertiary),
        text(s.id.as_str())
            .size(FONT_SMALL)
            .color(pal.text_tertiary),
        text(s.title.as_str())
            .size(FONT_BODY)
            .color(pal.text_primary),
    ]
    .spacing(6)
    .padding([2, 8])
    .align_y(iced::Alignment::Center)
    .into()
}

// ── Acceptance criteria section ──────────────────────

fn view_acceptance_criteria_section<'a>(
    edit: &'a AcceptanceCriteriaEdit,
    pal: &Palette,
) -> Element<'a, Message> {
    let pal = *pal;

    let header = text("Acceptance Criteria")
        .size(FONT_LABEL)
        .color(pal.text_tertiary);

    let mut col = column![header].spacing(4);

    if edit.items.is_empty() {
        col = col.push(
            text("No acceptance criteria yet")
                .size(FONT_SMALL)
                .color(pal.text_tertiary),
        );
    } else {
        let last = edit.items.len() - 1;
        for (i, item) in edit.items.iter().enumerate() {
            col = col.push(view_acceptance_row(i, item, last, &pal));
        }
    }

    // "+ Add criterion" + optional inline undo for the most recent delete.
    let add_btn = button(
        row![
            text("+").size(FONT_ICON).color(pal.accent),
            text("Add criterion").size(FONT_LABEL).color(pal.accent),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .style(button::text)
    .padding([2, 6])
    .on_press(Message::AcceptanceCriterionAdd);

    let mut footer = row![add_btn]
        .spacing(8)
        .align_y(iced::Alignment::Center);
    if edit.last_deleted.is_some() {
        footer = footer.push(
            button(
                text("\u{21B6} Undo delete")
                    .size(FONT_LABEL)
                    .color(pal.text_secondary),
            )
            .style(button::text)
            .padding([2, 6])
            .on_press(Message::AcceptanceCriterionUndoDelete),
        );
    }
    col = col.push(footer);

    col.into()
}

fn view_acceptance_row<'a>(
    index: usize,
    value: &'a str,
    last_index: usize,
    pal: &Palette,
) -> Element<'a, Message> {
    let pal = *pal;

    // Reorder handle: two stacked arrows. Iced 0.14 doesn't give us a
    // first-class drag affordance so we expose the reorder operation as
    // an up/down pair. Functionally this is the "drag handle" — it
    // updates the vec order and persists on click. Using the unicode
    // vertical double arrow glyph keeps it visually compact.
    let up_btn = {
        let mut b = button(text("\u{25B4}").size(FONT_SMALL).color(pal.text_tertiary))
            .style(button::text)
            .padding([0, 4]);
        if index > 0 {
            b = b.on_press(Message::AcceptanceCriterionMoveUp(index));
        }
        b
    };
    let down_btn = {
        let mut b = button(text("\u{25BE}").size(FONT_SMALL).color(pal.text_tertiary))
            .style(button::text)
            .padding([0, 4]);
        if index < last_index {
            b = b.on_press(Message::AcceptanceCriterionMoveDown(index));
        }
        b
    };
    let handle = column![up_btn, down_btn].spacing(0).width(Length::Shrink);

    let input = text_input("New criterion…", value)
        .id(acceptance_row_id(index))
        .size(FONT_BODY)
        .padding([4, 6])
        .on_input(move |s| Message::AcceptanceCriterionChanged(index, s))
        .on_submit(Message::AcceptanceCriterionSubmit(index));

    let delete_btn = button(text("\u{00D7}").size(FONT_ICON).color(pal.text_tertiary))
        .style(button::text)
        .padding([2, 6])
        .on_press(Message::AcceptanceCriterionDelete(index));

    row![handle, input, delete_btn]
        .spacing(6)
        .align_y(iced::Alignment::Center)
        .into()
}

// ── Contracts section ────────────────────────────────

fn view_contracts_section<'a>(
    spark_id: &'a str,
    contracts: &'a [Contract],
    form: &'a ContractCreateForm,
    pal: &Palette,
) -> Element<'a, Message> {
    let pal = *pal;

    let header = row![
        text("Contracts").size(FONT_LABEL).color(pal.text_tertiary),
        Space::new().width(Length::Fill),
        button(text("+").size(FONT_ICON).color(pal.accent))
            .style(button::text)
            .padding([2, 6])
            .on_press(Message::ShowCreateContract),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);

    let mut col = column![header].spacing(6);

    if form.visible {
        col = col.push(view_create_form(spark_id, form, &pal));
    }

    if contracts.is_empty() && !form.visible {
        col = col.push(
            text("No contracts yet")
                .size(FONT_SMALL)
                .color(pal.text_tertiary),
        );
    } else {
        for c in contracts {
            col = col.push(view_contract_row(spark_id, c, &pal));
        }
    }

    col.into()
}

fn view_create_form<'a>(
    spark_id: &'a str,
    form: &'a ContractCreateForm,
    pal: &Palette,
) -> Element<'a, Message> {
    let pal = *pal;

    let kind_btn = button(
        row![
            text("kind:").size(FONT_SMALL).color(pal.text_tertiary),
            text(contract_kind_label(form.kind))
                .size(FONT_LABEL)
                .color(pal.accent),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .style(button::text)
    .padding([2, 6])
    .on_press(Message::CycleContractKind);

    let enforcement_btn = button(
        row![
            text("enforcement:")
                .size(FONT_SMALL)
                .color(pal.text_tertiary),
            text(enforcement_label(form.enforcement))
                .size(FONT_LABEL)
                .color(match form.enforcement {
                    ContractEnforcement::Required => pal.danger,
                    ContractEnforcement::Advisory => pal.text_secondary,
                }),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center),
    )
    .style(button::text)
    .padding([2, 6])
    .on_press(Message::ToggleContractEnforcement);

    let description_input = text_input("Description...", &form.description)
        .size(FONT_BODY)
        .padding([6, 8])
        .on_input(Message::ContractDescriptionChanged);

    let check_input = text_input("Check command (e.g. 'cargo test')", &form.check_command)
        .size(FONT_BODY)
        .padding([6, 8])
        .on_input(Message::ContractCheckCommandChanged)
        .on_submit(Message::SubmitContract(spark_id.to_string()));

    let submit_btn = button(text("Create").size(FONT_LABEL).color(pal.accent))
        .style(button::text)
        .padding([3, 8])
        .on_press(Message::SubmitContract(spark_id.to_string()));

    let cancel_btn = button(text("Cancel").size(FONT_LABEL).color(pal.text_tertiary))
        .style(button::text)
        .padding([3, 8])
        .on_press(Message::CancelCreateContract);

    let actions = row![submit_btn, cancel_btn].spacing(8);

    column![
        row![kind_btn, enforcement_btn]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        description_input,
        check_input,
        actions,
    ]
    .spacing(4)
    .padding([4, 0])
    .into()
}

fn view_contract_row<'a>(
    spark_id: &'a str,
    c: &'a Contract,
    pal: &Palette,
) -> Element<'a, Message> {
    let pal = *pal;

    let (status_icon, status_color) = contract_status_visuals(&c.status, &pal);

    let kind_text = text(c.kind.as_str())
        .size(FONT_SMALL)
        .color(pal.text_tertiary);

    let enforcement_color = if c.enforcement == "required" {
        pal.danger
    } else {
        pal.text_tertiary
    };
    let enforcement_text = text(c.enforcement.as_str())
        .size(FONT_SMALL)
        .color(enforcement_color);

    let desc_text = text(c.description.as_str())
        .size(FONT_BODY)
        .color(pal.text_primary);

    let mut actions = row![].spacing(4).align_y(iced::Alignment::Center);

    if c.check_command.as_deref().unwrap_or("").trim().is_empty() {
        // No command: show a disabled "Run" placeholder for clarity.
        actions = actions.push(text("\u{25B6}").size(FONT_ICON).color(pal.text_tertiary));
    } else {
        actions = actions.push(
            button(text("\u{25B6} Run").size(FONT_LABEL).color(pal.accent))
                .style(button::text)
                .padding([2, 6])
                .on_press(Message::RunContract {
                    spark_id: spark_id.to_string(),
                    contract_id: c.id,
                }),
        );
    }

    actions = actions.push(
        button(text("\u{00D7}").size(FONT_ICON).color(pal.text_tertiary))
            .style(button::text)
            .padding([2, 6])
            .on_press(Message::DeleteContract {
                spark_id: spark_id.to_string(),
                contract_id: c.id,
            }),
    );

    let header = row![
        text(status_icon).size(FONT_LABEL).color(status_color),
        kind_text,
        enforcement_text,
        Space::new().width(Length::Fill),
        actions,
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let mut col = column![header, desc_text].spacing(2);

    if let Some(cmd) = c.check_command.as_deref()
        && !cmd.is_empty()
    {
        col = col.push(
            text(format!("$ {cmd}"))
                .size(FONT_SMALL)
                .color(pal.text_secondary),
        );
    }

    if let Some(ref when) = c.last_checked_at {
        col = col.push(
            text(format!("last checked {when}"))
                .size(FONT_SMALL)
                .color(pal.text_tertiary),
        );
    }

    container(col)
        .padding([6, 8])
        .width(Length::Fill)
        .style(move |_t: &Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(iced::Color {
                a: 0.04,
                ..pal.text_primary
            })),
            border: iced::Border {
                radius: 4.0.into(),
                width: 0.0,
                color: iced::Color::TRANSPARENT,
            },
            ..Default::default()
        })
        .into()
}

fn contract_status_visuals(status: &str, pal: &Palette) -> (&'static str, iced::Color) {
    match status {
        "pass" => ("\u{25CF}", iced::Color {
            r: 0.298,
            g: 0.851,
            b: 0.392,
            a: 1.0,
        }),
        "fail" => ("\u{25CF}", pal.danger),
        "skipped" => ("\u{25CC}", pal.text_tertiary),
        _ /* pending */ => ("\u{25CB}", pal.text_secondary),
    }
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
        0 => pal.danger, // P0 — critical
        1 => iced::Color {
            // P1 — orange-ish
            r: 1.0,
            g: 0.6,
            b: 0.0,
            a: 1.0,
        },
        2 => pal.accent,         // P2 — normal
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_kind_round_trips() {
        let mut k = ContractKind::CustomCommand;
        for _ in 0..5 {
            k = next_contract_kind(k);
        }
        assert_eq!(k, ContractKind::CustomCommand);
    }

    #[test]
    fn toggle_enforcement_round_trips() {
        let e = ContractEnforcement::Required;
        assert_eq!(toggle_enforcement(e), ContractEnforcement::Advisory);
        assert_eq!(
            toggle_enforcement(toggle_enforcement(e)),
            ContractEnforcement::Required
        );
    }

    #[test]
    fn create_form_default_is_hidden_required_custom_command() {
        let f = ContractCreateForm::default();
        assert!(!f.visible);
        assert_eq!(f.kind, ContractKind::CustomCommand);
        assert_eq!(f.enforcement, ContractEnforcement::Required);
    }

    #[test]
    fn create_form_reset_clears_fields() {
        let mut f = ContractCreateForm {
            visible: true,
            kind: ContractKind::TestPass,
            description: "do the thing".into(),
            check_command: "cargo test".into(),
            enforcement: ContractEnforcement::Advisory,
        };
        f.reset();
        assert!(!f.visible);
        assert_eq!(f.kind, ContractKind::CustomCommand);
        assert!(f.description.is_empty());
        assert!(f.check_command.is_empty());
        assert_eq!(f.enforcement, ContractEnforcement::Required);
    }

    fn make_spark(id: &str, status: &str) -> Spark {
        Spark {
            id: id.to_string(),
            title: format!("title {id}"),
            description: String::new(),
            status: status.to_string(),
            priority: 2,
            spark_type: "task".to_string(),
            assignee: None,
            owner: None,
            parent_id: None,
            workshop_id: "ws".to_string(),
            estimated_minutes: None,
            github_issue_number: None,
            github_repo: None,
            metadata: "{}".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            closed_at: None,
            closed_reason: None,
            due_at: None,
            defer_until: None,
            risk_level: None,
            scope_boundary: None,
        }
    }

    fn make_bond(id: i64, from: &str, to: &str, ty: &str) -> Bond {
        Bond {
            id,
            from_id: from.to_string(),
            to_id: to.to_string(),
            bond_type: ty.to_string(),
        }
    }

    #[test]
    fn classify_bonds_splits_blocks_blocked_by_and_other() {
        let me = make_spark("sp-me", "open");
        let upstream = make_spark("sp-up", "open");
        let downstream = make_spark("sp-down", "open");
        let related = make_spark("sp-rel", "open");
        let all = vec![
            me.clone(),
            upstream.clone(),
            downstream.clone(),
            related.clone(),
        ];

        let bonds = vec![
            make_bond(1, "sp-up", "sp-me", "blocks"), // upstream blocks me
            make_bond(2, "sp-me", "sp-down", "blocks"), // me blocks downstream
            make_bond(3, "sp-me", "sp-rel", "related"),
            make_bond(4, "sp-other", "sp-me", "blocks"), // unknown spark — should be skipped
        ];

        let (blocks, blocked_by, other) = classify_bonds(&me, &bonds, &all);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0.id, "sp-down");
        assert_eq!(blocked_by.len(), 1);
        assert_eq!(blocked_by[0].0.id, "sp-up");
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].0.id, "sp-rel");
        assert!(other[0].2, "related bond from me is outgoing");
    }

    #[test]
    fn classify_bonds_treats_conditional_blocks_as_blocking() {
        let me = make_spark("sp-me", "open");
        let blocker = make_spark("sp-b", "open");
        let all = vec![me.clone(), blocker.clone()];
        let bonds = vec![make_bond(1, "sp-b", "sp-me", "conditional_blocks")];
        let (_, blocked_by, _) = classify_bonds(&me, &bonds, &all);
        assert_eq!(blocked_by.len(), 1);
    }

    // ── Acceptance criteria editor tests ─────────────

    fn edit_with(items: Vec<&str>) -> AcceptanceCriteriaEdit {
        AcceptanceCriteriaEdit {
            spark_id: "sp-me".to_string(),
            items: items.into_iter().map(String::from).collect(),
            last_deleted: None,
        }
    }

    #[test]
    fn add_criterion_appends_empty_row_and_returns_index() {
        let mut e = edit_with(vec!["a", "b"]);
        let idx = add_criterion(&mut e);
        assert_eq!(idx, 2);
        assert_eq!(e.items, vec!["a", "b", ""]);
    }

    #[test]
    fn delete_criterion_removes_and_stashes_for_undo() {
        let mut e = edit_with(vec!["a", "b", "c"]);
        assert!(delete_criterion(&mut e, 1));
        assert_eq!(e.items, vec!["a", "c"]);
        assert_eq!(e.last_deleted, Some((1, "b".to_string())));
    }

    #[test]
    fn delete_criterion_out_of_range_is_noop() {
        let mut e = edit_with(vec!["a"]);
        assert!(!delete_criterion(&mut e, 5));
        assert_eq!(e.items, vec!["a"]);
        assert!(e.last_deleted.is_none());
    }

    #[test]
    fn undo_delete_restores_at_original_index() {
        let mut e = edit_with(vec!["a", "b", "c"]);
        delete_criterion(&mut e, 1);
        assert!(undo_delete(&mut e));
        assert_eq!(e.items, vec!["a", "b", "c"]);
        assert!(e.last_deleted.is_none());
    }

    #[test]
    fn undo_delete_without_buffer_is_noop() {
        let mut e = edit_with(vec!["a"]);
        assert!(!undo_delete(&mut e));
        assert_eq!(e.items, vec!["a"]);
    }

    #[test]
    fn move_up_and_down_reorder_vec() {
        let mut e = edit_with(vec!["a", "b", "c"]);
        assert!(move_up(&mut e, 2));
        assert_eq!(e.items, vec!["a", "c", "b"]);
        assert!(move_down(&mut e, 0));
        assert_eq!(e.items, vec!["c", "a", "b"]);
        // edges
        assert!(!move_up(&mut e, 0));
        assert!(!move_down(&mut e, 2));
    }

    #[test]
    fn trim_blank_on_blur_removes_whitespace_only_row() {
        let mut e = edit_with(vec!["a", "   ", "c"]);
        assert!(trim_blank_on_blur(&mut e, 1));
        assert_eq!(e.items, vec!["a", "c"]);
        // Blank trim should NOT populate the undo buffer — there's
        // nothing to undo since the user never typed anything real.
        assert!(e.last_deleted.is_none());
    }

    #[test]
    fn trim_blank_on_blur_keeps_nonempty_row() {
        let mut e = edit_with(vec!["real"]);
        assert!(!trim_blank_on_blur(&mut e, 0));
        assert_eq!(e.items, vec!["real"]);
    }

    #[test]
    fn merge_into_existing_metadata_preserves_other_intent_fields() {
        let existing = serde_json::json!({
            "intent": {
                "problem_statement": "keep me",
                "invariants": ["x"],
                "acceptance_criteria": ["old"],
            },
            "unrelated": 42,
        })
        .to_string();
        let merged = merge_acceptance_criteria_into_metadata(
            &existing,
            &["new1".to_string(), "new2".to_string()],
        );
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["intent"]["problem_statement"], "keep me");
        assert_eq!(v["intent"]["invariants"][0], "x");
        assert_eq!(v["intent"]["acceptance_criteria"][0], "new1");
        assert_eq!(v["intent"]["acceptance_criteria"][1], "new2");
        assert_eq!(v["unrelated"], 42);
    }

    #[test]
    fn merge_into_empty_metadata_creates_intent_shell() {
        let merged =
            merge_acceptance_criteria_into_metadata("{}", &["only".to_string()]);
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["intent"]["acceptance_criteria"][0], "only");
    }

    #[test]
    fn merge_into_malformed_metadata_falls_back_to_fresh_object() {
        // Malformed input shouldn't panic or lose the new criteria.
        let merged = merge_acceptance_criteria_into_metadata(
            "not json at all",
            &["rescued".to_string()],
        );
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["intent"]["acceptance_criteria"][0], "rescued");
    }

    #[test]
    fn load_reads_acceptance_criteria_from_spark_metadata() {
        let mut s = make_spark("sp-me", "open");
        s.metadata = serde_json::json!({
            "intent": { "acceptance_criteria": ["one", "two"] }
        })
        .to_string();
        let e = AcceptanceCriteriaEdit::load(&s);
        assert_eq!(e.spark_id, "sp-me");
        assert_eq!(e.items, vec!["one", "two"]);
        assert!(e.is_for("sp-me"));
        assert!(!e.is_for("sp-other"));
    }

    #[test]
    fn contract_status_visuals_distinguishes_states() {
        let pal = Palette::dark();
        let (pass_icon, _) = contract_status_visuals("pass", &pal);
        let (fail_icon, _) = contract_status_visuals("fail", &pal);
        let (pending_icon, _) = contract_status_visuals("pending", &pal);
        assert_eq!(pass_icon, "\u{25CF}");
        assert_eq!(fail_icon, "\u{25CF}");
        assert_eq!(pending_icon, "\u{25CB}");
    }
}
