//! Runtime wiring for the fixed, privacy-preserving selection acceptance preset.
//!
//! The UI may arm a server-defined scenario and ordinal, but cannot provide
//! thresholds, metadata, fixture contents, expected classifications, or an
//! export path. Selected text is compared against embedded fixed fixtures and
//! immediately discarded; it is never represented by the report model.

use crate::acceptance_diagnostics::{
    full_baseline_latency_threshold, AcceptanceDiagnostics, AcceptanceRecord, AcceptanceReport,
    AcceptanceSessionConfig, AppMetadata, BaselineApplicationMetadata, DiagnosticsError,
    DuplicateEvidenceKind, ExpectedMatch, FixtureKind, FixtureMetadata, LifecycleEvent,
    LifecycleRecord, LocationCategory, LocationEvidence, LocationMatchResult, Scenario,
    ScenarioOrdinal, SelectionExpectation, SelectionOutcome, SelectionRecord,
    SelfInteractionAggregate, SelfInteractionKind, SelfInteractionRecord, SelfInteractionWindow,
    SessionPhase, SourceAppIdentifier, SystemMetadata, TriggerReason, WatcherResourceCounts,
    WatcherResourceRecord, DEFAULT_TAP_RESOLUTION_LIMIT_MS, MAX_MULTI_CLICK_QUIET_WINDOW_MS,
    SELF_INTERACTION_BUCKET_COUNT,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fmt, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub const ACCEPTANCE_ENABLE_ENV: &str = "PAPER_FLOAT_ACCEPTANCE_DIAGNOSTICS";
pub const FULL_BASELINE_PRESET: &str = "full_baseline";
pub const DEFAULT_ACCEPTANCE_CAPACITY: usize = 4_096;
pub const ACCEPTANCE_EXPORT_FILE: &str = "selection-acceptance-report.json";
pub const RAPID_PROBE_DELAY_MS: u64 = 600;

/// Repeated selections are compared in global mouse-up logical points. Six
/// points absorb ordinary hand jitter and event-coordinate rounding without
/// allowing a neighbouring TextEdit line to count as the same location.
pub const SAME_LOCATION_TOLERANCE_LOGICAL_POINTS: f64 = 6.0;
/// Fourteen logical points is greater than two tolerance radii while remaining
/// compatible with adjacent lines in TextEdit's default layout. Equality is
/// accepted by the frozen contract; smaller separation fails closed.
pub const DIFFERENT_LOCATION_MIN_SEPARATION_LOGICAL_POINTS: f64 = 14.0;
pub const SELF_INTERACTION_BUCKET_DURATION_SECS: u64 = 30;

const TEXTEDIT_BUNDLE_ID: &str = "com.apple.TextEdit";
const SAFARI_BUNDLE_ID: &str = "com.apple.Safari";
const PREVIEW_BUNDLE_ID: &str = "com.apple.Preview";
const UNKNOWN_BUNDLE_ID: &str = "unknown_bundle_id";
const TEXTEDIT_EXPECTED_COUNT: u32 = 30;
const RAPID_PAIR_COUNT: u32 = 20;
const WATCHER_RESTART_COUNT: u32 = 50;
const SELF_ISOLATION_DURATION_MS: u64 = 300_000;
const MAX_NATIVE_TRIGGER_TO_READ_MS: f64 = 60_000.0;

const TEXT_FIXTURE: &str = include_str!("../../tests/fixtures/selection-baseline.txt");
const HTML_FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/selection-baseline.html");
const PDF_FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/selection-baseline.pdf");
const EXPECTED_TEXT_SHA256: &str =
    "c91895a9f01b3637f3e34570124ea3c20ac77dff9852e841a732060425ac6a6d";
const EXPECTED_HTML_SHA256: &str =
    "9dbc666eee63e4d91556a2584de0afcad4e5e9d5649740bcdd9dccc8a6e4ff7b";
const EXPECTED_PDF_SHA256: &str =
    "32e1a2e85170cd4de04c388a816f9e408317237f039f103747f1e14721c26857";

static EXPORT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct AcceptanceRuntimeMetadata {
    pub app: AppMetadata,
    pub system: SystemMetadata,
    pub baseline_applications: Vec<BaselineApplicationMetadata>,
    pub multi_click_quiet_window_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SelfIsolationAction {
    Start,
    End,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmedScenarioStatus {
    pub scenario: Scenario,
    pub ordinal: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceRuntimeStatus {
    pub enabled: bool,
    pub tap_injection_available: bool,
    pub preset: &'static str,
    pub phase: SessionPhase,
    pub armed: Option<ArmedScenarioStatus>,
    pub self_isolation_active: bool,
    pub pending_generations: usize,
    pub pending_rapid_probes: usize,
    pub recorded_generations: usize,
    pub duplicate_generations_suppressed: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceExportReceipt {
    pub path: String,
    pub recorded_records: usize,
    pub valid: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookDisposition {
    Ignored,
    Pending,
    Recorded,
    DuplicateSuppressed,
}

#[derive(Clone, Debug)]
pub struct NativeReadObservation<'a> {
    pub status: &'a str,
    pub generation: u64,
    pub attempt: u16,
    pub trigger_to_read_ms: f64,
    pub reason: &'a str,
    pub source_bundle_id: &'a str,
    pub source_pid: i32,
    pub found_text: bool,
    pub terminal: bool,
}

/// A native mouse-up anchor accepted only as transient runtime input. This
/// type intentionally implements neither `Serialize` nor report conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseUpAnchor {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy)]
struct PopupOutcomeObservation<'a> {
    generation: u64,
    selected_text: Option<&'a str>,
    source_pid: Option<i32>,
    selection_revision: u64,
    popup_revision: Option<u64>,
    outcome: SelectionOutcome,
    mouse_up_anchor: Option<MouseUpAnchor>,
}

#[derive(Clone, Copy)]
struct SelectionRecordDetails {
    selection_revision: u64,
    popup_revision: Option<u64>,
    outcome: SelectionOutcome,
    expected_match: ExpectedMatch,
    location_evidence: Option<LocationEvidence>,
}

#[derive(Clone, Copy, Debug, Default)]
struct RepeatedLocationClusters {
    same_reference: Option<MouseUpAnchor>,
    different_reference_a: Option<MouseUpAnchor>,
    different_reference_b: Option<MouseUpAnchor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcceptanceRuntimeError {
    Diagnostics(DiagnosticsError),
    NotRunning,
    ScenarioNotArmable,
    InvalidOrdinal,
    DuplicateArm,
    SelfIsolationActive,
    SelfIsolationNotActive,
    PendingRapidProbes,
    InvalidNativeObservation,
    GenerationRebound,
    MissingNativeRead,
    InvalidMultiClickQuietWindow,
    FixtureIntegrity,
    Export(String),
}

impl fmt::Display for AcceptanceRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagnostics(error) => write!(formatter, "{error}"),
            Self::NotRunning => formatter.write_str("acceptance diagnostics are not running"),
            Self::ScenarioNotArmable => {
                formatter.write_str("this scenario cannot be armed through the generic arm command")
            }
            Self::InvalidOrdinal => {
                formatter.write_str("scenario ordinal is outside the fixed preset")
            }
            Self::DuplicateArm => {
                formatter.write_str("this scenario ordinal was already completed")
            }
            Self::SelfIsolationActive => {
                formatter.write_str("self-isolation must end before this operation")
            }
            Self::SelfIsolationNotActive => formatter.write_str("self-isolation is not active"),
            Self::PendingRapidProbes => formatter.write_str(
                "acceptance rapid-selection probes are still waiting for controller evidence",
            ),
            Self::InvalidNativeObservation => {
                formatter.write_str("native acceptance observation contains invalid numeric fields")
            }
            Self::GenerationRebound => formatter.write_str(
                "one native generation attempted to bind to multiple acceptance ordinals",
            ),
            Self::MissingNativeRead => formatter.write_str(
                "a popup outcome arrived without a generation-matched native read diagnostic",
            ),
            Self::InvalidMultiClickQuietWindow => formatter.write_str(
                "native multi-click quiet window is missing, invalid, or cannot be represented safely",
            ),
            Self::FixtureIntegrity => {
                formatter.write_str("embedded acceptance fixture digest mismatch")
            }
            Self::Export(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for AcceptanceRuntimeError {}

impl From<DiagnosticsError> for AcceptanceRuntimeError {
    fn from(value: DiagnosticsError) -> Self {
        Self::Diagnostics(value)
    }
}

#[derive(Clone)]
struct FixedFixtureTargets {
    textedit_drag: String,
    textedit_double: String,
    textedit_triple: String,
    textedit_shift: String,
    textedit_keyboard: String,
    repeated: String,
    rapid_a: String,
    rapid_b: String,
    safari_preview: String,
}

impl FixedFixtureTargets {
    fn load() -> Result<Self, AcceptanceRuntimeError> {
        let line_value = |label: &str| {
            TEXT_FIXTURE
                .lines()
                .find(|line| line.starts_with(label))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or(AcceptanceRuntimeError::FixtureIntegrity)
        };
        let value = |label: &str| {
            line_value(label).and_then(|line| {
                line.strip_prefix(label)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
                    .ok_or(AcceptanceRuntimeError::FixtureIntegrity)
            })
        };
        Ok(Self {
            textedit_drag: value("Drag target:")?,
            textedit_double: value("Double-click target:")?,
            // TextEdit triple-click selects the full paragraph, including the
            // fixture label. The expected value must model that real result.
            textedit_triple: line_value("Triple-click target:")?,
            textedit_shift: value("Shift-extend target:")?,
            textedit_keyboard: value("Keyboard target:")?,
            repeated: value("Repeated target at position A:")?,
            rapid_a: value("Race target A:")?,
            rapid_b: value("Race target B:")?,
            safari_preview: "The same selection appears here.".to_owned(),
        })
    }

    fn expected_for(&self, scenario: Scenario) -> Option<&str> {
        match scenario {
            Scenario::TextEditDrag => Some(&self.textedit_drag),
            Scenario::TextEditDoubleClick => Some(&self.textedit_double),
            Scenario::TextEditTripleClick => Some(&self.textedit_triple),
            Scenario::TextEditShiftExtend => Some(&self.textedit_shift),
            Scenario::TextEditKeyboardSelection => Some(&self.textedit_keyboard),
            Scenario::SameTextSameLocation | Scenario::SameTextDifferentLocation => {
                Some(&self.repeated)
            }
            Scenario::SafariFixture | Scenario::PreviewFixture => Some(&self.safari_preview),
            Scenario::RapidAThenB
            | Scenario::SelfWindowIsolation
            | Scenario::TapDisabledRecovery
            | Scenario::WatcherRestart => None,
        }
    }
}

#[derive(Clone)]
struct ArmedScenario {
    scenario: Scenario,
    ordinal: u32,
    armed_at: Instant,
    tap_disabled_seen: bool,
    tap_resolved: bool,
    tap_watcher_generation: Option<u64>,
    watcher_generation: Option<u64>,
    watcher_release_seen: bool,
    watcher_started_seen: bool,
    watcher_started_resources_seen: bool,
    watcher_resolution_event: Option<LifecycleEvent>,
    watcher_active_resources_seen: bool,
}

impl ArmedScenario {
    fn new(scenario: Scenario, ordinal: u32, armed_at: Instant) -> Self {
        Self {
            scenario,
            ordinal,
            armed_at,
            tap_disabled_seen: false,
            tap_resolved: false,
            tap_watcher_generation: None,
            watcher_generation: None,
            watcher_release_seen: false,
            watcher_started_seen: false,
            watcher_started_resources_seen: false,
            watcher_resolution_event: None,
            watcher_active_resources_seen: false,
        }
    }

    fn key(&self) -> ScenarioOrdinal {
        ScenarioOrdinal {
            scenario: self.scenario,
            ordinal: self.ordinal,
        }
    }
}

#[derive(Clone)]
struct PendingRead {
    binding: ScenarioOrdinal,
    generation: u64,
    attempt: u16,
    reason: TriggerReason,
    source_app: SourceAppIdentifier,
    source_pid: i32,
    triggered_at: Instant,
    found_text: bool,
}

#[derive(Clone)]
struct RecordedSelectionBinding {
    binding: ScenarioOrdinal,
    generation: u64,
    attempt: u16,
    reason: TriggerReason,
    source_app: SourceAppIdentifier,
    triggered_at: Instant,
    expected_match: ExpectedMatch,
    popup_revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranslationCommitObservation {
    Applied,
    StaleGuard,
}

#[derive(Clone, Debug, Default)]
struct SelfInteractionCell {
    count: u64,
    buckets: BTreeSet<u8>,
}

#[derive(Clone)]
struct SelfIsolationRun {
    generation: u64,
    started_at: Instant,
    interactions: BTreeMap<(SelfInteractionWindow, SelfInteractionKind), SelfInteractionCell>,
}

pub struct AcceptanceRuntime {
    enabled: bool,
    capacity: usize,
    diagnostics: AcceptanceDiagnostics,
    fixtures: FixedFixtureTargets,
    self_bundle_identifier: Option<String>,
    session_multi_click_quiet_window_ms: Option<u64>,
    multi_click_window_mismatch_recorded: bool,
    session_started_at: Option<Instant>,
    armed: Option<ArmedScenario>,
    completed_arms: BTreeSet<ScenarioOrdinal>,
    generation_bindings: BTreeMap<u64, ScenarioOrdinal>,
    generation_attempts: BTreeMap<u64, u16>,
    generation_reasons: BTreeMap<u64, TriggerReason>,
    generation_source_apps: BTreeMap<u64, SourceAppIdentifier>,
    generation_source_pids: BTreeMap<u64, i32>,
    pending_reads: BTreeMap<u64, PendingRead>,
    recorded_generations: BTreeSet<u64>,
    selection_revision_bindings: BTreeMap<u64, RecordedSelectionBinding>,
    rapid_probe_claimed_revisions: BTreeSet<u64>,
    translation_evidence_revisions: BTreeSet<u64>,
    duplicate_generations_suppressed: u64,
    self_isolation: Option<SelfIsolationRun>,
    self_isolation_completed: bool,
    lifecycle_generation: u64,
    completed_tap_generations: BTreeSet<u64>,
    last_completed_watcher_generation: Option<u64>,
    repeated_location_clusters: RepeatedLocationClusters,
}

impl AcceptanceRuntime {
    pub fn from_environment(capacity: usize) -> Result<Self, AcceptanceRuntimeError> {
        let value = std::env::var_os(ACCEPTANCE_ENABLE_ENV);
        let enabled =
            acceptance_enabled_from_value(value.as_deref(), cfg!(feature = "acceptance-testing"));
        Self::new(enabled, capacity)
    }

    pub fn new(enabled: bool, capacity: usize) -> Result<Self, AcceptanceRuntimeError> {
        Ok(Self {
            enabled,
            capacity,
            diagnostics: AcceptanceDiagnostics::new(enabled, capacity)?,
            fixtures: FixedFixtureTargets::load()?,
            self_bundle_identifier: None,
            session_multi_click_quiet_window_ms: None,
            multi_click_window_mismatch_recorded: false,
            session_started_at: None,
            armed: None,
            completed_arms: BTreeSet::new(),
            generation_bindings: BTreeMap::new(),
            generation_attempts: BTreeMap::new(),
            generation_reasons: BTreeMap::new(),
            generation_source_apps: BTreeMap::new(),
            generation_source_pids: BTreeMap::new(),
            pending_reads: BTreeMap::new(),
            recorded_generations: BTreeSet::new(),
            selection_revision_bindings: BTreeMap::new(),
            rapid_probe_claimed_revisions: BTreeSet::new(),
            translation_evidence_revisions: BTreeSet::new(),
            duplicate_generations_suppressed: 0,
            self_isolation: None,
            self_isolation_completed: false,
            lifecycle_generation: 0,
            completed_tap_generations: BTreeSet::new(),
            last_completed_watcher_generation: None,
            repeated_location_clusters: RepeatedLocationClusters::default(),
        })
    }

    pub fn status(&self) -> AcceptanceRuntimeStatus {
        AcceptanceRuntimeStatus {
            enabled: self.enabled,
            tap_injection_available: self.enabled && cfg!(feature = "acceptance-testing"),
            preset: FULL_BASELINE_PRESET,
            phase: self.diagnostics.phase(),
            armed: self.armed.as_ref().map(|armed| ArmedScenarioStatus {
                scenario: armed.scenario,
                ordinal: armed.ordinal,
            }),
            self_isolation_active: self.self_isolation.is_some(),
            pending_generations: self.pending_reads.len(),
            pending_rapid_probes: self.pending_rapid_probe_count(),
            recorded_generations: self.recorded_generations.len(),
            duplicate_generations_suppressed: self.duplicate_generations_suppressed,
        }
    }

    pub fn start_full_baseline(
        &mut self,
        metadata: AcceptanceRuntimeMetadata,
    ) -> Result<AcceptanceRuntimeStatus, AcceptanceRuntimeError> {
        self.start_full_baseline_at(metadata, Instant::now())
    }

    fn start_full_baseline_at(
        &mut self,
        metadata: AcceptanceRuntimeMetadata,
        now: Instant,
    ) -> Result<AcceptanceRuntimeStatus, AcceptanceRuntimeError> {
        let self_bundle_identifier = metadata.app.bundle_identifier.clone();
        let config = full_baseline_config(metadata)?;
        let multi_click_quiet_window_ms = config.multi_click_quiet_window_ms;
        self.diagnostics.start(config)?;
        self.self_bundle_identifier = Some(self_bundle_identifier);
        self.session_multi_click_quiet_window_ms = Some(multi_click_quiet_window_ms);
        self.multi_click_window_mismatch_recorded = false;
        self.session_started_at = Some(now);
        self.armed = None;
        self.completed_arms.clear();
        self.generation_bindings.clear();
        self.generation_attempts.clear();
        self.generation_reasons.clear();
        self.generation_source_apps.clear();
        self.generation_source_pids.clear();
        self.pending_reads.clear();
        self.recorded_generations.clear();
        self.selection_revision_bindings.clear();
        self.rapid_probe_claimed_revisions.clear();
        self.translation_evidence_revisions.clear();
        self.duplicate_generations_suppressed = 0;
        self.self_isolation = None;
        self.self_isolation_completed = false;
        self.lifecycle_generation = 0;
        self.completed_tap_generations.clear();
        self.last_completed_watcher_generation = None;
        self.repeated_location_clusters = RepeatedLocationClusters::default();
        Ok(self.status())
    }

    /// Compare the currently active native state-machine value with the value
    /// frozen into this acceptance session. Watcher restarts may legitimately
    /// recapture a changed macOS preference, but that makes an in-flight report
    /// internally inconsistent and therefore invalid rather than silently
    /// changing its latency contract.
    pub fn validate_runtime_multi_click_quiet_window(
        &mut self,
        observed_ms: u64,
    ) -> Result<(), AcceptanceRuntimeError> {
        if self.diagnostics.phase() != SessionPhase::Running {
            return Ok(());
        }
        let expected = self.session_multi_click_quiet_window_ms;
        let observed_valid =
            full_baseline_latency_threshold(Scenario::TextEditDoubleClick, observed_ms).is_some();
        if expected == Some(observed_ms) && observed_valid {
            return Ok(());
        }
        if !self.multi_click_window_mismatch_recorded {
            self.mark_integrity_failure()?;
            self.multi_click_window_mismatch_recorded = true;
        }
        Err(AcceptanceRuntimeError::InvalidMultiClickQuietWindow)
    }

    pub fn arm(
        &mut self,
        scenario: Scenario,
        ordinal: u32,
    ) -> Result<AcceptanceRuntimeStatus, AcceptanceRuntimeError> {
        self.arm_at(scenario, ordinal, Instant::now())
    }

    fn arm_at(
        &mut self,
        scenario: Scenario,
        ordinal: u32,
        now: Instant,
    ) -> Result<AcceptanceRuntimeStatus, AcceptanceRuntimeError> {
        self.ensure_running()?;
        if self.self_isolation.is_some() {
            return Err(AcceptanceRuntimeError::SelfIsolationActive);
        }
        if self.pending_rapid_probe_count() > 0 {
            return Err(AcceptanceRuntimeError::PendingRapidProbes);
        }
        if matches!(scenario, Scenario::SelfWindowIsolation) {
            return Err(AcceptanceRuntimeError::ScenarioNotArmable);
        }
        let maximum =
            scenario_maximum_ordinal(scenario).ok_or(AcceptanceRuntimeError::ScenarioNotArmable)?;
        if ordinal == 0 || ordinal > maximum {
            return Err(AcceptanceRuntimeError::InvalidOrdinal);
        }
        let key = ScenarioOrdinal { scenario, ordinal };
        if self.completed_arms.contains(&key)
            || self.armed.as_ref().is_some_and(|armed| armed.key() == key)
        {
            return Err(AcceptanceRuntimeError::DuplicateArm);
        }

        if let Some(previous) = self.armed.take() {
            self.flush_pending_for_binding(previous.key(), now)?;
            self.completed_arms.insert(previous.key());
        }
        self.armed = Some(ArmedScenario::new(scenario, ordinal, now));
        Ok(self.status())
    }

    pub fn observe_native_read(
        &mut self,
        observation: NativeReadObservation<'_>,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        self.observe_native_read_at(observation, Instant::now())
    }

    pub fn reject_native_observation(&mut self) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.diagnostics.phase() != SessionPhase::Running {
            return Ok(HookDisposition::Ignored);
        }
        self.mark_integrity_failure()?;
        Err(AcceptanceRuntimeError::InvalidNativeObservation)
    }

    fn observe_native_read_at(
        &mut self,
        observation: NativeReadObservation<'_>,
        now: Instant,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.diagnostics.phase() != SessionPhase::Running {
            return Ok(HookDisposition::Ignored);
        }
        let status_consistent = match observation.status {
            "selection_read_found" => observation.found_text && observation.terminal,
            "selection_read_empty" => !observation.found_text,
            "selection_read_duplicate" => observation.found_text && observation.terminal,
            _ => false,
        };
        let reason = classify_trigger_reason(observation.reason);
        if observation.generation == 0
            || observation.attempt == 0
            || observation.source_pid <= 0
            || !observation.trigger_to_read_ms.is_finite()
            || observation.trigger_to_read_ms < 0.0
            || observation.trigger_to_read_ms > MAX_NATIVE_TRIGGER_TO_READ_MS
            || !status_consistent
            || reason.is_none()
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let reason = reason.expect("validated trigger reason");
        if observation.status == "selection_read_duplicate" {
            self.duplicate_generations_suppressed =
                self.duplicate_generations_suppressed.saturating_add(1);
            self.mark_duplicate_evidence(DuplicateEvidenceKind::NativeRead)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let source_app = classify_source_app(
            observation.source_bundle_id,
            self.self_bundle_identifier.as_deref(),
        );

        if self.recorded_generations.contains(&observation.generation) {
            let existing_binding = self
                .generation_bindings
                .get(&observation.generation)
                .copied();
            let active_binding = self
                .self_isolation
                .as_ref()
                .map(|_| ScenarioOrdinal {
                    scenario: Scenario::SelfWindowIsolation,
                    ordinal: 1,
                })
                .or_else(|| self.armed.as_ref().map(ArmedScenario::key));
            let replay_is_consistent = existing_binding.is_some()
                && active_binding.is_none_or(|active| Some(active) == existing_binding)
                && self.generation_source_pids.get(&observation.generation)
                    == Some(&observation.source_pid)
                && self.generation_attempts.get(&observation.generation)
                    == Some(&observation.attempt)
                && self.generation_reasons.get(&observation.generation) == Some(&reason)
                && self
                    .generation_source_apps
                    .get(&observation.generation)
                    .is_some_and(|existing| source_apps_are_consistent(existing, &source_app))
                && existing_binding.is_some_and(|binding| {
                    trigger_reason_matches_scenario(reason, binding.scenario)
                });
            if !replay_is_consistent {
                self.mark_integrity_failure()?;
                return Err(AcceptanceRuntimeError::GenerationRebound);
            }
            self.duplicate_generations_suppressed =
                self.duplicate_generations_suppressed.saturating_add(1);
            self.mark_duplicate_evidence(DuplicateEvidenceKind::NativeRead)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }

        let (binding, binding_started_at) = if let Some(run) = self.self_isolation.as_ref() {
            (
                ScenarioOrdinal {
                    scenario: Scenario::SelfWindowIsolation,
                    ordinal: 1,
                },
                run.started_at,
            )
        } else {
            let Some(armed) = self.armed.as_ref() else {
                self.mark_integrity_failure()?;
                return Err(AcceptanceRuntimeError::MissingNativeRead);
            };
            if !is_selection_scenario(armed.scenario) {
                self.mark_integrity_failure()?;
                return Err(AcceptanceRuntimeError::MissingNativeRead);
            }
            (armed.key(), armed.armed_at)
        };
        if self
            .generation_bindings
            .get(&observation.generation)
            .is_some_and(|existing| *existing != binding)
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::GenerationRebound);
        }
        if !trigger_reason_matches_scenario(reason, binding.scenario) {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let elapsed = Duration::try_from_secs_f64(observation.trigger_to_read_ms / 1_000.0)
            .map_err(|_| AcceptanceRuntimeError::InvalidNativeObservation)?;
        let session_start = self
            .session_started_at
            .ok_or(AcceptanceRuntimeError::NotRunning)?;
        if elapsed > now.saturating_duration_since(session_start) {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let triggered_at = now
            .checked_sub(elapsed)
            .map(|instant| instant.max(session_start))
            .unwrap_or(session_start);
        if triggered_at < binding_started_at {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }

        if self
            .generation_bindings
            .get(&observation.generation)
            .is_some_and(|existing| *existing != binding)
            || self
                .generation_source_pids
                .get(&observation.generation)
                .is_some_and(|existing| *existing != observation.source_pid)
            || self
                .generation_reasons
                .get(&observation.generation)
                .is_some_and(|existing| *existing != reason)
            || self
                .generation_source_apps
                .get(&observation.generation)
                .is_some_and(|existing| !source_apps_are_consistent(existing, &source_app))
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::GenerationRebound);
        }
        if let Some(previous_attempt) = self.generation_attempts.get(&observation.generation) {
            if observation.attempt < *previous_attempt {
                self.mark_integrity_failure()?;
                return Err(AcceptanceRuntimeError::InvalidNativeObservation);
            }
            if observation.attempt == *previous_attempt {
                self.duplicate_generations_suppressed =
                    self.duplicate_generations_suppressed.saturating_add(1);
                self.mark_duplicate_evidence(DuplicateEvidenceKind::NativeRead)?;
                return Err(AcceptanceRuntimeError::InvalidNativeObservation);
            }
        }
        if !self
            .generation_bindings
            .contains_key(&observation.generation)
            && self.generation_bindings.len() >= self.capacity
        {
            self.diagnostics.mark_capacity_overflow()?;
            return Err(AcceptanceRuntimeError::Diagnostics(
                DiagnosticsError::CapacityExceeded,
            ));
        }
        self.generation_bindings
            .entry(observation.generation)
            .or_insert(binding);
        self.generation_source_pids
            .entry(observation.generation)
            .or_insert(observation.source_pid);
        self.generation_reasons
            .entry(observation.generation)
            .or_insert(reason);
        self.generation_source_apps
            .entry(observation.generation)
            .and_modify(|existing| {
                if matches!(existing, SourceAppIdentifier::Unknown) {
                    *existing = source_app.clone();
                }
            })
            .or_insert_with(|| source_app.clone());
        self.generation_attempts
            .insert(observation.generation, observation.attempt);

        if !observation.terminal && binding.scenario != Scenario::SelfWindowIsolation {
            return Ok(HookDisposition::Pending);
        }

        self.pending_reads
            .entry(observation.generation)
            .and_modify(|pending| {
                if observation.attempt >= pending.attempt {
                    pending.attempt = observation.attempt;
                    pending.reason = reason;
                    pending.found_text |= observation.found_text;
                    pending.triggered_at = pending.triggered_at.min(triggered_at);
                    if matches!(pending.source_app, SourceAppIdentifier::Unknown)
                        && !matches!(source_app, SourceAppIdentifier::Unknown)
                    {
                        pending.source_app = source_app.clone();
                    }
                }
            })
            .or_insert(PendingRead {
                binding,
                generation: observation.generation,
                attempt: observation.attempt,
                reason,
                source_app,
                source_pid: observation.source_pid,
                triggered_at,
                found_text: observation.found_text,
            });
        if !observation.terminal {
            return self.record_popup_outcome_at(
                PopupOutcomeObservation {
                    generation: observation.generation,
                    selected_text: None,
                    source_pid: Some(observation.source_pid),
                    selection_revision: 0,
                    popup_revision: None,
                    outcome: SelectionOutcome::Cancelled,
                    mouse_up_anchor: None,
                },
                now,
            );
        }
        if observation.found_text {
            Ok(HookDisposition::Pending)
        } else {
            self.record_popup_outcome_at(
                PopupOutcomeObservation {
                    generation: observation.generation,
                    selected_text: None,
                    source_pid: Some(observation.source_pid),
                    selection_revision: 0,
                    popup_revision: None,
                    outcome: SelectionOutcome::NoSelection,
                    mouse_up_anchor: None,
                },
                now,
            )
        }
    }

    pub fn record_popup_commit(
        &mut self,
        generation: u64,
        selected_text: &str,
        source_pid: i32,
        selection_revision: u64,
        popup_revision: u64,
        mouse_up_anchor: MouseUpAnchor,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        self.record_popup_outcome_at(
            PopupOutcomeObservation {
                generation,
                selected_text: Some(selected_text),
                source_pid: Some(source_pid),
                selection_revision,
                popup_revision: Some(popup_revision),
                outcome: SelectionOutcome::PopupCommitted,
                mouse_up_anchor: Some(mouse_up_anchor),
            },
            Instant::now(),
        )
    }

    pub fn record_popup_failure(
        &mut self,
        generation: u64,
        selected_text: Option<&str>,
        source_pid: i32,
        mouse_up_anchor: Option<MouseUpAnchor>,
        outcome: SelectionOutcome,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if !matches!(
            outcome,
            SelectionOutcome::NoSelection
                | SelectionOutcome::ReadFailed
                | SelectionOutcome::StaleWriteRejected
                | SelectionOutcome::Cancelled
        ) {
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        self.record_popup_outcome_at(
            PopupOutcomeObservation {
                generation,
                selected_text,
                source_pid: Some(source_pid),
                selection_revision: 0,
                popup_revision: None,
                outcome,
                mouse_up_anchor,
            },
            Instant::now(),
        )
    }

    fn record_popup_outcome_at(
        &mut self,
        observation: PopupOutcomeObservation<'_>,
        now: Instant,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        let PopupOutcomeObservation {
            generation,
            selected_text,
            source_pid: popup_source_pid,
            selection_revision,
            popup_revision,
            outcome,
            mouse_up_anchor,
        } = observation;
        if self.diagnostics.phase() != SessionPhase::Running {
            return Ok(HookDisposition::Ignored);
        }
        if self.recorded_generations.contains(&generation) {
            if popup_source_pid.is_none_or(|source_pid| {
                source_pid <= 0 || self.generation_source_pids.get(&generation) != Some(&source_pid)
            }) {
                self.mark_integrity_failure()?;
                return Err(AcceptanceRuntimeError::GenerationRebound);
            }
            let active_binding = self
                .self_isolation
                .as_ref()
                .map(|_| ScenarioOrdinal {
                    scenario: Scenario::SelfWindowIsolation,
                    ordinal: 1,
                })
                .or_else(|| self.armed.as_ref().map(ArmedScenario::key));
            if active_binding
                .is_some_and(|binding| self.generation_bindings.get(&generation) != Some(&binding))
            {
                self.mark_integrity_failure()?;
                return Err(AcceptanceRuntimeError::GenerationRebound);
            }
            self.duplicate_generations_suppressed =
                self.duplicate_generations_suppressed.saturating_add(1);
            self.mark_duplicate_evidence(DuplicateEvidenceKind::PopupOutcome)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let Some(pending) = self.pending_reads.remove(&generation) else {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::MissingNativeRead);
        };
        if popup_source_pid
            .is_some_and(|source_pid| source_pid <= 0 || source_pid != pending.source_pid)
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::GenerationRebound);
        }

        let location_evidence = if outcome == SelectionOutcome::PopupCommitted {
            self.classify_repeated_location(pending.binding, pending.reason, mouse_up_anchor)?
        } else {
            None
        };
        let expected_match = selected_text
            .map(|text| self.classify_fixture(pending.binding.scenario, &pending.source_app, text))
            .unwrap_or(ExpectedMatch::NotApplicable);
        let record = self.selection_record_from_pending(
            &pending,
            now,
            SelectionRecordDetails {
                selection_revision,
                popup_revision,
                outcome,
                expected_match,
                location_evidence,
            },
        );
        if outcome == SelectionOutcome::PopupCommitted
            && self
                .selection_revision_bindings
                .contains_key(&selection_revision)
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::GenerationRebound);
        }
        self.recorded_generations.insert(generation);
        self.diagnostics
            .record(AcceptanceRecord::Selection(record))?;
        if let (SelectionOutcome::PopupCommitted, Some(committed_popup_revision)) =
            (outcome, popup_revision)
        {
            self.selection_revision_bindings.insert(
                selection_revision,
                RecordedSelectionBinding {
                    binding: pending.binding,
                    generation: pending.generation,
                    attempt: pending.attempt,
                    reason: pending.reason,
                    source_app: pending.source_app,
                    triggered_at: pending.triggered_at,
                    expected_match,
                    popup_revision: committed_popup_revision,
                },
            );
        }
        self.advance_armed_after_selection(pending.binding, expected_match);
        Ok(HookDisposition::Recorded)
    }

    pub fn observe_translation_commit(
        &mut self,
        guard_selection_revision: u64,
        observation: TranslationCommitObservation,
        controller_selection_revision: u64,
        controller_popup_revision: u64,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        self.observe_translation_commit_at(
            guard_selection_revision,
            observation,
            controller_selection_revision,
            controller_popup_revision,
            Instant::now(),
        )
    }

    /// Claims the fixed, acceptance-only stale-write probe for a successfully
    /// committed rapid-A selection. The claim is server-owned and one-shot:
    /// callers cannot turn an arbitrary selection into probe evidence.
    pub fn claim_rapid_probe(
        &mut self,
        selection_revision: u64,
    ) -> Result<bool, AcceptanceRuntimeError> {
        if self.diagnostics.phase() != SessionPhase::Running || selection_revision == 0 {
            return Ok(false);
        }
        let Some(binding) = self.selection_revision_bindings.get(&selection_revision) else {
            return Ok(false);
        };
        if binding.binding.scenario != Scenario::RapidAThenB
            || binding.expected_match != ExpectedMatch::A
            || self.armed.as_ref().map(ArmedScenario::key) != Some(binding.binding)
        {
            return Ok(false);
        }
        if self
            .rapid_probe_claimed_revisions
            .contains(&selection_revision)
        {
            return Ok(false);
        }
        if self.rapid_probe_claimed_revisions.len() >= self.capacity {
            self.diagnostics.mark_capacity_overflow()?;
            return Err(AcceptanceRuntimeError::Diagnostics(
                DiagnosticsError::CapacityExceeded,
            ));
        }
        self.rapid_probe_claimed_revisions
            .insert(selection_revision);
        Ok(true)
    }

    /// Resolves a claimed probe as an integrity failure when the controller
    /// could not start or schedule the fixed probe. This prevents a poisoned
    /// acceptance run from remaining permanently pending or being reported as
    /// successful without controller evidence.
    pub fn reject_rapid_probe(
        &mut self,
        selection_revision: u64,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.diagnostics.phase() != SessionPhase::Running
            || !self
                .rapid_probe_claimed_revisions
                .contains(&selection_revision)
        {
            return Ok(HookDisposition::Ignored);
        }
        if !self
            .translation_evidence_revisions
            .insert(selection_revision)
        {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::RapidProbe)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let binding = self
            .selection_revision_bindings
            .get(&selection_revision)
            .map(|binding| binding.binding);
        self.mark_integrity_failure()?;
        if self.armed.as_ref().map(ArmedScenario::key) == binding {
            self.complete_current_arm();
        }
        Ok(HookDisposition::Recorded)
    }

    fn observe_translation_commit_at(
        &mut self,
        guard_selection_revision: u64,
        observation: TranslationCommitObservation,
        controller_selection_revision: u64,
        controller_popup_revision: u64,
        now: Instant,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.diagnostics.phase() != SessionPhase::Running {
            return Ok(HookDisposition::Ignored);
        }
        if guard_selection_revision == 0 || controller_selection_revision == 0 {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let Some(binding) = self
            .selection_revision_bindings
            .get(&guard_selection_revision)
            .cloned()
        else {
            return Ok(HookDisposition::Ignored);
        };
        if binding.binding.scenario != Scenario::RapidAThenB
            || binding.expected_match != ExpectedMatch::A
            || !self
                .rapid_probe_claimed_revisions
                .contains(&guard_selection_revision)
        {
            return Ok(HookDisposition::Ignored);
        }
        let b_controller_binding =
            self.selection_revision_bindings
                .iter()
                .find_map(|(revision, candidate)| {
                    (candidate.binding == binding.binding
                        && candidate.expected_match == ExpectedMatch::B)
                        .then_some((*revision, candidate.popup_revision))
                });
        if !self
            .translation_evidence_revisions
            .insert(guard_selection_revision)
        {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::RapidProbe)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        if controller_popup_revision == 0 {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        if observation == TranslationCommitObservation::StaleGuard
            && b_controller_binding.is_none_or(|(b_selection_revision, b_popup_revision)| {
                controller_selection_revision != b_selection_revision
                    || controller_popup_revision != b_popup_revision
            })
        {
            self.mark_integrity_failure()?;
        }
        let outcome = match observation {
            TranslationCommitObservation::Applied => SelectionOutcome::StaleWriteCommitted,
            TranslationCommitObservation::StaleGuard => SelectionOutcome::StaleWriteRejected,
        };
        let record = SelectionRecord {
            scenario: binding.binding.scenario,
            ordinal: binding.binding.ordinal,
            generation: binding.generation,
            selection_revision: guard_selection_revision,
            popup_revision: (observation == TranslationCommitObservation::Applied)
                .then_some(controller_popup_revision),
            controller_snapshot_selection_revision: Some(controller_selection_revision),
            controller_snapshot_popup_revision: Some(controller_popup_revision),
            attempt: binding.attempt,
            reason: binding.reason,
            source_app: binding.source_app,
            event_offset_micros: self.event_offset_micros(binding.triggered_at)?,
            controller_commit_offset_micros: (observation == TranslationCommitObservation::Applied)
                .then_some(self.event_offset_micros(now)?),
            end_to_end_latency_ms: duration_millis(
                now.saturating_duration_since(binding.triggered_at),
            ),
            outcome,
            expected_match: ExpectedMatch::A,
            location_evidence: None,
        };
        self.diagnostics
            .record(AcceptanceRecord::Selection(record))?;
        if self.armed.as_ref().map(ArmedScenario::key) == Some(binding.binding) {
            self.complete_current_arm();
        }
        Ok(HookDisposition::Recorded)
    }

    pub fn observe_tap_status(
        &mut self,
        status: &str,
        watcher_generation: u64,
        resolution_elapsed_micros: Option<u64>,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        self.observe_tap_status_at(
            status,
            watcher_generation,
            resolution_elapsed_micros,
            Instant::now(),
        )
    }

    fn observe_tap_status_at(
        &mut self,
        status: &str,
        watcher_generation: u64,
        resolution_elapsed_micros: Option<u64>,
        now: Instant,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.completed_tap_generations.contains(&watcher_generation) {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::TapStatus)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let Some(armed) = self.armed.as_ref() else {
            return Ok(HookDisposition::Ignored);
        };
        if armed.scenario != Scenario::TapDisabledRecovery {
            return Ok(HookDisposition::Ignored);
        }
        if watcher_generation == 0 {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let resolution = match status {
            "selection_mouse_tap_disabled_timeout_recovered"
            | "selection_mouse_tap_disabled_user_input_recovered" => LifecycleEvent::TapReady,
            "selection_mouse_tap_disabled_timeout_fallback_ax"
            | "selection_mouse_tap_disabled_user_input_fallback_ax"
            | "selection_mouse_tap_disabled_timeout_unavailable"
            | "selection_mouse_tap_disabled_user_input_unavailable" => LifecycleEvent::TapDegraded,
            _ => {
                self.mark_integrity_failure()?;
                return Err(AcceptanceRuntimeError::InvalidNativeObservation);
            }
        };
        if resolution_elapsed_micros.is_none_or(|elapsed| elapsed == 0) {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        if resolution_elapsed_micros.is_some_and(|elapsed| {
            self.session_started_at.is_none_or(|session_start| {
                elapsed > duration_micros(now.saturating_duration_since(session_start))
            })
        }) {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let disabled_at = resolution_elapsed_micros
            .and_then(|elapsed| now.checked_sub(Duration::from_micros(elapsed)))
            .map(|instant| {
                self.session_started_at
                    .map_or(instant, |session_start| instant.max(session_start))
            })
            .unwrap_or(now);

        let ordinal = armed.ordinal;
        let armed_at = armed.armed_at;
        let bound_generation = armed.tap_watcher_generation;
        let should_record_disabled = !armed.tap_disabled_seen;
        let should_record_resolution = !armed.tap_resolved;
        if disabled_at < armed_at
            || bound_generation.is_some_and(|generation| generation != watcher_generation)
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        if should_record_disabled {
            self.record_lifecycle(
                Scenario::TapDisabledRecovery,
                ordinal,
                watcher_generation,
                LifecycleEvent::TapDisabled,
                disabled_at,
            )?;
        }
        if should_record_resolution {
            self.record_lifecycle(
                Scenario::TapDisabledRecovery,
                ordinal,
                watcher_generation,
                resolution,
                now,
            )?;
        }
        if let Some(armed) = self.armed.as_mut() {
            armed
                .tap_watcher_generation
                .get_or_insert(watcher_generation);
            armed.tap_disabled_seen |= should_record_disabled;
            armed.tap_resolved |= should_record_resolution;
        }
        if should_record_resolution {
            self.completed_tap_generations.insert(watcher_generation);
            self.complete_current_arm();
        }
        Ok(if should_record_disabled || should_record_resolution {
            HookDisposition::Recorded
        } else {
            HookDisposition::DuplicateSuppressed
        })
    }

    pub fn observe_watcher_started(
        &mut self,
        watcher_generation: u64,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.last_completed_watcher_generation == Some(watcher_generation) {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::WatcherPhase)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let Some(armed) = self.armed.as_ref() else {
            return Ok(HookDisposition::Ignored);
        };
        if armed.scenario != Scenario::WatcherRestart {
            return Ok(HookDisposition::Ignored);
        }
        if armed.watcher_generation == Some(watcher_generation) && armed.watcher_started_seen {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::WatcherPhase)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        if watcher_generation == 0
            || armed.watcher_generation != Some(watcher_generation)
            || !armed.watcher_release_seen
            || armed.watcher_started_seen
            || armed.watcher_started_resources_seen
            || armed.watcher_resolution_event.is_some()
            || armed.watcher_active_resources_seen
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let ordinal = armed.ordinal;
        self.record_lifecycle(
            Scenario::WatcherRestart,
            ordinal,
            watcher_generation,
            LifecycleEvent::WatcherStarted,
            Instant::now(),
        )?;
        if let Some(armed) = self.armed.as_mut() {
            armed.watcher_started_seen = true;
        }
        Ok(HookDisposition::Recorded)
    }

    pub fn observe_watcher_resolution(
        &mut self,
        watcher_generation: u64,
        degraded: bool,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.last_completed_watcher_generation == Some(watcher_generation) {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::WatcherPhase)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let Some(armed) = self.armed.as_ref() else {
            return Ok(HookDisposition::Ignored);
        };
        if armed.scenario != Scenario::WatcherRestart {
            return Ok(HookDisposition::Ignored);
        }
        let event = if degraded {
            LifecycleEvent::WatcherDegraded
        } else {
            LifecycleEvent::WatcherReady
        };
        if armed.watcher_generation == Some(watcher_generation)
            && armed.watcher_resolution_event.is_some()
        {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::WatcherPhase)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        if watcher_generation == 0
            || armed.watcher_generation != Some(watcher_generation)
            || !armed.watcher_release_seen
            || !armed.watcher_started_seen
            || !armed.watcher_started_resources_seen
            || armed.watcher_resolution_event.is_some()
            || armed.watcher_active_resources_seen
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let ordinal = armed.ordinal;
        self.record_lifecycle(
            Scenario::WatcherRestart,
            ordinal,
            watcher_generation,
            event,
            Instant::now(),
        )?;
        if let Some(armed) = self.armed.as_mut() {
            armed.watcher_resolution_event = Some(event);
        }
        Ok(HookDisposition::Recorded)
    }

    pub fn observe_watcher_resources(
        &mut self,
        watcher_generation: u64,
        event: LifecycleEvent,
        resources: WatcherResourceCounts,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.last_completed_watcher_generation == Some(watcher_generation) {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::WatcherPhase)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let Some(armed) = self.armed.as_ref() else {
            return Ok(HookDisposition::Ignored);
        };
        if armed.scenario != Scenario::WatcherRestart {
            return Ok(HookDisposition::Ignored);
        }
        if watcher_generation == 0
            || !matches!(
                event,
                LifecycleEvent::WatcherStarted
                    | LifecycleEvent::WatcherReady
                    | LifecycleEvent::WatcherDegraded
                    | LifecycleEvent::WatcherReleased
            )
        {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let duplicate_phase = armed.watcher_generation == Some(watcher_generation)
            && match event {
                LifecycleEvent::WatcherReleased => armed.watcher_release_seen,
                LifecycleEvent::WatcherStarted => armed.watcher_started_resources_seen,
                LifecycleEvent::WatcherReady | LifecycleEvent::WatcherDegraded => {
                    armed.watcher_active_resources_seen
                }
                _ => false,
            };
        if duplicate_phase {
            self.mark_duplicate_evidence(DuplicateEvidenceKind::WatcherPhase)?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let phase_valid = match event {
            LifecycleEvent::WatcherReleased => {
                armed.watcher_generation.is_none()
                    && !armed.watcher_release_seen
                    && !armed.watcher_started_seen
                    && !armed.watcher_started_resources_seen
                    && armed.watcher_resolution_event.is_none()
                    && !armed.watcher_active_resources_seen
                    && self
                        .last_completed_watcher_generation
                        .is_none_or(|previous| watcher_generation > previous)
            }
            LifecycleEvent::WatcherStarted => {
                armed.watcher_generation == Some(watcher_generation)
                    && armed.watcher_release_seen
                    && armed.watcher_started_seen
                    && !armed.watcher_started_resources_seen
                    && armed.watcher_resolution_event.is_none()
                    && !armed.watcher_active_resources_seen
            }
            LifecycleEvent::WatcherReady | LifecycleEvent::WatcherDegraded => {
                armed.watcher_generation == Some(watcher_generation)
                    && armed.watcher_release_seen
                    && armed.watcher_started_seen
                    && armed.watcher_started_resources_seen
                    && armed.watcher_resolution_event == Some(event)
                    && !armed.watcher_active_resources_seen
            }
            _ => false,
        };
        if !phase_valid {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let record = WatcherResourceRecord {
            scenario: Scenario::WatcherRestart,
            ordinal: armed.ordinal,
            generation: watcher_generation,
            event_offset_micros: self.event_offset_micros(Instant::now())?,
            event,
            resources,
        };
        self.diagnostics
            .record(AcceptanceRecord::WatcherResources(record))?;
        if let Some(armed) = self.armed.as_mut() {
            match event {
                LifecycleEvent::WatcherReleased => {
                    armed.watcher_generation = Some(watcher_generation);
                    armed.watcher_release_seen = true;
                }
                LifecycleEvent::WatcherStarted => {
                    armed.watcher_started_resources_seen = true;
                }
                LifecycleEvent::WatcherReady | LifecycleEvent::WatcherDegraded => {
                    armed.watcher_active_resources_seen = true;
                }
                _ => {}
            }
        }
        if matches!(
            event,
            LifecycleEvent::WatcherReady | LifecycleEvent::WatcherDegraded
        ) {
            self.last_completed_watcher_generation = Some(watcher_generation);
            self.complete_current_arm();
        }
        Ok(HookDisposition::Recorded)
    }

    pub fn record_self_interaction(
        &mut self,
        window: SelfInteractionWindow,
        kind: SelfInteractionKind,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        self.record_self_interaction_at(window, kind, Instant::now())
    }

    fn record_self_interaction_at(
        &mut self,
        window: SelfInteractionWindow,
        kind: SelfInteractionKind,
        now: Instant,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        if self.diagnostics.phase() != SessionPhase::Running {
            return Ok(HookDisposition::Ignored);
        }
        let Some(run) = self.self_isolation.as_mut() else {
            return Ok(HookDisposition::Ignored);
        };
        if now < run.started_at {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }
        let bucket = now.saturating_duration_since(run.started_at).as_secs()
            / SELF_INTERACTION_BUCKET_DURATION_SECS;
        if bucket >= u64::from(SELF_INTERACTION_BUCKET_COUNT) {
            return Ok(HookDisposition::Ignored);
        }
        let bucket =
            u8::try_from(bucket).map_err(|_| AcceptanceRuntimeError::InvalidNativeObservation)?;
        let cell = run.interactions.entry((window, kind)).or_default();
        let Some(next_count) = cell.count.checked_add(1) else {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        };
        cell.count = next_count;
        cell.buckets.insert(bucket);
        Ok(HookDisposition::Recorded)
    }

    pub fn set_self_isolation(
        &mut self,
        action: SelfIsolationAction,
    ) -> Result<AcceptanceRuntimeStatus, AcceptanceRuntimeError> {
        self.set_self_isolation_at(action, Instant::now())
    }

    fn set_self_isolation_at(
        &mut self,
        action: SelfIsolationAction,
        now: Instant,
    ) -> Result<AcceptanceRuntimeStatus, AcceptanceRuntimeError> {
        self.ensure_running()?;
        if self.pending_rapid_probe_count() > 0 {
            return Err(AcceptanceRuntimeError::PendingRapidProbes);
        }
        match action {
            SelfIsolationAction::Start => {
                if self.self_isolation.is_some() {
                    return Err(AcceptanceRuntimeError::SelfIsolationActive);
                }
                if self.self_isolation_completed {
                    self.mark_integrity_failure()?;
                    return Err(AcceptanceRuntimeError::DuplicateArm);
                }
                if let Some(previous) = self.armed.take() {
                    self.flush_pending_for_binding(previous.key(), now)?;
                    self.completed_arms.insert(previous.key());
                }
                self.lifecycle_generation = self.lifecycle_generation.saturating_add(1).max(1);
                let generation = self.lifecycle_generation;
                self.record_lifecycle(
                    Scenario::SelfWindowIsolation,
                    1,
                    generation,
                    LifecycleEvent::ScenarioStarted,
                    now,
                )?;
                self.self_isolation = Some(SelfIsolationRun {
                    generation,
                    started_at: now,
                    interactions: BTreeMap::new(),
                });
            }
            SelfIsolationAction::End => {
                let run = self
                    .self_isolation
                    .take()
                    .ok_or(AcceptanceRuntimeError::SelfIsolationNotActive)?;
                self.flush_pending_for_binding(
                    ScenarioOrdinal {
                        scenario: Scenario::SelfWindowIsolation,
                        ordinal: 1,
                    },
                    now,
                )?;
                self.diagnostics.record(AcceptanceRecord::SelfInteraction(
                    self_interaction_record(&run),
                ))?;
                self.record_lifecycle(
                    Scenario::SelfWindowIsolation,
                    1,
                    run.generation,
                    LifecycleEvent::ScenarioEnded,
                    now.max(run.started_at),
                )?;
                self.self_isolation_completed = true;
            }
        }
        Ok(self.status())
    }

    pub fn end(&mut self) -> Result<AcceptanceReport, AcceptanceRuntimeError> {
        self.end_at(Instant::now())
    }

    fn end_at(&mut self, now: Instant) -> Result<AcceptanceReport, AcceptanceRuntimeError> {
        self.ensure_running()?;
        if self.self_isolation.is_some() {
            return Err(AcceptanceRuntimeError::SelfIsolationActive);
        }
        if self.pending_rapid_probe_count() > 0 {
            return Err(AcceptanceRuntimeError::PendingRapidProbes);
        }
        let bindings = self
            .pending_reads
            .values()
            .map(|pending| pending.binding)
            .collect::<BTreeSet<_>>();
        for binding in bindings {
            self.flush_pending_for_binding(binding, now)?;
        }
        if let Some(armed) = self.armed.take() {
            self.completed_arms.insert(armed.key());
        }
        Ok(self.diagnostics.end()?)
    }

    pub fn export_atomic(
        &self,
        path: &Path,
    ) -> Result<AcceptanceExportReceipt, AcceptanceRuntimeError> {
        let report = self.diagnostics.export_snapshot()?;
        let content = serde_json::to_vec_pretty(&report)
            .map_err(|error| AcceptanceRuntimeError::Export(error.to_string()))?;
        ensure_privacy_safe_json(&content)?;
        atomic_write(path, &content)?;
        Ok(AcceptanceExportReceipt {
            path: path.to_string_lossy().into_owned(),
            recorded_records: report.recorded_records,
            valid: report.status.valid,
        })
    }

    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.self_bundle_identifier = None;
        self.session_multi_click_quiet_window_ms = None;
        self.multi_click_window_mismatch_recorded = false;
        self.session_started_at = None;
        self.armed = None;
        self.completed_arms.clear();
        self.generation_bindings.clear();
        self.generation_attempts.clear();
        self.generation_reasons.clear();
        self.generation_source_apps.clear();
        self.generation_source_pids.clear();
        self.pending_reads.clear();
        self.recorded_generations.clear();
        self.selection_revision_bindings.clear();
        self.rapid_probe_claimed_revisions.clear();
        self.translation_evidence_revisions.clear();
        self.duplicate_generations_suppressed = 0;
        self.self_isolation = None;
        self.self_isolation_completed = false;
        self.lifecycle_generation = 0;
        self.completed_tap_generations.clear();
        self.last_completed_watcher_generation = None;
        self.repeated_location_clusters = RepeatedLocationClusters::default();
    }

    fn ensure_running(&self) -> Result<(), AcceptanceRuntimeError> {
        if self.diagnostics.phase() == SessionPhase::Running {
            Ok(())
        } else {
            Err(AcceptanceRuntimeError::NotRunning)
        }
    }

    fn mark_integrity_failure(&mut self) -> Result<(), AcceptanceRuntimeError> {
        self.diagnostics.mark_rejected_record()?;
        Ok(())
    }

    fn mark_duplicate_evidence(
        &mut self,
        kind: DuplicateEvidenceKind,
    ) -> Result<(), AcceptanceRuntimeError> {
        self.diagnostics.mark_duplicate_evidence(kind)?;
        Ok(())
    }

    fn pending_rapid_probe_count(&self) -> usize {
        self.rapid_probe_claimed_revisions
            .difference(&self.translation_evidence_revisions)
            .count()
    }

    fn classify_repeated_location(
        &mut self,
        binding: ScenarioOrdinal,
        reason: TriggerReason,
        anchor: Option<MouseUpAnchor>,
    ) -> Result<Option<LocationEvidence>, AcceptanceRuntimeError> {
        if !matches!(
            binding.scenario,
            Scenario::SameTextSameLocation | Scenario::SameTextDifferentLocation
        ) {
            return Ok(None);
        }
        let Some(anchor) = anchor else {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        };
        if reason != TriggerReason::MouseDrag || !anchor.x.is_finite() || !anchor.y.is_finite() {
            self.mark_integrity_failure()?;
            return Err(AcceptanceRuntimeError::InvalidNativeObservation);
        }

        let evidence = match binding.scenario {
            Scenario::SameTextSameLocation => {
                let result = if binding.ordinal == 1 {
                    self.repeated_location_clusters.same_reference = Some(anchor);
                    LocationMatchResult::ReferenceEstablished
                } else if self
                    .repeated_location_clusters
                    .same_reference
                    .is_some_and(|reference| {
                        anchors_within(reference, anchor, SAME_LOCATION_TOLERANCE_LOGICAL_POINTS)
                    })
                {
                    LocationMatchResult::MatchedExpectedCluster
                } else {
                    LocationMatchResult::MismatchedExpectedCluster
                };
                LocationEvidence {
                    category: LocationCategory::ReferenceA,
                    result,
                }
            }
            Scenario::SameTextDifferentLocation => {
                let category = if binding.ordinal % 2 == 1 {
                    LocationCategory::ReferenceA
                } else {
                    LocationCategory::ReferenceB
                };
                let result = match binding.ordinal {
                    1 => {
                        self.repeated_location_clusters.different_reference_a = Some(anchor);
                        LocationMatchResult::ReferenceEstablished
                    }
                    2 => {
                        let sufficiently_separated = self
                            .repeated_location_clusters
                            .different_reference_a
                            .is_some_and(|reference_a| {
                                anchors_separated_by_at_least(
                                    reference_a,
                                    anchor,
                                    DIFFERENT_LOCATION_MIN_SEPARATION_LOGICAL_POINTS,
                                )
                            });
                        if sufficiently_separated {
                            self.repeated_location_clusters.different_reference_b = Some(anchor);
                            LocationMatchResult::ReferenceEstablished
                        } else if self
                            .repeated_location_clusters
                            .different_reference_a
                            .is_some()
                        {
                            LocationMatchResult::InsufficientClusterSeparation
                        } else {
                            LocationMatchResult::MismatchedExpectedCluster
                        }
                    }
                    _ => {
                        let expected_reference = match category {
                            LocationCategory::ReferenceA => {
                                self.repeated_location_clusters.different_reference_a
                            }
                            LocationCategory::ReferenceB => {
                                self.repeated_location_clusters.different_reference_b
                            }
                        };
                        let references_remain_separated = match (
                            self.repeated_location_clusters.different_reference_a,
                            self.repeated_location_clusters.different_reference_b,
                        ) {
                            (Some(reference_a), Some(reference_b)) => {
                                anchors_separated_by_at_least(
                                    reference_a,
                                    reference_b,
                                    DIFFERENT_LOCATION_MIN_SEPARATION_LOGICAL_POINTS,
                                )
                            }
                            _ => false,
                        };
                        if !references_remain_separated {
                            LocationMatchResult::InsufficientClusterSeparation
                        } else if expected_reference.is_some_and(|reference| {
                            anchors_within(
                                reference,
                                anchor,
                                SAME_LOCATION_TOLERANCE_LOGICAL_POINTS,
                            )
                        }) {
                            LocationMatchResult::MatchedExpectedCluster
                        } else {
                            LocationMatchResult::MismatchedExpectedCluster
                        }
                    }
                };
                LocationEvidence { category, result }
            }
            _ => unreachable!("non-location scenario returned before classification"),
        };
        Ok(Some(evidence))
    }

    fn classify_fixture(
        &self,
        scenario: Scenario,
        source_app: &SourceAppIdentifier,
        selected_text: &str,
    ) -> ExpectedMatch {
        if !source_matches_scenario(source_app, scenario) {
            return ExpectedMatch::Mismatch;
        }
        let candidate = normalize_fixture_text(selected_text);
        if scenario == Scenario::RapidAThenB {
            if candidate == normalize_fixture_text(&self.fixtures.rapid_a) {
                ExpectedMatch::A
            } else if candidate == normalize_fixture_text(&self.fixtures.rapid_b) {
                ExpectedMatch::B
            } else {
                ExpectedMatch::Mismatch
            }
        } else if self
            .fixtures
            .expected_for(scenario)
            .is_some_and(|expected| candidate == normalize_fixture_text(expected))
        {
            ExpectedMatch::Match
        } else {
            ExpectedMatch::Mismatch
        }
    }

    fn selection_record_from_pending(
        &self,
        pending: &PendingRead,
        now: Instant,
        details: SelectionRecordDetails,
    ) -> SelectionRecord {
        let SelectionRecordDetails {
            selection_revision,
            popup_revision,
            outcome,
            expected_match,
            location_evidence,
        } = details;
        SelectionRecord {
            scenario: pending.binding.scenario,
            ordinal: pending.binding.ordinal,
            generation: pending.generation,
            selection_revision,
            popup_revision,
            controller_snapshot_selection_revision: (outcome == SelectionOutcome::PopupCommitted)
                .then_some(selection_revision),
            controller_snapshot_popup_revision: (outcome == SelectionOutcome::PopupCommitted)
                .then_some(popup_revision)
                .flatten(),
            attempt: pending.attempt,
            reason: pending.reason,
            source_app: pending.source_app.clone(),
            event_offset_micros: self
                .event_offset_micros(pending.triggered_at)
                .unwrap_or_default(),
            controller_commit_offset_micros: (outcome == SelectionOutcome::PopupCommitted)
                .then(|| self.event_offset_micros(now).unwrap_or_default()),
            end_to_end_latency_ms: duration_millis(
                now.saturating_duration_since(pending.triggered_at),
            ),
            outcome,
            expected_match,
            location_evidence,
        }
    }

    fn flush_pending_for_binding(
        &mut self,
        binding: ScenarioOrdinal,
        now: Instant,
    ) -> Result<(), AcceptanceRuntimeError> {
        let generations = self
            .pending_reads
            .iter()
            .filter_map(|(generation, pending)| (pending.binding == binding).then_some(*generation))
            .collect::<Vec<_>>();
        for generation in generations {
            let Some(pending) = self.pending_reads.remove(&generation) else {
                continue;
            };
            if !self.recorded_generations.insert(generation) {
                self.duplicate_generations_suppressed =
                    self.duplicate_generations_suppressed.saturating_add(1);
                self.mark_duplicate_evidence(DuplicateEvidenceKind::NativeRead)?;
                return Err(AcceptanceRuntimeError::InvalidNativeObservation);
            }
            let outcome = if pending.found_text {
                SelectionOutcome::Cancelled
            } else {
                SelectionOutcome::ReadFailed
            };
            let record = self.selection_record_from_pending(
                &pending,
                now,
                SelectionRecordDetails {
                    selection_revision: 0,
                    popup_revision: None,
                    outcome,
                    expected_match: ExpectedMatch::NotApplicable,
                    location_evidence: None,
                },
            );
            self.diagnostics
                .record(AcceptanceRecord::Selection(record))?;
        }
        Ok(())
    }

    fn advance_armed_after_selection(
        &mut self,
        binding: ScenarioOrdinal,
        expected_match: ExpectedMatch,
    ) {
        let Some(armed) = self.armed.as_mut() else {
            return;
        };
        if armed.key() != binding {
            return;
        }
        let complete = if armed.scenario == Scenario::RapidAThenB {
            expected_match == ExpectedMatch::Mismatch
        } else {
            true
        };
        if complete {
            self.complete_current_arm();
        }
    }

    fn complete_current_arm(&mut self) {
        if let Some(armed) = self.armed.take() {
            self.completed_arms.insert(armed.key());
        }
    }

    fn record_lifecycle(
        &mut self,
        scenario: Scenario,
        ordinal: u32,
        generation: u64,
        event: LifecycleEvent,
        now: Instant,
    ) -> Result<(), AcceptanceRuntimeError> {
        let record = LifecycleRecord {
            scenario,
            ordinal,
            generation,
            event_offset_micros: self.event_offset_micros(now)?,
            event,
        };
        self.diagnostics
            .record(AcceptanceRecord::Lifecycle(record))?;
        Ok(())
    }

    fn event_offset_micros(&self, now: Instant) -> Result<u64, AcceptanceRuntimeError> {
        let start = self
            .session_started_at
            .ok_or(AcceptanceRuntimeError::NotRunning)?;
        Ok(duration_micros(now.saturating_duration_since(start)))
    }
}

fn self_interaction_record(run: &SelfIsolationRun) -> SelfInteractionRecord {
    let aggregate = |window, kind| {
        let cell = run.interactions.get(&(window, kind));
        SelfInteractionAggregate {
            window,
            kind,
            count: cell.map_or(0, |cell| cell.count),
            bucket_coverage: cell
                .map(|cell| cell.buckets.iter().copied().collect())
                .unwrap_or_default(),
        }
    };
    SelfInteractionRecord {
        scenario: Scenario::SelfWindowIsolation,
        ordinal: 1,
        generation: run.generation,
        aggregates: vec![
            aggregate(
                SelfInteractionWindow::Settings,
                SelfInteractionKind::Pointer,
            ),
            aggregate(
                SelfInteractionWindow::Settings,
                SelfInteractionKind::Keyboard,
            ),
            aggregate(SelfInteractionWindow::Popup, SelfInteractionKind::Pointer),
            aggregate(SelfInteractionWindow::Popup, SelfInteractionKind::Keyboard),
        ],
    }
}

fn acceptance_enabled_from_value(value: Option<&OsStr>, acceptance_testing_build: bool) -> bool {
    value == Some(OsStr::new("1")) || acceptance_testing_build
}

pub fn multi_click_quiet_window_ms_from_seconds(
    seconds: f64,
) -> Result<u64, AcceptanceRuntimeError> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err(AcceptanceRuntimeError::InvalidMultiClickQuietWindow);
    }
    let milliseconds = seconds * 1_000.0;
    if !milliseconds.is_finite()
        || milliseconds <= 0.0
        || milliseconds > MAX_MULTI_CLICK_QUIET_WINDOW_MS as f64
    {
        return Err(AcceptanceRuntimeError::InvalidMultiClickQuietWindow);
    }
    // Never under-report the production state machine's quiet window when the
    // native floating-point value falls between integer milliseconds.
    let rounded_up = milliseconds.ceil() as u64;
    if rounded_up == 0 || rounded_up > MAX_MULTI_CLICK_QUIET_WINDOW_MS {
        return Err(AcceptanceRuntimeError::InvalidMultiClickQuietWindow);
    }
    Ok(rounded_up)
}

pub fn full_baseline_config(
    metadata: AcceptanceRuntimeMetadata,
) -> Result<AcceptanceSessionConfig, AcceptanceRuntimeError> {
    let fixtures = fixed_fixture_metadata()?;
    let multi_click_quiet_window_ms = metadata.multi_click_quiet_window_ms;
    let mut selection_expectations = [
        Scenario::TextEditDrag,
        Scenario::TextEditDoubleClick,
        Scenario::TextEditTripleClick,
        Scenario::TextEditShiftExtend,
        Scenario::TextEditKeyboardSelection,
        Scenario::SameTextSameLocation,
        Scenario::SameTextDifferentLocation,
    ]
    .into_iter()
    .map(|scenario| {
        let latency_threshold =
            full_baseline_latency_threshold(scenario, multi_click_quiet_window_ms)
                .ok_or(AcceptanceRuntimeError::InvalidMultiClickQuietWindow)?;
        Ok(SelectionExpectation {
            scenario,
            expected_count: TEXTEDIT_EXPECTED_COUNT,
            expected_match: ExpectedMatch::Match,
            latency_threshold: Some(latency_threshold),
        })
    })
    .collect::<Result<Vec<_>, AcceptanceRuntimeError>>()?;
    for scenario in [Scenario::SafariFixture, Scenario::PreviewFixture] {
        let latency_threshold =
            full_baseline_latency_threshold(scenario, multi_click_quiet_window_ms)
                .ok_or(AcceptanceRuntimeError::InvalidMultiClickQuietWindow)?;
        selection_expectations.push(SelectionExpectation {
            scenario,
            expected_count: 1,
            expected_match: ExpectedMatch::Match,
            latency_threshold: Some(latency_threshold),
        });
    }
    Ok(AcceptanceSessionConfig {
        app: metadata.app,
        system: metadata.system,
        baseline_applications: metadata.baseline_applications,
        fixtures,
        multi_click_quiet_window_ms,
        selection_expectations,
        rapid_a_to_b_pairs: RAPID_PAIR_COUNT,
        tap_recovery_attempts: 1,
        tap_resolution_limit_ms: DEFAULT_TAP_RESOLUTION_LIMIT_MS,
        watcher_restart_count: WATCHER_RESTART_COUNT,
        self_isolation_min_duration_ms: Some(SELF_ISOLATION_DURATION_MS),
    })
}

fn fixed_fixture_metadata() -> Result<Vec<FixtureMetadata>, AcceptanceRuntimeError> {
    let text_digest = sha256_hex(TEXT_FIXTURE.as_bytes());
    let html_digest = sha256_hex(HTML_FIXTURE);
    let pdf_digest = sha256_hex(PDF_FIXTURE);
    if text_digest != EXPECTED_TEXT_SHA256
        || html_digest != EXPECTED_HTML_SHA256
        || pdf_digest != EXPECTED_PDF_SHA256
    {
        return Err(AcceptanceRuntimeError::FixtureIntegrity);
    }
    Ok(vec![
        FixtureMetadata {
            kind: FixtureKind::TextEditPlainText,
            fixture_id: "paper-float-textedit-selection-baseline-v1".to_owned(),
            sha256: text_digest,
        },
        FixtureMetadata {
            kind: FixtureKind::SafariHtml,
            fixture_id: "paper-float-selection-baseline-v1".to_owned(),
            sha256: html_digest,
        },
        FixtureMetadata {
            kind: FixtureKind::PreviewPdf,
            fixture_id: "paper-float-selection-baseline-v1".to_owned(),
            sha256: pdf_digest,
        },
    ])
}

fn scenario_maximum_ordinal(scenario: Scenario) -> Option<u32> {
    match scenario {
        Scenario::TextEditDrag
        | Scenario::TextEditDoubleClick
        | Scenario::TextEditTripleClick
        | Scenario::TextEditShiftExtend
        | Scenario::TextEditKeyboardSelection
        | Scenario::SameTextSameLocation
        | Scenario::SameTextDifferentLocation => Some(TEXTEDIT_EXPECTED_COUNT),
        Scenario::SafariFixture | Scenario::PreviewFixture | Scenario::TapDisabledRecovery => {
            Some(1)
        }
        Scenario::RapidAThenB => Some(RAPID_PAIR_COUNT),
        Scenario::WatcherRestart => Some(WATCHER_RESTART_COUNT),
        Scenario::SelfWindowIsolation => None,
    }
}

fn is_selection_scenario(scenario: Scenario) -> bool {
    matches!(
        scenario,
        Scenario::TextEditDrag
            | Scenario::TextEditDoubleClick
            | Scenario::TextEditTripleClick
            | Scenario::TextEditShiftExtend
            | Scenario::TextEditKeyboardSelection
            | Scenario::SameTextSameLocation
            | Scenario::SameTextDifferentLocation
            | Scenario::SafariFixture
            | Scenario::PreviewFixture
            | Scenario::RapidAThenB
    )
}

fn classify_source_app(bundle_identifier: &str, self_bundle: Option<&str>) -> SourceAppIdentifier {
    match bundle_identifier {
        TEXTEDIT_BUNDLE_ID => SourceAppIdentifier::TextEdit,
        SAFARI_BUNDLE_ID => SourceAppIdentifier::Safari,
        PREVIEW_BUNDLE_ID => SourceAppIdentifier::Preview,
        UNKNOWN_BUNDLE_ID | "" => SourceAppIdentifier::Unknown,
        value if self_bundle == Some(value) => SourceAppIdentifier::SelfApplication,
        value if is_safe_bundle_identifier(value) => {
            SourceAppIdentifier::OtherBundleIdentifier(value.to_owned())
        }
        _ => SourceAppIdentifier::Unknown,
    }
}

fn source_matches_scenario(source: &SourceAppIdentifier, scenario: Scenario) -> bool {
    match scenario {
        Scenario::TextEditDrag
        | Scenario::TextEditDoubleClick
        | Scenario::TextEditTripleClick
        | Scenario::TextEditShiftExtend
        | Scenario::TextEditKeyboardSelection
        | Scenario::SameTextSameLocation
        | Scenario::SameTextDifferentLocation
        | Scenario::RapidAThenB => matches!(source, SourceAppIdentifier::TextEdit),
        Scenario::SafariFixture => matches!(source, SourceAppIdentifier::Safari),
        Scenario::PreviewFixture => matches!(source, SourceAppIdentifier::Preview),
        Scenario::SelfWindowIsolation => matches!(source, SourceAppIdentifier::SelfApplication),
        Scenario::TapDisabledRecovery | Scenario::WatcherRestart => false,
    }
}

fn classify_trigger_reason(reason: &str) -> Option<TriggerReason> {
    match reason {
        "mouse_up_drag" => Some(TriggerReason::MouseDrag),
        "mouse_up_double_click" => Some(TriggerReason::MouseDoubleClick),
        "mouse_up_triple_click" => Some(TriggerReason::MouseTripleClick),
        "mouse_up_shift" => Some(TriggerReason::MouseShiftExtend),
        "ax_observer" => Some(TriggerReason::AccessibilityNotification),
        _ => None,
    }
}

fn trigger_reason_matches_scenario(reason: TriggerReason, scenario: Scenario) -> bool {
    match scenario {
        Scenario::TextEditDrag => reason == TriggerReason::MouseDrag,
        Scenario::TextEditDoubleClick => reason == TriggerReason::MouseDoubleClick,
        Scenario::TextEditTripleClick => reason == TriggerReason::MouseTripleClick,
        Scenario::TextEditShiftExtend => reason == TriggerReason::MouseShiftExtend,
        Scenario::TextEditKeyboardSelection => reason == TriggerReason::AccessibilityNotification,
        Scenario::SameTextSameLocation | Scenario::SameTextDifferentLocation => {
            reason == TriggerReason::MouseDrag
        }
        Scenario::SafariFixture
        | Scenario::PreviewFixture
        | Scenario::RapidAThenB
        | Scenario::SelfWindowIsolation => matches!(
            reason,
            TriggerReason::MouseDrag
                | TriggerReason::MouseDoubleClick
                | TriggerReason::MouseTripleClick
                | TriggerReason::MouseShiftExtend
                | TriggerReason::AccessibilityNotification
        ),
        Scenario::TapDisabledRecovery | Scenario::WatcherRestart => false,
    }
}

fn source_apps_are_consistent(
    existing: &SourceAppIdentifier,
    candidate: &SourceAppIdentifier,
) -> bool {
    matches!(existing, SourceAppIdentifier::Unknown)
        || matches!(candidate, SourceAppIdentifier::Unknown)
        || existing == candidate
}

fn normalize_fixture_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_safe_bundle_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.contains('.')
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros().div_ceil(1_000)).unwrap_or(u64::MAX)
}

fn anchors_within(first: MouseUpAnchor, second: MouseUpAnchor, tolerance: f64) -> bool {
    (first.x - second.x).hypot(first.y - second.y) <= tolerance
}

fn anchors_separated_by_at_least(
    first: MouseUpAnchor,
    second: MouseUpAnchor,
    minimum: f64,
) -> bool {
    (first.x - second.x).hypot(first.y - second.y) >= minimum
}

fn ensure_privacy_safe_json(content: &[u8]) -> Result<(), AcceptanceRuntimeError> {
    let value: serde_json::Value = serde_json::from_slice(content)
        .map_err(|error| AcceptanceRuntimeError::Export(error.to_string()))?;
    const FORBIDDEN_KEYS: &[&str] = &[
        "text",
        "selectedText",
        "cleanedText",
        "translation",
        "clipboard",
        "windowTitle",
        "textPrefix",
        "textLength",
        "textHash",
        "apiKey",
        "key",
        "control",
        "target",
        "isTrusted",
        "x",
        "y",
        "coordinate",
        "coordinates",
        "anchor",
        "mouseUpAnchor",
        "position",
        "location",
        "range",
        "axRange",
        "elementId",
        "elementHash",
        "elementIdentifier",
        "elementIdentity",
    ];
    fn visit(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(object) => object
                .iter()
                .all(|(key, child)| !FORBIDDEN_KEYS.contains(&key.as_str()) && visit(child)),
            serde_json::Value::Array(values) => values.iter().all(visit),
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_) => true,
        }
    }
    if visit(&value) {
        Ok(())
    } else {
        Err(AcceptanceRuntimeError::Export(
            "acceptance export contains a forbidden content-bearing field".to_owned(),
        ))
    }
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), AcceptanceRuntimeError> {
    let parent = path.parent().ok_or_else(|| {
        AcceptanceRuntimeError::Export("acceptance export path has no parent".to_owned())
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| AcceptanceRuntimeError::Export(error.to_string()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(ACCEPTANCE_EXPORT_FILE);
    let sequence = EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary_path = parent.join(format!(
        ".{file_name}.tmp-{}-{sequence}",
        std::process::id()
    ));
    atomic_write_with_temporary_path(path, &temporary_path, content)
}

fn atomic_write_with_temporary_path(
    path: &Path,
    temporary_path: &Path,
    content: &[u8],
) -> Result<(), AcceptanceRuntimeError> {
    let parent = path.parent().ok_or_else(|| {
        AcceptanceRuntimeError::Export("acceptance export path has no parent".to_owned())
    })?;
    if temporary_path.parent() != Some(parent) {
        return Err(AcceptanceRuntimeError::Export(
            "acceptance export temporary path must share its destination directory".to_owned(),
        ));
    }
    let file = match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temporary_path)
    {
        Ok(file) => file,
        // create_new failed before this call owned the path. Never remove a
        // pre-existing file or symlink supplied by another actor.
        Err(error) => return Err(AcceptanceRuntimeError::Export(error.to_string())),
    };
    let mut file = Some(file);
    let mut temporary_owned = true;
    let result = (|| {
        let writer = file.as_mut().ok_or_else(|| {
            AcceptanceRuntimeError::Export(
                "acceptance export temporary file closed unexpectedly".to_owned(),
            )
        })?;
        writer
            .write_all(content)
            .and_then(|()| writer.sync_all())
            .map_err(|error| AcceptanceRuntimeError::Export(error.to_string()))?;
        drop(file.take());

        crate::validate_atomic_write_destination(path).map_err(AcceptanceRuntimeError::Export)?;
        fs::rename(temporary_path, path)
            .map_err(|error| AcceptanceRuntimeError::Export(error.to_string()))?;
        temporary_owned = false;
        let directory = fs::File::open(parent)
            .map_err(|error| AcceptanceRuntimeError::Export(error.to_string()))?;
        directory
            .sync_all()
            .map_err(|error| AcceptanceRuntimeError::Export(error.to_string()))?;
        Ok(())
    })();
    drop(file.take());
    if result.is_err() && temporary_owned {
        let _ = fs::remove_file(temporary_path);
    }
    result
}

pub fn export_path(data_dir: &Path) -> PathBuf {
    data_dir.join("acceptance").join(ACCEPTANCE_EXPORT_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acceptance_diagnostics::{
        Architecture, BaselineApplication, BuildKind, LatencyThreshold, OperatingSystem,
        ReportInvalidReason,
    };

    fn metadata() -> AcceptanceRuntimeMetadata {
        AcceptanceRuntimeMetadata {
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
            multi_click_quiet_window_ms: 530,
        }
    }

    fn started(capacity: usize) -> (AcceptanceRuntime, Instant) {
        let mut runtime = AcceptanceRuntime::new(true, capacity).unwrap();
        let start = Instant::now();
        runtime
            .start_full_baseline_at(metadata(), start)
            .expect("fixed full baseline should start");
        (runtime, start)
    }

    fn observation(
        generation: u64,
        attempt: u16,
        source_bundle_id: &'static str,
        found_text: bool,
        terminal: bool,
    ) -> NativeReadObservation<'static> {
        NativeReadObservation {
            status: if found_text {
                "selection_read_found"
            } else {
                "selection_read_empty"
            },
            generation,
            attempt,
            trigger_to_read_ms: 10.0,
            reason: "mouse_up_drag",
            source_bundle_id,
            source_pid: 321,
            found_text,
            terminal,
        }
    }

    fn commit_at(
        runtime: &mut AcceptanceRuntime,
        generation: u64,
        selected_text: &str,
        revision: u64,
        now: Instant,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        commit_at_anchor(
            runtime,
            generation,
            selected_text,
            revision,
            MouseUpAnchor { x: 100.0, y: 100.0 },
            now,
        )
    }

    fn commit_at_anchor(
        runtime: &mut AcceptanceRuntime,
        generation: u64,
        selected_text: &str,
        revision: u64,
        mouse_up_anchor: MouseUpAnchor,
        now: Instant,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        runtime.record_popup_outcome_at(
            PopupOutcomeObservation {
                generation,
                selected_text: Some(selected_text),
                source_pid: Some(321),
                selection_revision: revision,
                popup_revision: Some(revision),
                outcome: SelectionOutcome::PopupCommitted,
                mouse_up_anchor: Some(mouse_up_anchor),
            },
            now,
        )
    }

    fn record_repeated_location(
        runtime: &mut AcceptanceRuntime,
        start: Instant,
        scenario: Scenario,
        ordinal: u32,
        generation: u64,
        anchor: MouseUpAnchor,
    ) -> Result<HookDisposition, AcceptanceRuntimeError> {
        let armed_at = start + Duration::from_millis(generation.saturating_mul(30));
        runtime.arm_at(scenario, ordinal, armed_at)?;
        runtime.observe_native_read_at(
            observation(generation, 1, TEXTEDIT_BUNDLE_ID, true, true),
            armed_at + Duration::from_millis(10),
        )?;
        commit_at_anchor(
            runtime,
            generation,
            "repeatable phrase",
            generation,
            anchor,
            armed_at + Duration::from_millis(20),
        )
    }

    fn record_valid_self_interaction_pattern(
        runtime: &mut AcceptanceRuntime,
        isolation_started_at: Instant,
    ) {
        for bucket in 0..SELF_INTERACTION_BUCKET_COUNT {
            let window = if bucket % 2 == 0 {
                SelfInteractionWindow::Settings
            } else {
                SelfInteractionWindow::Popup
            };
            let kind = if bucket % 4 < 2 {
                SelfInteractionKind::Pointer
            } else {
                SelfInteractionKind::Keyboard
            };
            let event_at = isolation_started_at
                + Duration::from_secs(u64::from(bucket) * SELF_INTERACTION_BUCKET_DURATION_SECS)
                + if bucket == 0 {
                    Duration::from_millis(1)
                } else {
                    Duration::ZERO
                };
            assert_eq!(
                runtime.record_self_interaction_at(window, kind, event_at),
                Ok(HookDisposition::Recorded)
            );
        }
    }

    #[test]
    fn full_baseline_is_server_owned_and_exact() {
        let config = full_baseline_config(metadata()).unwrap();
        let expected_textedit = [
            Scenario::TextEditDrag,
            Scenario::TextEditDoubleClick,
            Scenario::TextEditTripleClick,
            Scenario::TextEditShiftExtend,
            Scenario::TextEditKeyboardSelection,
            Scenario::SameTextSameLocation,
            Scenario::SameTextDifferentLocation,
        ];
        for scenario in expected_textedit {
            let expectation = config
                .selection_expectations
                .iter()
                .find(|expectation| expectation.scenario == scenario)
                .unwrap();
            assert_eq!(expectation.expected_count, 30);
            let expected_threshold = if scenario == Scenario::TextEditDoubleClick {
                LatencyThreshold {
                    p95_limit_ms: 880,
                    max_limit_ms: 1_330,
                }
            } else {
                LatencyThreshold {
                    p95_limit_ms: 350,
                    max_limit_ms: 800,
                }
            };
            assert_eq!(expectation.latency_threshold, Some(expected_threshold));
        }
        for scenario in [Scenario::SafariFixture, Scenario::PreviewFixture] {
            let expectation = config
                .selection_expectations
                .iter()
                .find(|expectation| expectation.scenario == scenario)
                .unwrap();
            assert_eq!(expectation.expected_count, 1);
            assert_eq!(
                expectation.latency_threshold,
                Some(LatencyThreshold {
                    p95_limit_ms: 1_200,
                    max_limit_ms: 1_200,
                })
            );
        }
        assert_eq!(config.multi_click_quiet_window_ms, 530);
        assert_eq!(config.rapid_a_to_b_pairs, 20);
        assert_eq!(config.tap_recovery_attempts, 1);
        assert_eq!(config.tap_resolution_limit_ms, 2_000);
        assert_eq!(config.watcher_restart_count, 50);
        assert_eq!(config.self_isolation_min_duration_ms, Some(300_000));
        assert_eq!(config.baseline_applications.len(), 3);
        assert_eq!(config.fixtures.len(), 3);
    }

    #[test]
    fn native_multi_click_window_conversion_is_bounded_and_rounds_up() {
        assert_eq!(multi_click_quiet_window_ms_from_seconds(0.53), Ok(530));
        assert_eq!(
            multi_click_quiet_window_ms_from_seconds(0.530_000_1),
            Ok(531)
        );
        assert_eq!(multi_click_quiet_window_ms_from_seconds(0.000_1), Ok(1));
        assert_eq!(multi_click_quiet_window_ms_from_seconds(2.0), Ok(2_000));
        for invalid in [0.0, -0.1, f64::NAN, f64::INFINITY, 2.000_001] {
            assert_eq!(
                multi_click_quiet_window_ms_from_seconds(invalid),
                Err(AcceptanceRuntimeError::InvalidMultiClickQuietWindow)
            );
        }
    }

    #[test]
    fn full_baseline_rejects_missing_oversized_and_overflowing_quiet_windows() {
        for invalid in [0, MAX_MULTI_CLICK_QUIET_WINDOW_MS + 1, u64::MAX] {
            let mut invalid_metadata = metadata();
            invalid_metadata.multi_click_quiet_window_ms = invalid;
            assert_eq!(
                full_baseline_config(invalid_metadata),
                Err(AcceptanceRuntimeError::InvalidMultiClickQuietWindow)
            );
        }
    }

    #[test]
    fn native_quiet_window_drift_invalidates_the_report_once() {
        let (mut runtime, start) = started(8);
        assert_eq!(
            runtime.validate_runtime_multi_click_quiet_window(530),
            Ok(())
        );
        assert_eq!(
            runtime.validate_runtime_multi_click_quiet_window(531),
            Err(AcceptanceRuntimeError::InvalidMultiClickQuietWindow)
        );
        assert_eq!(
            runtime.validate_runtime_multi_click_quiet_window(0),
            Err(AcceptanceRuntimeError::InvalidMultiClickQuietWindow)
        );
        let report = runtime.end_at(start + Duration::from_millis(1)).unwrap();
        assert_eq!(report.rejected_records, 1);
        assert!(!report.status.integrity_valid);
        assert!(!report.status.valid);
        assert_eq!(report.multi_click_quiet_window_ms, 530);
    }

    #[test]
    fn latency_evidence_rounds_partial_milliseconds_up() {
        assert_eq!(duration_millis(Duration::from_micros(350_000)), 350);
        assert_eq!(duration_millis(Duration::from_micros(350_001)), 351);
        assert_eq!(duration_millis(Duration::from_micros(800_999)), 801);
        assert_eq!(duration_millis(Duration::from_micros(1_200_001)), 1_201);
    }

    #[test]
    fn explicit_enable_and_bounded_metadata_are_mandatory() {
        assert!(acceptance_enabled_from_value(Some(OsStr::new("1")), false));
        assert!(!acceptance_enabled_from_value(None, false));
        assert!(!acceptance_enabled_from_value(
            Some(OsStr::new("true")),
            false
        ));
        assert!(!acceptance_enabled_from_value(
            Some(OsStr::new("01")),
            false
        ));
        assert!(acceptance_enabled_from_value(None, true));
        assert!(acceptance_enabled_from_value(
            Some(OsStr::new("disabled")),
            true
        ));
        let status = AcceptanceRuntime::new(true, 16).unwrap().status();
        assert_eq!(
            status.tap_injection_available,
            cfg!(feature = "acceptance-testing")
        );
        let mut disabled = AcceptanceRuntime::new(false, 16).unwrap();
        assert!(matches!(
            disabled.start_full_baseline(metadata()),
            Err(AcceptanceRuntimeError::Diagnostics(
                DiagnosticsError::Disabled
            ))
        ));

        let mut forged = metadata();
        forged.app.version = "0.1.0 selected secret".to_owned();
        let mut runtime = AcceptanceRuntime::new(true, 16).unwrap();
        assert!(matches!(
            runtime.start_full_baseline(forged),
            Err(AcceptanceRuntimeError::Diagnostics(
                DiagnosticsError::InvalidConfig(_)
            ))
        ));
    }

    #[test]
    fn intermediate_empty_is_not_failure_and_terminal_commit_is_generation_bound() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        assert_eq!(
            runtime
                .observe_native_read_at(
                    observation(7, 1, TEXTEDIT_BUNDLE_ID, false, false),
                    start + Duration::from_millis(10),
                )
                .unwrap(),
            HookDisposition::Pending
        );
        assert!(runtime.pending_reads.is_empty());
        runtime
            .observe_native_read_at(
                observation(7, 2, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(20),
            )
            .unwrap();
        commit_at(
            &mut runtime,
            7,
            "Quartz rivers carry patient light across the valley.",
            1,
            start + Duration::from_millis(30),
        )
        .unwrap();
        let report = runtime.end_at(start + Duration::from_millis(40)).unwrap();
        assert_eq!(report.recorded_records, 1);
        assert_eq!(report.rejected_records, 0);
        let AcceptanceRecord::Selection(record) = &report.records[0] else {
            panic!("expected selection record")
        };
        assert_eq!(record.expected_match, ExpectedMatch::Match);
        assert_eq!(record.generation, 7);
        assert_eq!(record.attempt, 2);
        assert_eq!(record.controller_snapshot_selection_revision, Some(1));
    }

    #[test]
    fn completed_generation_replays_and_cross_pid_popup_stitching_are_rejected() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(71, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(10),
            )
            .unwrap();
        commit_at(
            &mut runtime,
            71,
            "Quartz rivers carry patient light across the valley.",
            71,
            start + Duration::from_millis(20),
        )
        .unwrap();

        let mut replay = observation(71, 1, TEXTEDIT_BUNDLE_ID, true, true);
        replay.source_pid = 999;
        assert!(matches!(
            runtime.observe_native_read_at(replay, start + Duration::from_millis(30)),
            Err(AcceptanceRuntimeError::GenerationRebound)
        ));
        assert!(matches!(
            runtime.record_popup_commit(
                71,
                "Quartz rivers carry patient light across the valley.",
                321,
                72,
                72,
                MouseUpAnchor { x: 10.0, y: 10.0 },
            ),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        let report = runtime.end_at(start + Duration::from_millis(40)).unwrap();
        assert_eq!(report.recorded_records, 1);
        assert_eq!(report.rejected_records, 2);
        assert_eq!(report.summary.duplicate_evidence.popup_outcome_replays, 1);
        assert!(!report.status.integrity_valid);
    }

    #[test]
    fn popup_pid_must_match_the_generation_native_read_pid() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(72, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(10),
            )
            .unwrap();
        assert!(matches!(
            runtime.record_popup_outcome_at(
                PopupOutcomeObservation {
                    generation: 72,
                    selected_text: Some("Quartz rivers carry patient light across the valley."),
                    source_pid: Some(999),
                    selection_revision: 72,
                    popup_revision: Some(72),
                    outcome: SelectionOutcome::PopupCommitted,
                    mouse_up_anchor: Some(MouseUpAnchor { x: 10.0, y: 10.0 }),
                },
                start + Duration::from_millis(20),
            ),
            Err(AcceptanceRuntimeError::GenerationRebound)
        ));
        let report = runtime.end_at(start + Duration::from_millis(30)).unwrap();
        assert_eq!(report.rejected_records, 1);
        assert!(!report.status.valid);
    }

    #[test]
    fn pre_arm_trigger_wrong_gesture_and_inconsistent_status_are_rejected_without_binding() {
        let (mut runtime, start) = started(32);
        runtime
            .arm_at(
                Scenario::TextEditDoubleClick,
                1,
                start + Duration::from_millis(500),
            )
            .unwrap();
        let mut pre_arm = observation(73, 1, TEXTEDIT_BUNDLE_ID, true, true);
        pre_arm.reason = "mouse_up_double_click";
        pre_arm.trigger_to_read_ms = 200.0;
        assert!(matches!(
            runtime.observe_native_read_at(pre_arm, start + Duration::from_millis(600)),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        assert!(!runtime.generation_bindings.contains_key(&73));

        let wrong_gesture = observation(74, 1, TEXTEDIT_BUNDLE_ID, true, true);
        assert!(matches!(
            runtime.observe_native_read_at(wrong_gesture, start + Duration::from_millis(620)),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        assert!(!runtime.generation_bindings.contains_key(&74));

        let mut inconsistent = observation(75, 1, TEXTEDIT_BUNDLE_ID, true, true);
        inconsistent.reason = "mouse_up_double_click";
        inconsistent.status = "selection_read_empty";
        assert!(matches!(
            runtime.observe_native_read_at(inconsistent, start + Duration::from_millis(630)),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        assert!(!runtime.generation_bindings.contains_key(&75));
        let report = runtime.end_at(start + Duration::from_millis(640)).unwrap();
        assert_eq!(report.rejected_records, 3);
        assert!(!report.status.integrity_valid);
    }

    #[test]
    fn terminal_replay_is_rejected_and_invalidates_the_report() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        assert_eq!(
            runtime
                .observe_native_read_at(
                    observation(8, 3, TEXTEDIT_BUNDLE_ID, false, true),
                    start + Duration::from_millis(650),
                )
                .unwrap(),
            HookDisposition::Recorded
        );
        assert!(matches!(
            runtime.observe_native_read_at(
                observation(8, 3, TEXTEDIT_BUNDLE_ID, false, true),
                start + Duration::from_millis(651),
            ),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        let report = runtime.end_at(start + Duration::from_millis(700)).unwrap();
        assert_eq!(report.recorded_records, 1);
        assert_eq!(runtime.duplicate_generations_suppressed, 1);
        assert_eq!(report.rejected_records, 1);
        assert_eq!(report.summary.duplicate_evidence.native_read_replays, 1);
        assert!(!report.status.valid);
    }

    #[test]
    fn duplicate_nonterminal_attempt_is_exported_as_duplicate_evidence() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(78, 1, TEXTEDIT_BUNDLE_ID, false, false),
                start + Duration::from_millis(10),
            )
            .unwrap();
        assert!(matches!(
            runtime.observe_native_read_at(
                observation(78, 1, TEXTEDIT_BUNDLE_ID, false, false),
                start + Duration::from_millis(11),
            ),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        runtime
            .observe_native_read_at(
                observation(78, 3, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(20),
            )
            .unwrap();
        commit_at(
            &mut runtime,
            78,
            "Quartz rivers carry patient light across the valley.",
            78,
            start + Duration::from_millis(30),
        )
        .unwrap();
        let report = runtime.end_at(start + Duration::from_millis(40)).unwrap();
        assert_eq!(report.summary.duplicate_evidence.native_read_replays, 1);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::DuplicateEvidence));
        assert!(!report.status.valid);
    }

    #[test]
    fn native_duplicate_status_is_never_accepted_as_selection_evidence() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        let mut duplicate = observation(79, 1, TEXTEDIT_BUNDLE_ID, true, true);
        duplicate.status = "selection_read_duplicate";
        assert!(matches!(
            runtime.observe_native_read_at(duplicate, start + Duration::from_millis(10)),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        assert!(!runtime.generation_bindings.contains_key(&79));
        let report = runtime.end_at(start + Duration::from_millis(20)).unwrap();
        assert_eq!(report.summary.duplicate_evidence.native_read_replays, 1);
        assert!(!report.status.valid);
    }

    #[test]
    fn source_and_fixed_fixture_classification_cannot_be_forged() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::SafariFixture, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(9, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(10),
            )
            .unwrap();
        commit_at(
            &mut runtime,
            9,
            "The same selection appears here.",
            1,
            start + Duration::from_millis(20),
        )
        .unwrap();
        let report = runtime.end_at(start + Duration::from_millis(30)).unwrap();
        let AcceptanceRecord::Selection(record) = &report.records[0] else {
            panic!("expected selection record")
        };
        assert_eq!(record.expected_match, ExpectedMatch::Mismatch);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::WrongClassification));
    }

    #[test]
    fn choosing_reference_a_for_every_different_location_ordinal_fails_acceptance() {
        let (mut runtime, start) = started(128);
        let reference_a = MouseUpAnchor { x: 220.0, y: 310.0 };
        for ordinal in 1..=TEXTEDIT_EXPECTED_COUNT {
            record_repeated_location(
                &mut runtime,
                start,
                Scenario::SameTextDifferentLocation,
                ordinal,
                u64::from(ordinal),
                reference_a,
            )
            .unwrap();
        }

        let report = runtime
            .end_at(start + Duration::from_millis(2_000))
            .unwrap();
        let second = report.records.iter().find_map(|record| match record {
            AcceptanceRecord::Selection(record)
                if record.scenario == Scenario::SameTextDifferentLocation
                    && record.ordinal == 2 =>
            {
                Some(record)
            }
            _ => None,
        });
        assert_eq!(
            second.and_then(|record| record.location_evidence),
            Some(LocationEvidence {
                category: LocationCategory::ReferenceB,
                result: LocationMatchResult::InsufficientClusterSeparation,
            })
        );
        assert!(report
            .summary
            .selections
            .wrong_classification
            .contains(&ScenarioOrdinal {
                scenario: Scenario::SameTextDifferentLocation,
                ordinal: 2,
            }));
        assert!(!report.status.acceptance_passed);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::WrongClassification));
    }

    #[test]
    fn same_location_tolerance_is_inclusive_and_larger_drift_is_rejected() {
        let (mut runtime, start) = started(32);
        let reference = MouseUpAnchor { x: 40.0, y: 80.0 };
        record_repeated_location(
            &mut runtime,
            start,
            Scenario::SameTextSameLocation,
            1,
            1,
            reference,
        )
        .unwrap();
        record_repeated_location(
            &mut runtime,
            start,
            Scenario::SameTextSameLocation,
            2,
            2,
            MouseUpAnchor {
                x: reference.x + SAME_LOCATION_TOLERANCE_LOGICAL_POINTS,
                y: reference.y,
            },
        )
        .unwrap();
        record_repeated_location(
            &mut runtime,
            start,
            Scenario::SameTextSameLocation,
            3,
            3,
            MouseUpAnchor {
                x: reference.x + SAME_LOCATION_TOLERANCE_LOGICAL_POINTS + 0.001,
                y: reference.y,
            },
        )
        .unwrap();

        let report = runtime.end_at(start + Duration::from_millis(200)).unwrap();
        let evidence = report
            .records
            .iter()
            .filter_map(|record| match record {
                AcceptanceRecord::Selection(record)
                    if record.scenario == Scenario::SameTextSameLocation =>
                {
                    record.location_evidence
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            evidence,
            vec![
                LocationEvidence {
                    category: LocationCategory::ReferenceA,
                    result: LocationMatchResult::ReferenceEstablished,
                },
                LocationEvidence {
                    category: LocationCategory::ReferenceA,
                    result: LocationMatchResult::MatchedExpectedCluster,
                },
                LocationEvidence {
                    category: LocationCategory::ReferenceA,
                    result: LocationMatchResult::MismatchedExpectedCluster,
                },
            ]
        );
        assert!(report
            .summary
            .selections
            .wrong_classification
            .contains(&ScenarioOrdinal {
                scenario: Scenario::SameTextSameLocation,
                ordinal: 3,
            }));
        assert!(!report.status.acceptance_passed);
    }

    #[test]
    fn different_location_minimum_separation_is_inclusive() {
        let (mut exact, exact_start) = started(32);
        record_repeated_location(
            &mut exact,
            exact_start,
            Scenario::SameTextDifferentLocation,
            1,
            1,
            MouseUpAnchor { x: 0.0, y: 0.0 },
        )
        .unwrap();
        record_repeated_location(
            &mut exact,
            exact_start,
            Scenario::SameTextDifferentLocation,
            2,
            2,
            MouseUpAnchor {
                x: DIFFERENT_LOCATION_MIN_SEPARATION_LOGICAL_POINTS,
                y: 0.0,
            },
        )
        .unwrap();
        let exact_report = exact
            .end_at(exact_start + Duration::from_millis(100))
            .unwrap();
        let AcceptanceRecord::Selection(second) = &exact_report.records[1] else {
            unreachable!()
        };
        assert_eq!(
            second.location_evidence,
            Some(LocationEvidence {
                category: LocationCategory::ReferenceB,
                result: LocationMatchResult::ReferenceEstablished,
            })
        );

        let (mut below, below_start) = started(32);
        record_repeated_location(
            &mut below,
            below_start,
            Scenario::SameTextDifferentLocation,
            1,
            11,
            MouseUpAnchor { x: 0.0, y: 0.0 },
        )
        .unwrap();
        record_repeated_location(
            &mut below,
            below_start,
            Scenario::SameTextDifferentLocation,
            2,
            12,
            MouseUpAnchor {
                x: DIFFERENT_LOCATION_MIN_SEPARATION_LOGICAL_POINTS - 0.001,
                y: 0.0,
            },
        )
        .unwrap();
        let below_report = below
            .end_at(below_start + Duration::from_millis(500))
            .unwrap();
        let AcceptanceRecord::Selection(second) = &below_report.records[1] else {
            unreachable!()
        };
        assert_eq!(
            second.location_evidence,
            Some(LocationEvidence {
                category: LocationCategory::ReferenceB,
                result: LocationMatchResult::InsufficientClusterSeparation,
            })
        );
        assert!(!below_report.status.acceptance_passed);
    }

    #[test]
    fn valid_different_location_alternation_matches_two_frozen_clusters() {
        let (mut runtime, start) = started(128);
        let reference_a = MouseUpAnchor { x: 500.0, y: 700.0 };
        let reference_b = MouseUpAnchor {
            x: 500.0,
            y: 700.0 + DIFFERENT_LOCATION_MIN_SEPARATION_LOGICAL_POINTS,
        };
        for ordinal in 1..=TEXTEDIT_EXPECTED_COUNT {
            let reference = if ordinal % 2 == 1 {
                reference_a
            } else {
                reference_b
            };
            let jitter = if ordinal <= 2 {
                0.0
            } else if ordinal % 4 < 2 {
                SAME_LOCATION_TOLERANCE_LOGICAL_POINTS
            } else {
                -SAME_LOCATION_TOLERANCE_LOGICAL_POINTS
            };
            record_repeated_location(
                &mut runtime,
                start,
                Scenario::SameTextDifferentLocation,
                ordinal,
                u64::from(ordinal),
                MouseUpAnchor {
                    x: reference.x + jitter,
                    y: reference.y,
                },
            )
            .unwrap();
        }
        let report = runtime
            .end_at(start + Duration::from_millis(2_000))
            .unwrap();
        assert_eq!(
            report
                .records
                .iter()
                .filter(|record| matches!(
                    record,
                    AcceptanceRecord::Selection(record)
                        if record.scenario == Scenario::SameTextDifferentLocation
                            && record.location_evidence.is_some_and(|evidence| {
                                evidence.result == LocationMatchResult::ReferenceEstablished
                                    || evidence.result
                                        == LocationMatchResult::MatchedExpectedCluster
                            })
                ))
                .count(),
            usize::try_from(TEXTEDIT_EXPECTED_COUNT).unwrap()
        );
        assert!(!report
            .summary
            .selections
            .wrong_classification
            .iter()
            .any(|key| key.scenario == Scenario::SameTextDifferentLocation));
    }

    #[test]
    fn repeated_location_ax_only_and_non_finite_anchor_fail_closed() {
        let (mut ax_only, ax_start) = started(32);
        ax_only
            .arm_at(Scenario::SameTextSameLocation, 1, ax_start)
            .unwrap();
        let mut ax_observation = observation(1, 1, TEXTEDIT_BUNDLE_ID, true, true);
        ax_observation.reason = "ax_observer";
        assert_eq!(
            ax_only.observe_native_read_at(ax_observation, ax_start + Duration::from_millis(10)),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        );
        let ax_report = ax_only
            .end_at(ax_start + Duration::from_millis(20))
            .unwrap();
        assert_eq!(ax_report.rejected_records, 1);
        assert!(!ax_report.status.integrity_valid);

        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let (mut runtime, start) = started(32);
            runtime
                .arm_at(Scenario::SameTextDifferentLocation, 1, start)
                .unwrap();
            runtime
                .observe_native_read_at(
                    observation(2, 1, TEXTEDIT_BUNDLE_ID, true, true),
                    start + Duration::from_millis(10),
                )
                .unwrap();
            assert_eq!(
                commit_at_anchor(
                    &mut runtime,
                    2,
                    "repeatable phrase",
                    2,
                    MouseUpAnchor {
                        x: invalid,
                        y: 20.0
                    },
                    start + Duration::from_millis(20),
                ),
                Err(AcceptanceRuntimeError::InvalidNativeObservation)
            );
            let report = runtime.end_at(start + Duration::from_millis(30)).unwrap();
            assert_eq!(report.rejected_records, 1);
            assert!(!report.status.integrity_valid);
            assert!(report.records.is_empty());
        }
    }

    #[test]
    fn exported_location_evidence_contains_no_raw_anchor_or_identity() {
        let (mut runtime, start) = started(32);
        record_repeated_location(
            &mut runtime,
            start,
            Scenario::SameTextSameLocation,
            1,
            1,
            MouseUpAnchor {
                x: 12_345.678_91,
                y: -98_765.432_19,
            },
        )
        .unwrap();
        let report = runtime.end_at(start + Duration::from_millis(100)).unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("12345.67891"));
        assert!(!json.contains("-98765.43219"));
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        fn assert_no_raw_location_keys(value: &serde_json::Value) {
            match value {
                serde_json::Value::Object(object) => {
                    for (key, child) in object {
                        assert!(
                            !matches!(
                                key.as_str(),
                                "x" | "y"
                                    | "coordinate"
                                    | "coordinates"
                                    | "anchor"
                                    | "mouseUpAnchor"
                                    | "position"
                                    | "location"
                                    | "range"
                                    | "axRange"
                                    | "elementId"
                                    | "elementHash"
                                    | "elementIdentifier"
                                    | "elementIdentity"
                            ),
                            "raw location/AX key leaked: {key}"
                        );
                        assert_no_raw_location_keys(child);
                    }
                }
                serde_json::Value::Array(values) => {
                    for child in values {
                        assert_no_raw_location_keys(child);
                    }
                }
                _ => {}
            }
        }
        assert_no_raw_location_keys(&value);
        let evidence = &value["records"][0]["locationEvidence"];
        assert_eq!(evidence.as_object().map(serde_json::Map::len), Some(2));
        assert_eq!(evidence["category"], "referenceA");
        assert_eq!(evidence["result"], "referenceEstablished");
    }

    #[test]
    fn double_and_triple_click_reasons_are_distinct_and_triple_matches_full_paragraph() {
        let (mut runtime, start) = started(32);
        runtime
            .arm_at(Scenario::TextEditTripleClick, 1, start)
            .unwrap();
        let mut triple = observation(76, 1, TEXTEDIT_BUNDLE_ID, true, true);
        triple.reason = "mouse_up_triple_click";
        runtime
            .observe_native_read_at(triple, start + Duration::from_millis(10))
            .unwrap();
        commit_at(
            &mut runtime,
            76,
            "Triple-click target: This entire paragraph is the fixed triple-click selection target.",
            76,
            start + Duration::from_millis(20),
        )
        .unwrap();

        runtime
            .arm_at(
                Scenario::TextEditDoubleClick,
                1,
                start + Duration::from_millis(30),
            )
            .unwrap();
        let mut wrong_click_count = observation(77, 1, TEXTEDIT_BUNDLE_ID, true, true);
        wrong_click_count.reason = "mouse_up_triple_click";
        assert!(matches!(
            runtime.observe_native_read_at(wrong_click_count, start + Duration::from_millis(40)),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        let report = runtime.end_at(start + Duration::from_millis(50)).unwrap();
        let triple_record = report.records.iter().find_map(|record| match record {
            AcceptanceRecord::Selection(record)
                if record.scenario == Scenario::TextEditTripleClick =>
            {
                Some(record)
            }
            _ => None,
        });
        assert_eq!(
            triple_record.map(|record| record.expected_match),
            Some(ExpectedMatch::Match)
        );
        assert_eq!(report.rejected_records, 1);
        assert!(!report.status.valid);
    }

    #[test]
    fn generation_rebind_and_invalid_numeric_observation_invalidate_integrity() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(10, 1, TEXTEDIT_BUNDLE_ID, false, false),
                start + Duration::from_millis(10),
            )
            .unwrap();
        runtime
            .arm_at(
                Scenario::TextEditDoubleClick,
                1,
                start + Duration::from_millis(20),
            )
            .unwrap();
        assert!(matches!(
            runtime.observe_native_read_at(
                observation(10, 2, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(30),
            ),
            Err(AcceptanceRuntimeError::GenerationRebound)
        ));
        let mut invalid = observation(11, 1, TEXTEDIT_BUNDLE_ID, true, true);
        invalid.trigger_to_read_ms = f64::INFINITY;
        assert!(matches!(
            runtime.observe_native_read_at(invalid, start + Duration::from_millis(40)),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        let report = runtime.end_at(start + Duration::from_millis(50)).unwrap();
        assert_eq!(report.rejected_records, 2);
        assert!(!report.status.integrity_valid);
    }

    #[test]
    fn capacity_overflow_is_visible_and_never_overwrites_evidence() {
        let (mut runtime, start) = started(1);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(20, 3, TEXTEDIT_BUNDLE_ID, false, true),
                start + Duration::from_millis(10),
            )
            .unwrap();
        runtime
            .arm_at(
                Scenario::TextEditDoubleClick,
                1,
                start + Duration::from_millis(20),
            )
            .unwrap();
        let mut next = observation(21, 3, TEXTEDIT_BUNDLE_ID, false, true);
        next.reason = "mouse_up_double_click";
        assert!(matches!(
            runtime.observe_native_read_at(next, start + Duration::from_millis(30)),
            Err(AcceptanceRuntimeError::Diagnostics(
                DiagnosticsError::CapacityExceeded
            ))
        ));
        let report = runtime.end_at(start + Duration::from_millis(40)).unwrap();
        assert_eq!(report.recorded_records, 1);
        assert_eq!(report.overflowed_records, 1);
        assert!(!report.status.integrity_valid);
    }

    fn record_rapid_pair(runtime: &mut AcceptanceRuntime, start: Instant) -> (u64, u64) {
        runtime.arm_at(Scenario::RapidAThenB, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(30, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(10),
            )
            .unwrap();
        commit_at(
            runtime,
            30,
            "FIRST_SELECTION_MUST_NOT_WIN",
            30,
            start + Duration::from_millis(20),
        )
        .unwrap();
        assert!(runtime.claim_rapid_probe(30).unwrap());
        assert!(!runtime.claim_rapid_probe(30).unwrap());
        assert_eq!(runtime.status().pending_rapid_probes, 1);
        runtime
            .observe_native_read_at(
                observation(31, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(200),
            )
            .unwrap();
        commit_at(
            runtime,
            31,
            "SECOND_SELECTION_MUST_WIN",
            31,
            start + Duration::from_millis(210),
        )
        .unwrap();
        (30, 31)
    }

    #[test]
    fn rapid_pair_requires_controller_confirmed_b_and_stale_a_rejection() {
        let (mut runtime, start) = started(32);
        let (a_revision, b_revision) = record_rapid_pair(&mut runtime, start);
        runtime
            .observe_translation_commit_at(
                a_revision,
                TranslationCommitObservation::StaleGuard,
                b_revision,
                b_revision,
                start + Duration::from_millis(220),
            )
            .unwrap();
        let report = runtime.end_at(start + Duration::from_millis(230)).unwrap();
        assert_eq!(report.summary.rapid_a_to_b.final_b_pairs, 1);
        assert_eq!(report.summary.rapid_a_to_b.stale_a_rejected_count, 1);
        assert!(!report
            .summary
            .rapid_a_to_b
            .missing_stale_rejection_pairs
            .contains(&1));
        assert!(!report
            .summary
            .rapid_a_to_b
            .invalid_stale_rejection_pairs
            .contains(&1));
        assert_eq!(report.summary.rapid_a_to_b.stale_a_write_count, 0);
    }

    #[test]
    fn rapid_pair_rejects_stale_guard_against_a_different_popup_snapshot() {
        let (mut runtime, start) = started(32);
        let (a_revision, b_revision) = record_rapid_pair(&mut runtime, start);
        runtime
            .observe_translation_commit_at(
                a_revision,
                TranslationCommitObservation::StaleGuard,
                b_revision,
                32,
                start + Duration::from_millis(220),
            )
            .unwrap();
        let report = runtime.end_at(start + Duration::from_millis(230)).unwrap();
        assert_eq!(
            report.summary.rapid_a_to_b.invalid_stale_rejection_pairs,
            vec![1]
        );
        assert!(!report.status.integrity_valid);
        assert!(!report.status.valid);
    }

    #[test]
    fn rapid_pair_remains_invalid_after_empty_evidence_then_valid_a_b() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::RapidAThenB, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(29, 1, TEXTEDIT_BUNDLE_ID, false, true),
                start + Duration::from_millis(15),
            )
            .unwrap();
        runtime
            .observe_native_read_at(
                observation(30, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(30),
            )
            .unwrap();
        commit_at(
            &mut runtime,
            30,
            "FIRST_SELECTION_MUST_NOT_WIN",
            30,
            start + Duration::from_millis(40),
        )
        .unwrap();
        assert!(runtime.claim_rapid_probe(30).unwrap());
        runtime
            .observe_native_read_at(
                observation(31, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(200),
            )
            .unwrap();
        commit_at(
            &mut runtime,
            31,
            "SECOND_SELECTION_MUST_WIN",
            31,
            start + Duration::from_millis(210),
        )
        .unwrap();
        runtime
            .observe_translation_commit_at(
                30,
                TranslationCommitObservation::StaleGuard,
                31,
                31,
                start + Duration::from_millis(220),
            )
            .unwrap();

        let report = runtime.end_at(start + Duration::from_millis(230)).unwrap();
        assert_eq!(
            report.summary.rapid_a_to_b.unexpected_evidence_pairs,
            vec![1]
        );
        assert_eq!(report.summary.rapid_a_to_b.unexpected_evidence_count, 1);
        assert!(!report.summary.rapid_a_to_b.passed);
        assert!(!report.status.valid);
    }

    #[test]
    fn rapid_probe_blocks_immediate_end_and_next_arm_until_controller_evidence() {
        let (mut runtime, start) = started(32);
        let (a_revision, b_revision) = record_rapid_pair(&mut runtime, start);
        assert!(matches!(
            runtime.end_at(start + Duration::from_millis(211)),
            Err(AcceptanceRuntimeError::PendingRapidProbes)
        ));
        assert!(matches!(
            runtime.arm_at(Scenario::RapidAThenB, 2, start + Duration::from_millis(212)),
            Err(AcceptanceRuntimeError::PendingRapidProbes)
        ));
        runtime
            .observe_translation_commit_at(
                a_revision,
                TranslationCommitObservation::StaleGuard,
                b_revision,
                b_revision,
                start + Duration::from_millis(620),
            )
            .unwrap();
        assert_eq!(runtime.status().pending_rapid_probes, 0);
        runtime
            .arm_at(Scenario::RapidAThenB, 2, start + Duration::from_millis(621))
            .unwrap();
    }

    #[test]
    fn rapid_probe_applied_without_b_is_explicit_failure_not_missing_evidence() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::RapidAThenB, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(32, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(10),
            )
            .unwrap();
        commit_at(
            &mut runtime,
            32,
            "FIRST_SELECTION_MUST_NOT_WIN",
            32,
            start + Duration::from_millis(20),
        )
        .unwrap();
        assert!(runtime.claim_rapid_probe(32).unwrap());
        runtime
            .observe_translation_commit_at(
                32,
                TranslationCommitObservation::Applied,
                32,
                33,
                start + Duration::from_millis(RAPID_PROBE_DELAY_MS),
            )
            .unwrap();
        let report = runtime
            .end_at(start + Duration::from_millis(RAPID_PROBE_DELAY_MS + 1))
            .unwrap();
        assert_eq!(report.summary.rapid_a_to_b.stale_a_write_count, 1);
        assert!(report.summary.rapid_a_to_b.missing_b_pairs.contains(&1));
        assert!(!report.status.valid);
    }

    #[test]
    fn stale_a_applied_mutant_is_detected_even_if_input_order_looked_correct() {
        let (mut runtime, start) = started(32);
        let (a_revision, _) = record_rapid_pair(&mut runtime, start);
        runtime
            .observe_translation_commit_at(
                a_revision,
                TranslationCommitObservation::Applied,
                a_revision,
                32,
                start + Duration::from_millis(220),
            )
            .unwrap();
        let report = runtime.end_at(start + Duration::from_millis(230)).unwrap();
        assert_eq!(report.summary.rapid_a_to_b.stale_a_write_count, 1);
        assert!(!report.summary.rapid_a_to_b.passed);
        assert!(report
            .status
            .invalid_reasons
            .contains(&ReportInvalidReason::StaleWriteCommitted));
    }

    #[test]
    fn native_elapsed_tap_recovery_and_monotonic_self_isolation_are_measured() {
        let (mut runtime, start) = started(32);
        runtime
            .arm_at(Scenario::TapDisabledRecovery, 1, start)
            .unwrap();
        runtime
            .observe_tap_status_at(
                "selection_mouse_tap_disabled_timeout_recovered",
                50,
                Some(1_500_000),
                start + Duration::from_millis(2_000),
            )
            .unwrap();
        let isolation_start = start + Duration::from_millis(3_000);
        runtime
            .set_self_isolation_at(SelfIsolationAction::Start, isolation_start)
            .unwrap();
        record_valid_self_interaction_pattern(&mut runtime, isolation_start);
        runtime
            .set_self_isolation_at(
                SelfIsolationAction::End,
                start + Duration::from_millis(303_000),
            )
            .unwrap();
        let report = runtime
            .end_at(start + Duration::from_millis(303_001))
            .unwrap();
        assert_eq!(report.summary.tap_recovery.max_resolution_ms, Some(1_500));
        assert!(report.summary.tap_recovery.passed);
        assert_eq!(
            report.summary.self_isolation.observed_duration_ms,
            Some(300_000)
        );
        assert!(report.summary.self_isolation.passed);
    }

    #[test]
    fn watcher_hook_accepts_two_distinct_taps_and_requires_zero_release_snapshot() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::WatcherRestart, 1, start).unwrap();
        let released = WatcherResourceCounts {
            snapshot_version: 1,
            ..WatcherResourceCounts::default()
        };
        let running = WatcherResourceCounts {
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
        runtime
            .observe_watcher_resources(70, LifecycleEvent::WatcherReleased, released)
            .unwrap();
        runtime.observe_watcher_started(70).unwrap();
        runtime
            .observe_watcher_resources(70, LifecycleEvent::WatcherStarted, running)
            .unwrap();
        runtime.observe_watcher_resolution(70, false).unwrap();
        runtime
            .observe_watcher_resources(70, LifecycleEvent::WatcherReady, running)
            .unwrap();
        assert!(runtime.status().armed.is_none());

        let report = runtime.end_at(start + Duration::from_millis(10)).unwrap();
        assert!(!report
            .summary
            .watcher_resources
            .missing_restarts
            .contains(&1));
        assert!(!report
            .summary
            .watcher_resources
            .missing_release_restarts
            .contains(&1));
        assert!(report
            .summary
            .watcher_resources
            .released_resource_leaks
            .is_empty());
        assert_eq!(report.summary.watcher_resources.final_active, Some(running));
    }

    #[test]
    fn watcher_restart_rejects_cross_generation_and_missing_phase_stitching() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::WatcherRestart, 1, start).unwrap();
        let released = WatcherResourceCounts {
            snapshot_version: 1,
            ..WatcherResourceCounts::default()
        };
        let running = WatcherResourceCounts {
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
        runtime
            .observe_watcher_resources(80, LifecycleEvent::WatcherReleased, released)
            .unwrap();
        assert!(matches!(
            runtime.observe_watcher_started(81),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        runtime.observe_watcher_started(80).unwrap();
        assert!(matches!(
            runtime.observe_watcher_resolution(80, false),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        runtime
            .observe_watcher_resources(80, LifecycleEvent::WatcherStarted, running)
            .unwrap();
        assert!(matches!(
            runtime.observe_watcher_resources(80, LifecycleEvent::WatcherStarted, running),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        let report = runtime.end_at(start + Duration::from_millis(10)).unwrap();
        assert!(!report.status.integrity_valid);
        assert!(!report.summary.watcher_resources.passed);
    }

    #[test]
    fn tap_event_that_started_before_arm_cannot_be_reused() {
        let (mut runtime, start) = started(32);
        runtime
            .arm_at(
                Scenario::TapDisabledRecovery,
                1,
                start + Duration::from_secs(2),
            )
            .unwrap();
        assert!(matches!(
            runtime.observe_tap_status_at(
                "selection_mouse_tap_disabled_timeout_recovered",
                90,
                Some(1_500_000),
                start + Duration::from_millis(3_000),
            ),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        let report = runtime
            .end_at(start + Duration::from_millis(3_001))
            .unwrap();
        assert!(!report.status.integrity_valid);
        assert!(!report.summary.tap_recovery.passed);
    }

    #[test]
    fn completed_tap_status_replay_is_exported_and_invalidates() {
        let (mut runtime, start) = started(32);
        runtime
            .arm_at(Scenario::TapDisabledRecovery, 1, start)
            .unwrap();
        runtime
            .observe_tap_status_at(
                "selection_mouse_tap_disabled_timeout_recovered",
                91,
                Some(500_000),
                start + Duration::from_millis(500),
            )
            .unwrap();
        assert!(matches!(
            runtime.observe_tap_status_at(
                "selection_mouse_tap_disabled_timeout_recovered",
                91,
                Some(500_000),
                start + Duration::from_millis(501),
            ),
            Err(AcceptanceRuntimeError::InvalidNativeObservation)
        ));
        let report = runtime.end_at(start + Duration::from_millis(502)).unwrap();
        assert_eq!(report.summary.duplicate_evidence.tap_status_replays, 1);
        assert!(!report.status.valid);
    }

    #[test]
    fn self_interaction_empty_wait_single_window_and_partial_buckets_all_fail() {
        let (mut empty, empty_start) = started(32);
        empty
            .set_self_isolation_at(SelfIsolationAction::Start, empty_start)
            .unwrap();
        empty
            .set_self_isolation_at(
                SelfIsolationAction::End,
                empty_start + Duration::from_secs(300),
            )
            .unwrap();
        let empty_report = empty
            .end_at(empty_start + Duration::from_secs(300) + Duration::from_millis(1))
            .unwrap();
        assert_eq!(
            empty_report.summary.self_isolation.interaction_record_count,
            1
        );
        assert_eq!(
            empty_report.summary.self_isolation.total_interaction_count,
            0
        );
        assert!(!empty_report.summary.self_isolation.every_bucket_covered);
        assert!(!empty_report.summary.self_isolation.passed);

        let (mut single_window, single_start) = started(32);
        single_window
            .set_self_isolation_at(SelfIsolationAction::Start, single_start)
            .unwrap();
        for bucket in 0..SELF_INTERACTION_BUCKET_COUNT {
            single_window
                .record_self_interaction_at(
                    SelfInteractionWindow::Settings,
                    SelfInteractionKind::Pointer,
                    single_start
                        + Duration::from_secs(
                            u64::from(bucket) * SELF_INTERACTION_BUCKET_DURATION_SECS,
                        )
                        + Duration::from_millis(1),
                )
                .unwrap();
        }
        single_window
            .set_self_isolation_at(
                SelfIsolationAction::End,
                single_start + Duration::from_secs(300),
            )
            .unwrap();
        let single_report = single_window
            .end_at(single_start + Duration::from_secs(300) + Duration::from_millis(1))
            .unwrap();
        assert!(single_report.summary.self_isolation.every_bucket_covered);
        assert_eq!(
            single_report.summary.self_isolation.settings_buckets,
            (0..SELF_INTERACTION_BUCKET_COUNT).collect::<Vec<_>>()
        );
        assert!(single_report
            .summary
            .self_isolation
            .popup_buckets
            .is_empty());
        assert!(
            !single_report
                .summary
                .self_isolation
                .window_bucket_coverage_valid
        );
        assert!(!single_report.summary.self_isolation.passed);

        let (mut partial, partial_start) = started(32);
        partial
            .set_self_isolation_at(SelfIsolationAction::Start, partial_start)
            .unwrap();
        for bucket in 0..5_u8 {
            partial
                .record_self_interaction_at(
                    if bucket % 2 == 0 {
                        SelfInteractionWindow::Settings
                    } else {
                        SelfInteractionWindow::Popup
                    },
                    SelfInteractionKind::Keyboard,
                    partial_start
                        + Duration::from_secs(
                            u64::from(bucket) * SELF_INTERACTION_BUCKET_DURATION_SECS,
                        )
                        + Duration::from_millis(1),
                )
                .unwrap();
        }
        partial
            .set_self_isolation_at(
                SelfIsolationAction::End,
                partial_start + Duration::from_secs(300),
            )
            .unwrap();
        let partial_report = partial
            .end_at(partial_start + Duration::from_secs(300) + Duration::from_millis(1))
            .unwrap();
        assert_eq!(
            partial_report.summary.self_isolation.populated_buckets,
            vec![0, 1, 2, 3, 4]
        );
        assert!(!partial_report.summary.self_isolation.every_bucket_covered);
        assert!(!partial_report.summary.self_isolation.passed);
    }

    #[test]
    fn self_interaction_complete_pattern_is_aggregated_without_event_details() {
        let (mut runtime, start) = started(32);
        runtime
            .set_self_isolation_at(SelfIsolationAction::Start, start)
            .unwrap();
        record_valid_self_interaction_pattern(&mut runtime, start);
        assert_eq!(
            runtime.record_self_interaction_at(
                SelfInteractionWindow::Popup,
                SelfInteractionKind::Pointer,
                start + Duration::from_secs(300),
            ),
            Ok(HookDisposition::Ignored)
        );
        runtime
            .set_self_isolation_at(SelfIsolationAction::End, start + Duration::from_secs(300))
            .unwrap();
        let report = runtime
            .end_at(start + Duration::from_secs(300) + Duration::from_millis(1))
            .unwrap();
        let summary = &report.summary.self_isolation;
        assert_eq!(summary.total_interaction_count, 10);
        assert_eq!(summary.settings_interaction_count, 5);
        assert_eq!(summary.popup_interaction_count, 5);
        assert_eq!(summary.populated_buckets, (0..10).collect::<Vec<_>>());
        assert!(summary.interaction_generation_matched);
        assert!(summary.every_bucket_covered);
        assert!(summary.window_bucket_coverage_valid);
        assert!(summary.passed);

        let aggregate = report.records.iter().find_map(|record| match record {
            AcceptanceRecord::SelfInteraction(record) => Some(record),
            _ => None,
        });
        let aggregate_json = serde_json::to_value(aggregate.unwrap()).unwrap();
        let keys = aggregate_json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            keys,
            BTreeSet::from(["aggregates", "generation", "ordinal", "scenario"])
        );
        for cell in aggregate_json["aggregates"].as_array().unwrap() {
            let cell_keys = cell
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            assert_eq!(
                cell_keys,
                BTreeSet::from(["bucketCoverage", "count", "kind", "window"])
            );
        }
        let json = serde_json::to_string(&aggregate_json).unwrap();
        for forbidden in [
            "\"eventOffsetMicros\":",
            "\"key\":",
            "\"control\":",
            "\"target\":",
            "\"x\":",
            "\"y\":",
            "\"coordinate\":",
            "\"anchor\":",
            "\"isTrusted\":",
        ] {
            assert!(!json.contains(forbidden));
        }
    }

    #[test]
    fn self_interaction_outside_active_run_is_ignored_without_side_effects() {
        let mut runtime = AcceptanceRuntime::new(true, 32).unwrap();
        assert_eq!(
            runtime.record_self_interaction(
                SelfInteractionWindow::Settings,
                SelfInteractionKind::Pointer,
            ),
            Ok(HookDisposition::Ignored)
        );
        let start = Instant::now();
        runtime.start_full_baseline_at(metadata(), start).unwrap();
        assert_eq!(
            runtime.record_self_interaction_at(
                SelfInteractionWindow::Settings,
                SelfInteractionKind::Pointer,
                start + Duration::from_millis(1),
            ),
            Ok(HookDisposition::Ignored)
        );
        runtime
            .set_self_isolation_at(SelfIsolationAction::Start, start)
            .unwrap();
        runtime
            .record_self_interaction_at(
                SelfInteractionWindow::Popup,
                SelfInteractionKind::Keyboard,
                start + Duration::from_millis(1),
            )
            .unwrap();
        runtime
            .set_self_isolation_at(SelfIsolationAction::End, start + Duration::from_secs(300))
            .unwrap();
        assert_eq!(
            runtime.record_self_interaction_at(
                SelfInteractionWindow::Settings,
                SelfInteractionKind::Pointer,
                start + Duration::from_secs(300) + Duration::from_millis(1),
            ),
            Ok(HookDisposition::Ignored)
        );
        let report = runtime
            .end_at(start + Duration::from_secs(300) + Duration::from_millis(2))
            .unwrap();
        assert_eq!(report.summary.self_isolation.total_interaction_count, 1);
        assert_eq!(
            runtime.record_self_interaction(
                SelfInteractionWindow::Popup,
                SelfInteractionKind::Keyboard,
            ),
            Ok(HookDisposition::Ignored)
        );
    }

    #[test]
    fn even_nonterminal_self_window_read_invalidates_isolation() {
        let (mut runtime, start) = started(16);
        runtime
            .set_self_isolation_at(SelfIsolationAction::Start, start)
            .unwrap();
        runtime
            .observe_native_read_at(
                observation(55, 1, "com.paperfloat.translator", false, false),
                start + Duration::from_millis(10),
            )
            .unwrap();
        runtime
            .set_self_isolation_at(
                SelfIsolationAction::End,
                start + Duration::from_millis(300_000),
            )
            .unwrap();
        let report = runtime
            .end_at(start + Duration::from_millis(300_001))
            .unwrap();
        assert_eq!(report.summary.self_isolation.selection_trigger_count, 1);
        assert!(!report.summary.self_isolation.passed);
    }

    #[test]
    fn self_isolation_allows_only_one_contiguous_run_per_session() {
        let (mut runtime, start) = started(16);
        runtime
            .set_self_isolation_at(SelfIsolationAction::Start, start)
            .unwrap();
        runtime
            .set_self_isolation_at(
                SelfIsolationAction::End,
                start + Duration::from_millis(300_000),
            )
            .unwrap();
        assert!(matches!(
            runtime.set_self_isolation_at(
                SelfIsolationAction::Start,
                start + Duration::from_millis(300_001),
            ),
            Err(AcceptanceRuntimeError::DuplicateArm)
        ));
        let report = runtime
            .end_at(start + Duration::from_millis(300_002))
            .unwrap();
        assert_eq!(report.summary.self_isolation.start_count, 1);
        assert_eq!(report.summary.self_isolation.end_count, 1);
        assert_eq!(report.rejected_records, 1);
        assert!(!report.status.integrity_valid);
    }

    #[test]
    fn atomic_export_is_content_free_and_clear_resets_runtime() {
        let (mut runtime, start) = started(32);
        runtime.arm_at(Scenario::TextEditDrag, 1, start).unwrap();
        runtime
            .observe_native_read_at(
                observation(60, 1, TEXTEDIT_BUNDLE_ID, true, true),
                start + Duration::from_millis(10),
            )
            .unwrap();
        let secret = "TOP_SECRET_SELECTED_SENTINEL";
        commit_at(
            &mut runtime,
            60,
            secret,
            60,
            start + Duration::from_millis(20),
        )
        .unwrap();
        runtime.end_at(start + Duration::from_millis(30)).unwrap();

        let directory = std::env::temp_dir().join(format!(
            "paper-float-acceptance-{}-{}",
            std::process::id(),
            EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let path = export_path(&directory);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"partial old report").unwrap();
        let receipt = runtime.export_atomic(&path).unwrap();
        assert!(!receipt.valid);
        let content = fs::read_to_string(&path).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&content).is_ok());
        assert!(!content.contains(secret));
        assert!(!content.contains("selectedText"));
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);

        runtime.clear();
        assert_eq!(runtime.status().phase, SessionPhase::Idle);
        assert!(matches!(
            runtime.export_atomic(&path),
            Err(AcceptanceRuntimeError::Diagnostics(
                DiagnosticsError::ReportNotReady
            ))
        ));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn atomic_export_preserves_a_preexisting_temporary_file() {
        let directory = std::env::temp_dir().join(format!(
            "paper-float-acceptance-preexisting-temp-{}-{}",
            std::process::id(),
            EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let path = export_path(&directory);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let temporary_path = path.parent().unwrap().join(".report.tmp-controlled");
        fs::write(&temporary_path, b"preexisting-temporary-sentinel").unwrap();

        atomic_write_with_temporary_path(&path, &temporary_path, br#"{"valid":true}"#)
            .expect_err("create_new must reject a pre-existing temporary file");

        assert_eq!(
            fs::read(&temporary_path).unwrap(),
            b"preexisting-temporary-sentinel"
        );
        assert!(!path.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn atomic_export_preserves_a_preexisting_temporary_symlink_and_outside_sentinel() {
        use std::os::unix::fs::symlink;

        let directory = std::env::temp_dir().join(format!(
            "paper-float-acceptance-preexisting-temp-link-{}-{}",
            std::process::id(),
            EXPORT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let path = export_path(&directory);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let temporary_path = path.parent().unwrap().join(".report.tmp-controlled");
        let outside = directory.join("outside-sentinel.json");
        fs::write(&outside, b"outside-sentinel").unwrap();
        symlink(&outside, &temporary_path).unwrap();

        atomic_write_with_temporary_path(&path, &temporary_path, br#"{"valid":true}"#)
            .expect_err("create_new must reject a pre-existing temporary symlink");

        assert_eq!(fs::read(&outside).unwrap(), b"outside-sentinel");
        assert!(fs::symlink_metadata(&temporary_path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!path.exists());
        fs::remove_file(&temporary_path).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn privacy_guard_rejects_content_bearing_json_keys() {
        for unsafe_json in [
            br#"{"selectedText":"secret"}"#.as_slice(),
            br#"{"key":"Enter"}"#.as_slice(),
            br#"{"control":"translate"}"#.as_slice(),
            br#"{"target":"button"}"#.as_slice(),
            br#"{"isTrusted":true}"#.as_slice(),
            br#"{"coordinate":12}"#.as_slice(),
            br#"{"anchor":34}"#.as_slice(),
        ] {
            assert!(ensure_privacy_safe_json(unsafe_json).is_err());
        }
        assert!(ensure_privacy_safe_json(br#"{"records":[],"sha256":"fixed-fixture"}"#).is_ok());
    }
}
