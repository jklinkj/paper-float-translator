use serde::{Deserialize, Serialize};
use std::fmt;

pub(crate) const POPUP_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PopupStatus {
    #[default]
    Hidden,
    SelectionPending,
    SelectionReady,
    Translating,
    Translated,
    Error,
}

impl PopupStatus {
    pub(crate) fn is_selection_prompt(self) -> bool {
        matches!(self, Self::SelectionPending | Self::SelectionReady)
    }

    pub(crate) fn is_visible(self) -> bool {
        self != Self::Hidden
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PopupErrorKind {
    Configuration,
    Permission,
    Selection,
    Translation,
    Protocol,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PopupRecoveryAction {
    RetryTranslation,
    OpenSettings,
    OpenAccessibility,
    OpenInputMonitoring,
    Reselect,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PopupState {
    pub(crate) protocol_version: u32,
    pub(crate) revision: u64,
    pub(crate) selection_revision: u64,
    pub(crate) visible: bool,
    pub(crate) status: PopupStatus,
    pub(crate) source_text: Option<String>,
    pub(crate) selected_text: Option<String>,
    pub(crate) cleaned_text: Option<String>,
    pub(crate) translation: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) error_kind: Option<PopupErrorKind>,
    pub(crate) retryable: Option<bool>,
    pub(crate) recovery_action: Option<PopupRecoveryAction>,
    pub(crate) cached: Option<bool>,
    pub(crate) target_language: Option<String>,
    pub(crate) pinned: bool,
}

impl Default for PopupState {
    fn default() -> Self {
        Self {
            protocol_version: POPUP_PROTOCOL_VERSION,
            revision: 0,
            selection_revision: 0,
            visible: false,
            status: PopupStatus::Hidden,
            source_text: None,
            selected_text: None,
            cleaned_text: None,
            translation: None,
            error: None,
            error_kind: None,
            retryable: None,
            recovery_action: None,
            cached: None,
            target_language: None,
            pinned: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TranslationGuard {
    pub(crate) generation: u64,
    pub(crate) selection_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BeginTranslation {
    Started {
        guard: TranslationGuard,
        snapshot: PopupState,
    },
    StaleSelection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TranslationCommit {
    Applied(PopupState),
    StaleGuard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupControlError {
    InvalidTransition { from: PopupStatus, to: PopupStatus },
    InvalidOperation { code: &'static str },
    CounterExhausted { counter: &'static str },
}

impl PopupControlError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::InvalidTransition { .. } => "popup_control.invalid_transition",
            Self::InvalidOperation { code } => code,
            Self::CounterExhausted { .. } => "popup_control.counter_exhausted",
        }
    }
}

impl fmt::Display for PopupControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for PopupControlError {}

#[derive(Debug, Default)]
pub(crate) struct PopupController {
    snapshot: PopupState,
    last_native_generation: Option<i64>,
    selection_revision_counter: u64,
    translation_generation: u64,
}

struct PreparedNewSelection {
    snapshot: PopupState,
    translation_generation: u64,
    selection_revision: u64,
}

impl PopupController {
    pub(crate) fn snapshot(&self) -> &PopupState {
        &self.snapshot
    }

    pub(crate) fn accept_native_generation(
        &mut self,
        incoming: i64,
    ) -> Result<(), PopupControlError> {
        self.validate_native_generation(incoming)?;
        self.last_native_generation = Some(incoming);
        Ok(())
    }

    pub(crate) fn commit_native_selection(
        &mut self,
        incoming: i64,
        next: PopupState,
    ) -> Result<PopupState, PopupControlError> {
        self.validate_native_generation(incoming)?;
        let prepared = self.prepare_new_selection(next)?;
        self.last_native_generation = Some(incoming);
        Ok(self.apply_new_selection(prepared))
    }

    pub(crate) fn cancel_translation_for_native_generation(
        &mut self,
        incoming: i64,
    ) -> Result<u64, PopupControlError> {
        self.validate_native_generation(incoming)?;
        let next_generation = next_counter(self.translation_generation, "translation_generation")?;
        self.last_native_generation = Some(incoming);
        self.translation_generation = next_generation;
        Ok(next_generation)
    }

    pub(crate) fn commit_new_selection(
        &mut self,
        next: PopupState,
    ) -> Result<PopupState, PopupControlError> {
        let prepared = self.prepare_new_selection(next)?;
        Ok(self.apply_new_selection(prepared))
    }

    fn prepare_new_selection(
        &self,
        mut next: PopupState,
    ) -> Result<PreparedNewSelection, PopupControlError> {
        if !matches!(
            next.status,
            PopupStatus::SelectionPending | PopupStatus::SelectionReady | PopupStatus::Error
        ) {
            return Err(PopupControlError::InvalidOperation {
                code: "popup_control.new_selection_invalid_status",
            });
        }
        validate_transition(self.snapshot.status, next.status)?;
        let next_translation_generation =
            next_counter(self.translation_generation, "translation_generation")?;
        let next_selection_revision =
            next_counter(self.selection_revision_counter, "selection_revision")?;
        let next_popup_revision = next_counter(self.snapshot.revision, "popup_revision")?;

        next.selection_revision = next_selection_revision;
        finalize_snapshot(&mut next, next_popup_revision);
        Ok(PreparedNewSelection {
            snapshot: next,
            translation_generation: next_translation_generation,
            selection_revision: next_selection_revision,
        })
    }

    fn apply_new_selection(&mut self, prepared: PreparedNewSelection) -> PopupState {
        self.translation_generation = prepared.translation_generation;
        self.selection_revision_counter = prepared.selection_revision;
        self.snapshot = prepared.snapshot.clone();
        prepared.snapshot
    }

    pub(crate) fn begin_translation(
        &mut self,
        selection_revision: u64,
        cleaned_text: &str,
        target_language: &str,
    ) -> Result<BeginTranslation, PopupControlError> {
        if selection_revision == 0
            || !self.snapshot.visible
            || self.snapshot.selection_revision != selection_revision
        {
            return Ok(BeginTranslation::StaleSelection);
        }

        let mut next = PopupState {
            selection_revision,
            status: PopupStatus::Translating,
            source_text: Some(cleaned_text.to_string()),
            cleaned_text: Some(cleaned_text.to_string()),
            translation: Some(String::new()),
            cached: Some(false),
            target_language: Some(target_language.to_string()),
            pinned: self.snapshot.pinned,
            ..PopupState::default()
        };
        validate_transition(self.snapshot.status, next.status)?;
        let next_translation_generation =
            next_counter(self.translation_generation, "translation_generation")?;
        let next_popup_revision = next_counter(self.snapshot.revision, "popup_revision")?;
        finalize_snapshot(&mut next, next_popup_revision);
        let guard = TranslationGuard {
            generation: next_translation_generation,
            selection_revision,
        };

        self.translation_generation = next_translation_generation;
        self.snapshot = next.clone();
        Ok(BeginTranslation::Started {
            guard,
            snapshot: next,
        })
    }

    pub(crate) fn commit_translation(
        &mut self,
        guard: TranslationGuard,
        mut next: PopupState,
    ) -> Result<TranslationCommit, PopupControlError> {
        if !self.translation_is_current(guard) {
            return Ok(TranslationCommit::StaleGuard);
        }
        if !matches!(
            next.status,
            PopupStatus::Translating | PopupStatus::Translated | PopupStatus::Error
        ) {
            return Err(PopupControlError::InvalidOperation {
                code: "popup_control.translation_commit_invalid_status",
            });
        }
        validate_transition(self.snapshot.status, next.status)?;
        let next_popup_revision = next_counter(self.snapshot.revision, "popup_revision")?;
        next.selection_revision = guard.selection_revision;
        next.pinned = self.snapshot.pinned;
        finalize_snapshot(&mut next, next_popup_revision);

        self.snapshot = next.clone();
        Ok(TranslationCommit::Applied(next))
    }

    pub(crate) fn translation_is_current(&self, guard: TranslationGuard) -> bool {
        self.translation_generation == guard.generation
            && self.snapshot.selection_revision == guard.selection_revision
            && self.snapshot.visible
            && self.snapshot.status == PopupStatus::Translating
    }

    pub(crate) fn cancel_translation(&mut self) -> Result<u64, PopupControlError> {
        let next_generation = next_counter(self.translation_generation, "translation_generation")?;
        self.translation_generation = next_generation;
        Ok(next_generation)
    }

    pub(crate) fn toggle_pin(&mut self) -> Result<PopupState, PopupControlError> {
        if !self.snapshot.visible {
            return Err(PopupControlError::InvalidOperation {
                code: "popup_control.toggle_pin_hidden",
            });
        }
        let mut next = self.snapshot.clone();
        next.pinned = !next.pinned;
        self.commit_state(next)
    }

    pub(crate) fn close(&mut self) -> Result<PopupState, PopupControlError> {
        validate_transition(self.snapshot.status, PopupStatus::Hidden)?;
        let next_translation_generation =
            next_counter(self.translation_generation, "translation_generation")?;
        let next_popup_revision = next_counter(self.snapshot.revision, "popup_revision")?;
        let mut next = PopupState {
            selection_revision: self.snapshot.selection_revision,
            status: PopupStatus::Hidden,
            pinned: self.snapshot.pinned,
            ..PopupState::default()
        };
        finalize_snapshot(&mut next, next_popup_revision);

        self.translation_generation = next_translation_generation;
        self.snapshot = next.clone();
        Ok(next)
    }

    fn commit_state(&mut self, mut next: PopupState) -> Result<PopupState, PopupControlError> {
        validate_transition(self.snapshot.status, next.status)?;
        let next_popup_revision = next_counter(self.snapshot.revision, "popup_revision")?;
        if next.selection_revision == 0 {
            next.selection_revision = self.snapshot.selection_revision;
        }
        finalize_snapshot(&mut next, next_popup_revision);
        self.snapshot = next.clone();
        Ok(next)
    }

    fn validate_native_generation(&self, incoming: i64) -> Result<(), PopupControlError> {
        if incoming <= 0
            || self
                .last_native_generation
                .is_some_and(|last| incoming <= last)
        {
            Err(PopupControlError::InvalidOperation {
                code: "popup_control.native_generation_not_monotonic",
            })
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(crate) fn from_snapshot(snapshot: PopupState) -> Self {
        Self {
            selection_revision_counter: snapshot.selection_revision,
            snapshot,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(crate) fn translation_generation(&self) -> u64 {
        self.translation_generation
    }
}

fn next_counter(value: u64, counter: &'static str) -> Result<u64, PopupControlError> {
    value
        .checked_add(1)
        .ok_or(PopupControlError::CounterExhausted { counter })
}

fn finalize_snapshot(snapshot: &mut PopupState, revision: u64) {
    snapshot.protocol_version = POPUP_PROTOCOL_VERSION;
    snapshot.revision = revision;
    snapshot.visible = snapshot.status.is_visible();
}

fn validate_transition(from: PopupStatus, to: PopupStatus) -> Result<(), PopupControlError> {
    if is_valid_popup_transition(from, to) {
        Ok(())
    } else {
        Err(PopupControlError::InvalidTransition { from, to })
    }
}

fn is_valid_popup_transition(from: PopupStatus, to: PopupStatus) -> bool {
    from == to
        || matches!(
            to,
            PopupStatus::Hidden
                | PopupStatus::SelectionPending
                | PopupStatus::SelectionReady
                | PopupStatus::Error
        )
        || (to == PopupStatus::Translating
            && matches!(
                from,
                PopupStatus::SelectionPending
                    | PopupStatus::SelectionReady
                    | PopupStatus::Translated
                    | PopupStatus::Error
            ))
        || (from == PopupStatus::Translating && to == PopupStatus::Translated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(status: PopupStatus, text: &str) -> PopupState {
        PopupState {
            status,
            source_text: Some(text.to_string()),
            selected_text: Some(text.to_string()),
            cleaned_text: Some(text.to_string()),
            ..PopupState::default()
        }
    }

    fn translated(text: &str) -> PopupState {
        PopupState {
            status: PopupStatus::Translated,
            translation: Some(text.to_string()),
            ..PopupState::default()
        }
    }

    #[test]
    fn transition_matrix_is_explicit_for_all_six_by_six_pairs() {
        use PopupStatus::*;
        let statuses = [
            Hidden,
            SelectionPending,
            SelectionReady,
            Translating,
            Translated,
            Error,
        ];
        let expected = [
            [true, true, true, false, false, true],
            [true, true, true, true, false, true],
            [true, true, true, true, false, true],
            [true, true, true, true, true, true],
            [true, true, true, true, true, true],
            [true, true, true, true, false, true],
        ];

        for (from_index, from) in statuses.into_iter().enumerate() {
            for (to_index, to) in statuses.into_iter().enumerate() {
                assert_eq!(
                    is_valid_popup_transition(from, to),
                    expected[from_index][to_index],
                    "unexpected transition result for {from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn release_invalid_transition_is_rejected_without_any_state_change() {
        let mut controller = PopupController::default();
        let before = controller.snapshot.clone();
        let before_counters = (
            controller.last_native_generation,
            controller.selection_revision_counter,
            controller.translation_generation,
        );

        let result = controller.commit_state(PopupState {
            status: PopupStatus::Translated,
            ..PopupState::default()
        });

        assert_eq!(
            result,
            Err(PopupControlError::InvalidTransition {
                from: PopupStatus::Hidden,
                to: PopupStatus::Translated,
            })
        );
        assert_eq!(controller.snapshot, before);
        assert_eq!(
            (
                controller.last_native_generation,
                controller.selection_revision_counter,
                controller.translation_generation,
            ),
            before_counters
        );
    }

    #[test]
    fn counter_exhaustion_is_rejected_transactionally() {
        let mut controller = PopupController {
            selection_revision_counter: u64::MAX,
            ..PopupController::default()
        };
        let before = (
            controller.snapshot.clone(),
            controller.selection_revision_counter,
            controller.translation_generation,
        );

        let error = controller
            .commit_new_selection(selection(PopupStatus::SelectionReady, "overflow"))
            .expect_err("selection revision overflow must fail");

        assert_eq!(
            error,
            PopupControlError::CounterExhausted {
                counter: "selection_revision"
            }
        );
        assert_eq!(
            (
                controller.snapshot.clone(),
                controller.selection_revision_counter,
                controller.translation_generation,
            ),
            before
        );
    }

    #[test]
    fn native_selection_commits_generation_and_all_state_counters_atomically() {
        let mut controller = PopupController {
            last_native_generation: Some(8),
            selection_revision_counter: u64::MAX,
            ..PopupController::default()
        };
        let before = (
            controller.snapshot.clone(),
            controller.last_native_generation,
            controller.selection_revision_counter,
            controller.translation_generation,
        );

        let error = controller
            .commit_native_selection(9, selection(PopupStatus::SelectionReady, "cannot-commit"))
            .expect_err("counter exhaustion must reject the whole native transaction");

        assert_eq!(
            error,
            PopupControlError::CounterExhausted {
                counter: "selection_revision"
            }
        );
        assert_eq!(
            (
                controller.snapshot.clone(),
                controller.last_native_generation,
                controller.selection_revision_counter,
                controller.translation_generation,
            ),
            before
        );
    }

    #[test]
    fn native_selection_rejects_equal_or_older_generation_without_any_write() {
        let mut controller = PopupController::default();
        controller
            .commit_native_selection(8, selection(PopupStatus::SelectionReady, "accepted"))
            .expect("first native selection should commit");

        for incoming in [8, 7, 0, -1] {
            let before = (
                controller.snapshot.clone(),
                controller.last_native_generation,
                controller.selection_revision_counter,
                controller.translation_generation,
            );
            let error = controller
                .commit_native_selection(
                    incoming,
                    selection(PopupStatus::SelectionReady, "rejected"),
                )
                .expect_err("non-increasing native generation must be rejected");
            assert_eq!(
                error.code(),
                "popup_control.native_generation_not_monotonic"
            );
            assert_eq!(
                (
                    controller.snapshot.clone(),
                    controller.last_native_generation,
                    controller.selection_revision_counter,
                    controller.translation_generation,
                ),
                before
            );
        }
    }

    #[test]
    fn hidden_popup_cannot_be_pinned_and_rejection_is_non_mutating() {
        let mut controller = PopupController::default();
        let before = controller.snapshot.clone();

        let error = controller
            .toggle_pin()
            .expect_err("a hidden popup has nothing to pin");

        assert_eq!(error.code(), "popup_control.toggle_pin_hidden");
        assert_eq!(controller.snapshot, before);
        assert_eq!(controller.translation_generation, 0);
        assert_eq!(controller.selection_revision_counter, 0);
    }

    #[test]
    fn selection_pending_can_begin_the_automatic_double_copy_translation() {
        let mut controller = PopupController::default();
        let pending = controller
            .commit_new_selection(selection(PopupStatus::SelectionPending, "pending"))
            .expect("pending selection should commit");

        assert!(matches!(
            controller
                .begin_translation(pending.selection_revision, "pending", "中文")
                .expect("pending to translating is a valid transition"),
            BeginTranslation::Started { .. }
        ));
        assert_eq!(controller.snapshot.status, PopupStatus::Translating);
    }

    #[test]
    fn visible_is_always_derived_from_status() {
        let mut controller = PopupController::default();
        let ready = controller
            .commit_new_selection(PopupState {
                visible: false,
                ..selection(PopupStatus::SelectionReady, "text")
            })
            .expect("selection should commit");
        assert!(ready.visible);

        let hidden = controller.close().expect("close should commit");
        assert!(!hidden.visible);
    }

    #[test]
    fn native_generation_must_be_strictly_positive_and_increasing() {
        let mut controller = PopupController::default();
        for invalid in [0, -1] {
            let before = (
                controller.snapshot.clone(),
                controller.last_native_generation,
                controller.selection_revision_counter,
                controller.translation_generation,
            );
            assert_eq!(
                controller
                    .accept_native_generation(invalid)
                    .expect_err("non-positive generation must fail")
                    .code(),
                "popup_control.native_generation_not_monotonic"
            );
            assert_eq!(
                (
                    controller.snapshot.clone(),
                    controller.last_native_generation,
                    controller.selection_revision_counter,
                    controller.translation_generation,
                ),
                before
            );
        }
        controller
            .accept_native_generation(8)
            .expect("first generation should pass");
        for invalid in [8, 7, 0, -9] {
            let before = (
                controller.snapshot.clone(),
                controller.last_native_generation,
                controller.selection_revision_counter,
                controller.translation_generation,
            );
            assert!(controller.accept_native_generation(invalid).is_err());
            assert_eq!(
                (
                    controller.snapshot.clone(),
                    controller.last_native_generation,
                    controller.selection_revision_counter,
                    controller.translation_generation,
                ),
                before
            );
        }
        controller
            .accept_native_generation(9)
            .expect("newer generation should pass");
        assert_eq!(controller.last_native_generation, Some(9));
    }

    #[test]
    fn revisions_are_monotonic_and_selection_revision_changes_only_for_new_selection() {
        let mut controller = PopupController::default();
        let first = controller
            .commit_new_selection(selection(PopupStatus::SelectionReady, "A"))
            .expect("first selection");
        let pinned = controller.toggle_pin().expect("pin");
        let hidden = controller.close().expect("close");
        let second = controller
            .commit_new_selection(selection(PopupStatus::SelectionPending, "B"))
            .expect("second selection");

        assert_eq!((first.revision, first.selection_revision), (1, 1));
        assert_eq!((pinned.revision, pinned.selection_revision), (2, 1));
        assert_eq!((hidden.revision, hidden.selection_revision), (3, 1));
        assert_eq!((second.revision, second.selection_revision), (4, 2));
    }

    #[test]
    fn stale_translation_guard_is_a_normal_non_mutating_cancellation() {
        let mut controller = PopupController::default();
        let first = controller
            .commit_new_selection(selection(PopupStatus::SelectionReady, "A"))
            .expect("selection A");
        let BeginTranslation::Started { guard, .. } = controller
            .begin_translation(first.selection_revision, "A", "中文")
            .expect("begin A")
        else {
            panic!("A should start")
        };
        controller
            .commit_new_selection(selection(PopupStatus::SelectionReady, "B"))
            .expect("selection B");
        let before = controller.snapshot.clone();

        assert_eq!(
            controller
                .commit_translation(guard, translated("late A"))
                .expect("stale guard is not an error"),
            TranslationCommit::StaleGuard
        );
        assert_eq!(controller.snapshot, before);
    }

    #[test]
    fn pin_state_survives_translation_and_close() {
        let mut controller = PopupController::default();
        let selected = controller
            .commit_new_selection(selection(PopupStatus::SelectionReady, "A"))
            .expect("selection");
        controller.toggle_pin().expect("pin");
        let BeginTranslation::Started { guard, snapshot } = controller
            .begin_translation(selected.selection_revision, "A", "中文")
            .expect("begin")
        else {
            panic!("translation should start")
        };
        assert!(snapshot.pinned);
        let TranslationCommit::Applied(done) = controller
            .commit_translation(guard, translated("result"))
            .expect("commit")
        else {
            panic!("translation should apply")
        };
        assert!(done.pinned);
        assert!(controller.close().expect("close").pinned);
    }

    #[test]
    fn popup_snapshot_serde_is_protocol_compatible() {
        let mut controller = PopupController::default();
        let snapshot = controller
            .commit_new_selection(selection(PopupStatus::SelectionReady, "snapshot"))
            .expect("selection");
        let value = serde_json::to_value(snapshot).expect("serialize popup state");

        assert_eq!(
            value,
            serde_json::json!({
                "protocolVersion": 1,
                "revision": 1,
                "selectionRevision": 1,
                "visible": true,
                "status": "selection_ready",
                "sourceText": "snapshot",
                "selectedText": "snapshot",
                "cleanedText": "snapshot",
                "translation": null,
                "error": null,
                "errorKind": null,
                "retryable": null,
                "recoveryAction": null,
                "cached": null,
                "targetLanguage": null,
                "pinned": false
            })
        );
    }

    #[test]
    fn one_thousand_out_of_order_guards_cannot_corrupt_the_latest_selection() {
        let mut controller = PopupController::default();
        let mut guards = Vec::with_capacity(1_000);
        for index in 0..1_000 {
            let text = format!("selection-{index}");
            let selected = controller
                .commit_new_selection(selection(PopupStatus::SelectionReady, &text))
                .expect("selection");
            let BeginTranslation::Started { guard, .. } = controller
                .begin_translation(selected.selection_revision, &text, "中文")
                .expect("begin")
            else {
                panic!("translation should start")
            };
            guards.push(guard);
        }

        let mut applied = 0;
        for step in 0..1_000 {
            let index = (step * 997) % 1_000;
            if matches!(
                controller
                    .commit_translation(guards[index], translated(&format!("result-{index}")))
                    .expect("commit"),
                TranslationCommit::Applied(_)
            ) {
                applied += 1;
            }
        }

        assert_eq!(applied, 1);
        assert_eq!(
            controller.snapshot().translation.as_deref(),
            Some("result-999")
        );
    }
}
