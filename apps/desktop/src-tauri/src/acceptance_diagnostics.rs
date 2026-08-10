//! Privacy-preserving, in-memory evidence for selection acceptance testing.
//!
//! This module deliberately stores only identifiers, counters, revisions,
//! monotonic time offsets, and fixed-fixture classifications. It has no field
//! capable of representing selected/cleaned/translated text, clipboard data,
//! window titles, text-derived hashes or lengths, or credentials. Callers must
//! classify a fixed fixture before recording it and pass only [`ExpectedMatch`].

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const ACCEPTANCE_SCHEMA_VERSION: u32 = 1;
pub const ACCEPTANCE_REPORT_VERSION: u32 = 4;
pub const DEFAULT_TAP_RESOLUTION_LIMIT_MS: u64 = 2_000;
pub const RAPID_TRANSITION_MAX_GAP_MS: u64 = 300;
pub const MAX_RECORD_CAPACITY: usize = 100_000;
pub const MAX_MULTI_CLICK_QUIET_WINDOW_MS: u64 = 2_000;
pub const SELF_INTERACTION_BUCKET_COUNT: u8 = 10;
pub const TEXTEDIT_P95_PROCESSING_BUDGET_MS: u64 = 350;
pub const TEXTEDIT_MAX_PROCESSING_BUDGET_MS: u64 = 800;
pub const SAFARI_PREVIEW_PROCESSING_BUDGET_MS: u64 = 1_200;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionPhase {
    Idle,
    Running,
    Ended,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scenario {
    TextEditDrag,
    TextEditDoubleClick,
    TextEditTripleClick,
    TextEditShiftExtend,
    TextEditKeyboardSelection,
    SameTextSameLocation,
    SameTextDifferentLocation,
    SafariFixture,
    PreviewFixture,
    RapidAThenB,
    SelfWindowIsolation,
    TapDisabledRecovery,
    WatcherRestart,
}

impl Scenario {
    fn is_standard_selection(self) -> bool {
        matches!(
            self,
            Self::TextEditDrag
                | Self::TextEditDoubleClick
                | Self::TextEditTripleClick
                | Self::TextEditShiftExtend
                | Self::TextEditKeyboardSelection
                | Self::SameTextSameLocation
                | Self::SameTextDifferentLocation
                | Self::SafariFixture
                | Self::PreviewFixture
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", content = "bundleIdentifier", rename_all = "camelCase")]
pub enum SourceAppIdentifier {
    TextEdit,
    Safari,
    Preview,
    SelfApplication,
    Unknown,
    OtherBundleIdentifier(String),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerReason {
    MouseDrag,
    MouseDoubleClick,
    MouseTripleClick,
    MouseShiftExtend,
    AccessibilityNotification,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionOutcome {
    PopupCommitted,
    NoSelection,
    ReadFailed,
    StaleWriteRejected,
    StaleWriteCommitted,
    Cancelled,
}

impl SelectionOutcome {
    fn wrote_popup(self) -> bool {
        matches!(self, Self::PopupCommitted | Self::StaleWriteCommitted)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExpectedMatch {
    Match,
    A,
    B,
    Mismatch,
    NotApplicable,
}

/// Privacy-safe classification of the mouse-up location used by the two
/// repeated-text scenarios. The raw point never enters an acceptance record.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocationCategory {
    ReferenceA,
    ReferenceB,
}

/// Privacy-safe result of matching an in-memory mouse-up point against the
/// server-owned repeated-location cluster contract.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocationMatchResult {
    ReferenceEstablished,
    MatchedExpectedCluster,
    MismatchedExpectedCluster,
    InsufficientClusterSeparation,
}

impl LocationMatchResult {
    fn passed(self) -> bool {
        matches!(
            self,
            Self::ReferenceEstablished | Self::MatchedExpectedCluster
        )
    }
}

/// The complete location evidence allowed in an exported report. It contains
/// no coordinate, AX range, element identity, or text-derived identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationEvidence {
    pub category: LocationCategory,
    pub result: LocationMatchResult,
}

/// Backend-derived acceptance window. The frontend can never submit this
/// value, so the report cannot be forged with an arbitrary window identifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SelfInteractionWindow {
    Settings,
    Popup,
}

/// Coarse, privacy-safe interaction kind accepted by the Tauri command. No
/// key, target, text, pointer coordinate, or trust flag is representable.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SelfInteractionKind {
    Pointer,
    Keyboard,
}

/// One of four canonical window/kind aggregate cells for the first ten
/// continuous 30-second buckets. Bucket indices are coarse coverage only;
/// individual event offsets are never recorded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfInteractionAggregate {
    pub window: SelfInteractionWindow,
    pub kind: SelfInteractionKind,
    pub count: u64,
    pub bucket_coverage: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfInteractionRecord {
    pub scenario: Scenario,
    pub ordinal: u32,
    pub generation: u64,
    pub aggregates: Vec<SelfInteractionAggregate>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleEvent {
    ScenarioStarted,
    ScenarioEnded,
    TapDisabled,
    TapReady,
    TapDegraded,
    WatcherStarted,
    WatcherReady,
    WatcherDegraded,
    WatcherReleased,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildKind {
    AdHocAcceptance,
    Development,
    DeveloperId,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OperatingSystem {
    MacOs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Architecture {
    Arm64,
    #[allow(dead_code)]
    X86_64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMetadata {
    pub version: String,
    pub bundle_identifier: String,
    pub build_kind: BuildKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMetadata {
    pub operating_system: OperatingSystem,
    pub version: String,
    pub architecture: Architecture,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BaselineApplication {
    TextEdit,
    Safari,
    Preview,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineApplicationMetadata {
    pub application: BaselineApplication,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FixtureKind {
    TextEditPlainText,
    SafariHtml,
    PreviewPdf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureMetadata {
    pub kind: FixtureKind,
    pub fixture_id: String,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyThreshold {
    pub p95_limit_ms: u64,
    pub max_limit_ms: u64,
}

/// Returns the server-owned latency contract for the fixed full baseline.
///
/// The double-click scenario measures from the second mouse-up, so its system
/// multi-click quiet window is separate from (and added to) the ordinary app
/// processing budget. Checked addition and the upper bound make malformed or
/// stale native metadata fail closed.
pub fn full_baseline_latency_threshold(
    scenario: Scenario,
    multi_click_quiet_window_ms: u64,
) -> Option<LatencyThreshold> {
    // Evaluate checked additions before the policy bound so overflow is also
    // an explicit fail-closed path rather than relying on wrapping arithmetic.
    let double_click_threshold = LatencyThreshold {
        p95_limit_ms: multi_click_quiet_window_ms.checked_add(TEXTEDIT_P95_PROCESSING_BUDGET_MS)?,
        max_limit_ms: multi_click_quiet_window_ms.checked_add(TEXTEDIT_MAX_PROCESSING_BUDGET_MS)?,
    };
    if multi_click_quiet_window_ms == 0
        || multi_click_quiet_window_ms > MAX_MULTI_CLICK_QUIET_WINDOW_MS
    {
        return None;
    }

    match scenario {
        Scenario::TextEditDoubleClick => Some(double_click_threshold),
        Scenario::TextEditDrag
        | Scenario::TextEditTripleClick
        | Scenario::TextEditShiftExtend
        | Scenario::TextEditKeyboardSelection
        | Scenario::SameTextSameLocation
        | Scenario::SameTextDifferentLocation => Some(LatencyThreshold {
            p95_limit_ms: TEXTEDIT_P95_PROCESSING_BUDGET_MS,
            max_limit_ms: TEXTEDIT_MAX_PROCESSING_BUDGET_MS,
        }),
        // Configuration stores the non-merge baseline. Report evaluation
        // selects the Q-inclusive browser/PDF threshold when the recorded
        // trigger proves that this particular fixture used double-click.
        Scenario::SafariFixture | Scenario::PreviewFixture => {
            browser_pdf_latency_threshold(multi_click_quiet_window_ms, false)
        }
        Scenario::RapidAThenB
        | Scenario::SelfWindowIsolation
        | Scenario::TapDisabledRecovery
        | Scenario::WatcherRestart => None,
    }
}

fn browser_pdf_latency_threshold(
    multi_click_quiet_window_ms: u64,
    requires_multi_click_merge: bool,
) -> Option<LatencyThreshold> {
    let merged_limit =
        multi_click_quiet_window_ms.checked_add(SAFARI_PREVIEW_PROCESSING_BUDGET_MS)?;
    if multi_click_quiet_window_ms == 0
        || multi_click_quiet_window_ms > MAX_MULTI_CLICK_QUIET_WINDOW_MS
    {
        return None;
    }
    let limit = if requires_multi_click_merge {
        merged_limit
    } else {
        SAFARI_PREVIEW_PROCESSING_BUDGET_MS
    };
    Some(LatencyThreshold {
        p95_limit_ms: limit,
        max_limit_ms: limit,
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionExpectation {
    pub scenario: Scenario,
    pub expected_count: u32,
    pub expected_match: ExpectedMatch,
    pub latency_threshold: Option<LatencyThreshold>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceSessionConfig {
    pub app: AppMetadata,
    pub system: SystemMetadata,
    pub baseline_applications: Vec<BaselineApplicationMetadata>,
    pub fixtures: Vec<FixtureMetadata>,
    pub multi_click_quiet_window_ms: u64,
    pub selection_expectations: Vec<SelectionExpectation>,
    pub rapid_a_to_b_pairs: u32,
    pub tap_recovery_attempts: u32,
    pub tap_resolution_limit_ms: u64,
    pub watcher_restart_count: u32,
    pub self_isolation_min_duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRecord {
    pub scenario: Scenario,
    pub ordinal: u32,
    pub generation: u64,
    pub selection_revision: u64,
    pub popup_revision: Option<u64>,
    /// Selection revision read from the controller snapshot at hook time.
    /// This is never accepted from the frontend.
    pub controller_snapshot_selection_revision: Option<u64>,
    /// Popup revision read from the same controller snapshot at hook time.
    /// This is distinct from `popup_revision`: a rejected stale guard does not
    /// write a popup, but still has to prove which controller snapshot rejected it.
    pub controller_snapshot_popup_revision: Option<u64>,
    pub attempt: u16,
    pub reason: TriggerReason,
    pub source_app: SourceAppIdentifier,
    /// Session-relative offset measured by a monotonic clock.
    pub event_offset_micros: u64,
    /// Session-relative controller write time. Trigger ordering must never be
    /// used as a substitute for the order in which popup writes actually landed.
    pub controller_commit_offset_micros: Option<u64>,
    /// Trigger-to-readable-popup latency measured by a monotonic clock.
    pub end_to_end_latency_ms: u64,
    pub outcome: SelectionOutcome,
    /// Classification produced by comparing against a fixed fixture in memory.
    pub expected_match: ExpectedMatch,
    /// Privacy-safe repeated-location classification. Raw mouse-up coordinates
    /// remain only in the runtime's bounded in-memory cluster state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_evidence: Option<LocationEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleRecord {
    pub scenario: Scenario,
    pub ordinal: u32,
    pub generation: u64,
    pub event_offset_micros: u64,
    pub event: LifecycleEvent,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherResourceCounts {
    pub snapshot_version: u32,
    pub selection_observers: u32,
    pub selection_observer_sources: u32,
    pub key_event_taps: u32,
    pub key_event_tap_sources: u32,
    pub mouse_event_taps: u32,
    pub mouse_event_tap_sources: u32,
    pub pasteboard_timers: u32,
    pub workspace_activation_observers: u32,
    pub callbacks: u32,
    pub contexts: u32,
    pub effective_source_sets: u32,
    pub total_sources: u32,
}

impl WatcherResourceCounts {
    fn component_max(self, other: Self) -> Self {
        Self {
            snapshot_version: self.snapshot_version.max(other.snapshot_version),
            selection_observers: self.selection_observers.max(other.selection_observers),
            selection_observer_sources: self
                .selection_observer_sources
                .max(other.selection_observer_sources),
            key_event_taps: self.key_event_taps.max(other.key_event_taps),
            key_event_tap_sources: self.key_event_tap_sources.max(other.key_event_tap_sources),
            mouse_event_taps: self.mouse_event_taps.max(other.mouse_event_taps),
            mouse_event_tap_sources: self
                .mouse_event_tap_sources
                .max(other.mouse_event_tap_sources),
            pasteboard_timers: self.pasteboard_timers.max(other.pasteboard_timers),
            workspace_activation_observers: self
                .workspace_activation_observers
                .max(other.workspace_activation_observers),
            callbacks: self.callbacks.max(other.callbacks),
            contexts: self.contexts.max(other.contexts),
            effective_source_sets: self.effective_source_sets.max(other.effective_source_sets),
            total_sources: self.total_sources.max(other.total_sources),
        }
    }

    fn calculated_total_sources(self) -> u32 {
        self.selection_observer_sources
            .saturating_add(self.key_event_tap_sources)
            .saturating_add(self.mouse_event_tap_sources)
            .saturating_add(self.pasteboard_timers)
            .saturating_add(self.workspace_activation_observers)
    }

    fn indicates_growth_or_duplicate(self) -> bool {
        self.snapshot_version != 1
            || self.selection_observers > 1
            || self.selection_observer_sources > 1
            || self.selection_observers != self.selection_observer_sources
            || self.key_event_taps > 1
            || self.key_event_tap_sources > 1
            || self.key_event_taps != self.key_event_tap_sources
            || self.mouse_event_taps > 1
            || self.mouse_event_tap_sources > 1
            || self.mouse_event_taps != self.mouse_event_tap_sources
            || self.pasteboard_timers > 1
            || self.workspace_activation_observers > 1
            || self.callbacks > 1
            || self.contexts > 1
            || self.effective_source_sets > 1
            || self.total_sources != self.calculated_total_sources()
            || self.total_sources > 5
    }

    fn is_single_effective_source(self) -> bool {
        !self.indicates_growth_or_duplicate()
            && self.pasteboard_timers == 1
            && self.workspace_activation_observers == 1
            && self.callbacks == 1
            && self.contexts == 1
            && self.effective_source_sets == 1
            && (2..=5).contains(&self.total_sources)
    }

    fn is_fully_released(self) -> bool {
        self.snapshot_version == 1
            && self.selection_observers == 0
            && self.selection_observer_sources == 0
            && self.key_event_taps == 0
            && self.key_event_tap_sources == 0
            && self.mouse_event_taps == 0
            && self.mouse_event_tap_sources == 0
            && self.pasteboard_timers == 0
            && self.workspace_activation_observers == 0
            && self.callbacks == 0
            && self.contexts == 0
            && self.effective_source_sets == 0
            && self.total_sources == 0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherResourceRecord {
    pub scenario: Scenario,
    pub ordinal: u32,
    pub generation: u64,
    pub event_offset_micros: u64,
    pub event: LifecycleEvent,
    pub resources: WatcherResourceCounts,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "recordType", rename_all = "camelCase")]
pub enum AcceptanceRecord {
    Selection(SelectionRecord),
    Lifecycle(LifecycleRecord),
    WatcherResources(WatcherResourceRecord),
    SelfInteraction(SelfInteractionRecord),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioOrdinal {
    pub scenario: Scenario,
    pub ordinal: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionSummary {
    pub expected_count: u64,
    pub popup_commit_count: usize,
    pub missing: Vec<ScenarioOrdinal>,
    pub duplicates: Vec<ScenarioOrdinal>,
    pub unexpected: Vec<ScenarioOrdinal>,
    pub wrong_classification: Vec<ScenarioOrdinal>,
    pub stale_write_committed_count: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencySummary {
    pub sample_count: usize,
    pub p95_ms: Option<u64>,
    pub max_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioLatencySummary {
    pub scenario: Scenario,
    pub latency: LatencySummary,
    pub threshold: Option<LatencyThreshold>,
    pub threshold_passed: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RapidTransitionSummary {
    pub expected_pairs: u32,
    pub max_trigger_gap_ms: u64,
    pub final_b_pairs: u32,
    pub missing_pairs: Vec<u32>,
    pub missing_a_pairs: Vec<u32>,
    pub missing_b_pairs: Vec<u32>,
    pub duplicate_a_pairs: Vec<u32>,
    pub duplicate_b_pairs: Vec<u32>,
    pub out_of_order_pairs: Vec<u32>,
    pub gap_exceeded_pairs: Vec<u32>,
    pub wrong_final_pairs: Vec<u32>,
    pub unexpected_pairs: Vec<u32>,
    pub unexpected_evidence_pairs: Vec<u32>,
    pub unexpected_evidence_count: usize,
    pub stale_a_rejected_count: usize,
    pub missing_stale_rejection_pairs: Vec<u32>,
    pub duplicate_stale_rejection_pairs: Vec<u32>,
    pub invalid_stale_rejection_pairs: Vec<u32>,
    pub stale_a_write_count: usize,
    pub passed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TapRecoverySummary {
    pub expected_attempts: u32,
    pub resolution_limit_ms: u64,
    pub ready_within_limit: u32,
    pub degraded_within_limit: u32,
    pub late: Vec<u32>,
    pub unresolved: Vec<u32>,
    pub unexpected_attempts: Vec<u32>,
    pub duplicate_disabled_events: usize,
    pub duplicate_resolution_events: usize,
    pub max_resolution_ms: Option<u64>,
    pub passed: bool,
}

impl Default for TapRecoverySummary {
    fn default() -> Self {
        Self {
            expected_attempts: 0,
            resolution_limit_ms: DEFAULT_TAP_RESOLUTION_LIMIT_MS,
            ready_within_limit: 0,
            degraded_within_limit: 0,
            late: Vec::new(),
            unresolved: Vec::new(),
            unexpected_attempts: Vec::new(),
            duplicate_disabled_events: 0,
            duplicate_resolution_events: 0,
            max_resolution_ms: None,
            passed: true,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatcherResourceSummary {
    pub expected_restarts: u32,
    pub sample_count: usize,
    pub missing_restarts: Vec<u32>,
    pub missing_release_restarts: Vec<u32>,
    pub missing_started_restarts: Vec<u32>,
    pub unexpected_restarts: Vec<u32>,
    pub duplicate_release_restarts: Vec<u32>,
    pub duplicate_started_restarts: Vec<u32>,
    pub duplicate_active_restarts: Vec<u32>,
    pub generation_mismatch_restarts: Vec<u32>,
    pub out_of_order_restarts: Vec<u32>,
    pub released_resource_leaks: Vec<u32>,
    pub peak: WatcherResourceCounts,
    pub final_active: Option<WatcherResourceCounts>,
    pub growth_or_duplicate_detected: bool,
    pub final_single_effective_source: bool,
    pub passed: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfIsolationSummary {
    pub configured_min_duration_ms: Option<u64>,
    pub observed_duration_ms: Option<u64>,
    pub selection_trigger_count: usize,
    pub start_count: usize,
    pub end_count: usize,
    pub lifecycle_generation_matched: bool,
    pub lifecycle_order_valid: bool,
    pub interaction_record_count: usize,
    pub interaction_generation_matched: bool,
    pub total_interaction_count: u64,
    pub settings_interaction_count: u64,
    pub popup_interaction_count: u64,
    pub pointer_interaction_count: u64,
    pub keyboard_interaction_count: u64,
    pub populated_buckets: Vec<u8>,
    pub settings_buckets: Vec<u8>,
    pub popup_buckets: Vec<u8>,
    pub every_bucket_covered: bool,
    pub window_bucket_coverage_valid: bool,
    pub passed: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateEvidenceSummary {
    pub native_read_replays: usize,
    pub popup_outcome_replays: usize,
    pub tap_status_replays: usize,
    pub watcher_phase_replays: usize,
    pub rapid_probe_replays: usize,
}

impl DuplicateEvidenceSummary {
    fn total(self) -> usize {
        self.native_read_replays
            .saturating_add(self.popup_outcome_replays)
            .saturating_add(self.tap_status_replays)
            .saturating_add(self.watcher_phase_replays)
            .saturating_add(self.rapid_probe_replays)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateEvidenceKind {
    NativeRead,
    PopupOutcome,
    TapStatus,
    WatcherPhase,
    RapidProbe,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceSummary {
    pub total_records: usize,
    pub selection_records: usize,
    pub unique_selection_revisions: usize,
    pub unique_popup_revisions: usize,
    pub duplicate_selection_revisions: Vec<u64>,
    pub duplicate_popup_revisions: Vec<u64>,
    pub duplicate_evidence: DuplicateEvidenceSummary,
    pub selections: SelectionSummary,
    pub latency: LatencySummary,
    pub scenario_latency: Vec<ScenarioLatencySummary>,
    pub rapid_a_to_b: RapidTransitionSummary,
    pub tap_recovery: TapRecoverySummary,
    pub watcher_resources: WatcherResourceSummary,
    pub self_isolation: SelfIsolationSummary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReportInvalidReason {
    CapacityOverflow,
    RejectedRecord,
    MissingSelection,
    DuplicateSelection,
    UnexpectedSelection,
    DuplicateRevision,
    DuplicateEvidence,
    WrongClassification,
    StaleWriteCommitted,
    LatencyThresholdExceeded,
    RapidTransitionFailure,
    TapRecoveryFailure,
    WatcherResourceFailure,
    SelfIsolationFailure,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportStatus {
    /// False when records overflowed or a structurally invalid record was rejected.
    pub integrity_valid: bool,
    /// False when a configured acceptance threshold was not met.
    pub acceptance_passed: bool,
    /// Overall validity: both integrity and configured acceptance checks passed.
    pub valid: bool,
    pub invalid_reasons: Vec<ReportInvalidReason>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClockKind {
    Monotonic,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceReport {
    pub schema_version: u32,
    pub report_version: u32,
    pub clock: ClockKind,
    pub app: AppMetadata,
    pub system: SystemMetadata,
    pub baseline_applications: Vec<BaselineApplicationMetadata>,
    pub fixtures: Vec<FixtureMetadata>,
    pub multi_click_quiet_window_ms: u64,
    pub capacity: usize,
    pub recorded_records: usize,
    pub overflowed_records: usize,
    pub rejected_records: usize,
    pub status: ReportStatus,
    pub summary: AcceptanceSummary,
    pub records: Vec<AcceptanceRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiagnosticsError {
    Disabled,
    InvalidCapacity,
    AlreadyRunning,
    MustClearEndedSession,
    NotRunning,
    ReportNotReady,
    InvalidConfig(&'static str),
    InvalidRecord(&'static str),
    CapacityExceeded,
}

impl fmt::Display for DiagnosticsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Disabled => "acceptance diagnostics require an explicit enable switch",
            Self::InvalidCapacity => "acceptance diagnostics capacity is outside the safe range",
            Self::AlreadyRunning => "an acceptance diagnostics session is already running",
            Self::MustClearEndedSession => {
                "the ended acceptance diagnostics session must be cleared before restart"
            }
            Self::NotRunning => "no acceptance diagnostics session is running",
            Self::ReportNotReady => "end the acceptance diagnostics session before exporting",
            Self::InvalidConfig(message) | Self::InvalidRecord(message) => message,
            Self::CapacityExceeded => {
                "acceptance diagnostics capacity exceeded; the report is now invalid"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for DiagnosticsError {}

struct RunningSession {
    config: AcceptanceSessionConfig,
    records: Vec<AcceptanceRecord>,
    overflowed_records: usize,
    rejected_records: usize,
    duplicate_evidence: DuplicateEvidenceSummary,
}

enum DiagnosticsState {
    Idle,
    Running(Box<RunningSession>),
    Ended(Box<AcceptanceReport>),
}

/// One explicitly enabled, bounded, in-memory acceptance evidence recorder.
pub struct AcceptanceDiagnostics {
    explicitly_enabled: bool,
    capacity: usize,
    state: DiagnosticsState,
}

impl AcceptanceDiagnostics {
    pub fn new(explicitly_enabled: bool, capacity: usize) -> Result<Self, DiagnosticsError> {
        if capacity == 0 || capacity > MAX_RECORD_CAPACITY {
            return Err(DiagnosticsError::InvalidCapacity);
        }

        Ok(Self {
            explicitly_enabled,
            capacity,
            state: DiagnosticsState::Idle,
        })
    }

    pub fn phase(&self) -> SessionPhase {
        match self.state {
            DiagnosticsState::Idle => SessionPhase::Idle,
            DiagnosticsState::Running(_) => SessionPhase::Running,
            DiagnosticsState::Ended(_) => SessionPhase::Ended,
        }
    }

    pub fn start(&mut self, config: AcceptanceSessionConfig) -> Result<(), DiagnosticsError> {
        if !self.explicitly_enabled {
            return Err(DiagnosticsError::Disabled);
        }
        validate_config(&config)?;

        match self.state {
            DiagnosticsState::Idle => {
                self.state = DiagnosticsState::Running(Box::new(RunningSession {
                    config,
                    records: Vec::with_capacity(self.capacity),
                    overflowed_records: 0,
                    rejected_records: 0,
                    duplicate_evidence: DuplicateEvidenceSummary::default(),
                }));
                Ok(())
            }
            DiagnosticsState::Running(_) => Err(DiagnosticsError::AlreadyRunning),
            DiagnosticsState::Ended(_) => Err(DiagnosticsError::MustClearEndedSession),
        }
    }

    pub fn record(&mut self, record: AcceptanceRecord) -> Result<(), DiagnosticsError> {
        let capacity = self.capacity;
        let DiagnosticsState::Running(running) = &mut self.state else {
            return Err(DiagnosticsError::NotRunning);
        };

        if let Err(error) = validate_record(&record, &running.config) {
            running.rejected_records = running.rejected_records.saturating_add(1);
            return Err(error);
        }

        if running.records.len() >= capacity {
            running.overflowed_records = running.overflowed_records.saturating_add(1);
            return Err(DiagnosticsError::CapacityExceeded);
        }

        running.records.push(record);
        Ok(())
    }

    /// Invalidates the running report when an observation is rejected before
    /// it can safely be represented by the content-free record schema.
    pub fn mark_rejected_record(&mut self) -> Result<(), DiagnosticsError> {
        let DiagnosticsState::Running(running) = &mut self.state else {
            return Err(DiagnosticsError::NotRunning);
        };
        running.rejected_records = running.rejected_records.saturating_add(1);
        Ok(())
    }

    pub fn mark_duplicate_evidence(
        &mut self,
        kind: DuplicateEvidenceKind,
    ) -> Result<(), DiagnosticsError> {
        let DiagnosticsState::Running(running) = &mut self.state else {
            return Err(DiagnosticsError::NotRunning);
        };
        let counter = match kind {
            DuplicateEvidenceKind::NativeRead => {
                &mut running.duplicate_evidence.native_read_replays
            }
            DuplicateEvidenceKind::PopupOutcome => {
                &mut running.duplicate_evidence.popup_outcome_replays
            }
            DuplicateEvidenceKind::TapStatus => &mut running.duplicate_evidence.tap_status_replays,
            DuplicateEvidenceKind::WatcherPhase => {
                &mut running.duplicate_evidence.watcher_phase_replays
            }
            DuplicateEvidenceKind::RapidProbe => {
                &mut running.duplicate_evidence.rapid_probe_replays
            }
        };
        *counter = counter.saturating_add(1);
        running.rejected_records = running.rejected_records.saturating_add(1);
        Ok(())
    }

    /// Invalidates the running report when a bounded runtime-side index reaches
    /// the same capacity as the record store.
    pub fn mark_capacity_overflow(&mut self) -> Result<(), DiagnosticsError> {
        let DiagnosticsState::Running(running) = &mut self.state else {
            return Err(DiagnosticsError::NotRunning);
        };
        running.overflowed_records = running.overflowed_records.saturating_add(1);
        Ok(())
    }

    pub fn end(&mut self) -> Result<AcceptanceReport, DiagnosticsError> {
        let previous = std::mem::replace(&mut self.state, DiagnosticsState::Idle);
        let DiagnosticsState::Running(running) = previous else {
            self.state = previous;
            return Err(DiagnosticsError::NotRunning);
        };

        let report = build_report(*running, self.capacity);
        self.state = DiagnosticsState::Ended(Box::new(report.clone()));
        Ok(report)
    }

    pub fn export_snapshot(&self) -> Result<AcceptanceReport, DiagnosticsError> {
        match &self.state {
            DiagnosticsState::Ended(report) => Ok((**report).clone()),
            DiagnosticsState::Idle | DiagnosticsState::Running(_) => {
                Err(DiagnosticsError::ReportNotReady)
            }
        }
    }

    pub fn clear(&mut self) {
        self.state = DiagnosticsState::Idle;
    }
}

fn validate_config(config: &AcceptanceSessionConfig) -> Result<(), DiagnosticsError> {
    if !is_safe_version_token(&config.app.version)
        || !is_safe_bundle_identifier(&config.app.bundle_identifier)
        || !is_safe_version_token(&config.system.version)
    {
        return Err(DiagnosticsError::InvalidConfig(
            "application and system metadata must use bounded identifier/version tokens",
        ));
    }
    if full_baseline_latency_threshold(
        Scenario::TextEditDoubleClick,
        config.multi_click_quiet_window_ms,
    )
    .is_none()
    {
        return Err(DiagnosticsError::InvalidConfig(
            "multi-click quiet window must be positive, bounded, and overflow-safe",
        ));
    }

    let baseline_applications = config
        .baseline_applications
        .iter()
        .map(|metadata| metadata.application)
        .collect::<BTreeSet<_>>();
    if config.baseline_applications.len() != 3
        || baseline_applications.len() != 3
        || config
            .baseline_applications
            .iter()
            .any(|metadata| !is_safe_version_token(&metadata.version))
    {
        return Err(DiagnosticsError::InvalidConfig(
            "baseline application metadata must contain one bounded version for TextEdit, Safari, and Preview",
        ));
    }

    let fixture_kinds = config
        .fixtures
        .iter()
        .map(|metadata| metadata.kind)
        .collect::<BTreeSet<_>>();
    if config.fixtures.len() != 3
        || fixture_kinds.len() != 3
        || config.fixtures.iter().any(|metadata| {
            !is_safe_version_token(&metadata.fixture_id)
                || metadata.sha256.len() != 64
                || !metadata.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err(DiagnosticsError::InvalidConfig(
            "fixture metadata must contain one bounded identifier and SHA-256 for each fixed fixture",
        ));
    }

    let mut scenarios = BTreeSet::new();
    for expectation in &config.selection_expectations {
        if !expectation.scenario.is_standard_selection() {
            return Err(DiagnosticsError::InvalidConfig(
                "selection expectations may contain only standard selection scenarios",
            ));
        }
        if expectation.expected_count == 0 || !scenarios.insert(expectation.scenario) {
            return Err(DiagnosticsError::InvalidConfig(
                "selection expectations require a positive count and unique scenario",
            ));
        }
        if matches!(
            expectation.expected_match,
            ExpectedMatch::Mismatch | ExpectedMatch::A | ExpectedMatch::B
        ) {
            return Err(DiagnosticsError::InvalidConfig(
                "standard selection expectations must be match or notApplicable",
            ));
        }
        if let Some(threshold) = expectation.latency_threshold {
            if threshold.p95_limit_ms == 0
                || threshold.max_limit_ms == 0
                || threshold.p95_limit_ms > threshold.max_limit_ms
            {
                return Err(DiagnosticsError::InvalidConfig(
                    "latency thresholds must be positive and p95 must not exceed max",
                ));
            }
        }
    }

    let full_baseline_scenarios = [
        Scenario::TextEditDrag,
        Scenario::TextEditDoubleClick,
        Scenario::TextEditTripleClick,
        Scenario::TextEditShiftExtend,
        Scenario::TextEditKeyboardSelection,
        Scenario::SameTextSameLocation,
        Scenario::SameTextDifferentLocation,
        Scenario::SafariFixture,
        Scenario::PreviewFixture,
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if scenarios == full_baseline_scenarios {
        for expectation in &config.selection_expectations {
            let expected = full_baseline_latency_threshold(
                expectation.scenario,
                config.multi_click_quiet_window_ms,
            )
            .ok_or(DiagnosticsError::InvalidConfig(
                "full baseline latency threshold cannot be represented safely",
            ))?;
            if expectation.latency_threshold != Some(expected) {
                return Err(DiagnosticsError::InvalidConfig(
                    "full baseline latency thresholds drifted from the server-owned contract",
                ));
            }
        }
    }

    if config.tap_recovery_attempts > 0 && config.tap_resolution_limit_ms == 0 {
        return Err(DiagnosticsError::InvalidConfig(
            "tap recovery requires a positive resolution limit",
        ));
    }
    if matches!(config.self_isolation_min_duration_ms, Some(0)) {
        return Err(DiagnosticsError::InvalidConfig(
            "self-isolation duration must be positive when configured",
        ));
    }
    if config.selection_expectations.is_empty()
        && config.rapid_a_to_b_pairs == 0
        && config.tap_recovery_attempts == 0
        && config.watcher_restart_count == 0
        && config.self_isolation_min_duration_ms.is_none()
    {
        return Err(DiagnosticsError::InvalidConfig(
            "at least one acceptance scenario must be configured",
        ));
    }

    Ok(())
}

fn validate_record(
    record: &AcceptanceRecord,
    config: &AcceptanceSessionConfig,
) -> Result<(), DiagnosticsError> {
    match record {
        AcceptanceRecord::Selection(record) => validate_selection_record(record, config),
        AcceptanceRecord::Lifecycle(record) => validate_lifecycle_record(record, config),
        AcceptanceRecord::WatcherResources(record) => {
            if config.watcher_restart_count == 0
                || record.scenario != Scenario::WatcherRestart
                || record.ordinal == 0
                || record.generation == 0
                || !matches!(
                    record.event,
                    LifecycleEvent::WatcherStarted
                        | LifecycleEvent::WatcherReady
                        | LifecycleEvent::WatcherDegraded
                        | LifecycleEvent::WatcherReleased
                )
            {
                return Err(DiagnosticsError::InvalidRecord(
                    "watcher resource record does not match the configured watcher scenario",
                ));
            }
            Ok(())
        }
        AcceptanceRecord::SelfInteraction(record) => {
            validate_self_interaction_record(record, config)
        }
    }
}

fn validate_self_interaction_record(
    record: &SelfInteractionRecord,
    config: &AcceptanceSessionConfig,
) -> Result<(), DiagnosticsError> {
    if config.self_isolation_min_duration_ms.is_none()
        || record.scenario != Scenario::SelfWindowIsolation
        || record.ordinal != 1
        || record.generation == 0
    {
        return Err(DiagnosticsError::InvalidRecord(
            "self-interaction aggregate does not match the configured isolation scenario",
        ));
    }
    let expected_cells = [
        (
            SelfInteractionWindow::Settings,
            SelfInteractionKind::Pointer,
        ),
        (
            SelfInteractionWindow::Settings,
            SelfInteractionKind::Keyboard,
        ),
        (SelfInteractionWindow::Popup, SelfInteractionKind::Pointer),
        (SelfInteractionWindow::Popup, SelfInteractionKind::Keyboard),
    ];
    if record.aggregates.len() != expected_cells.len() {
        return Err(DiagnosticsError::InvalidRecord(
            "self-interaction aggregate requires exactly four canonical window-kind cells",
        ));
    }
    for (aggregate, expected) in record.aggregates.iter().zip(expected_cells) {
        if (aggregate.window, aggregate.kind) != expected
            || aggregate
                .bucket_coverage
                .iter()
                .any(|bucket| *bucket >= SELF_INTERACTION_BUCKET_COUNT)
            || aggregate
                .bucket_coverage
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || aggregate.count < aggregate.bucket_coverage.len() as u64
            || (aggregate.count == 0) != aggregate.bucket_coverage.is_empty()
        {
            return Err(DiagnosticsError::InvalidRecord(
                "self-interaction aggregate is non-canonical or internally inconsistent",
            ));
        }
    }
    Ok(())
}

fn validate_selection_record(
    record: &SelectionRecord,
    config: &AcceptanceSessionConfig,
) -> Result<(), DiagnosticsError> {
    let configured = if record.scenario.is_standard_selection() {
        config
            .selection_expectations
            .iter()
            .any(|expectation| expectation.scenario == record.scenario)
    } else {
        match record.scenario {
            Scenario::RapidAThenB => config.rapid_a_to_b_pairs > 0,
            Scenario::SelfWindowIsolation => config.self_isolation_min_duration_ms.is_some(),
            _ => false,
        }
    };

    if !configured || record.ordinal == 0 || record.generation == 0 || record.attempt == 0 {
        return Err(DiagnosticsError::InvalidRecord(
            "selection record does not match a configured scenario or has a zero identifier",
        ));
    }
    if record.outcome.wrote_popup()
        && (record.selection_revision == 0
            || record.popup_revision.is_none_or(|revision| revision == 0)
            || record
                .controller_snapshot_selection_revision
                .is_none_or(|revision| revision == 0)
            || record
                .controller_snapshot_popup_revision
                .is_none_or(|revision| revision == 0)
            || record.controller_commit_offset_micros.is_none())
    {
        return Err(DiagnosticsError::InvalidRecord(
            "a popup write outcome requires controller-confirmed revisions and a commit time",
        ));
    }
    if !record.outcome.wrote_popup()
        && (record.popup_revision.is_some() || record.controller_commit_offset_micros.is_some())
    {
        return Err(DiagnosticsError::InvalidRecord(
            "a non-write outcome cannot claim a popup revision or controller commit time",
        ));
    }
    if record
        .controller_commit_offset_micros
        .is_some_and(|offset| offset < record.event_offset_micros)
    {
        return Err(DiagnosticsError::InvalidRecord(
            "a controller commit cannot precede its selection trigger",
        ));
    }
    if record.popup_revision == Some(0) {
        return Err(DiagnosticsError::InvalidRecord(
            "a present popup revision must be non-zero",
        ));
    }
    if record.controller_snapshot_selection_revision == Some(0) {
        return Err(DiagnosticsError::InvalidRecord(
            "a present controller snapshot selection revision must be non-zero",
        ));
    }
    if record.controller_snapshot_popup_revision == Some(0) {
        return Err(DiagnosticsError::InvalidRecord(
            "a present controller snapshot popup revision must be non-zero",
        ));
    }
    if record.outcome == SelectionOutcome::PopupCommitted
        && (record.controller_snapshot_selection_revision != Some(record.selection_revision)
            || record.controller_snapshot_popup_revision != record.popup_revision)
    {
        return Err(DiagnosticsError::InvalidRecord(
            "a popup commit must be confirmed by the matching controller snapshot revisions",
        ));
    }
    if record.outcome == SelectionOutcome::StaleWriteRejected
        && (record.controller_snapshot_selection_revision.is_some()
            != record.controller_snapshot_popup_revision.is_some()
            || (record.controller_snapshot_selection_revision.is_some()
                && record.selection_revision == 0))
    {
        return Err(DiagnosticsError::InvalidRecord(
            "a controller-confirmed stale guard requires a guard and both snapshot revisions",
        ));
    }
    if let SourceAppIdentifier::OtherBundleIdentifier(identifier) = &record.source_app {
        if !is_safe_bundle_identifier(identifier) {
            return Err(DiagnosticsError::InvalidRecord(
                "source application must be represented by a valid bundle identifier",
            ));
        }
    }
    validate_location_evidence(record)?;
    Ok(())
}

fn validate_location_evidence(record: &SelectionRecord) -> Result<(), DiagnosticsError> {
    let is_location_scenario = matches!(
        record.scenario,
        Scenario::SameTextSameLocation | Scenario::SameTextDifferentLocation
    );
    if !is_location_scenario {
        if record.location_evidence.is_some() {
            return Err(DiagnosticsError::InvalidRecord(
                "location evidence is allowed only for repeated-location scenarios",
            ));
        }
        return Ok(());
    }
    if record.outcome != SelectionOutcome::PopupCommitted {
        if record.location_evidence.is_some() {
            return Err(DiagnosticsError::InvalidRecord(
                "only a committed repeated-location popup may claim location evidence",
            ));
        }
        return Ok(());
    }
    if record.reason != TriggerReason::MouseDrag {
        return Err(DiagnosticsError::InvalidRecord(
            "repeated-location evidence requires a mouse-drag trigger",
        ));
    }
    let Some(evidence) = record.location_evidence else {
        return Err(DiagnosticsError::InvalidRecord(
            "a committed repeated-location popup requires privacy-safe location evidence",
        ));
    };
    let expected_category =
        if record.scenario == Scenario::SameTextSameLocation || record.ordinal % 2 == 1 {
            LocationCategory::ReferenceA
        } else {
            LocationCategory::ReferenceB
        };
    if evidence.category != expected_category {
        return Err(DiagnosticsError::InvalidRecord(
            "repeated-location category does not match the server-owned ordinal contract",
        ));
    }
    let result_allowed = match (record.scenario, record.ordinal) {
        (Scenario::SameTextSameLocation, 1) | (Scenario::SameTextDifferentLocation, 1) => {
            evidence.result == LocationMatchResult::ReferenceEstablished
        }
        (Scenario::SameTextDifferentLocation, 2) => matches!(
            evidence.result,
            LocationMatchResult::ReferenceEstablished
                | LocationMatchResult::MismatchedExpectedCluster
                | LocationMatchResult::InsufficientClusterSeparation
        ),
        (Scenario::SameTextSameLocation | Scenario::SameTextDifferentLocation, _) => matches!(
            evidence.result,
            LocationMatchResult::MatchedExpectedCluster
                | LocationMatchResult::MismatchedExpectedCluster
                | LocationMatchResult::InsufficientClusterSeparation
        ),
        _ => false,
    };
    if !result_allowed {
        return Err(DiagnosticsError::InvalidRecord(
            "repeated-location result does not match the server-owned ordinal contract",
        ));
    }
    Ok(())
}

fn validate_lifecycle_record(
    record: &LifecycleRecord,
    config: &AcceptanceSessionConfig,
) -> Result<(), DiagnosticsError> {
    if record.ordinal == 0 || record.generation == 0 {
        return Err(DiagnosticsError::InvalidRecord(
            "lifecycle record identifiers must be non-zero",
        ));
    }

    let configured_event = match record.scenario {
        Scenario::TapDisabledRecovery => {
            config.tap_recovery_attempts > 0
                && matches!(
                    record.event,
                    LifecycleEvent::TapDisabled
                        | LifecycleEvent::TapReady
                        | LifecycleEvent::TapDegraded
                )
        }
        Scenario::SelfWindowIsolation => {
            config.self_isolation_min_duration_ms.is_some()
                && matches!(
                    record.event,
                    LifecycleEvent::ScenarioStarted | LifecycleEvent::ScenarioEnded
                )
        }
        Scenario::WatcherRestart => {
            config.watcher_restart_count > 0
                && matches!(
                    record.event,
                    LifecycleEvent::WatcherStarted
                        | LifecycleEvent::WatcherReady
                        | LifecycleEvent::WatcherDegraded
                        | LifecycleEvent::WatcherReleased
                )
        }
        _ => false,
    };

    if !configured_event {
        return Err(DiagnosticsError::InvalidRecord(
            "lifecycle event does not match its configured scenario",
        ));
    }
    Ok(())
}

fn is_safe_version_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
}

fn is_safe_bundle_identifier(value: &str) -> bool {
    if value.is_empty() || value.len() > 255 || !value.contains('.') {
        return false;
    }
    value.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn build_report(running: RunningSession, capacity: usize) -> AcceptanceReport {
    let summary = build_summary(
        &running.config,
        &running.records,
        running.duplicate_evidence,
    );
    let mut invalid_reasons = report_invalid_reasons(
        &summary,
        running.overflowed_records,
        running.rejected_records,
    );
    invalid_reasons.sort_unstable_by_key(|reason| *reason as u8);
    invalid_reasons.dedup();

    let integrity_valid = running.overflowed_records == 0 && running.rejected_records == 0;
    let acceptance_passed = invalid_reasons.iter().all(|reason| {
        matches!(
            reason,
            ReportInvalidReason::CapacityOverflow
                | ReportInvalidReason::RejectedRecord
                | ReportInvalidReason::DuplicateEvidence
        )
    });
    let status = ReportStatus {
        integrity_valid,
        acceptance_passed,
        valid: integrity_valid && acceptance_passed,
        invalid_reasons,
    };

    AcceptanceReport {
        schema_version: ACCEPTANCE_SCHEMA_VERSION,
        report_version: ACCEPTANCE_REPORT_VERSION,
        clock: ClockKind::Monotonic,
        app: running.config.app,
        system: running.config.system,
        baseline_applications: running.config.baseline_applications,
        fixtures: running.config.fixtures,
        multi_click_quiet_window_ms: running.config.multi_click_quiet_window_ms,
        capacity,
        recorded_records: running.records.len(),
        overflowed_records: running.overflowed_records,
        rejected_records: running.rejected_records,
        status,
        summary,
        records: running.records,
    }
}

fn build_summary(
    config: &AcceptanceSessionConfig,
    records: &[AcceptanceRecord],
    duplicate_evidence: DuplicateEvidenceSummary,
) -> AcceptanceSummary {
    let selections: Vec<&SelectionRecord> = records
        .iter()
        .filter_map(|record| match record {
            AcceptanceRecord::Selection(selection) => Some(selection),
            AcceptanceRecord::Lifecycle(_)
            | AcceptanceRecord::WatcherResources(_)
            | AcceptanceRecord::SelfInteraction(_) => None,
        })
        .collect();
    let popup_writes: Vec<&SelectionRecord> = selections
        .iter()
        .copied()
        .filter(|record| record.outcome.wrote_popup())
        .collect();

    let (unique_selection_revisions, duplicate_selection_revisions) =
        revision_counts(popup_writes.iter().map(|record| record.selection_revision));
    let (unique_popup_revisions, duplicate_popup_revisions) = revision_counts(
        popup_writes
            .iter()
            .filter_map(|record| record.popup_revision),
    );

    let latency_values: Vec<u64> = popup_writes
        .iter()
        .map(|record| record.end_to_end_latency_ms)
        .collect();
    let latency = summarize_latency(&latency_values);
    let scenario_latency = summarize_scenario_latency(config, &popup_writes);

    AcceptanceSummary {
        total_records: records.len(),
        selection_records: selections.len(),
        unique_selection_revisions,
        unique_popup_revisions,
        duplicate_selection_revisions,
        duplicate_popup_revisions,
        duplicate_evidence,
        selections: summarize_standard_selections(config, &popup_writes),
        latency,
        scenario_latency,
        rapid_a_to_b: summarize_rapid_transitions(config, &selections),
        tap_recovery: summarize_tap_recovery(config, records),
        watcher_resources: summarize_watcher_resources(config, records),
        self_isolation: summarize_self_isolation(config, records, &selections),
    }
}

fn revision_counts<I>(revisions: I) -> (usize, Vec<u64>)
where
    I: IntoIterator<Item = u64>,
{
    let mut counts = BTreeMap::<u64, usize>::new();
    for revision in revisions {
        *counts.entry(revision).or_default() += 1;
    }
    let duplicates = counts
        .iter()
        .filter_map(|(revision, count)| (*count > 1).then_some(*revision))
        .collect();
    (counts.len(), duplicates)
}

fn summarize_standard_selections(
    config: &AcceptanceSessionConfig,
    popup_writes: &[&SelectionRecord],
) -> SelectionSummary {
    let mut summary = SelectionSummary::default();
    let expectations: BTreeMap<Scenario, &SelectionExpectation> = config
        .selection_expectations
        .iter()
        .map(|expectation| (expectation.scenario, expectation))
        .collect();
    summary.expected_count = config
        .selection_expectations
        .iter()
        .map(|expectation| u64::from(expectation.expected_count))
        .sum();

    let mut writes_by_key = BTreeMap::<ScenarioOrdinal, Vec<&SelectionRecord>>::new();
    for record in popup_writes
        .iter()
        .copied()
        .filter(|record| record.scenario.is_standard_selection())
    {
        writes_by_key
            .entry(ScenarioOrdinal {
                scenario: record.scenario,
                ordinal: record.ordinal,
            })
            .or_default()
            .push(record);
    }
    summary.popup_commit_count = writes_by_key.values().map(Vec::len).sum();

    for expectation in &config.selection_expectations {
        for ordinal in 1..=expectation.expected_count {
            let key = ScenarioOrdinal {
                scenario: expectation.scenario,
                ordinal,
            };
            match writes_by_key.get(&key) {
                None => summary.missing.push(key),
                Some(records) => {
                    if records.len() > 1 {
                        summary.duplicates.push(key);
                    }
                    if expectation.expected_match != ExpectedMatch::NotApplicable
                        && records.iter().any(|record| {
                            record.expected_match != expectation.expected_match
                                || !location_evidence_passed(record)
                        })
                    {
                        summary.wrong_classification.push(key);
                    }
                }
            }
        }
    }

    for key in writes_by_key.keys() {
        if expectations
            .get(&key.scenario)
            .is_some_and(|expectation| key.ordinal > expectation.expected_count)
        {
            summary.unexpected.push(*key);
        }
    }
    summary.stale_write_committed_count = popup_writes
        .iter()
        .filter(|record| record.outcome == SelectionOutcome::StaleWriteCommitted)
        .count();
    summary
}

fn location_evidence_passed(record: &SelectionRecord) -> bool {
    if !matches!(
        record.scenario,
        Scenario::SameTextSameLocation | Scenario::SameTextDifferentLocation
    ) {
        return true;
    }
    record
        .location_evidence
        .is_some_and(|evidence| evidence.result.passed())
}

fn summarize_latency(values: &[u64]) -> LatencySummary {
    if values.is_empty() {
        return LatencySummary::default();
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p95_rank = (sorted.len() * 95).div_ceil(100);
    LatencySummary {
        sample_count: sorted.len(),
        p95_ms: sorted.get(p95_rank.saturating_sub(1)).copied(),
        max_ms: sorted.last().copied(),
    }
}

fn summarize_scenario_latency(
    config: &AcceptanceSessionConfig,
    popup_writes: &[&SelectionRecord],
) -> Vec<ScenarioLatencySummary> {
    config
        .selection_expectations
        .iter()
        .map(|expectation| {
            let scenario_records: Vec<&SelectionRecord> = popup_writes
                .iter()
                .filter(|record| record.scenario == expectation.scenario)
                .copied()
                .collect();
            let values: Vec<u64> = scenario_records
                .iter()
                .map(|record| record.end_to_end_latency_ms)
                .collect();
            let latency = summarize_latency(&values);
            let (threshold, threshold_contract_valid) = if matches!(
                expectation.scenario,
                Scenario::SafariFixture | Scenario::PreviewFixture
            ) {
                let has_double = scenario_records
                    .iter()
                    .any(|record| record.reason == TriggerReason::MouseDoubleClick);
                let has_non_double = scenario_records
                    .iter()
                    .any(|record| record.reason != TriggerReason::MouseDoubleClick);
                if has_double && has_non_double {
                    // A fixed baseline expects one browser/PDF sample. Never
                    // hide a malformed mixed-path configuration behind the
                    // more permissive Q-inclusive threshold.
                    (None, false)
                } else {
                    (
                        browser_pdf_latency_threshold(
                            config.multi_click_quiet_window_ms,
                            has_double,
                        ),
                        true,
                    )
                }
            } else {
                (expectation.latency_threshold, true)
            };
            let threshold_passed = threshold_contract_valid
                && threshold.is_none_or(|threshold| {
                    latency
                        .p95_ms
                        .is_some_and(|value| value <= threshold.p95_limit_ms)
                        && latency
                            .max_ms
                            .is_some_and(|value| value <= threshold.max_limit_ms)
                });
            ScenarioLatencySummary {
                scenario: expectation.scenario,
                latency,
                threshold,
                threshold_passed,
            }
        })
        .collect()
}

fn summarize_rapid_transitions(
    config: &AcceptanceSessionConfig,
    selections: &[&SelectionRecord],
) -> RapidTransitionSummary {
    let expected_pairs = config.rapid_a_to_b_pairs;
    if expected_pairs == 0 {
        return RapidTransitionSummary {
            max_trigger_gap_ms: RAPID_TRANSITION_MAX_GAP_MS,
            passed: true,
            ..RapidTransitionSummary::default()
        };
    }

    let mut summary = RapidTransitionSummary {
        expected_pairs,
        max_trigger_gap_ms: RAPID_TRANSITION_MAX_GAP_MS,
        ..RapidTransitionSummary::default()
    };
    let mut by_ordinal = BTreeMap::<u32, Vec<&SelectionRecord>>::new();
    for record in selections
        .iter()
        .copied()
        .filter(|record| record.scenario == Scenario::RapidAThenB)
    {
        by_ordinal.entry(record.ordinal).or_default().push(record);
    }

    for ordinal in 1..=expected_pairs {
        let Some(records) = by_ordinal.get_mut(&ordinal) else {
            summary.missing_pairs.push(ordinal);
            continue;
        };
        records
            .sort_unstable_by_key(|record| (record.event_offset_micros, record.selection_revision));
        let a_records: Vec<&&SelectionRecord> = records
            .iter()
            .filter(|record| {
                record.expected_match == ExpectedMatch::A
                    && record.outcome == SelectionOutcome::PopupCommitted
            })
            .collect();
        let b_records: Vec<&&SelectionRecord> = records
            .iter()
            .filter(|record| {
                record.expected_match == ExpectedMatch::B
                    && record.outcome == SelectionOutcome::PopupCommitted
            })
            .collect();

        let unexpected_evidence_count = records
            .iter()
            .filter(|record| {
                !matches!(
                    (record.expected_match, record.outcome),
                    (
                        ExpectedMatch::A | ExpectedMatch::B,
                        SelectionOutcome::PopupCommitted
                    ) | (ExpectedMatch::A, SelectionOutcome::StaleWriteRejected)
                )
            })
            .count();
        if unexpected_evidence_count > 0 {
            summary.unexpected_evidence_pairs.push(ordinal);
            summary.unexpected_evidence_count = summary
                .unexpected_evidence_count
                .saturating_add(unexpected_evidence_count);
        }

        if a_records.is_empty() {
            summary.missing_a_pairs.push(ordinal);
        } else if a_records.len() > 1 {
            summary.duplicate_a_pairs.push(ordinal);
        }
        if b_records.is_empty() {
            summary.missing_b_pairs.push(ordinal);
        } else if b_records.len() > 1 {
            summary.duplicate_b_pairs.push(ordinal);
        }

        let earliest_a = a_records.first().map(|record| record.event_offset_micros);
        let earliest_b = b_records.first().map(|record| record.event_offset_micros);
        if let (Some(a_offset), Some(b_offset)) = (earliest_a, earliest_b) {
            if b_offset < a_offset {
                summary.out_of_order_pairs.push(ordinal);
            } else if b_offset.saturating_sub(a_offset)
                > RAPID_TRANSITION_MAX_GAP_MS.saturating_mul(1_000)
            {
                summary.gap_exceeded_pairs.push(ordinal);
            }
        }
        let b_selection_revision = b_records.first().map(|record| record.selection_revision);
        let b_popup_revision = b_records.first().and_then(|record| record.popup_revision);
        let earliest_b_commit = b_records
            .iter()
            .filter_map(|record| controller_commit_order(record))
            .min();
        let stale_rejections = records
            .iter()
            .filter(|record| {
                record.expected_match == ExpectedMatch::A
                    && record.outcome == SelectionOutcome::StaleWriteRejected
            })
            .collect::<Vec<_>>();
        summary.stale_a_rejected_count += stale_rejections.len();
        if stale_rejections.is_empty() {
            summary.missing_stale_rejection_pairs.push(ordinal);
        } else if stale_rejections.len() > 1 {
            summary.duplicate_stale_rejection_pairs.push(ordinal);
        }
        if stale_rejections.iter().any(|record| {
            b_selection_revision.is_none()
                || record.controller_snapshot_selection_revision != b_selection_revision
                || b_popup_revision.is_none()
                || record.controller_snapshot_popup_revision != b_popup_revision
        }) {
            summary.invalid_stale_rejection_pairs.push(ordinal);
        }
        summary.stale_a_write_count += records
            .iter()
            .filter(|record| {
                record.expected_match == ExpectedMatch::A
                    && record.outcome.wrote_popup()
                    && (record.outcome == SelectionOutcome::StaleWriteCommitted
                        || earliest_b_commit.is_some_and(|b_commit| {
                            controller_commit_order(record).is_some_and(|commit| commit > b_commit)
                        }))
            })
            .count();

        let final_write = records
            .iter()
            .filter(|record| record.outcome.wrote_popup())
            .max_by_key(|record| controller_commit_order(record));
        match final_write {
            None => summary.missing_pairs.push(ordinal),
            Some(record) if record.expected_match == ExpectedMatch::B => {
                summary.final_b_pairs += 1;
            }
            Some(_) => summary.wrong_final_pairs.push(ordinal),
        }
    }
    summary.unexpected_pairs = by_ordinal
        .keys()
        .copied()
        .filter(|ordinal| *ordinal > expected_pairs)
        .collect();
    summary.passed = summary.final_b_pairs == expected_pairs
        && summary.missing_pairs.is_empty()
        && summary.missing_a_pairs.is_empty()
        && summary.missing_b_pairs.is_empty()
        && summary.duplicate_a_pairs.is_empty()
        && summary.duplicate_b_pairs.is_empty()
        && summary.out_of_order_pairs.is_empty()
        && summary.gap_exceeded_pairs.is_empty()
        && summary.wrong_final_pairs.is_empty()
        && summary.unexpected_pairs.is_empty()
        && summary.unexpected_evidence_pairs.is_empty()
        && summary.unexpected_evidence_count == 0
        && summary.missing_stale_rejection_pairs.is_empty()
        && summary.duplicate_stale_rejection_pairs.is_empty()
        && summary.invalid_stale_rejection_pairs.is_empty()
        && summary.stale_a_write_count == 0;
    summary
}

fn controller_commit_order(record: &SelectionRecord) -> Option<(u64, u64, u64)> {
    Some((
        record.controller_commit_offset_micros?,
        record.popup_revision?,
        record.selection_revision,
    ))
}

fn summarize_tap_recovery(
    config: &AcceptanceSessionConfig,
    records: &[AcceptanceRecord],
) -> TapRecoverySummary {
    let expected_attempts = config.tap_recovery_attempts;
    let mut summary = TapRecoverySummary {
        expected_attempts,
        resolution_limit_ms: config.tap_resolution_limit_ms,
        ..TapRecoverySummary::default()
    };
    if expected_attempts == 0 {
        summary.passed = true;
        return summary;
    }

    let mut by_ordinal = BTreeMap::<u32, Vec<&LifecycleRecord>>::new();
    for record in records {
        if let AcceptanceRecord::Lifecycle(record) = record {
            if record.scenario == Scenario::TapDisabledRecovery {
                by_ordinal.entry(record.ordinal).or_default().push(record);
            }
        }
    }

    for ordinal in 1..=expected_attempts {
        let Some(events) = by_ordinal.get_mut(&ordinal) else {
            summary.unresolved.push(ordinal);
            continue;
        };
        events.sort_unstable_by_key(|event| event.event_offset_micros);
        let disabled: Vec<&&LifecycleRecord> = events
            .iter()
            .filter(|event| event.event == LifecycleEvent::TapDisabled)
            .collect();
        let resolutions: Vec<&&LifecycleRecord> = events
            .iter()
            .filter(|event| {
                matches!(
                    event.event,
                    LifecycleEvent::TapReady | LifecycleEvent::TapDegraded
                )
            })
            .collect();
        summary.duplicate_disabled_events += disabled.len().saturating_sub(1);
        summary.duplicate_resolution_events += resolutions.len().saturating_sub(1);

        let Some(start) = disabled.first() else {
            summary.unresolved.push(ordinal);
            continue;
        };
        let Some(resolution) = resolutions
            .iter()
            .find(|event| event.event_offset_micros >= start.event_offset_micros)
        else {
            summary.unresolved.push(ordinal);
            continue;
        };
        let elapsed_micros = resolution
            .event_offset_micros
            .saturating_sub(start.event_offset_micros);
        let elapsed_ms = elapsed_micros.div_ceil(1_000);
        summary.max_resolution_ms = Some(
            summary
                .max_resolution_ms
                .unwrap_or_default()
                .max(elapsed_ms),
        );
        if elapsed_ms > summary.resolution_limit_ms {
            summary.late.push(ordinal);
        } else if resolution.event == LifecycleEvent::TapReady {
            summary.ready_within_limit += 1;
        } else {
            summary.degraded_within_limit += 1;
        }
    }

    summary.unexpected_attempts = by_ordinal
        .keys()
        .copied()
        .filter(|ordinal| *ordinal > expected_attempts)
        .collect();
    summary.passed = summary.ready_within_limit + summary.degraded_within_limit
        == expected_attempts
        && summary.late.is_empty()
        && summary.unresolved.is_empty()
        && summary.unexpected_attempts.is_empty()
        && summary.duplicate_disabled_events == 0
        && summary.duplicate_resolution_events == 0;
    summary
}

fn summarize_watcher_resources(
    config: &AcceptanceSessionConfig,
    records: &[AcceptanceRecord],
) -> WatcherResourceSummary {
    let expected_restarts = config.watcher_restart_count;
    let mut summary = WatcherResourceSummary {
        expected_restarts,
        ..WatcherResourceSummary::default()
    };
    if expected_restarts == 0 {
        summary.passed = true;
        return summary;
    }

    let mut active_samples = Vec::<&WatcherResourceRecord>::new();
    let mut by_ordinal = BTreeMap::<u32, Vec<&WatcherResourceRecord>>::new();
    for record in records {
        if let AcceptanceRecord::WatcherResources(record) = record {
            summary.sample_count += 1;
            summary.peak = summary.peak.component_max(record.resources);
            summary.growth_or_duplicate_detected |=
                record.resources.indicates_growth_or_duplicate();
            by_ordinal.entry(record.ordinal).or_default().push(record);
            if matches!(
                record.event,
                LifecycleEvent::WatcherReady | LifecycleEvent::WatcherDegraded
            ) {
                active_samples.push(record);
            } else if record.event == LifecycleEvent::WatcherReleased
                && !record.resources.is_fully_released()
            {
                summary.released_resource_leaks.push(record.ordinal);
            }
        }
    }

    for ordinal in 1..=expected_restarts {
        let Some(records) = by_ordinal.get(&ordinal) else {
            summary.missing_release_restarts.push(ordinal);
            summary.missing_started_restarts.push(ordinal);
            summary.missing_restarts.push(ordinal);
            continue;
        };
        let releases = records
            .iter()
            .filter(|record| record.event == LifecycleEvent::WatcherReleased)
            .collect::<Vec<_>>();
        let started = records
            .iter()
            .filter(|record| record.event == LifecycleEvent::WatcherStarted)
            .collect::<Vec<_>>();
        let active = records
            .iter()
            .filter(|record| {
                matches!(
                    record.event,
                    LifecycleEvent::WatcherReady | LifecycleEvent::WatcherDegraded
                )
            })
            .collect::<Vec<_>>();
        if releases.is_empty() {
            summary.missing_release_restarts.push(ordinal);
        } else if releases.len() > 1 {
            summary.duplicate_release_restarts.push(ordinal);
        }
        if started.is_empty() {
            summary.missing_started_restarts.push(ordinal);
        } else if started.len() > 1 {
            summary.duplicate_started_restarts.push(ordinal);
        }
        if active.is_empty() {
            summary.missing_restarts.push(ordinal);
        } else if active.len() > 1 {
            summary.duplicate_active_restarts.push(ordinal);
        }
        if let (Some(release), Some(start), Some(terminal)) =
            (releases.first(), started.first(), active.first())
        {
            if release.generation != start.generation || start.generation != terminal.generation {
                summary.generation_mismatch_restarts.push(ordinal);
            }
            if release.event_offset_micros > start.event_offset_micros
                || start.event_offset_micros > terminal.event_offset_micros
            {
                summary.out_of_order_restarts.push(ordinal);
            }
        }
    }
    summary.released_resource_leaks.sort_unstable();
    summary.released_resource_leaks.dedup();
    summary.unexpected_restarts = by_ordinal
        .keys()
        .copied()
        .filter(|ordinal| *ordinal > expected_restarts)
        .collect();
    active_samples.sort_unstable_by_key(|record| record.event_offset_micros);
    summary.final_active = active_samples.last().map(|record| record.resources);
    summary.final_single_effective_source = summary
        .final_active
        .is_some_and(WatcherResourceCounts::is_single_effective_source);
    summary.passed = summary.missing_restarts.is_empty()
        && summary.missing_release_restarts.is_empty()
        && summary.missing_started_restarts.is_empty()
        && summary.unexpected_restarts.is_empty()
        && summary.duplicate_release_restarts.is_empty()
        && summary.duplicate_started_restarts.is_empty()
        && summary.duplicate_active_restarts.is_empty()
        && summary.generation_mismatch_restarts.is_empty()
        && summary.out_of_order_restarts.is_empty()
        && summary.released_resource_leaks.is_empty()
        && !summary.growth_or_duplicate_detected
        && summary.final_single_effective_source;
    summary
}

fn summarize_self_isolation(
    config: &AcceptanceSessionConfig,
    records: &[AcceptanceRecord],
    selections: &[&SelectionRecord],
) -> SelfIsolationSummary {
    let configured_min_duration_ms = config.self_isolation_min_duration_ms;
    let selection_trigger_count = selections
        .iter()
        .filter(|record| record.scenario == Scenario::SelfWindowIsolation)
        .count();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    let mut interactions = Vec::new();
    for record in records {
        match record {
            AcceptanceRecord::Lifecycle(record)
                if record.scenario == Scenario::SelfWindowIsolation =>
            {
                match record.event {
                    LifecycleEvent::ScenarioStarted => starts.push(record),
                    LifecycleEvent::ScenarioEnded => ends.push(record),
                    _ => {}
                }
            }
            AcceptanceRecord::SelfInteraction(record) => interactions.push(record),
            AcceptanceRecord::Selection(_)
            | AcceptanceRecord::Lifecycle(_)
            | AcceptanceRecord::WatcherResources(_) => {}
        }
    }
    let lifecycle_generation_matched =
        starts.len() == 1 && ends.len() == 1 && starts[0].generation == ends[0].generation;
    let lifecycle_order_valid = starts.len() == 1
        && ends.len() == 1
        && starts[0].event_offset_micros <= ends[0].event_offset_micros;
    let observed_duration_ms = (lifecycle_generation_matched && lifecycle_order_valid).then(|| {
        ends[0]
            .event_offset_micros
            .saturating_sub(starts[0].event_offset_micros)
            / 1_000
    });
    let interaction_generation_matched = lifecycle_generation_matched
        && interactions.len() == 1
        && interactions[0].generation == starts[0].generation;
    let mut total_interaction_count = 0_u64;
    let mut settings_interaction_count = 0_u64;
    let mut popup_interaction_count = 0_u64;
    let mut pointer_interaction_count = 0_u64;
    let mut keyboard_interaction_count = 0_u64;
    let mut populated_buckets = BTreeSet::new();
    let mut settings_buckets = BTreeSet::new();
    let mut popup_buckets = BTreeSet::new();
    if let Some(interaction) = interactions.first() {
        for aggregate in &interaction.aggregates {
            total_interaction_count = total_interaction_count.saturating_add(aggregate.count);
            match aggregate.window {
                SelfInteractionWindow::Settings => {
                    settings_interaction_count =
                        settings_interaction_count.saturating_add(aggregate.count);
                    settings_buckets.extend(aggregate.bucket_coverage.iter().copied());
                }
                SelfInteractionWindow::Popup => {
                    popup_interaction_count =
                        popup_interaction_count.saturating_add(aggregate.count);
                    popup_buckets.extend(aggregate.bucket_coverage.iter().copied());
                }
            }
            match aggregate.kind {
                SelfInteractionKind::Pointer => {
                    pointer_interaction_count =
                        pointer_interaction_count.saturating_add(aggregate.count);
                }
                SelfInteractionKind::Keyboard => {
                    keyboard_interaction_count =
                        keyboard_interaction_count.saturating_add(aggregate.count);
                }
            }
            populated_buckets.extend(aggregate.bucket_coverage.iter().copied());
        }
    }
    let populated_buckets = populated_buckets.into_iter().collect::<Vec<_>>();
    let settings_buckets = settings_buckets.into_iter().collect::<Vec<_>>();
    let popup_buckets = popup_buckets.into_iter().collect::<Vec<_>>();
    let every_bucket_covered =
        populated_buckets == (0..SELF_INTERACTION_BUCKET_COUNT).collect::<Vec<_>>();
    let window_spans_both_halves = |buckets: &[u8]| {
        buckets.len() >= 5
            && buckets.iter().any(|bucket| *bucket < 5)
            && buckets.iter().any(|bucket| *bucket >= 5)
    };
    let window_bucket_coverage_valid =
        window_spans_both_halves(&settings_buckets) && window_spans_both_halves(&popup_buckets);
    let passed = configured_min_duration_ms.is_none_or(|minimum| {
        selection_trigger_count == 0
            && lifecycle_generation_matched
            && lifecycle_order_valid
            && observed_duration_ms.is_some_and(|duration| duration >= minimum)
            && interaction_generation_matched
            && every_bucket_covered
            && window_bucket_coverage_valid
    });
    SelfIsolationSummary {
        configured_min_duration_ms,
        observed_duration_ms,
        selection_trigger_count,
        start_count: starts.len(),
        end_count: ends.len(),
        lifecycle_generation_matched,
        lifecycle_order_valid,
        interaction_record_count: interactions.len(),
        interaction_generation_matched,
        total_interaction_count,
        settings_interaction_count,
        popup_interaction_count,
        pointer_interaction_count,
        keyboard_interaction_count,
        populated_buckets,
        settings_buckets,
        popup_buckets,
        every_bucket_covered,
        window_bucket_coverage_valid,
        passed,
    }
}

fn report_invalid_reasons(
    summary: &AcceptanceSummary,
    overflowed_records: usize,
    rejected_records: usize,
) -> Vec<ReportInvalidReason> {
    let mut reasons = Vec::new();
    if overflowed_records > 0 {
        reasons.push(ReportInvalidReason::CapacityOverflow);
    }
    if rejected_records > 0 {
        reasons.push(ReportInvalidReason::RejectedRecord);
    }
    if summary.duplicate_evidence.total() > 0 {
        reasons.push(ReportInvalidReason::DuplicateEvidence);
    }
    if !summary.selections.missing.is_empty() {
        reasons.push(ReportInvalidReason::MissingSelection);
    }
    if !summary.selections.duplicates.is_empty() {
        reasons.push(ReportInvalidReason::DuplicateSelection);
    }
    if !summary.selections.unexpected.is_empty() {
        reasons.push(ReportInvalidReason::UnexpectedSelection);
    }
    if !summary.duplicate_selection_revisions.is_empty()
        || !summary.duplicate_popup_revisions.is_empty()
    {
        reasons.push(ReportInvalidReason::DuplicateRevision);
    }
    if !summary.selections.wrong_classification.is_empty() {
        reasons.push(ReportInvalidReason::WrongClassification);
    }
    if summary.selections.stale_write_committed_count > 0 {
        reasons.push(ReportInvalidReason::StaleWriteCommitted);
    }
    if summary
        .scenario_latency
        .iter()
        .any(|scenario| !scenario.threshold_passed)
    {
        reasons.push(ReportInvalidReason::LatencyThresholdExceeded);
    }
    if !summary.rapid_a_to_b.passed {
        reasons.push(ReportInvalidReason::RapidTransitionFailure);
    }
    if !summary.tap_recovery.passed {
        reasons.push(ReportInvalidReason::TapRecoveryFailure);
    }
    if !summary.watcher_resources.passed {
        reasons.push(ReportInvalidReason::WatcherResourceFailure);
    }
    if !summary.self_isolation.passed {
        reasons.push(ReportInvalidReason::SelfIsolationFailure);
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn base_config() -> AcceptanceSessionConfig {
        AcceptanceSessionConfig {
            app: AppMetadata {
                version: "0.1.0".to_owned(),
                bundle_identifier: "com.paperfloat.translator".to_owned(),
                build_kind: BuildKind::AdHocAcceptance,
            },
            system: SystemMetadata {
                operating_system: OperatingSystem::MacOs,
                version: "15.5".to_owned(),
                architecture: Architecture::Arm64,
            },
            baseline_applications: vec![
                BaselineApplicationMetadata {
                    application: BaselineApplication::TextEdit,
                    version: "1.20".to_owned(),
                },
                BaselineApplicationMetadata {
                    application: BaselineApplication::Safari,
                    version: "26.5".to_owned(),
                },
                BaselineApplicationMetadata {
                    application: BaselineApplication::Preview,
                    version: "11.0".to_owned(),
                },
            ],
            fixtures: vec![
                FixtureMetadata {
                    kind: FixtureKind::TextEditPlainText,
                    fixture_id: "paper-float-textedit-selection-baseline-v1".to_owned(),
                    sha256: "c91895a9f01b3637f3e34570124ea3c20ac77dff9852e841a732060425ac6a6d"
                        .to_owned(),
                },
                FixtureMetadata {
                    kind: FixtureKind::SafariHtml,
                    fixture_id: "paper-float-selection-baseline-v1".to_owned(),
                    sha256: "9dbc666eee63e4d91556a2584de0afcad4e5e9d5649740bcdd9dccc8a6e4ff7b"
                        .to_owned(),
                },
                FixtureMetadata {
                    kind: FixtureKind::PreviewPdf,
                    fixture_id: "paper-float-selection-baseline-v1".to_owned(),
                    sha256: "32e1a2e85170cd4de04c388a816f9e408317237f039f103747f1e14721c26857"
                        .to_owned(),
                },
            ],
            multi_click_quiet_window_ms: 530,
            selection_expectations: vec![SelectionExpectation {
                scenario: Scenario::TextEditDrag,
                expected_count: 1,
                expected_match: ExpectedMatch::Match,
                latency_threshold: Some(LatencyThreshold {
                    p95_limit_ms: 350,
                    max_limit_ms: 800,
                }),
            }],
            rapid_a_to_b_pairs: 0,
            tap_recovery_attempts: 0,
            tap_resolution_limit_ms: DEFAULT_TAP_RESOLUTION_LIMIT_MS,
            watcher_restart_count: 0,
            self_isolation_min_duration_ms: None,
        }
    }

    fn full_selection_contract_config() -> AcceptanceSessionConfig {
        let mut config = base_config();
        config.selection_expectations = [
            Scenario::TextEditDrag,
            Scenario::TextEditDoubleClick,
            Scenario::TextEditTripleClick,
            Scenario::TextEditShiftExtend,
            Scenario::TextEditKeyboardSelection,
            Scenario::SameTextSameLocation,
            Scenario::SameTextDifferentLocation,
            Scenario::SafariFixture,
            Scenario::PreviewFixture,
        ]
        .into_iter()
        .map(|scenario| SelectionExpectation {
            scenario,
            expected_count: 1,
            expected_match: ExpectedMatch::Match,
            latency_threshold: full_baseline_latency_threshold(
                scenario,
                config.multi_click_quiet_window_ms,
            ),
        })
        .collect();
        config
    }

    fn selection_record(
        scenario: Scenario,
        ordinal: u32,
        revision: u64,
        event_offset_micros: u64,
        latency_ms: u64,
        expected_match: ExpectedMatch,
    ) -> AcceptanceRecord {
        let location_evidence = match scenario {
            Scenario::SameTextSameLocation => Some(LocationEvidence {
                category: LocationCategory::ReferenceA,
                result: if ordinal == 1 {
                    LocationMatchResult::ReferenceEstablished
                } else {
                    LocationMatchResult::MatchedExpectedCluster
                },
            }),
            Scenario::SameTextDifferentLocation => Some(LocationEvidence {
                category: if ordinal % 2 == 1 {
                    LocationCategory::ReferenceA
                } else {
                    LocationCategory::ReferenceB
                },
                result: if ordinal <= 2 {
                    LocationMatchResult::ReferenceEstablished
                } else {
                    LocationMatchResult::MatchedExpectedCluster
                },
            }),
            _ => None,
        };
        AcceptanceRecord::Selection(SelectionRecord {
            scenario,
            ordinal,
            generation: revision,
            selection_revision: revision,
            popup_revision: Some(revision),
            controller_snapshot_selection_revision: Some(revision),
            controller_snapshot_popup_revision: Some(revision),
            attempt: 1,
            reason: TriggerReason::MouseDrag,
            source_app: SourceAppIdentifier::TextEdit,
            event_offset_micros,
            controller_commit_offset_micros: Some(event_offset_micros),
            end_to_end_latency_ms: latency_ms,
            outcome: SelectionOutcome::PopupCommitted,
            expected_match,
            location_evidence,
        })
    }

    fn lifecycle_record(
        scenario: Scenario,
        ordinal: u32,
        offset_micros: u64,
        event: LifecycleEvent,
    ) -> AcceptanceRecord {
        AcceptanceRecord::Lifecycle(LifecycleRecord {
            scenario,
            ordinal,
            generation: ordinal as u64,
            event_offset_micros: offset_micros,
            event,
        })
    }

    fn self_interaction_record(
        generation: u64,
        settings_pointer: &[u8],
        settings_keyboard: &[u8],
        popup_pointer: &[u8],
        popup_keyboard: &[u8],
    ) -> AcceptanceRecord {
        let aggregate = |window, kind, buckets: &[u8]| SelfInteractionAggregate {
            window,
            kind,
            count: u64::try_from(buckets.len()).unwrap(),
            bucket_coverage: buckets.to_vec(),
        };
        AcceptanceRecord::SelfInteraction(SelfInteractionRecord {
            scenario: Scenario::SelfWindowIsolation,
            ordinal: 1,
            generation,
            aggregates: vec![
                aggregate(
                    SelfInteractionWindow::Settings,
                    SelfInteractionKind::Pointer,
                    settings_pointer,
                ),
                aggregate(
                    SelfInteractionWindow::Settings,
                    SelfInteractionKind::Keyboard,
                    settings_keyboard,
                ),
                aggregate(
                    SelfInteractionWindow::Popup,
                    SelfInteractionKind::Pointer,
                    popup_pointer,
                ),
                aggregate(
                    SelfInteractionWindow::Popup,
                    SelfInteractionKind::Keyboard,
                    popup_keyboard,
                ),
            ],
        })
    }

    fn watcher_record(
        ordinal: u32,
        offset_micros: u64,
        event: LifecycleEvent,
        resources: WatcherResourceCounts,
    ) -> AcceptanceRecord {
        watcher_record_with_generation(ordinal, ordinal as u64, offset_micros, event, resources)
    }

    fn watcher_record_with_generation(
        ordinal: u32,
        generation: u64,
        offset_micros: u64,
        event: LifecycleEvent,
        resources: WatcherResourceCounts,
    ) -> AcceptanceRecord {
        AcceptanceRecord::WatcherResources(WatcherResourceRecord {
            scenario: Scenario::WatcherRestart,
            ordinal,
            generation,
            event_offset_micros: offset_micros,
            event,
            resources,
        })
    }

    #[test]
    fn lifecycle_requires_end_then_clear_before_restart() {
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        assert_eq!(diagnostics.phase(), SessionPhase::Idle);
        assert_eq!(
            diagnostics.export_snapshot(),
            Err(DiagnosticsError::ReportNotReady)
        );

        diagnostics.start(base_config()).unwrap();
        assert_eq!(diagnostics.phase(), SessionPhase::Running);
        assert_eq!(
            diagnostics.start(base_config()),
            Err(DiagnosticsError::AlreadyRunning)
        );
        diagnostics
            .record(selection_record(
                Scenario::TextEditDrag,
                1,
                1,
                10_000,
                10,
                ExpectedMatch::Match,
            ))
            .unwrap();

        let report = diagnostics.end().unwrap();
        assert!(report.status.valid);
        assert_eq!(diagnostics.phase(), SessionPhase::Ended);
        assert_eq!(diagnostics.export_snapshot().unwrap(), report);
        assert_eq!(
            diagnostics.start(base_config()),
            Err(DiagnosticsError::MustClearEndedSession)
        );

        diagnostics.clear();
        assert_eq!(diagnostics.phase(), SessionPhase::Idle);
        diagnostics.start(base_config()).unwrap();
    }

    #[test]
    fn explicit_enable_switch_is_required() {
        let mut diagnostics = AcceptanceDiagnostics::new(false, 8).unwrap();
        assert_eq!(
            diagnostics.start(base_config()),
            Err(DiagnosticsError::Disabled)
        );
        assert_eq!(diagnostics.phase(), SessionPhase::Idle);
    }

    #[test]
    fn multi_click_quiet_window_is_required_bounded_and_overflow_safe() {
        for invalid in [0, MAX_MULTI_CLICK_QUIET_WINDOW_MS + 1, u64::MAX] {
            let mut config = base_config();
            config.multi_click_quiet_window_ms = invalid;
            let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
            assert!(matches!(
                diagnostics.start(config),
                Err(DiagnosticsError::InvalidConfig(_))
            ));
        }
        assert!(full_baseline_latency_threshold(Scenario::TextEditDoubleClick, u64::MAX).is_none());
    }

    #[test]
    fn full_baseline_threshold_contract_rejects_double_and_non_double_drift() {
        let config = full_selection_contract_config();
        let double_click = config
            .selection_expectations
            .iter()
            .find(|expectation| expectation.scenario == Scenario::TextEditDoubleClick)
            .unwrap();
        assert_eq!(
            double_click.latency_threshold,
            Some(LatencyThreshold {
                p95_limit_ms: 880,
                max_limit_ms: 1_330,
            })
        );

        for scenario in [Scenario::TextEditDoubleClick, Scenario::TextEditDrag] {
            let mut drifted = config.clone();
            drifted
                .selection_expectations
                .iter_mut()
                .find(|expectation| expectation.scenario == scenario)
                .unwrap()
                .latency_threshold
                .as_mut()
                .unwrap()
                .max_limit_ms += 1;
            let mut diagnostics = AcceptanceDiagnostics::new(true, 16).unwrap();
            assert!(matches!(
                diagnostics.start(drifted),
                Err(DiagnosticsError::InvalidConfig(_))
            ));
        }
    }

    #[test]
    fn browser_pdf_threshold_adds_q_only_for_the_double_click_merge_path() {
        for (reason, limit) in [
            (TriggerReason::MouseDrag, 1_200),
            (TriggerReason::MouseDoubleClick, 1_730),
        ] {
            for (latency, should_pass) in [(limit, true), (limit + 1, false)] {
                let mut config = base_config();
                config.selection_expectations[0] = SelectionExpectation {
                    scenario: Scenario::SafariFixture,
                    expected_count: 1,
                    expected_match: ExpectedMatch::Match,
                    latency_threshold: full_baseline_latency_threshold(
                        Scenario::SafariFixture,
                        config.multi_click_quiet_window_ms,
                    ),
                };
                let mut diagnostics = AcceptanceDiagnostics::new(true, 4).unwrap();
                diagnostics.start(config).unwrap();
                let AcceptanceRecord::Selection(mut record) = selection_record(
                    Scenario::SafariFixture,
                    1,
                    1,
                    1_000,
                    latency,
                    ExpectedMatch::Match,
                ) else {
                    unreachable!();
                };
                record.reason = reason;
                record.source_app = SourceAppIdentifier::Safari;
                diagnostics
                    .record(AcceptanceRecord::Selection(record))
                    .unwrap();
                let report = diagnostics.end().unwrap();
                let latency_summary = report
                    .summary
                    .scenario_latency
                    .iter()
                    .find(|summary| summary.scenario == Scenario::SafariFixture)
                    .unwrap();
                assert_eq!(
                    latency_summary.threshold,
                    Some(LatencyThreshold {
                        p95_limit_ms: limit,
                        max_limit_ms: limit,
                    })
                );
                assert_eq!(latency_summary.threshold_passed, should_pass);
                assert_eq!(report.status.valid, should_pass);
            }
        }
    }

    #[test]
    fn report_version_four_distinguishes_the_rc50_self_interaction_contract() {
        let mut diagnostics = AcceptanceDiagnostics::new(true, 4).unwrap();
        diagnostics.start(base_config()).unwrap();
        let report = diagnostics.end().unwrap();
        assert_eq!(report.schema_version, 1);
        assert_eq!(report.report_version, 4);
        assert_ne!(report.report_version, 3);
        assert_eq!(report.multi_click_quiet_window_ms, 530);
    }

    #[test]
    fn overflow_invalidates_without_evicting_existing_records() {
        let mut config = base_config();
        config.selection_expectations[0].expected_count = 2;
        let mut diagnostics = AcceptanceDiagnostics::new(true, 1).unwrap();
        diagnostics.start(config).unwrap();
        let first = selection_record(
            Scenario::TextEditDrag,
            1,
            11,
            1_000,
            20,
            ExpectedMatch::Match,
        );
        diagnostics.record(first.clone()).unwrap();
        assert_eq!(
            diagnostics.record(selection_record(
                Scenario::TextEditDrag,
                2,
                12,
                2_000,
                20,
                ExpectedMatch::Match,
            )),
            Err(DiagnosticsError::CapacityExceeded)
        );

        let report = diagnostics.end().unwrap();
        assert!(!report.status.integrity_valid);
        assert!(!report.status.valid);
        assert_eq!(report.recorded_records, 1);
        assert_eq!(report.overflowed_records, 1);
        assert_eq!(report.records, vec![first]);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::CapacityOverflow));
    }

    #[test]
    fn terminal_read_failure_can_be_recorded_without_revisions() {
        let mut diagnostics = AcceptanceDiagnostics::new(true, 4).unwrap();
        diagnostics.start(base_config()).unwrap();
        diagnostics
            .record(AcceptanceRecord::Selection(SelectionRecord {
                scenario: Scenario::TextEditDrag,
                ordinal: 1,
                generation: 1,
                selection_revision: 0,
                popup_revision: None,
                controller_snapshot_selection_revision: None,
                controller_snapshot_popup_revision: None,
                attempt: 3,
                reason: TriggerReason::MouseDrag,
                source_app: SourceAppIdentifier::TextEdit,
                event_offset_micros: 650_000,
                controller_commit_offset_micros: None,
                end_to_end_latency_ms: 650,
                outcome: SelectionOutcome::ReadFailed,
                expected_match: ExpectedMatch::NotApplicable,
                location_evidence: None,
            }))
            .unwrap();

        let report = diagnostics.end().unwrap();
        assert_eq!(report.recorded_records, 1);
        assert_eq!(report.summary.unique_selection_revisions, 0);
        assert_eq!(report.summary.selections.missing.len(), 1);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::MissingSelection));
    }

    #[test]
    fn popup_commit_rejects_zero_or_missing_revisions() {
        let mut diagnostics = AcceptanceDiagnostics::new(true, 4).unwrap();
        diagnostics.start(base_config()).unwrap();
        let record = AcceptanceRecord::Selection(SelectionRecord {
            scenario: Scenario::TextEditDrag,
            ordinal: 1,
            generation: 1,
            selection_revision: 0,
            popup_revision: None,
            controller_snapshot_selection_revision: None,
            controller_snapshot_popup_revision: None,
            attempt: 1,
            reason: TriggerReason::MouseDrag,
            source_app: SourceAppIdentifier::TextEdit,
            event_offset_micros: 1_000,
            controller_commit_offset_micros: None,
            end_to_end_latency_ms: 1,
            outcome: SelectionOutcome::PopupCommitted,
            expected_match: ExpectedMatch::Match,
            location_evidence: None,
        });
        assert!(matches!(
            diagnostics.record(record),
            Err(DiagnosticsError::InvalidRecord(_))
        ));

        let report = diagnostics.end().unwrap();
        assert_eq!(report.recorded_records, 0);
        assert_eq!(report.rejected_records, 1);
        assert!(!report.status.integrity_valid);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::RejectedRecord));
    }

    #[test]
    fn latency_summary_uses_nearest_rank_p95_and_max() {
        let mut config = base_config();
        config.selection_expectations[0].expected_count = 20;
        config.selection_expectations[0].latency_threshold = Some(LatencyThreshold {
            p95_limit_ms: 190,
            max_limit_ms: 200,
        });
        let mut diagnostics = AcceptanceDiagnostics::new(true, 32).unwrap();
        diagnostics.start(config).unwrap();
        for ordinal in 1..=20 {
            diagnostics
                .record(selection_record(
                    Scenario::TextEditDrag,
                    ordinal,
                    ordinal as u64,
                    u64::from(ordinal) * 1_000,
                    u64::from(ordinal) * 10,
                    ExpectedMatch::Match,
                ))
                .unwrap();
        }

        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.latency.sample_count, 20);
        assert_eq!(report.summary.latency.p95_ms, Some(190));
        assert_eq!(report.summary.latency.max_ms, Some(200));
        assert!(report.status.valid);
    }

    #[test]
    fn reports_missing_duplicate_unexpected_wrong_and_duplicate_revision() {
        let mut config = base_config();
        config.selection_expectations[0].expected_count = 3;
        config.selection_expectations[0].latency_threshold = None;
        let mut diagnostics = AcceptanceDiagnostics::new(true, 16).unwrap();
        diagnostics.start(config).unwrap();
        diagnostics
            .record(selection_record(
                Scenario::TextEditDrag,
                1,
                7,
                1_000,
                10,
                ExpectedMatch::Match,
            ))
            .unwrap();
        diagnostics
            .record(selection_record(
                Scenario::TextEditDrag,
                1,
                8,
                2_000,
                10,
                ExpectedMatch::Mismatch,
            ))
            .unwrap();
        diagnostics
            .record(selection_record(
                Scenario::TextEditDrag,
                3,
                8,
                3_000,
                10,
                ExpectedMatch::Match,
            ))
            .unwrap();
        diagnostics
            .record(selection_record(
                Scenario::TextEditDrag,
                4,
                9,
                4_000,
                10,
                ExpectedMatch::Match,
            ))
            .unwrap();

        let report = diagnostics.end().unwrap();
        assert_eq!(
            report.summary.selections.missing,
            vec![ScenarioOrdinal {
                scenario: Scenario::TextEditDrag,
                ordinal: 2,
            }]
        );
        assert_eq!(report.summary.selections.duplicates.len(), 1);
        assert_eq!(report.summary.selections.unexpected.len(), 1);
        assert_eq!(report.summary.selections.wrong_classification.len(), 1);
        assert_eq!(report.summary.duplicate_selection_revisions, vec![8]);
        assert!(!report.status.valid);
    }

    #[test]
    fn rapid_a_to_b_detects_final_state_and_stale_a_write() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.rapid_a_to_b_pairs = 2;
        let mut diagnostics = AcceptanceDiagnostics::new(true, 16).unwrap();
        diagnostics.start(config).unwrap();

        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                1,
                1,
                1_000,
                20,
                ExpectedMatch::A,
            ))
            .unwrap();
        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                1,
                2,
                2_000,
                20,
                ExpectedMatch::B,
            ))
            .unwrap();
        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                2,
                3,
                3_000,
                20,
                ExpectedMatch::B,
            ))
            .unwrap();
        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                2,
                4,
                4_000,
                20,
                ExpectedMatch::A,
            ))
            .unwrap();

        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.rapid_a_to_b.final_b_pairs, 1);
        assert_eq!(report.summary.rapid_a_to_b.wrong_final_pairs, vec![2]);
        assert_eq!(report.summary.rapid_a_to_b.stale_a_write_count, 1);
        assert!(!report.summary.rapid_a_to_b.passed);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::RapidTransitionFailure));
    }

    #[test]
    fn rapid_final_write_uses_controller_commit_order_not_trigger_order() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.rapid_a_to_b_pairs = 1;
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config).unwrap();

        let AcceptanceRecord::Selection(mut a) =
            selection_record(Scenario::RapidAThenB, 1, 2, 1_000, 3, ExpectedMatch::A)
        else {
            unreachable!();
        };
        // A triggered first but its controller write landed after B.
        a.controller_commit_offset_micros = Some(4_000);
        let AcceptanceRecord::Selection(mut b) =
            selection_record(Scenario::RapidAThenB, 1, 1, 2_000, 1, ExpectedMatch::B)
        else {
            unreachable!();
        };
        b.controller_commit_offset_micros = Some(3_000);
        diagnostics.record(AcceptanceRecord::Selection(a)).unwrap();
        diagnostics.record(AcceptanceRecord::Selection(b)).unwrap();

        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.rapid_a_to_b.final_b_pairs, 0);
        assert_eq!(report.summary.rapid_a_to_b.wrong_final_pairs, vec![1]);
        assert_eq!(report.summary.rapid_a_to_b.stale_a_write_count, 1);
        assert!(!report.summary.rapid_a_to_b.passed);
        assert!(!report.status.valid);
    }

    #[test]
    fn rapid_a_to_b_requires_both_gestures_in_order_within_300ms() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.rapid_a_to_b_pairs = 3;
        let mut diagnostics = AcceptanceDiagnostics::new(true, 16).unwrap();
        diagnostics.start(config).unwrap();

        // A lone B must not masquerade as a completed A-to-B pair.
        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                1,
                1,
                10_000,
                20,
                ExpectedMatch::B,
            ))
            .unwrap();
        // Correct order but outside the configured 300 ms pressure interval.
        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                2,
                2,
                20_000,
                20,
                ExpectedMatch::A,
            ))
            .unwrap();
        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                2,
                3,
                321_000,
                20,
                ExpectedMatch::B,
            ))
            .unwrap();
        // B before A is not an A-to-B pair even when the final write is B-free.
        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                3,
                4,
                400_000,
                20,
                ExpectedMatch::B,
            ))
            .unwrap();
        diagnostics
            .record(selection_record(
                Scenario::RapidAThenB,
                3,
                5,
                450_000,
                20,
                ExpectedMatch::A,
            ))
            .unwrap();

        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.rapid_a_to_b.max_trigger_gap_ms, 300);
        assert_eq!(report.summary.rapid_a_to_b.missing_a_pairs, vec![1]);
        assert_eq!(report.summary.rapid_a_to_b.gap_exceeded_pairs, vec![2]);
        assert_eq!(report.summary.rapid_a_to_b.out_of_order_pairs, vec![3]);
        assert!(!report.summary.rapid_a_to_b.passed);
        assert!(!report.status.valid);
    }

    #[test]
    fn rapid_a_to_b_rejects_duplicate_gesture_evidence() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.rapid_a_to_b_pairs = 1;
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config).unwrap();
        for (revision, offset, expected_match) in [
            (1, 1_000, ExpectedMatch::A),
            (2, 2_000, ExpectedMatch::A),
            (3, 3_000, ExpectedMatch::B),
            (4, 4_000, ExpectedMatch::B),
        ] {
            diagnostics
                .record(selection_record(
                    Scenario::RapidAThenB,
                    1,
                    revision,
                    offset,
                    20,
                    expected_match,
                ))
                .unwrap();
        }

        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.rapid_a_to_b.duplicate_a_pairs, vec![1]);
        assert_eq!(report.summary.rapid_a_to_b.duplicate_b_pairs, vec![1]);
        assert!(!report.summary.rapid_a_to_b.passed);
    }

    #[test]
    fn tap_recovery_resolves_ready_or_degraded_within_two_seconds() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.tap_recovery_attempts = 2;
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config).unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::TapDisabledRecovery,
                1,
                0,
                LifecycleEvent::TapDisabled,
            ))
            .unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::TapDisabledRecovery,
                1,
                1_999_000,
                LifecycleEvent::TapReady,
            ))
            .unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::TapDisabledRecovery,
                2,
                3_000_000,
                LifecycleEvent::TapDisabled,
            ))
            .unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::TapDisabledRecovery,
                2,
                5_000_000,
                LifecycleEvent::TapDegraded,
            ))
            .unwrap();

        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.tap_recovery.ready_within_limit, 1);
        assert_eq!(report.summary.tap_recovery.degraded_within_limit, 1);
        assert_eq!(report.summary.tap_recovery.max_resolution_ms, Some(2_000));
        assert!(report.summary.tap_recovery.passed);
        assert!(report.status.valid);
    }

    #[test]
    fn tap_recovery_later_than_two_seconds_fails_acceptance() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.tap_recovery_attempts = 1;
        let mut diagnostics = AcceptanceDiagnostics::new(true, 4).unwrap();
        diagnostics.start(config).unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::TapDisabledRecovery,
                1,
                10_000,
                LifecycleEvent::TapDisabled,
            ))
            .unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::TapDisabledRecovery,
                1,
                2_010_001,
                LifecycleEvent::TapReady,
            ))
            .unwrap();

        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.tap_recovery.late, vec![1]);
        assert_eq!(report.summary.tap_recovery.max_resolution_ms, Some(2_001));
        assert!(!report.summary.tap_recovery.passed);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::TapRecoveryFailure));
    }

    #[test]
    fn watcher_summary_requires_one_effective_source_without_growth() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.watcher_restart_count = 2;
        let one_source = WatcherResourceCounts {
            snapshot_version: 1,
            selection_observers: 1,
            selection_observer_sources: 1,
            key_event_taps: 1,
            key_event_tap_sources: 1,
            mouse_event_taps: 1,
            mouse_event_tap_sources: 1,
            pasteboard_timers: 1,
            workspace_activation_observers: 1,
            callbacks: 1,
            contexts: 1,
            effective_source_sets: 1,
            total_sources: 5,
        };
        let released = WatcherResourceCounts {
            snapshot_version: 1,
            ..WatcherResourceCounts::default()
        };
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config).unwrap();
        diagnostics
            .record(watcher_record(
                1,
                500,
                LifecycleEvent::WatcherReleased,
                released,
            ))
            .unwrap();
        diagnostics
            .record(watcher_record(
                1,
                750,
                LifecycleEvent::WatcherStarted,
                one_source,
            ))
            .unwrap();
        diagnostics
            .record(watcher_record(
                1,
                1_000,
                LifecycleEvent::WatcherReady,
                one_source,
            ))
            .unwrap();
        diagnostics
            .record(watcher_record(
                2,
                1_500,
                LifecycleEvent::WatcherReleased,
                released,
            ))
            .unwrap();
        diagnostics
            .record(watcher_record(
                2,
                1_750,
                LifecycleEvent::WatcherStarted,
                one_source,
            ))
            .unwrap();
        diagnostics
            .record(watcher_record(
                2,
                2_000,
                LifecycleEvent::WatcherReady,
                one_source,
            ))
            .unwrap();

        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.watcher_resources.peak, one_source);
        assert_eq!(
            report.summary.watcher_resources.final_active,
            Some(one_source)
        );
        assert!(
            report
                .summary
                .watcher_resources
                .final_single_effective_source
        );
        assert!(
            !report
                .summary
                .watcher_resources
                .growth_or_duplicate_detected
        );
        assert!(report.summary.watcher_resources.passed);
        assert!(report.status.valid);
    }

    #[test]
    fn watcher_growth_is_reported_as_invalid() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.watcher_restart_count = 1;
        let duplicate_sources = WatcherResourceCounts {
            snapshot_version: 1,
            selection_observers: 2,
            selection_observer_sources: 2,
            key_event_taps: 1,
            key_event_tap_sources: 1,
            mouse_event_taps: 1,
            mouse_event_tap_sources: 1,
            pasteboard_timers: 1,
            workspace_activation_observers: 1,
            callbacks: 2,
            contexts: 2,
            effective_source_sets: 2,
            total_sources: 6,
        };
        let mut diagnostics = AcceptanceDiagnostics::new(true, 4).unwrap();
        diagnostics.start(config).unwrap();
        diagnostics
            .record(watcher_record(
                1,
                500,
                LifecycleEvent::WatcherReleased,
                WatcherResourceCounts {
                    snapshot_version: 1,
                    ..WatcherResourceCounts::default()
                },
            ))
            .unwrap();
        diagnostics
            .record(watcher_record(
                1,
                750,
                LifecycleEvent::WatcherStarted,
                duplicate_sources,
            ))
            .unwrap();
        diagnostics
            .record(watcher_record(
                1,
                1_000,
                LifecycleEvent::WatcherReady,
                duplicate_sources,
            ))
            .unwrap();
        let report = diagnostics.end().unwrap();
        assert!(
            report
                .summary
                .watcher_resources
                .growth_or_duplicate_detected
        );
        assert!(!report.status.valid);
    }

    #[test]
    fn watcher_summary_rejects_cross_generation_phase_stitching() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.watcher_restart_count = 1;
        let released = WatcherResourceCounts {
            snapshot_version: 1,
            ..WatcherResourceCounts::default()
        };
        let active = WatcherResourceCounts {
            snapshot_version: 1,
            key_event_taps: 1,
            key_event_tap_sources: 1,
            pasteboard_timers: 1,
            callbacks: 1,
            contexts: 1,
            effective_source_sets: 1,
            total_sources: 2,
            ..WatcherResourceCounts::default()
        };
        let mut diagnostics = AcceptanceDiagnostics::new(true, 4).unwrap();
        diagnostics.start(config).unwrap();
        diagnostics
            .record(watcher_record_with_generation(
                1,
                40,
                100,
                LifecycleEvent::WatcherReleased,
                released,
            ))
            .unwrap();
        diagnostics
            .record(watcher_record_with_generation(
                1,
                41,
                200,
                LifecycleEvent::WatcherStarted,
                active,
            ))
            .unwrap();
        diagnostics
            .record(watcher_record_with_generation(
                1,
                41,
                300,
                LifecycleEvent::WatcherReady,
                active,
            ))
            .unwrap();
        let report = diagnostics.end().unwrap();
        assert_eq!(
            report
                .summary
                .watcher_resources
                .generation_mismatch_restarts,
            vec![1]
        );
        assert!(!report.summary.watcher_resources.passed);
        assert!(!report.status.valid);
    }

    #[test]
    fn self_isolation_empty_wait_fails_even_with_duration_and_zero_selection_triggers() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.self_isolation_min_duration_ms = Some(300_000);
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config).unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::SelfWindowIsolation,
                1,
                100,
                LifecycleEvent::ScenarioStarted,
            ))
            .unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::SelfWindowIsolation,
                1,
                300_000_100,
                LifecycleEvent::ScenarioEnded,
            ))
            .unwrap();
        let report = diagnostics.end().unwrap();
        assert_eq!(
            report.summary.self_isolation.observed_duration_ms,
            Some(300_000)
        );
        assert_eq!(report.summary.self_isolation.selection_trigger_count, 0);
        assert_eq!(report.summary.self_isolation.interaction_record_count, 0);
        assert!(!report.summary.self_isolation.every_bucket_covered);
        assert!(!report.summary.self_isolation.passed);
        assert!(!report.status.valid);
    }

    #[test]
    fn self_isolation_passes_with_all_buckets_both_windows_and_no_selection_trigger() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.self_isolation_min_duration_ms = Some(300_000);
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config).unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::SelfWindowIsolation,
                1,
                100,
                LifecycleEvent::ScenarioStarted,
            ))
            .unwrap();
        diagnostics
            .record(self_interaction_record(
                1,
                &[0, 2, 4],
                &[6, 8],
                &[1, 3, 5],
                &[7, 9],
            ))
            .unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::SelfWindowIsolation,
                1,
                300_000_100,
                LifecycleEvent::ScenarioEnded,
            ))
            .unwrap();
        let report = diagnostics.end().unwrap();
        let summary = &report.summary.self_isolation;
        assert_eq!(summary.interaction_record_count, 1);
        assert!(summary.interaction_generation_matched);
        assert_eq!(summary.total_interaction_count, 10);
        assert_eq!(summary.settings_interaction_count, 5);
        assert_eq!(summary.popup_interaction_count, 5);
        assert_eq!(summary.pointer_interaction_count, 6);
        assert_eq!(summary.keyboard_interaction_count, 4);
        assert_eq!(summary.populated_buckets, (0..10).collect::<Vec<_>>());
        assert!(summary.every_bucket_covered);
        assert!(summary.window_bucket_coverage_valid);
        assert!(summary.passed);
        assert!(report.status.valid);
    }

    #[test]
    fn self_interaction_generation_must_match_and_aggregate_shape_is_fail_closed() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.self_isolation_min_duration_ms = Some(300_000);
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config.clone()).unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::SelfWindowIsolation,
                1,
                100,
                LifecycleEvent::ScenarioStarted,
            ))
            .unwrap();
        diagnostics
            .record(self_interaction_record(
                2,
                &[0, 2, 4],
                &[6, 8],
                &[1, 3, 5],
                &[7, 9],
            ))
            .unwrap();
        diagnostics
            .record(lifecycle_record(
                Scenario::SelfWindowIsolation,
                1,
                300_000_100,
                LifecycleEvent::ScenarioEnded,
            ))
            .unwrap();
        let report = diagnostics.end().unwrap();
        assert!(!report.summary.self_isolation.interaction_generation_matched);
        assert!(!report.summary.self_isolation.passed);

        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config).unwrap();
        let AcceptanceRecord::SelfInteraction(mut malformed) =
            self_interaction_record(1, &[0, 2, 4], &[6, 8], &[1, 3, 5], &[7, 9])
        else {
            unreachable!()
        };
        malformed.aggregates[0]
            .bucket_coverage
            .push(SELF_INTERACTION_BUCKET_COUNT);
        assert!(matches!(
            diagnostics.record(AcceptanceRecord::SelfInteraction(malformed)),
            Err(DiagnosticsError::InvalidRecord(_))
        ));
        let report = diagnostics.end().unwrap();
        assert_eq!(report.rejected_records, 1);
        assert!(!report.status.integrity_valid);
    }

    #[test]
    fn self_isolation_cannot_merge_two_short_runs_across_a_long_gap() {
        let mut config = base_config();
        config.selection_expectations.clear();
        config.self_isolation_min_duration_ms = Some(300_000);
        let mut diagnostics = AcceptanceDiagnostics::new(true, 8).unwrap();
        diagnostics.start(config).unwrap();
        for record in [
            LifecycleRecord {
                scenario: Scenario::SelfWindowIsolation,
                ordinal: 1,
                generation: 1,
                event_offset_micros: 100,
                event: LifecycleEvent::ScenarioStarted,
            },
            LifecycleRecord {
                scenario: Scenario::SelfWindowIsolation,
                ordinal: 1,
                generation: 1,
                event_offset_micros: 200,
                event: LifecycleEvent::ScenarioEnded,
            },
            LifecycleRecord {
                scenario: Scenario::SelfWindowIsolation,
                ordinal: 1,
                generation: 2,
                event_offset_micros: 300_000_300,
                event: LifecycleEvent::ScenarioStarted,
            },
            LifecycleRecord {
                scenario: Scenario::SelfWindowIsolation,
                ordinal: 1,
                generation: 2,
                event_offset_micros: 300_000_400,
                event: LifecycleEvent::ScenarioEnded,
            },
        ] {
            diagnostics
                .record(AcceptanceRecord::Lifecycle(record))
                .unwrap();
        }
        let report = diagnostics.end().unwrap();
        assert_eq!(report.summary.self_isolation.start_count, 2);
        assert_eq!(report.summary.self_isolation.end_count, 2);
        assert_eq!(report.summary.self_isolation.observed_duration_ms, None);
        assert!(!report.summary.self_isolation.passed);
        assert!(!report.status.valid);
    }

    #[test]
    fn serialized_report_has_no_content_bearing_fields() {
        let mut diagnostics = AcceptanceDiagnostics::new(true, 4).unwrap();
        diagnostics.start(base_config()).unwrap();

        let mut unsafe_record = match selection_record(
            Scenario::TextEditDrag,
            1,
            1,
            1_000,
            20,
            ExpectedMatch::Match,
        ) {
            AcceptanceRecord::Selection(record) => record,
            _ => unreachable!(),
        };
        unsafe_record.source_app =
            SourceAppIdentifier::OtherBundleIdentifier("TOP_SECRET_SELECTION_SENTINEL".to_owned());
        assert!(matches!(
            diagnostics.record(AcceptanceRecord::Selection(unsafe_record)),
            Err(DiagnosticsError::InvalidRecord(_))
        ));
        diagnostics
            .record(selection_record(
                Scenario::TextEditDrag,
                1,
                1,
                1_000,
                20,
                ExpectedMatch::Match,
            ))
            .unwrap();
        let report = diagnostics.end().unwrap();
        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["reportVersion"], 4);
        assert_eq!(value["multiClickQuietWindowMs"], 530);
        let forbidden_keys = [
            "text",
            "hash",
            "length",
            "window",
            "title",
            "selectedText",
            "cleanedText",
            "translation",
            "clipboard",
            "windowTitle",
            "textPrefix",
            "textLength",
            "textHash",
            "apiKey",
            "reasonText",
            "reasonMessage",
        ];
        assert_no_forbidden_keys(&value, &forbidden_keys);
        assert!(!serde_json::to_string(&value)
            .unwrap()
            .contains("TOP_SECRET_SELECTION_SENTINEL"));
    }

    fn assert_no_forbidden_keys(value: &Value, forbidden: &[&str]) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    assert!(!forbidden.contains(&key.as_str()), "forbidden key: {key}");
                    assert_no_forbidden_keys(child, forbidden);
                }
            }
            Value::Array(array) => {
                for child in array {
                    assert_no_forbidden_keys(child, forbidden);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }
}
