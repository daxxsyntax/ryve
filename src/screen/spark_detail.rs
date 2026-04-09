// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Loomantix

//! Spark detail view — shown when a spark is selected in the workgraph panel.

use data::sparks::types::{Bond, Contract, ContractEnforcement, ContractKind, Spark};
use iced::widget::{Space, button, column, container, row, scrollable, stack, text, text_input};
use iced::{Element, Length, Theme};

use crate::screen::delegation_trace::DelegationTrace;
use crate::style::{self, FONT_BODY, FONT_HEADER, FONT_ICON, FONT_LABEL, FONT_SMALL, Palette};

// ── Inline validation + closed-edit confirmation ─────
//
// See spark ryve-8ad372cf. This module-level machinery is deliberately
// expressed as pure data + pure functions so it can be exercised in unit
// tests and plugged into whichever field-edit plumbing lands from the
// sibling "editable <field>" sparks. The view layer only asks:
//
//   1. `validate_title` — is the current draft legal to persist?
//   2. `begin_edit` — may this field enter edit mode right now, or do we
//      need to prompt because the spark is in a terminal state?
//
// INVARIANT: validation here must be a *strict subset* of the data-layer
// rules. We reject empty titles (which the CLI also rejects at
// `src/cli.rs:639`); we do not invent new rules the data layer wouldn't
// enforce, because the data layer is the source of truth.

/// Fields on a spark that participate in inline editing. This enum lives
/// here so the confirmation machinery can be consulted for *any* field —
/// the title spark, description spark, and acceptance-list spark all
/// route their `begin_edit` through the same gate. Non-Title variants
/// are declared ahead of their callsites so sibling sparks
/// (ryve-4742d98b, ryve-9b98f949, and the rest of the ryve-82e1102f
/// epic) can wire themselves in without re-touching this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EditField {
    Title,
    Description,
    Priority,
    SparkType,
    Assignee,
    ProblemStatement,
    Invariants,
    NonGoals,
    AcceptanceCriteria,
}

/// Reasons a draft value is not acceptable to persist. Kept as a small
/// enum rather than a free-form String so the view layer can render each
/// case (tooltip copy, aria-label, etc.) without string matching.
/// Consumed by the sibling editable-title spark (ryve-f58d0492) which
/// will call `validate_title` from its save path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ValidationError {
    /// Title was empty (or whitespace-only) after trimming.
    TitleEmpty,
}

impl ValidationError {
    /// Tooltip/hover copy explaining the rejection. Short and user-facing.
    #[allow(dead_code)]
    pub fn message(self) -> &'static str {
        match self {
            Self::TitleEmpty => "Title cannot be empty",
        }
    }
}

/// Validate a draft title. Mirrors the CLI rule at `src/cli.rs:639` and
/// the create-form rule at `src/screen/sparks.rs:51` — empty after
/// trimming is rejected. This is the short-circuit the save path should
/// consult *before* dispatching a `SparkUpdate` message.
#[allow(dead_code)]
pub fn validate_title(draft: &str) -> Result<(), ValidationError> {
    if draft.trim().is_empty() {
        Err(ValidationError::TitleEmpty)
    } else {
        Ok(())
    }
}

/// Is this status one where editing should require an explicit
/// confirmation from the user? "closed" is the project's terminal state;
/// "completed" is accepted defensively because the spark description and
/// other subsystems (delegation, contracts) use that label and a future
/// SparkStatus::Completed would otherwise silently bypass the guard.
pub fn is_terminal_status(status: &str) -> bool {
    matches!(status, "closed" | "completed")
}

/// Outcome of asking "can this field enter edit mode right now?".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeginEditOutcome {
    /// Proceed — the caller may flip the field into edit mode.
    Proceed,
    /// The spark is in a terminal state and the user has not yet
    /// acknowledged it for this edit session. The caller should render
    /// the confirmation modal with the carried status string and stash
    /// the pending field so `confirm_closed_edit` can release it.
    NeedsConfirmation {
        status: String,
        field: EditField,
    },
}

/// Per-edit-session state for the closed/completed confirmation gate.
///
/// "Session" here means: a single continuous selection of a single
/// spark, bounded on one side by the user selecting that spark and on
/// the other by either a selection change or the spark's status moving
/// out of a terminal state. Within that window, one confirmation
/// unlocks every subsequent `begin_edit` on that spark — we do not
/// re-prompt per field.
#[derive(Debug, Default, Clone)]
pub struct SparkEditSession {
    /// The spark this session is tracking. `None` when no spark is
    /// selected (e.g. before the detail view has ever been shown).
    selection_id: Option<String>,
    /// Snapshot of the last status we saw for the tracked spark. Used
    /// to detect a status transition *away from* terminal, which
    /// invalidates a previously granted confirmation.
    last_seen_status: Option<String>,
    /// True once the user has clicked "Edit anyway" on the confirmation
    /// modal for the current selection.
    confirmed_closed_edit: bool,
    /// Which field is pending a confirmation answer, if the modal is up.
    /// `None` when the modal is not showing.
    pending_field: Option<EditField>,
}

impl SparkEditSession {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the closed-edit modal should be rendered.
    #[allow(dead_code)]
    pub fn is_modal_open(&self) -> bool {
        self.pending_field.is_some()
    }

    /// The status string to interpolate into the modal copy, if any.
    pub fn modal_status(&self) -> Option<&str> {
        if self.pending_field.is_some() {
            self.last_seen_status.as_deref()
        } else {
            None
        }
    }

    /// Call whenever the detail view receives a (possibly new) spark to
    /// display — either because the user selected a different spark or
    /// because a refresh pulled down a new status. This is where the
    /// "selection changed" and "status moved away from terminal"
    /// invalidation rules fire.
    pub fn observe_spark(&mut self, spark: &Spark) {
        let new_id = &spark.id;
        let selection_changed = self.selection_id.as_deref() != Some(new_id.as_str());

        if selection_changed {
            // Brand-new selection: wipe the slate.
            self.selection_id = Some(new_id.clone());
            self.last_seen_status = Some(spark.status.clone());
            self.confirmed_closed_edit = false;
            self.pending_field = None;
            return;
        }

        // Same spark — check for a status transition. If the spark has
        // moved *out* of terminal (e.g. reopened), revoke any prior
        // confirmation so a future re-close prompts again. This is the
        // rule from the spark's acceptance criteria.
        let was_terminal = self
            .last_seen_status
            .as_deref()
            .map(is_terminal_status)
            .unwrap_or(false);
        let is_terminal_now = is_terminal_status(&spark.status);
        if was_terminal && !is_terminal_now {
            self.confirmed_closed_edit = false;
            self.pending_field = None;
        }
        self.last_seen_status = Some(spark.status.clone());
    }

    /// Ask whether `field` may enter edit mode right now. The caller
    /// should treat `NeedsConfirmation` as "open the modal and stop" —
    /// the pending field is already recorded on the session, so
    /// `confirm_closed_edit` can finish the transition without the
    /// caller having to remember what was clicked.
    pub fn begin_edit(&mut self, spark: &Spark, field: EditField) -> BeginEditOutcome {
        // Keep the session's view of the spark in sync. Callers may
        // have just received a fresh copy from a refresh; we want to
        // reason against the most recent status.
        self.observe_spark(spark);

        if !is_terminal_status(&spark.status) || self.confirmed_closed_edit {
            self.pending_field = None;
            return BeginEditOutcome::Proceed;
        }

        self.pending_field = Some(field);
        BeginEditOutcome::NeedsConfirmation {
            status: spark.status.clone(),
            field,
        }
    }

    /// User clicked "Edit anyway" on the modal. Returns the field that
    /// was pending so the caller can flip it into edit mode without
    /// having to re-derive which click started this.
    pub fn confirm_closed_edit(&mut self) -> Option<EditField> {
        let field = self.pending_field.take();
        if field.is_some() {
            self.confirmed_closed_edit = true;
        }
        field
    }

    /// User clicked "Cancel" on the modal. Leaves the view unchanged.
    pub fn cancel_closed_edit(&mut self) {
        self.pending_field = None;
    }
}

/// Container style for a text_input that failed inline validation.
/// Applied as an overlay border so the input widget itself retains its
/// normal glass styling. The red border + tooltip on hover is the
/// "rejected inline" signal the spark's acceptance criteria call for.
/// Consumed by the sibling editable-title spark (ryve-f58d0492) which
/// wraps its text_input in a container with this style when
/// `validate_title` returns an error.
#[allow(dead_code)]
pub fn validation_error_border(pal: &Palette) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: None,
        border: iced::Border {
            color: pal.danger,
            width: 1.5,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}

/// Render the closed/completed edit confirmation modal body. This is a
/// standalone helper so the detail view can overlay it without
/// rebuilding the modal every frame. Callers are responsible for the
/// backdrop/dimming layer.
pub fn view_closed_edit_modal<'a>(
    status: &str,
    pal: &Palette,
) -> Element<'a, Message> {
    let pal = *pal;

    let title = text("Edit finished spark?")
        .size(FONT_HEADER)
        .color(pal.text_primary);

    let body = text(format!("This spark is {status}. Edit anyway?"))
        .size(FONT_BODY)
        .color(pal.text_secondary);

    let cancel_btn = button(text("Cancel").size(FONT_LABEL).color(pal.text_primary))
        .style(button::text)
        .padding([6, 14])
        .on_press(Message::CancelClosedEdit);

    let confirm_btn = button(
        text("Edit anyway")
            .size(FONT_LABEL)
            .color(pal.window_bg),
    )
    .style(move |_t: &Theme, _s| button::Style {
        background: Some(iced::Background::Color(pal.danger)),
        text_color: pal.window_bg,
        border: iced::Border {
            color: pal.danger,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    })
    .padding([6, 14])
    .on_press(Message::ConfirmClosedEdit);

    let actions = row![Space::new().width(Length::Fill), cancel_btn, confirm_btn].spacing(8);

    container(column![title, body, actions].spacing(12).padding(16))
        .width(Length::Shrink)
        .style(move |_t: &Theme| iced::widget::container::Style {
            background: Some(iced::Background::Color(pal.surface)),
            border: iced::Border {
                color: pal.border,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

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
    /// Request to enter edit mode for a field. The workshop-level
    /// handler must route this through `SparkEditSession::begin_edit`
    /// so the closed-spark confirmation gate fires. See ryve-8ad372cf.
    BeginEditField(EditField),
    /// User clicked "Edit anyway" in the closed-edit confirmation
    /// modal. The handler should call
    /// `SparkEditSession::confirm_closed_edit` and then flip the
    /// returned field into edit mode.
    ConfirmClosedEdit,
    /// User dismissed the closed-edit confirmation modal — the view
    /// stays unchanged.
    CancelClosedEdit,
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
    edit_session: &'a SparkEditSession,
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

    // Title — clicking the title asks to begin editing. The actual
    // edit-mode UI lives in the sibling editable-title spark
    // (ryve-f58d0492); here we just route the click through the
    // closed/completed confirmation gate. See ryve-8ad372cf.
    let title_button = button(
        text(&spark.title)
            .size(FONT_HEADER + 4.0)
            .color(pal.text_primary),
    )
    .style(button::text)
    .padding(0)
    .on_press(Message::BeginEditField(EditField::Title));

    let title_row = container(title_button).padding([4, 10]);

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

    let base: Element<'a, Message> = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme: &Theme| style::glass_panel(&pal, has_bg))
        .into();

    // Overlay the closed/completed confirmation modal when the edit
    // session has a pending field. A dimming backdrop sits between the
    // base content and the modal so clicks outside don't accidentally
    // land on the detail view's widgets.
    if let Some(status) = edit_session.modal_status() {
        let backdrop = container(Space::new().width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_t: &Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(iced::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.4,
                })),
                ..Default::default()
            });

        let modal = container(view_closed_edit_modal(status, &pal))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center);

        stack![base, backdrop, modal].into()
    } else {
        base
    }
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

    // ── Validation + closed-edit confirmation tests (ryve-8ad372cf) ──

    #[test]
    fn validate_title_rejects_empty_and_whitespace() {
        assert_eq!(validate_title(""), Err(ValidationError::TitleEmpty));
        assert_eq!(validate_title("   "), Err(ValidationError::TitleEmpty));
        assert_eq!(validate_title("\t\n  "), Err(ValidationError::TitleEmpty));
    }

    #[test]
    fn validate_title_accepts_non_empty() {
        assert_eq!(validate_title("fix auth"), Ok(()));
        assert_eq!(validate_title("  leading space is fine  "), Ok(()));
    }

    #[test]
    fn is_terminal_status_matches_closed_and_completed() {
        assert!(is_terminal_status("closed"));
        assert!(is_terminal_status("completed"));
        assert!(!is_terminal_status("open"));
        assert!(!is_terminal_status("in_progress"));
        assert!(!is_terminal_status("blocked"));
        assert!(!is_terminal_status("deferred"));
    }

    #[test]
    fn begin_edit_proceeds_for_open_spark() {
        let mut session = SparkEditSession::new();
        let spark = make_spark("sp-x", "open");
        assert_eq!(
            session.begin_edit(&spark, EditField::Title),
            BeginEditOutcome::Proceed
        );
        assert!(!session.is_modal_open());
    }

    #[test]
    fn begin_edit_prompts_for_closed_spark() {
        let mut session = SparkEditSession::new();
        let spark = make_spark("sp-x", "closed");
        let outcome = session.begin_edit(&spark, EditField::Title);
        assert_eq!(
            outcome,
            BeginEditOutcome::NeedsConfirmation {
                status: "closed".to_string(),
                field: EditField::Title,
            }
        );
        assert!(session.is_modal_open());
        assert_eq!(session.modal_status(), Some("closed"));
    }

    #[test]
    fn begin_edit_prompts_for_completed_spark() {
        let mut session = SparkEditSession::new();
        let spark = make_spark("sp-x", "completed");
        let outcome = session.begin_edit(&spark, EditField::Description);
        assert!(matches!(outcome, BeginEditOutcome::NeedsConfirmation { .. }));
    }

    #[test]
    fn confirmation_is_session_scoped_once_granted() {
        let mut session = SparkEditSession::new();
        let spark = make_spark("sp-x", "closed");

        // First edit prompts.
        assert!(matches!(
            session.begin_edit(&spark, EditField::Title),
            BeginEditOutcome::NeedsConfirmation { .. }
        ));

        // User confirms. The pending field comes back so the caller
        // knows what to flip into edit mode.
        assert_eq!(session.confirm_closed_edit(), Some(EditField::Title));
        assert!(!session.is_modal_open());

        // Subsequent edits on *any* field proceed without re-prompting.
        assert_eq!(
            session.begin_edit(&spark, EditField::Title),
            BeginEditOutcome::Proceed
        );
        assert_eq!(
            session.begin_edit(&spark, EditField::Description),
            BeginEditOutcome::Proceed
        );
        assert_eq!(
            session.begin_edit(&spark, EditField::AcceptanceCriteria),
            BeginEditOutcome::Proceed
        );
    }

    #[test]
    fn cancel_leaves_view_unchanged_and_re_prompts_next_time() {
        let mut session = SparkEditSession::new();
        let spark = make_spark("sp-x", "closed");
        assert!(matches!(
            session.begin_edit(&spark, EditField::Title),
            BeginEditOutcome::NeedsConfirmation { .. }
        ));
        session.cancel_closed_edit();
        assert!(!session.is_modal_open());

        // Next begin_edit should still prompt — the user did not grant
        // consent.
        assert!(matches!(
            session.begin_edit(&spark, EditField::Title),
            BeginEditOutcome::NeedsConfirmation { .. }
        ));
    }

    #[test]
    fn reopening_spark_clears_confirmation() {
        let mut session = SparkEditSession::new();
        let closed = make_spark("sp-x", "closed");
        session.begin_edit(&closed, EditField::Title);
        session.confirm_closed_edit();

        // Status transitions away from terminal — e.g. the user flips
        // it back to open in another window, and a refresh pulls it in.
        let reopened = make_spark("sp-x", "open");
        session.observe_spark(&reopened);

        // Now close it again. A future edit must re-prompt, because
        // the original consent was for the *previous* closed state.
        let closed_again = make_spark("sp-x", "closed");
        assert!(matches!(
            session.begin_edit(&closed_again, EditField::Title),
            BeginEditOutcome::NeedsConfirmation { .. }
        ));
    }

    #[test]
    fn switching_selected_spark_resets_confirmation() {
        let mut session = SparkEditSession::new();
        let a = make_spark("sp-a", "closed");
        session.begin_edit(&a, EditField::Title);
        session.confirm_closed_edit();

        // User selects a different closed spark. We must not carry
        // consent across selections — the per-session guarantee is
        // scoped to a single spark.
        let b = make_spark("sp-b", "closed");
        assert!(matches!(
            session.begin_edit(&b, EditField::Title),
            BeginEditOutcome::NeedsConfirmation { .. }
        ));
    }

    #[test]
    fn confirm_without_pending_field_is_noop() {
        let mut session = SparkEditSession::new();
        assert_eq!(session.confirm_closed_edit(), None);
    }

    #[test]
    fn modal_status_reflects_current_terminal_state() {
        let mut session = SparkEditSession::new();
        let completed = make_spark("sp-x", "completed");
        session.begin_edit(&completed, EditField::Title);
        assert_eq!(session.modal_status(), Some("completed"));
    }

    #[test]
    fn validation_error_border_uses_danger_color() {
        let pal = Palette::dark();
        let style_ = validation_error_border(&pal);
        assert_eq!(style_.border.color, pal.danger);
        assert!(style_.border.width > 0.0);
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
