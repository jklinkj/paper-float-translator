// The macOS watcher and acceptance implementation intentionally remains compiled on Windows so
// its deterministic contract tests still run there. Platform-only entry points and imports are
// therefore expected to be unused in a normal Windows library build.
#![cfg_attr(target_os = "windows", allow(dead_code, unused_imports))]

mod acceptance_diagnostics;
mod acceptance_runtime;
mod native_work_area;
mod platform;
mod popup_controller;
mod popup_geometry;
mod text_cleaning;
mod watcher_readiness;

#[cfg(all(feature = "acceptance-testing", feature = "local-api-key-file"))]
compile_error!("acceptance-testing and local-api-key-file are mutually exclusive artifact classes");

#[cfg(feature = "acceptance-testing")]
use acceptance_diagnostics::SessionPhase as AcceptanceSessionPhase;
use acceptance_diagnostics::{
    AcceptanceReport, AppMetadata as AcceptanceAppMetadata, Architecture as AcceptanceArchitecture,
    BaselineApplication, BaselineApplicationMetadata, BuildKind as AcceptanceBuildKind,
    LifecycleEvent as AcceptanceLifecycleEvent, OperatingSystem as AcceptanceOperatingSystem,
    Scenario as AcceptanceScenario, SelectionOutcome as AcceptanceSelectionOutcome,
    SelfInteractionKind as AcceptanceSelfInteractionKind,
    SelfInteractionWindow as AcceptanceSelfInteractionWindow,
    SystemMetadata as AcceptanceSystemMetadata, WatcherResourceCounts,
};
use acceptance_runtime::{
    AcceptanceExportReceipt, AcceptanceRuntime, AcceptanceRuntimeMetadata, AcceptanceRuntimeStatus,
    HookDisposition, MouseUpAnchor, NativeReadObservation, SelfIsolationAction,
    TranslationCommitObservation, DEFAULT_ACCEPTANCE_CAPACITY, RAPID_PROBE_DELAY_MS,
};
use arboard::Clipboard;
use atomic_write_file::AtomicWriteFile;
#[cfg(test)]
use popup_controller::POPUP_PROTOCOL_VERSION;
use popup_controller::{
    BeginTranslation, PopupControlError, PopupController, PopupErrorKind, PopupRecoveryAction,
    PopupState, PopupStatus, TranslationCommit, TranslationGuard,
};
use popup_geometry::{
    logical_work_area_to_physical, place_popup, popup_contains_point, LogicalPoint,
    MonitorGeometry, PhysicalPoint, PhysicalRect, PhysicalSize as GeometryPhysicalSize,
    PopupHitTest, PopupPlacement,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, RunEvent, State, WebviewWindow,
    WindowEvent,
};
use text_cleaning::clean_selected_text;
use watcher_readiness::{
    MissingWatcherSources, WatcherReadiness, WatcherSource, WatcherWaitOutcome,
};

const POPUP_DEFAULT_HEIGHT: f64 = 260.0;
const SELECTION_POPUP_WIDTH: f64 = 440.0;
const SELECTION_POPUP_HEIGHT: f64 = 66.0;
const POPUP_OFFSET: f64 = 18.0;
const POPUP_HIT_TEST_PADDING: f64 = 12.0;
const DOUBLE_COPY_COOLDOWN_MS: u128 = 1000;
const DEFAULT_TARGET_LANGUAGE: &str = "中文";
#[cfg(all(
    not(feature = "acceptance-testing"),
    not(feature = "local-api-key-file")
))]
const KEYCHAIN_SERVICE: &str = "Paper Float Translator API Key v2";
#[cfg(feature = "acceptance-testing")]
const KEYCHAIN_SERVICE: &str = "Paper Float Translator Acceptance API Key v2";
#[cfg(all(
    not(feature = "acceptance-testing"),
    not(feature = "local-api-key-file")
))]
const DEEPSEEK_ACCOUNT: &str = "deepseek-api-key-v2";
#[cfg(feature = "acceptance-testing")]
const DEEPSEEK_ACCOUNT: &str = "deepseek-api-key-acceptance-v2";
#[cfg(not(feature = "acceptance-testing"))]
const EXPECTED_PRODUCT_NAME: &str = "Paper Float Translator";
#[cfg(not(feature = "acceptance-testing"))]
const EXPECTED_BUNDLE_IDENTIFIER: &str = "com.paperfloat.translator";
#[cfg(feature = "acceptance-testing")]
const EXPECTED_PRODUCT_NAME: &str = "Paper Float Translator Acceptance";
#[cfg(feature = "acceptance-testing")]
const EXPECTED_BUNDLE_IDENTIFIER: &str = "com.paperfloat.translator.acceptance";
#[cfg(all(not(feature = "local-api-key-file"), any(target_os = "macos", test)))]
const KEYCHAIN_ITEM_NOT_FOUND: i32 = 1;
const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
const DEEPSEEK_CONNECT_TIMEOUT_SECS: u64 = 10;
const DEEPSEEK_REQUEST_TIMEOUT_SECS: u64 = 45;
const STREAM_UPDATE_THROTTLE_MS: u64 = 100;
const CACHE_MAX_ENTRIES: usize = 500;
const CACHE_MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;
const CACHE_FUTURE_SKEW_SECS: u64 = 5 * 60;
#[cfg(target_os = "windows")]
const WINDOWS_UIA_READ_TIMEOUT_MS: u64 = 450;
#[cfg(target_os = "windows")]
static WINDOWS_SELECTION_GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "local-api-key-file")]
const LOCAL_API_KEY_FILE_NAME: &str = "api-key.local";
#[cfg(feature = "local-api-key-file")]
const LOCAL_API_KEY_MAX_BYTES: u64 = 16 * 1024;
#[cfg(feature = "local-api-key-file")]
const LOCAL_API_KEY_ARTIFACT_MARKER: &str = "PAPER_FLOAT_ARTIFACT_CLASS_LOCAL_API_KEY_FILE_V1";

static CACHE_IO_LOCK: Mutex<()> = Mutex::new(());
static ATOMIC_FILE_WRITE_LOCK: Mutex<()> = Mutex::new(());
static ATOMIC_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum DeepSeekModel {
    #[serde(rename = "deepseek-v4-flash")]
    #[default]
    Flash,
    #[serde(rename = "deepseek-v4-pro")]
    Pro,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TranslateMode {
    #[default]
    #[serde(rename = "academic_zh", alias = "literal", alias = "natural")]
    AcademicZh,
    Bilingual,
    Terminology,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    model: DeepSeekModel,
    mode: TranslateMode,
    clean_pdf_text: bool,
    enable_cache: bool,
    enable_selection_popup: bool,
    enable_automatic_selection: bool,
    target_language: String,
    popup_width: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            model: DeepSeekModel::default(),
            mode: TranslateMode::default(),
            clean_pdf_text: true,
            enable_cache: true,
            enable_selection_popup: true,
            enable_automatic_selection: cfg!(target_os = "macos"),
            target_language: DEFAULT_TARGET_LANGUAGE.to_string(),
            popup_width: 420,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatcherStatus {
    available: bool,
    running: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

impl WatcherStatus {
    fn new(available: bool, running: bool, message: impl Into<String>) -> Self {
        Self {
            available,
            running,
            message: message.into(),
            code: None,
        }
    }

    fn with_code(
        available: bool,
        running: bool,
        message: impl Into<String>,
        code: impl Into<String>,
    ) -> Self {
        Self {
            available,
            running,
            message: message.into(),
            code: Some(code.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PermissionGrant {
    Granted,
    Denied,
    Unknown,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CapabilityHealth {
    Ready,
    Degraded,
    Disabled,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PermissionCapabilityState {
    grant: PermissionGrant,
    health: CapabilityHealth,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct RuntimeCapabilityState {
    health: CapabilityHealth,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_code: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CapabilitySnapshot {
    accessibility: PermissionCapabilityState,
    listen_event: PermissionCapabilityState,
    mouse_tap: RuntimeCapabilityState,
    key_tap: RuntimeCapabilityState,
    ax_selected_text_observer: RuntimeCapabilityState,
    direct_selection_read: RuntimeCapabilityState,
    clipboard_double_copy_fallback: RuntimeCapabilityState,
}

impl CapabilitySnapshot {
    fn unknown() -> Self {
        Self {
            accessibility: PermissionCapabilityState::unknown(),
            listen_event: PermissionCapabilityState::unknown(),
            mouse_tap: RuntimeCapabilityState::unknown(),
            key_tap: RuntimeCapabilityState::unknown(),
            ax_selected_text_observer: RuntimeCapabilityState::unknown(),
            direct_selection_read: RuntimeCapabilityState::unknown(),
            clipboard_double_copy_fallback: RuntimeCapabilityState::unknown(),
        }
    }

    fn unsupported() -> Self {
        let unsupported_permission = PermissionCapabilityState {
            grant: PermissionGrant::Unsupported,
            health: CapabilityHealth::Unavailable,
            status_code: Some("unsupported_os".to_string()),
        };
        let unsupported_capability =
            RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, "unsupported_os");
        Self {
            accessibility: unsupported_permission.clone(),
            listen_event: unsupported_permission,
            mouse_tap: unsupported_capability.clone(),
            key_tap: unsupported_capability.clone(),
            ax_selected_text_observer: unsupported_capability.clone(),
            direct_selection_read: unsupported_capability.clone(),
            clipboard_double_copy_fallback: unsupported_capability,
        }
    }

    fn apply_permission_grants(
        &mut self,
        accessibility: PermissionGrant,
        listen_event: PermissionGrant,
    ) {
        self.accessibility =
            PermissionCapabilityState::from_grant(accessibility, "accessibility_preflight");
        self.listen_event =
            PermissionCapabilityState::from_grant(listen_event, "listen_event_preflight");

        if accessibility == PermissionGrant::Denied {
            self.ax_selected_text_observer = RuntimeCapabilityState::with_status(
                CapabilityHealth::Unavailable,
                "accessibility_preflight_denied",
            );
            self.direct_selection_read = RuntimeCapabilityState::with_status(
                CapabilityHealth::Unavailable,
                "accessibility_preflight_denied",
            );
        }
        if listen_event == PermissionGrant::Denied {
            self.mouse_tap = RuntimeCapabilityState::with_status(
                CapabilityHealth::Unavailable,
                "listen_event_preflight_denied",
            );
        }
    }
}

impl PermissionCapabilityState {
    fn unknown() -> Self {
        Self {
            grant: PermissionGrant::Unknown,
            health: CapabilityHealth::Unknown,
            status_code: None,
        }
    }

    fn from_grant(grant: PermissionGrant, prefix: &str) -> Self {
        let (health, suffix) = match grant {
            PermissionGrant::Granted => (CapabilityHealth::Ready, "granted"),
            PermissionGrant::Denied => (CapabilityHealth::Unavailable, "denied"),
            PermissionGrant::Unknown => (CapabilityHealth::Unknown, "unknown"),
            PermissionGrant::Unsupported => (CapabilityHealth::Unavailable, "unsupported"),
        };
        Self {
            grant,
            health,
            status_code: Some(format!("{prefix}_{suffix}")),
        }
    }
}

impl RuntimeCapabilityState {
    fn unknown() -> Self {
        Self {
            health: CapabilityHealth::Unknown,
            status_code: None,
        }
    }

    fn with_status(health: CapabilityHealth, status_code: impl Into<String>) -> Self {
        Self {
            health,
            status_code: Some(status_code.into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SignatureKind {
    Adhoc,
    AppleDevelopment,
    DeveloperId,
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionDiagnostics {
    bundle_identifier: String,
    app_version: String,
    bundle_path: String,
    executable_path: String,
    team_identifier: Option<String>,
    cd_hash: Option<String>,
    signature_kind: SignatureKind,
    accessibility_trusted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    selection_read: Option<SelectionReadDiagnostics>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct SelectionReadDiagnostics {
    status: String,
    reason: String,
    found_text: bool,
    candidate_count: u32,
    duration_ms: f64,
    source_bundle_id: String,
    ax_error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attempt: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trigger_to_read_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPayload {
    settings: AppSettings,
    has_api_key: bool,
    api_key_status: ApiKeyStatus,
    api_key_storage: ApiKeyStorage,
    runtime_platform: RuntimePlatform,
    double_copy_status: WatcherStatus,
    selection_status: WatcherStatus,
    capability_snapshot: CapabilitySnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    permission_diagnostics: Option<PermissionDiagnostics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runtime_warning: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ApiKeyStatus {
    Configured,
    Missing,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ApiKeyStorage {
    #[cfg(feature = "local-api-key-file")]
    LocalFile,
    #[cfg(all(not(feature = "local-api-key-file"), not(target_os = "windows")))]
    SystemKeychain,
    #[cfg(all(not(feature = "local-api-key-file"), target_os = "windows"))]
    WindowsCredentialManager,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RuntimePlatform {
    #[cfg(target_os = "macos")]
    Macos,
    #[cfg(target_os = "windows")]
    Windows,
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Other,
}

impl ApiKeyStatus {
    fn has_api_key(self) -> bool {
        matches!(self, Self::Configured)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone)]
struct TriggerRecord {
    text: String,
    triggered_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectionTriggerKind {
    Automatic,
    Deliberate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelectionIdentity {
    source_pid: i32,
    text_fingerprint: [u8; 32],
}

impl SelectionIdentity {
    fn new(source_pid: i32, cleaned_text: &str) -> Self {
        Self {
            source_pid,
            text_fingerprint: Sha256::digest(cleaned_text.as_bytes()).into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ActivePopupSelection {
    identity: SelectionIdentity,
    selection_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PopupFocusReturnTarget {
    selection_revision: u64,
    pid: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AcceptanceRapidProbe {
    epoch: u64,
    guard: TranslationGuard,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PopupFocusDirection {
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct PopupKeyboardEntry {
    revision: u64,
    selection_revision: u64,
    direction: PopupFocusDirection,
}

struct InnerState {
    settings: AppSettings,
    popup_controller: PopupController,
    last_cleaned_text: String,
    last_cursor_point: Option<Point>,
    double_copy_status: WatcherStatus,
    selection_status: WatcherStatus,
    capability_snapshot: CapabilitySnapshot,
    last_double_copy_trigger: Option<TriggerRecord>,
    active_selection_text: Option<String>,
    active_popup_selection: Option<ActivePopupSelection>,
    dismissed_automatic_selection: Option<SelectionIdentity>,
    last_selection_read: Option<SelectionReadDiagnostics>,
    latched_selection_baseline_error: Option<String>,
    popup_positioned: bool,
    popup_dragging: bool,
    popup_focus_return_target: Option<PopupFocusReturnTarget>,
    acceptance_rapid_probe: Option<AcceptanceRapidProbe>,
    last_acceptance_probe_commit: Option<(u64, u64)>,
}

struct AppState {
    data_dir: PathBuf,
    inner: Arc<Mutex<InnerState>>,
    startup_gate: Arc<StartupGate>,
    watcher_readiness: Arc<WatcherReadiness>,
    quitting: Arc<AtomicBool>,
    watcher_lifecycle: Arc<Mutex<()>>,
    acceptance: Arc<Mutex<AcceptanceRuntime>>,
    acceptance_probe_epoch: AtomicU64,
    #[cfg(feature = "acceptance-testing")]
    acceptance_injection_epoch: Arc<AtomicU64>,
    watcher_transition: tauri::async_runtime::Mutex<()>,
    http: reqwest::Client,
}

#[derive(Default)]
struct StartupGate {
    ready: Mutex<bool>,
    changed: Condvar,
}

impl StartupGate {
    fn wait(&self, timeout: Duration) -> Result<(), String> {
        let ready = self.ready.lock().map_err(lock_error)?;
        let (ready, result) = self
            .changed
            .wait_timeout_while(ready, timeout, |ready| !*ready)
            .map_err(lock_error)?;
        if *ready {
            Ok(())
        } else if result.timed_out() {
            Err("应用初始化超时；请重新读取设置。".to_owned())
        } else {
            Err("应用初始化尚未完成；请重新读取设置。".to_owned())
        }
    }

    fn mark_ready(&self) -> Result<(), String> {
        let mut ready = self.ready.lock().map_err(lock_error)?;
        *ready = true;
        self.changed.notify_all();
        Ok(())
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.lock() {
            log_popup_control_error(inner.popup_controller.cancel_translation());
            clear_active_selection(&mut inner);
        }
        shutdown_platform_watchers(&self.quitting, &self.watcher_lifecycle);
    }
}

struct TranslateContext {
    inner: Arc<Mutex<InnerState>>,
    data_dir: PathBuf,
    http: reqwest::Client,
}

struct TranslateTask {
    cleaned_text: String,
    bypass_cache: bool,
    mode: TranslateMode,
    target_language: String,
    selection_revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PopupCloseReason {
    OutsideClick,
    ExplicitClose,
    SelectionHandled,
    ConfigurationChanged,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheEntry {
    key: String,
    cleaned_text: String,
    translation: String,
    model: DeepSeekModel,
    mode: TranslateMode,
    target_language: String,
    glossary_version: String,
    created_at: String,
}

#[derive(Default, Serialize, Deserialize)]
struct CacheFile {
    entries: HashMap<String, CacheEntry>,
}

type Glossary = HashMap<String, String>;

pub fn run() {
    let context = tauri::generate_context!();
    if let Err(error) = validate_compiled_runtime_identity(
        &context.config().identifier,
        &context.package_info().name,
    ) {
        eprintln!("{error}");
        return;
    }

    let quitting = Arc::new(AtomicBool::new(false));
    let watcher_lifecycle = Arc::new(Mutex::new(()));
    let setup_quitting = quitting.clone();
    let setup_watcher_lifecycle = watcher_lifecycle.clone();

    #[cfg(feature = "acceptance-testing")]
    let data_dir = match expected_acceptance_data_dir() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{error}");
            return;
        }
    };
    #[cfg(feature = "acceptance-testing")]
    if let Err(error) = prepare_acceptance_data_dir(&data_dir) {
        eprintln!("{error}");
        return;
    }
    #[cfg(not(feature = "acceptance-testing"))]
    let data_dir = dirs::data_dir()
        .map(|path| path.join(EXPECTED_BUNDLE_IDENTIFIER))
        .unwrap_or_else(fallback_data_dir);

    let settings = match ensure_data_dir(&data_dir).and_then(|()| load_settings(&data_dir)) {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            AppSettings {
                enable_selection_popup: false,
                ..AppSettings::default()
            }
        }
    };
    let inner = Arc::new(Mutex::new(InnerState {
        settings,
        popup_controller: PopupController::default(),
        last_cleaned_text: String::new(),
        last_cursor_point: None,
        double_copy_status: initial_double_copy_status(),
        selection_status: initial_selection_status(),
        capability_snapshot: initial_capability_snapshot(),
        last_double_copy_trigger: None,
        active_selection_text: None,
        active_popup_selection: None,
        dismissed_automatic_selection: None,
        last_selection_read: None,
        latched_selection_baseline_error: None,
        popup_positioned: false,
        popup_dragging: false,
        popup_focus_return_target: None,
        acceptance_rapid_probe: None,
        last_acceptance_probe_commit: None,
    }));
    let watcher_readiness = Arc::new(WatcherReadiness::default());
    let startup_gate = Arc::new(StartupGate::default());
    let acceptance = Arc::new(Mutex::new(
        AcceptanceRuntime::from_environment(DEFAULT_ACCEPTANCE_CAPACITY)
            .expect("failed to initialize bounded acceptance diagnostics"),
    ));
    let setup_inner = inner.clone();
    let setup_watcher_readiness = watcher_readiness.clone();
    let setup_startup_gate = startup_gate.clone();
    let builder = platform::configure_builder(
        tauri::Builder::default().manage(AppState {
            data_dir,
            inner,
            startup_gate,
            watcher_readiness,
            quitting: setup_quitting.clone(),
            watcher_lifecycle: setup_watcher_lifecycle.clone(),
            acceptance,
            acceptance_probe_epoch: AtomicU64::new(1),
            #[cfg(feature = "acceptance-testing")]
            acceptance_injection_epoch: Arc::new(AtomicU64::new(1)),
            watcher_transition: tauri::async_runtime::Mutex::new(()),
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(DEEPSEEK_CONNECT_TIMEOUT_SECS))
                .timeout(Duration::from_secs(DEEPSEEK_REQUEST_TIMEOUT_SECS))
                .build()
                .expect("failed to build DeepSeek HTTP client"),
        }),
    );
    let app = builder
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            save_api_key,
            clear_cache,
            open_settings,
            open_accessibility_settings,
            open_input_monitoring_settings,
            refresh_watcher_status,
            open_external,
            copy_text,
            set_popup_dragging,
            copy_translation,
            copy_source,
            close_popup,
            get_popup_snapshot,
            toggle_pin,
            translate_selection,
            retry_translation,
            explain_terms,
            resize_popup,
            acceptance_diagnostics_status,
            start_acceptance_diagnostics,
            arm_acceptance_scenario,
            set_acceptance_self_isolation,
            record_acceptance_self_interaction,
            end_acceptance_diagnostics,
            export_acceptance_diagnostics,
            clear_acceptance_diagnostics,
            #[cfg(feature = "acceptance-testing")]
            inject_acceptance_tap_disabled
        ])
        .setup(move |app| {
            if let Err(error) = validate_compiled_runtime_identity(
                &app.config().identifier,
                &app.package_info().name,
            ) {
                return Err(std::io::Error::other(error).into());
            }

            let managed_data_dir = app.state::<AppState>().data_dir.clone();
            match app.path().app_data_dir() {
                Ok(resolved_data_dir) if resolved_data_dir != managed_data_dir => {
                    return Err(std::io::Error::other(format!(
                        "应用数据目录不一致：预注册 {}，运行时解析 {}。",
                        managed_data_dir.display(),
                        resolved_data_dir.display()
                    ))
                    .into());
                }
                Ok(_) => {}
                Err(error) => {
                    #[cfg(feature = "acceptance-testing")]
                    return Err(std::io::Error::other(format!(
                        "验收数据目录解析失败，已阻止访问正式用户状态：{error}"
                    ))
                    .into());
                    #[cfg(not(feature = "acceptance-testing"))]
                    eprintln!(
                        "应用数据目录运行时解析失败；继续使用预注册目录 {}：{error}",
                        managed_data_dir.display()
                    );
                }
            }

            platform::setup(app, setup_quitting.clone())?;

            if let Some(popup) = app.get_webview_window("popup") {
                let _ = popup.hide();
            }
            if let Some(settings_window) = app.get_webview_window("settings") {
                let close_window = settings_window.clone();
                let close_quitting = setup_quitting.clone();
                settings_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        if should_hide_settings_window_on_close(
                            close_quitting.load(Ordering::Acquire),
                        ) {
                            api.prevent_close();
                            let _ = close_window.hide();
                        }
                    }
                });
            }

            if let Err(error) = restart_platform_watchers(
                app.handle().clone(),
                setup_inner,
                setup_watcher_readiness,
                &setup_quitting,
                &setup_watcher_lifecycle,
            ) {
                eprintln!("{error}");
            }
            setup_startup_gate
                .mark_ready()
                .map_err(std::io::Error::other)?;
            Ok(())
        })
        .build(context)
        .expect("failed to build Paper Float Translator");
    app.run(move |app_handle, event| match event {
        RunEvent::ExitRequested { code, api, .. } => {
            if should_keep_background_agent_running(code, quitting.load(Ordering::Acquire)) {
                api.prevent_exit();
                return;
            }
            platform::shutdown(app_handle);
            shutdown_platform_watchers(&quitting, &watcher_lifecycle);
        }
        #[cfg(target_os = "macos")]
        RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } => {
            show_settings_window(app_handle);
        }
        _ => {}
    });
}

fn should_hide_settings_window_on_close(is_quitting: bool) -> bool {
    !is_quitting
}

fn should_keep_background_agent_running(exit_code: Option<i32>, is_quitting: bool) -> bool {
    #[cfg(target_os = "windows")]
    {
        exit_code.is_none() && !is_quitting
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (exit_code, is_quitting);
        false
    }
}

#[cfg(target_os = "macos")]
fn show_settings_window(app: &AppHandle) {
    platform::show_settings_window(app);
}

#[cfg(target_os = "windows")]
pub(crate) fn handle_windows_selection_shortcut(app: &AppHandle) {
    let app_state = app.state::<AppState>();
    let enabled = app_state
        .inner
        .lock()
        .ok()
        .is_some_and(|inner| inner.settings.enable_selection_popup);
    if !enabled || app_state.quitting.load(Ordering::Acquire) {
        return;
    }
    if let Ok(mut inner) = app_state.inner.lock() {
        inner.dismissed_automatic_selection = None;
    }

    let Some(generation) = next_windows_selection_generation() else {
        update_windows_selection_error(app, platform::WindowsSelectionError::WorkerStopped);
        return;
    };

    let anchor = windows_cursor_logical_anchor(app);
    let reader = app
        .state::<platform::WindowsSelectionReader>()
        .inner()
        .clone();
    let worker_app = app.clone();
    let spawn_result = std::thread::Builder::new()
        .name("paper-float-windows-selection-trigger".to_owned())
        .spawn(move || {
            match reader.read(
                generation,
                Duration::from_millis(WINDOWS_UIA_READ_TIMEOUT_MS),
            ) {
                Ok(selection) => handle_windows_selection_result(
                    &worker_app,
                    selection,
                    anchor,
                    WindowsSelectionTrigger::Shortcut,
                ),
                Err(error) => update_windows_selection_error(&worker_app, error),
            }
        });
    if spawn_result.is_err() {
        update_windows_selection_error(app, platform::WindowsSelectionError::WorkerStopped);
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
enum WindowsSelectionTrigger {
    Shortcut,
    Automatic,
}

#[cfg(target_os = "windows")]
pub(crate) fn handle_windows_automatic_selection_trigger(app: &AppHandle, source_pid: i32) {
    if windows_cursor_logical_anchor(app).is_some_and(|point| is_point_inside_popup(app, point)) {
        return;
    }
    let _ = platform::request_windows_automatic_selection(app, source_pid);
}

#[cfg(target_os = "windows")]
pub(crate) fn handle_windows_global_mouse_down(app: &AppHandle) {
    platform::cancel_windows_automatic_selection(app);
    hide_popup_if_unpinned(
        app,
        &app.state::<AppState>().inner,
        windows_cursor_logical_anchor(app),
    );
}

#[cfg(target_os = "windows")]
pub(crate) fn handle_windows_global_keyboard_input(app: &AppHandle) {
    platform::cancel_windows_automatic_selection(app);
}

#[cfg(target_os = "windows")]
pub(crate) fn handle_windows_automatic_selection_cleared(app: &AppHandle, focused_source_pid: i32) {
    if !should_clear_automatic_dismissal_for_focus(focused_source_pid, std::process::id() as i32) {
        return;
    }
    let state = app.state::<AppState>();
    if let Ok(mut inner) = state.inner.lock() {
        inner.dismissed_automatic_selection = None;
    };
}

fn should_clear_automatic_dismissal_for_focus(
    focused_source_pid: i32,
    translator_pid: i32,
) -> bool {
    focused_source_pid > 0 && focused_source_pid != translator_pid
}

#[cfg(target_os = "windows")]
pub(crate) fn handle_windows_automatic_selection(text: String, source_pid: i32, app: &AppHandle) {
    let state = app.state::<AppState>();
    let enabled = state.inner.lock().ok().is_some_and(|inner| {
        inner.settings.enable_selection_popup && inner.settings.enable_automatic_selection
    });
    if !enabled || state.quitting.load(Ordering::Acquire) {
        return;
    }
    let Some(generation) = next_windows_selection_generation() else {
        handle_windows_automatic_selection_error(
            app,
            platform::WindowsSelectionError::WorkerStopped,
        );
        return;
    };
    let selection = platform::WindowsSelection {
        generation,
        text,
        source_pid,
    };
    handle_windows_selection_result(
        app,
        selection,
        windows_cursor_logical_anchor(app),
        WindowsSelectionTrigger::Automatic,
    );
}

#[cfg(target_os = "windows")]
pub(crate) fn handle_windows_automatic_selection_error(
    app: &AppHandle,
    error: platform::WindowsSelectionError,
) {
    let state = app.state::<AppState>();
    if let Ok(mut inner) = state.inner.lock() {
        if !inner.settings.enable_selection_popup || !inner.settings.enable_automatic_selection {
            return;
        }
        inner.selection_status = WatcherStatus::with_code(
            true,
            true,
            format!(
                "自动划词本次未能读取选区（{}）；Ctrl+Alt+T 与 Ctrl+C+C 仍可使用。",
                error.code()
            ),
            error.code(),
        );
        inner.capability_snapshot.mouse_tap =
            RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, error.code());
        inner.capability_snapshot.ax_selected_text_observer =
            RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, error.code());
    };
}

#[cfg(target_os = "windows")]
fn next_windows_selection_generation() -> Option<i64> {
    WINDOWS_SELECTION_GENERATION
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < i64::MAX as u64).then_some(current + 1)
        })
        .ok()
        .and_then(|previous| i64::try_from(previous + 1).ok())
}

#[cfg(target_os = "windows")]
pub(crate) fn handle_windows_double_copy_selection(
    app: &AppHandle,
    selection: platform::WindowsDoubleCopySelection,
) {
    let state = app.state::<AppState>();
    let enabled = state
        .inner
        .lock()
        .ok()
        .is_some_and(|inner| inner.settings.enable_selection_popup);
    if !enabled || state.quitting.load(Ordering::Acquire) {
        return;
    }
    if selection.source_pid > 0 && selection.source_pid == std::process::id() as i32 {
        return;
    }

    let anchor = selection
        .physical_anchor
        .and_then(|(x, y)| windows_physical_anchor_to_logical(app, f64::from(x), f64::from(y)))
        .or_else(|| windows_default_logical_anchor(app));
    let Some(anchor) = anchor else {
        if let Ok(mut inner) = state.inner.lock() {
            inner.double_copy_status = WatcherStatus::with_code(
                true,
                true,
                "已确认双复制，但无法确定浮窗所在屏幕，请重试。",
                "windows_double_copy_anchor_unavailable",
            );
        }
        return;
    };

    if let Ok(mut inner) = state.inner.lock() {
        inner.dismissed_automatic_selection = None;
        inner.double_copy_status = WatcherStatus::with_code(
            true,
            true,
            "Ctrl+C+C 双复制取词已就绪。",
            "windows_double_copy_ready",
        );
        inner.capability_snapshot.key_tap =
            RuntimeCapabilityState::with_status(CapabilityHealth::Ready, "windows_raw_input_ready");
        inner.capability_snapshot.clipboard_double_copy_fallback =
            RuntimeCapabilityState::with_status(
                CapabilityHealth::Ready,
                "windows_double_copy_ready",
            );
    }
    handle_confirmed_double_copy(app.clone(), state.inner.clone(), selection.text, anchor);
}

#[cfg(target_os = "windows")]
fn handle_windows_selection_result(
    app: &AppHandle,
    selection: platform::WindowsSelection,
    anchor: Option<Point>,
    trigger: WindowsSelectionTrigger,
) {
    if selection.source_pid > 0 && selection.source_pid == std::process::id() as i32 {
        update_windows_selection_runtime_status(
            app,
            true,
            true,
            "已忽略翻译器自身窗口中的选区。",
            "windows_uia_self_ignored",
            CapabilityHealth::Ready,
        );
        return;
    }

    let Some(anchor) = anchor.or_else(|| windows_default_logical_anchor(app)) else {
        update_windows_selection_runtime_status(
            app,
            true,
            true,
            "已读取选中文本，但无法确定浮窗所在屏幕，请重试。",
            "windows_anchor_unavailable",
            CapabilityHealth::Degraded,
        );
        return;
    };

    let state = app.state::<AppState>();
    if let Ok(mut inner) = state.inner.lock() {
        inner.selection_status = match trigger {
            WindowsSelectionTrigger::Shortcut => WatcherStatus::with_code(
                true,
                true,
                format!(
                    "Windows UI Automation 取词正常；快捷键为 {}。",
                    platform::WINDOWS_SELECTION_SHORTCUT_LABEL
                ),
                "windows_uia_read_ready",
            ),
            WindowsSelectionTrigger::Automatic => WatcherStatus::with_code(
                true,
                true,
                "Windows 自动划词已读取当前选区；不会改写剪贴板。",
                "windows_auto_selection_read_ready",
            ),
        };
        inner.capability_snapshot.direct_selection_read =
            RuntimeCapabilityState::with_status(CapabilityHealth::Ready, "windows_uia_read_ready");
    }
    show_selection_popup(
        app,
        &state.inner,
        selection.text,
        anchor,
        selection.generation,
        selection.source_pid,
        match trigger {
            WindowsSelectionTrigger::Shortcut => SelectionTriggerKind::Deliberate,
            WindowsSelectionTrigger::Automatic => SelectionTriggerKind::Automatic,
        },
    );
}

#[cfg(target_os = "windows")]
fn update_windows_selection_error(app: &AppHandle, error: platform::WindowsSelectionError) {
    let (available, running, health) = match error {
        platform::WindowsSelectionError::UiAutomationUnavailable
        | platform::WindowsSelectionError::WorkerStopped => {
            (false, false, CapabilityHealth::Unavailable)
        }
        platform::WindowsSelectionError::PatternUnavailable
        | platform::WindowsSelectionError::Busy
        | platform::WindowsSelectionError::Timeout => (true, true, CapabilityHealth::Degraded),
        platform::WindowsSelectionError::EmptySelection
        | platform::WindowsSelectionError::SelectionTooLarge
        | platform::WindowsSelectionError::SecureField => (true, true, CapabilityHealth::Ready),
    };
    update_windows_selection_runtime_status(
        app,
        available,
        running,
        error.message(),
        error.code(),
        health,
    );
}

#[cfg(target_os = "windows")]
fn update_windows_selection_runtime_status(
    app: &AppHandle,
    available: bool,
    running: bool,
    message: &str,
    code: &str,
    health: CapabilityHealth,
) {
    let state = app.state::<AppState>();
    if let Ok(mut inner) = state.inner.lock() {
        inner.selection_status = WatcherStatus::with_code(available, running, message, code);
        inner.capability_snapshot.direct_selection_read =
            RuntimeCapabilityState::with_status(health, code);
    };
}

#[cfg(target_os = "windows")]
fn windows_cursor_logical_anchor(app: &AppHandle) -> Option<Point> {
    let window = app.get_webview_window("popup")?;
    let cursor = window.cursor_position().ok()?;
    windows_physical_anchor_to_logical(app, cursor.x, cursor.y)
}

#[cfg(target_os = "windows")]
fn windows_physical_anchor_to_logical(
    app: &AppHandle,
    physical_x: f64,
    physical_y: f64,
) -> Option<Point> {
    let window = app.get_webview_window("popup")?;
    window
        .available_monitors()
        .ok()?
        .into_iter()
        .find_map(|monitor| {
            windows_physical_to_monitor_logical(
                physical_x,
                physical_y,
                PhysicalPoint {
                    x: monitor.position().x,
                    y: monitor.position().y,
                },
                GeometryPhysicalSize {
                    width: monitor.size().width,
                    height: monitor.size().height,
                },
                monitor.scale_factor(),
            )
        })
}

#[cfg(target_os = "windows")]
fn windows_default_logical_anchor(app: &AppHandle) -> Option<Point> {
    let window = app.get_webview_window("popup")?;
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let center = monitor_logical_center(&monitor)?;
    Some(Point {
        x: center.x,
        y: center.y,
    })
}

#[cfg(target_os = "windows")]
fn windows_physical_to_monitor_logical(
    physical_x: f64,
    physical_y: f64,
    monitor_origin: PhysicalPoint,
    monitor_size: GeometryPhysicalSize,
    scale_factor: f64,
) -> Option<Point> {
    if !physical_x.is_finite()
        || !physical_y.is_finite()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || monitor_size.width == 0
        || monitor_size.height == 0
    {
        return None;
    }
    let origin_x = monitor_origin.x as f64;
    let origin_y = monitor_origin.y as f64;
    if physical_x < origin_x
        || physical_y < origin_y
        || physical_x >= origin_x + monitor_size.width as f64
        || physical_y >= origin_y + monitor_size.height as f64
    {
        return None;
    }
    Some(Point {
        x: origin_x / scale_factor + (physical_x - origin_x) / scale_factor,
        y: origin_y / scale_factor + (physical_y - origin_y) / scale_factor,
    })
}

#[tauri::command]
async fn get_settings(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SettingsPayload, String> {
    state.startup_gate.wait(Duration::from_secs(2))?;
    let _transition = state.watcher_transition.lock().await;
    ensure_data_dir(&state.data_dir)?;
    let persisted_settings = load_settings(&state.data_dir)?;
    let (api_key_status, keychain_warning) = get_api_key_status_for_settings(&state.data_dir);
    let runtime_settings = state.inner.lock().map_err(lock_error)?.settings.clone();
    if persisted_settings != runtime_settings {
        return Err("运行中的设置与磁盘文件不一致；请刷新监听诊断以安全应用外部变更。".to_owned());
    }
    current_settings_payload(&app, &state, api_key_status, keychain_warning)
}

#[tauri::command]
async fn refresh_watcher_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SettingsPayload, String> {
    let _transition = state.watcher_transition.lock().await;
    ensure_data_dir(&state.data_dir)?;
    let settings = load_settings(&state.data_dir)?;
    let selection_enabled = settings.enable_selection_popup;
    let (api_key_status, keychain_warning) = get_api_key_status_for_settings(&state.data_dir);
    let mut runtime_warnings: Vec<String> = keychain_warning.into_iter().collect();
    let mut hidden_snapshot = None;
    {
        let mut inner = state.inner.lock().map_err(lock_error)?;
        inner.settings = settings;
        inner.double_copy_status =
            WatcherStatus::new(true, false, "正在静默重新检测 Cmd+C+C 监听。");
        inner.selection_status = WatcherStatus::new(true, false, "正在静默重新检测自动选区监听。");
        inner.capability_snapshot = initial_capability_snapshot();
        clear_active_selection(&mut inner);
        // Preserve the last external selection attempt so a diagnostics
        // refresh cannot erase the Preview failure it is meant to inspect.
        if !selection_enabled {
            match commit_popup_close(&mut inner, PopupCloseReason::ConfigurationChanged) {
                Ok(snapshot) => hidden_snapshot = Some(snapshot),
                Err(error) => runtime_warnings.push(format!(
                    "已读取磁盘设置，但关闭现有划词浮窗失败：{}",
                    error.code()
                )),
            }
        }
    }
    if let Some(snapshot) = hidden_snapshot {
        hide_committed_popup(
            &app,
            &state.inner,
            &snapshot,
            PopupCloseReason::ConfigurationChanged,
        );
    }

    let generation = match restart_platform_watchers(
        app.clone(),
        state.inner.clone(),
        state.watcher_readiness.clone(),
        &state.quitting,
        &state.watcher_lifecycle,
    ) {
        Ok(generation) => generation,
        Err(error) => {
            runtime_warnings.push(format!("监听刷新未能启动：{error}"));
            apply_watcher_restart_failure(
                &state.inner,
                &state.quitting,
                &state.watcher_lifecycle,
                selection_enabled,
                "watcher_restart_failed",
            )?;
            return current_settings_payload(
                &app,
                &state,
                api_key_status,
                Some(runtime_warnings.join("；")),
            );
        }
    };
    observe_acceptance_watcher_snapshot(&state, AcceptanceLifecycleEvent::WatcherStarted)?;
    let readiness = state.watcher_readiness.clone();
    let wait_result = tauri::async_runtime::spawn_blocking(move || {
        readiness.wait_for(generation, Duration::from_secs(2))
    })
    .await;

    match wait_result {
        Ok(Ok(WatcherWaitOutcome::Ready)) => {
            observe_acceptance_watcher_snapshot(&state, AcceptanceLifecycleEvent::WatcherReady)?;
        }
        Ok(Ok(WatcherWaitOutcome::Superseded)) => {
            observe_acceptance_watcher_snapshot(&state, AcceptanceLifecycleEvent::WatcherDegraded)?;
            runtime_warnings.push("本次监听刷新被意外的新代际替代；已保留降级状态。".to_owned());
            apply_watcher_restart_failure(
                &state.inner,
                &state.quitting,
                &state.watcher_lifecycle,
                selection_enabled,
                "watcher_generation_superseded",
            )?;
        }
        Ok(Ok(WatcherWaitOutcome::TimedOut(missing))) => {
            {
                let mut inner = state.inner.lock().map_err(lock_error)?;
                apply_watcher_refresh_timeout(&mut inner, missing);
            }
            runtime_warnings
                .push("监听器没有在 2 秒内完成状态握手；已显示缺失来源的降级诊断。".to_owned());
            if !selection_enabled && missing.selection {
                enforce_selection_disabled_after_timeout(
                    &state.inner,
                    &state.quitting,
                    &state.watcher_lifecycle,
                    &mut runtime_warnings,
                )?;
            }
            observe_acceptance_watcher_snapshot(&state, AcceptanceLifecycleEvent::WatcherDegraded)?;
        }
        Ok(Err(error)) => {
            runtime_warnings.push(format!("等待监听刷新状态失败：{error}"));
            apply_watcher_restart_failure(
                &state.inner,
                &state.quitting,
                &state.watcher_lifecycle,
                selection_enabled,
                "watcher_readiness_failed",
            )?;
        }
        Err(error) => {
            runtime_warnings.push(format!("等待监听刷新任务失败：{error}"));
            apply_watcher_restart_failure(
                &state.inner,
                &state.quitting,
                &state.watcher_lifecycle,
                selection_enabled,
                "watcher_wait_task_failed",
            )?;
        }
    }

    current_settings_payload(
        &app,
        &state,
        api_key_status,
        (!runtime_warnings.is_empty()).then(|| runtime_warnings.join("；")),
    )
}

fn current_settings_payload(
    app: &AppHandle,
    state: &AppState,
    api_key_status: ApiKeyStatus,
    runtime_warning: Option<String>,
) -> Result<SettingsPayload, String> {
    let (accessibility_grant, listen_event_grant) = current_permission_grants();
    let (settings, double_copy_status, selection_status, capability_snapshot, selection_read) = {
        let mut inner = state.inner.lock().map_err(lock_error)?;
        inner
            .capability_snapshot
            .apply_permission_grants(accessibility_grant, listen_event_grant);
        if inner.selection_status.code.as_deref() == Some("selection_disabled_by_setting") {
            mark_selection_capabilities_disabled(&mut inner.capability_snapshot);
        } else {
            apply_latched_selection_baseline_degradation(&mut inner);
        }
        (
            inner.settings.clone(),
            inner.double_copy_status.clone(),
            inner.selection_status.clone(),
            inner.capability_snapshot.clone(),
            inner.last_selection_read.clone(),
        )
    };
    Ok(SettingsPayload {
        settings,
        has_api_key: api_key_status.has_api_key(),
        api_key_status,
        api_key_storage: compiled_api_key_storage(),
        runtime_platform: compiled_runtime_platform(),
        double_copy_status,
        selection_status,
        capability_snapshot,
        permission_diagnostics: permission_diagnostics(app, selection_read),
        runtime_warning,
    })
}

fn observe_acceptance_watcher_snapshot(
    state: &State<'_, AppState>,
    event: AcceptanceLifecycleEvent,
) -> Result<(), String> {
    let should_observe = {
        let acceptance = state.acceptance.lock().map_err(lock_error)?;
        acceptance
            .status()
            .armed
            .is_some_and(|armed| armed.scenario == AcceptanceScenario::WatcherRestart)
    };
    if !should_observe {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let (generation, resources) = macos_native::resource_snapshot()?;
        let mut acceptance = state.acceptance.lock().map_err(lock_error)?;
        match event {
            AcceptanceLifecycleEvent::WatcherStarted => {
                acceptance
                    .observe_watcher_started(generation)
                    .map_err(|error| error.to_string())?;
            }
            AcceptanceLifecycleEvent::WatcherReady | AcceptanceLifecycleEvent::WatcherDegraded => {
                acceptance
                    .observe_watcher_resolution(
                        generation,
                        event == AcceptanceLifecycleEvent::WatcherDegraded,
                    )
                    .map_err(|error| error.to_string())?;
            }
            AcceptanceLifecycleEvent::WatcherReleased
            | AcceptanceLifecycleEvent::ScenarioStarted
            | AcceptanceLifecycleEvent::ScenarioEnded
            | AcceptanceLifecycleEvent::TapDisabled
            | AcceptanceLifecycleEvent::TapReady
            | AcceptanceLifecycleEvent::TapDegraded => {}
        }
        acceptance
            .observe_watcher_resources(generation, event, resources)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = event;
        Ok(())
    }
}

#[tauri::command]
fn acceptance_diagnostics_status(
    state: State<'_, AppState>,
) -> Result<AcceptanceRuntimeStatus, String> {
    let acceptance = state.acceptance.lock().map_err(lock_error)?;
    Ok(acceptance.status())
}

#[cfg(feature = "acceptance-testing")]
#[tauri::command]
async fn inject_acceptance_tap_disabled(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let expected_epoch = {
            let acceptance = state.acceptance.lock().map_err(lock_error)?;
            let epoch = state.acceptance_injection_epoch.load(Ordering::SeqCst);
            if !acceptance_tap_injection_authorized_at(&acceptance.status(), epoch, epoch) {
                return Err("当前验收会话未布置 tap-disabled 恢复场景。".to_owned());
            }
            epoch
        };
        let acceptance = state.acceptance.clone();
        let injection_epoch = state.acceptance_injection_epoch.clone();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        app.run_on_main_thread(move || {
            let result = execute_acceptance_tap_injection_at_epoch(
                &acceptance,
                &injection_epoch,
                expected_epoch,
                || {
                    // The feature-only native hook executes synchronously when
                    // called on the main queue. Holding the session lock here
                    // prevents clear/re-arm between epoch validation and execution;
                    // native status delivery is deferred until after this returns.
                    macos_native::inject_acceptance_mouse_tap_disabled()
                },
            );
            let _ = sender.send(result);
        })
        .map_err(|error| format!("无法在主线程执行 tap-disabled 验收注入：{error}"))?;

        tauri::async_runtime::spawn_blocking(move || receiver.recv_timeout(Duration::from_secs(5)))
            .await
            .map_err(|error| format!("等待 tap-disabled 验收注入失败：{error}"))?
            .map_err(|error| format!("tap-disabled 验收注入执行超时：{error}"))?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, state);
        Err("tap-disabled 验收注入仅支持 macOS。".to_owned())
    }
}

#[cfg(feature = "acceptance-testing")]
fn acceptance_tap_injection_authorized(status: &AcceptanceRuntimeStatus) -> bool {
    status.enabled
        && status.tap_injection_available
        && status.phase == AcceptanceSessionPhase::Running
        && status.armed.as_ref().is_some_and(|armed| {
            armed.scenario == AcceptanceScenario::TapDisabledRecovery && armed.ordinal == 1
        })
}

#[cfg(feature = "acceptance-testing")]
fn acceptance_tap_injection_authorized_at(
    status: &AcceptanceRuntimeStatus,
    expected_epoch: u64,
    execution_epoch: u64,
) -> bool {
    expected_epoch == execution_epoch && acceptance_tap_injection_authorized(status)
}

#[cfg(feature = "acceptance-testing")]
fn execute_acceptance_tap_injection_at_epoch(
    acceptance: &Mutex<AcceptanceRuntime>,
    injection_epoch: &AtomicU64,
    expected_epoch: u64,
    inject: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let acceptance = acceptance.lock().map_err(lock_error)?;
    let execution_epoch = injection_epoch.load(Ordering::SeqCst);
    if !acceptance_tap_injection_authorized_at(
        &acceptance.status(),
        expected_epoch,
        execution_epoch,
    ) {
        return Err("tap-disabled 验收注入授权已在执行前失效，请重新布置场景。".to_owned());
    }
    inject()
}

#[tauri::command]
async fn start_acceptance_diagnostics(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AcceptanceRuntimeStatus, String> {
    with_watcher_transition_ordered(&state.watcher_transition, || {
        // Metadata comes exclusively from the signed process and installed
        // baseline applications. The transition gate makes the native Q
        // snapshot and session start one ordered operation with every restart.
        let metadata = acceptance_runtime_metadata(&app)?;
        let mut acceptance = state.acceptance.lock().map_err(lock_error)?;
        let result = acceptance
            .start_full_baseline(metadata)
            .map_err(|error| error.to_string());
        #[cfg(feature = "acceptance-testing")]
        if result.is_ok() {
            state
                .acceptance_injection_epoch
                .fetch_add(1, Ordering::SeqCst);
        }
        result
    })
    .await
}

#[tauri::command]
fn arm_acceptance_scenario(
    scenario: AcceptanceScenario,
    ordinal: u32,
    state: State<'_, AppState>,
) -> Result<AcceptanceRuntimeStatus, String> {
    let mut acceptance = state.acceptance.lock().map_err(lock_error)?;
    let result = acceptance
        .arm(scenario, ordinal)
        .map_err(|error| error.to_string());
    #[cfg(feature = "acceptance-testing")]
    if result.is_ok() {
        state
            .acceptance_injection_epoch
            .fetch_add(1, Ordering::SeqCst);
    }
    result
}

#[tauri::command]
fn set_acceptance_self_isolation(
    action: SelfIsolationAction,
    state: State<'_, AppState>,
) -> Result<AcceptanceRuntimeStatus, String> {
    let mut acceptance = state.acceptance.lock().map_err(lock_error)?;
    let result = acceptance
        .set_self_isolation(action)
        .map_err(|error| error.to_string());
    #[cfg(feature = "acceptance-testing")]
    if result.is_ok() {
        state
            .acceptance_injection_epoch
            .fetch_add(1, Ordering::SeqCst);
    }
    result
}

fn acceptance_self_interaction_window_from_label(
    label: &str,
) -> Result<AcceptanceSelfInteractionWindow, String> {
    match label {
        "settings" => Ok(AcceptanceSelfInteractionWindow::Settings),
        "popup" => Ok(AcceptanceSelfInteractionWindow::Popup),
        _ => Err("未知验收窗口，已拒绝记录自身交互证据。".to_owned()),
    }
}

#[tauri::command]
fn record_acceptance_self_interaction(
    event_kind: AcceptanceSelfInteractionKind,
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let acceptance_window = acceptance_self_interaction_window_from_label(window.label())?;
    let mut acceptance = state.acceptance.lock().map_err(lock_error)?;
    acceptance
        .record_self_interaction(acceptance_window, event_kind)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn end_acceptance_diagnostics(
    state: State<'_, AppState>,
) -> Result<AcceptanceReport, String> {
    with_watcher_transition_ordered(&state.watcher_transition, || {
        // Read production Q after acquiring the same transition gate held by
        // every runtime restart. Do not hold acceptance across this main-queue
        // read because native status callbacks also acquire acceptance.
        #[cfg(target_os = "macos")]
        let observed_multi_click_quiet_window_ms =
            current_native_multi_click_quiet_window_ms().unwrap_or(0);
        let mut acceptance = state.acceptance.lock().map_err(lock_error)?;
        #[cfg(target_os = "macos")]
        if let Err(error) = acceptance
            .validate_runtime_multi_click_quiet_window(observed_multi_click_quiet_window_ms)
        {
            eprintln!("acceptance_diagnostics.multi_click_window: {error}");
        }
        let result = acceptance.end().map_err(|error| error.to_string());
        #[cfg(feature = "acceptance-testing")]
        if result.is_ok() {
            state
                .acceptance_injection_epoch
                .fetch_add(1, Ordering::SeqCst);
        }
        result
    })
    .await
}

#[tauri::command]
fn export_acceptance_diagnostics(
    state: State<'_, AppState>,
) -> Result<AcceptanceExportReceipt, String> {
    let path = acceptance_runtime::export_path(&state.data_dir);
    #[cfg(feature = "acceptance-testing")]
    validate_acceptance_managed_path(&state.data_dir, &path)?;
    let acceptance = state.acceptance.lock().map_err(lock_error)?;
    acceptance
        .export_atomic(&path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_acceptance_diagnostics(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AcceptanceRuntimeStatus, String> {
    // Export and clear are serialized by the acceptance lock. Clear owns the
    // lock before deleting the fixed report path, so an older export can never
    // recreate the file after this command returns.
    let mut acceptance = state.acceptance.lock().map_err(lock_error)?;
    let path = acceptance_runtime::export_path(&state.data_dir);
    #[cfg(feature = "acceptance-testing")]
    validate_acceptance_managed_path(&state.data_dir, &path)?;
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("清除验收报告失败：{error}")),
    }
    state.acceptance_probe_epoch.fetch_add(1, Ordering::SeqCst);
    let hidden_snapshot = {
        let mut inner = state.inner.lock().map_err(lock_error)?;
        cancel_acceptance_rapid_probe_locked(&mut inner)?
    };
    acceptance.clear();
    #[cfg(feature = "acceptance-testing")]
    state
        .acceptance_injection_epoch
        .fetch_add(1, Ordering::SeqCst);
    let status = acceptance.status();
    drop(acceptance);
    if let Some(snapshot) = hidden_snapshot {
        hide_committed_popup(
            &app,
            &state.inner,
            &snapshot,
            PopupCloseReason::SelectionHandled,
        );
    }
    Ok(status)
}

#[tauri::command]
async fn save_settings(
    settings: AppSettings,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SettingsPayload, String> {
    let _transition = state.watcher_transition.lock().await;
    let normalized = normalize_settings(settings);
    ensure_data_dir(&state.data_dir)?;
    // Everything that can fail without changing runtime state is deliberately
    // completed before the settings file becomes the committed source of truth.
    let (api_key_status, keychain_warning) = get_api_key_status_for_settings(&state.data_dir);
    let previous_selection_enabled = state
        .inner
        .lock()
        .map_err(lock_error)?
        .settings
        .enable_selection_popup;
    let previous_automatic_selection_enabled = state
        .inner
        .lock()
        .map_err(lock_error)?
        .settings
        .enable_automatic_selection;
    let restart_required = selection_setting_requires_restart(
        previous_selection_enabled,
        normalized.enable_selection_popup,
    ) || previous_automatic_selection_enabled
        != normalized.enable_automatic_selection;
    save_managed_json(
        &state.data_dir,
        &settings_path(&state.data_dir),
        &normalized,
    )?;

    // From this point on the save is committed. Runtime activation failures are
    // returned in the effective payload instead of falsely reporting that the
    // persisted setting was rejected.
    let mut runtime_warnings: Vec<String> = keychain_warning.into_iter().collect();
    let mut hidden_snapshot = None;
    {
        let mut inner = state.inner.lock().map_err(lock_error)?;
        inner.settings = normalized.clone();
        clear_active_selection(&mut inner);
        // Saving unchanged settings must not erase the most recent external
        // selection trace before the user can copy it.
        if restart_required {
            inner.double_copy_status =
                WatcherStatus::new(true, false, "正在应用 Cmd+C+C 监听设置。");
            inner.selection_status = WatcherStatus::new(
                true,
                false,
                if normalized.enable_selection_popup {
                    "正在启用自动选区监听。"
                } else {
                    "正在停用自动选区监听。"
                },
            );
            inner.capability_snapshot = initial_capability_snapshot();
        }
        if !inner.settings.enable_selection_popup {
            match commit_popup_close(&mut inner, PopupCloseReason::ConfigurationChanged) {
                Ok(snapshot) => hidden_snapshot = Some(snapshot),
                Err(error) => runtime_warnings.push(format!(
                    "设置已保存，但关闭现有划词浮窗失败：{}",
                    error.code()
                )),
            }
        }
    }
    if let Some(snapshot) = hidden_snapshot {
        hide_committed_popup(
            &app,
            &state.inner,
            &snapshot,
            PopupCloseReason::ConfigurationChanged,
        );
    }
    resize_existing_popup_to_settings(&app, &state.inner);

    if restart_required {
        let generation = match restart_platform_watchers(
            app.clone(),
            state.inner.clone(),
            state.watcher_readiness.clone(),
            &state.quitting,
            &state.watcher_lifecycle,
        ) {
            Ok(generation) => Some(generation),
            Err(error) => {
                runtime_warnings.push(format!("监听器未能应用已保存的设置：{error}"));
                None
            }
        };

        if let Some(generation) = generation {
            if let Err(error) = observe_acceptance_watcher_snapshot(
                &state,
                AcceptanceLifecycleEvent::WatcherStarted,
            ) {
                runtime_warnings.push(format!("验收观察器未能记录监听器启动：{error}"));
            }
            let readiness = state.watcher_readiness.clone();
            let wait_result = tauri::async_runtime::spawn_blocking(move || {
                readiness.wait_for(generation, Duration::from_secs(2))
            })
            .await;

            match wait_result {
                Ok(Ok(WatcherWaitOutcome::Ready)) => {
                    if let Err(error) = observe_acceptance_watcher_snapshot(
                        &state,
                        AcceptanceLifecycleEvent::WatcherReady,
                    ) {
                        runtime_warnings.push(format!("验收观察器未能记录监听器就绪：{error}"));
                    }
                }
                Ok(Ok(WatcherWaitOutcome::Superseded)) => {
                    runtime_warnings.push(
                        "设置已保存，但本次监听器代际被意外替代；请刷新监听诊断。".to_owned(),
                    );
                    apply_watcher_restart_failure(
                        &state.inner,
                        &state.quitting,
                        &state.watcher_lifecycle,
                        normalized.enable_selection_popup,
                        "watcher_generation_superseded",
                    )?;
                }
                Ok(Ok(WatcherWaitOutcome::TimedOut(missing))) => {
                    {
                        let mut inner = state.inner.lock().map_err(lock_error)?;
                        apply_watcher_refresh_timeout(&mut inner, missing);
                    }
                    runtime_warnings
                        .push("设置已保存，但监听器没有在 2 秒内完成状态握手。".to_owned());
                    if !normalized.enable_selection_popup && missing.selection {
                        enforce_selection_disabled_after_timeout(
                            &state.inner,
                            &state.quitting,
                            &state.watcher_lifecycle,
                            &mut runtime_warnings,
                        )?;
                    }
                    if let Err(error) = observe_acceptance_watcher_snapshot(
                        &state,
                        AcceptanceLifecycleEvent::WatcherDegraded,
                    ) {
                        runtime_warnings.push(format!("验收观察器未能记录监听器降级：{error}"));
                    }
                }
                Ok(Err(error)) => {
                    runtime_warnings.push(format!("等待监听器状态失败：{error}"));
                    apply_watcher_restart_failure(
                        &state.inner,
                        &state.quitting,
                        &state.watcher_lifecycle,
                        normalized.enable_selection_popup,
                        "watcher_readiness_failed",
                    )?;
                }
                Err(error) => {
                    runtime_warnings.push(format!("等待监听器任务失败：{error}"));
                    apply_watcher_restart_failure(
                        &state.inner,
                        &state.quitting,
                        &state.watcher_lifecycle,
                        normalized.enable_selection_popup,
                        "watcher_wait_task_failed",
                    )?;
                }
            }
        } else {
            apply_watcher_restart_failure(
                &state.inner,
                &state.quitting,
                &state.watcher_lifecycle,
                normalized.enable_selection_popup,
                "watcher_restart_failed",
            )?;
        }
    }

    current_settings_payload(
        &app,
        &state,
        api_key_status,
        (!runtime_warnings.is_empty()).then(|| runtime_warnings.join("；")),
    )
}

fn selection_setting_requires_restart(previous: bool, next: bool) -> bool {
    previous != next
}

fn selection_resources_released(resources: WatcherResourceCounts) -> bool {
    resources.snapshot_version == 1
        && resources.selection_observers == 0
        && resources.selection_observer_sources == 0
        && resources.mouse_event_taps == 0
        && resources.mouse_event_tap_sources == 0
        && resources.workspace_activation_observers == 0
}

fn apply_watcher_restart_failure(
    inner: &Arc<Mutex<InnerState>>,
    quitting: &AtomicBool,
    watcher_lifecycle: &Mutex<()>,
    selection_enabled: bool,
    code: &str,
) -> Result<(), String> {
    if !selection_enabled {
        stop_platform_watchers_ordered(quitting, watcher_lifecycle);
        return enforce_selection_disabled_after_restart_failure(inner, code);
    }

    let mut inner = inner.lock().map_err(lock_error)?;
    inner.double_copy_status = WatcherStatus::with_code(
        true,
        false,
        "设置已保存，但 Cmd+C+C 监听器未能启动；请刷新监听诊断。",
        code,
    );
    inner.selection_status = WatcherStatus::with_code(
        true,
        false,
        "设置已保存，但自动选区监听器未能启动；请刷新监听诊断。",
        code,
    );
    inner.capability_snapshot.key_tap =
        RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, code);
    inner.capability_snapshot.clipboard_double_copy_fallback =
        RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, code);
    inner.capability_snapshot.mouse_tap =
        RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, code);
    inner.capability_snapshot.ax_selected_text_observer =
        RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, code);
    inner.capability_snapshot.direct_selection_read =
        RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, code);
    Ok(())
}

fn enforce_selection_disabled_after_restart_failure(
    inner: &Arc<Mutex<InnerState>>,
    code: &str,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let released = macos_native::resource_snapshot()
        .map(|(_, resources)| selection_resources_released(resources))
        .unwrap_or(false);
    #[cfg(not(target_os = "macos"))]
    let released = true;

    let mut inner = inner.lock().map_err(lock_error)?;
    if released {
        apply_selection_disabled_by_setting(&mut inner);
        mark_double_copy_stopped_for_fail_closed(&mut inner, code);
    } else {
        apply_selection_disable_unconfirmed(&mut inner, code);
    }
    Ok(())
}

fn enforce_selection_disabled_after_timeout(
    inner: &Arc<Mutex<InnerState>>,
    quitting: &AtomicBool,
    watcher_lifecycle: &Mutex<()>,
    runtime_warnings: &mut Vec<String>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let initially_released = macos_native::resource_snapshot()
        .map(|(_, resources)| selection_resources_released(resources))
        .unwrap_or(false);
    #[cfg(not(target_os = "macos"))]
    let initially_released = true;

    if initially_released {
        let mut inner = inner.lock().map_err(lock_error)?;
        apply_selection_disabled_by_setting(&mut inner);
        runtime_warnings
            .push("停用回调未在 2 秒内返回，但同步资源快照确认自动选区资源均已释放。".to_owned());
        return Ok(());
    }

    // Native stop is queued on the macOS main queue. The following resource
    // snapshot performs a synchronous main-queue read and therefore acts as the
    // release barrier for that stop request.
    stop_platform_watchers_ordered(quitting, watcher_lifecycle);
    #[cfg(target_os = "macos")]
    let released_after_stop = macos_native::resource_snapshot()
        .map(|(_, resources)| selection_resources_released(resources))
        .unwrap_or(false);
    #[cfg(not(target_os = "macos"))]
    let released_after_stop = true;

    let mut inner = inner.lock().map_err(lock_error)?;
    if released_after_stop {
        apply_selection_disabled_by_setting(&mut inner);
        mark_double_copy_stopped_for_fail_closed(&mut inner, "selection_disable_fail_closed");
        runtime_warnings.push(
            "停用握手超时后已强制释放全部原生监听资源；Cmd+C+C 也已暂时停止，请刷新监听诊断以恢复。"
                .to_owned(),
        );
    } else {
        apply_selection_disable_unconfirmed(&mut inner, "selection_disable_unconfirmed");
        runtime_warnings.push(
            "无法确认自动选区原生资源已经释放；已隐藏浮窗并保持故障诊断，请退出应用后重新打开。"
                .to_owned(),
        );
    }
    Ok(())
}

fn apply_selection_disabled_by_setting(inner: &mut InnerState) {
    clear_active_selection(inner);
    inner.last_selection_read = None;
    inner.latched_selection_baseline_error = None;
    mark_selection_capabilities_disabled(&mut inner.capability_snapshot);
    inner.selection_status = WatcherStatus::with_code(
        true,
        false,
        "自动划词已按设置停用；不会安装鼠标或辅助功能选区监听。",
        "selection_disabled_by_setting",
    );
}

fn mark_selection_capabilities_disabled(capabilities: &mut CapabilitySnapshot) {
    capabilities.mouse_tap = RuntimeCapabilityState::with_status(
        CapabilityHealth::Disabled,
        "selection_disabled_by_setting",
    );
    capabilities.ax_selected_text_observer = RuntimeCapabilityState::with_status(
        CapabilityHealth::Disabled,
        "selection_disabled_by_setting",
    );
    capabilities.direct_selection_read = RuntimeCapabilityState::with_status(
        CapabilityHealth::Disabled,
        "selection_disabled_by_setting",
    );
}

fn apply_selection_disable_unconfirmed(inner: &mut InnerState, code: &str) {
    clear_active_selection(inner);
    inner.last_selection_read = None;
    inner.latched_selection_baseline_error = None;
    inner.capability_snapshot.mouse_tap =
        RuntimeCapabilityState::with_status(CapabilityHealth::Unknown, code);
    inner.capability_snapshot.ax_selected_text_observer =
        RuntimeCapabilityState::with_status(CapabilityHealth::Unknown, code);
    inner.capability_snapshot.direct_selection_read =
        RuntimeCapabilityState::with_status(CapabilityHealth::Unknown, code);
    inner.selection_status = WatcherStatus::with_code(
        true,
        false,
        "自动划词设置已关闭，但无法确认所有原生选区资源均已释放；请退出应用后重新打开。",
        code,
    );
}

fn mark_double_copy_stopped_for_fail_closed(inner: &mut InnerState, code: &str) {
    inner.double_copy_status = WatcherStatus::with_code(
        true,
        false,
        "为确保自动划词完全停用，Cmd+C+C 监听也已暂时停止；请刷新监听诊断以恢复。",
        code,
    );
    inner.capability_snapshot.key_tap =
        RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, code);
    inner.capability_snapshot.clipboard_double_copy_fallback =
        RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, code);
}

#[tauri::command]
async fn save_api_key(
    api_key: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SettingsPayload, String> {
    let _transition = state.watcher_transition.lock().await;
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        delete_api_key(&state.data_dir)?;
    } else {
        set_api_key(&state.data_dir, trimmed)?;
    }
    let (api_key_status, keychain_warning) = get_api_key_status_for_settings(&state.data_dir);
    current_settings_payload(&app, &state, api_key_status, keychain_warning)
}

#[tauri::command]
async fn clear_cache(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    clear_cache_file(&state.data_dir)?;
    Ok(serde_json::json!({ "ok": true }))
}

#[tauri::command]
async fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show().map_err(to_string)?;
        window.set_focus().map_err(to_string)?;
    }
    Ok(())
}

#[tauri::command]
async fn open_accessibility_settings() -> Result<(), String> {
    prompt_accessibility_authorization();
    open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
}

#[tauri::command]
async fn open_input_monitoring_settings() -> Result<(), String> {
    open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
}

#[tauri::command]
async fn open_external(url: String) -> Result<(), String> {
    if url.starts_with("https://") {
        open_url(&url)?;
    }
    Ok(())
}

#[tauri::command]
async fn copy_text(text: String) -> Result<(), String> {
    write_clipboard(&text)
}

#[tauri::command]
async fn set_popup_dragging(is_dragging: bool, state: State<'_, AppState>) -> Result<(), String> {
    let mut inner = state.inner.lock().map_err(lock_error)?;
    inner.popup_dragging = is_dragging;
    Ok(())
}

#[tauri::command]
async fn copy_translation(state: State<'_, AppState>) -> Result<(), String> {
    let translation = state
        .inner
        .lock()
        .map_err(lock_error)?
        .popup_controller
        .snapshot()
        .translation
        .clone();
    if let Some(text) = translation {
        write_clipboard(&text)?;
    }
    Ok(())
}

#[tauri::command]
async fn copy_source(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let (text, hidden_snapshot) = {
        let mut inner = state.inner.lock().map_err(lock_error)?;
        let text = inner
            .popup_controller
            .snapshot()
            .source_text
            .clone()
            .or_else(|| inner.popup_controller.snapshot().selected_text.clone())
            .or_else(|| inner.popup_controller.snapshot().cleaned_text.clone());
        let hidden_snapshot = if text.is_some() {
            Some(
                commit_popup_close(&mut inner, PopupCloseReason::ExplicitClose)
                    .map_err(popup_control_error)?,
            )
        } else {
            None
        };
        (text, hidden_snapshot)
    };
    if let Some(snapshot) = hidden_snapshot {
        hide_committed_popup(
            &app,
            &state.inner,
            &snapshot,
            PopupCloseReason::ExplicitClose,
        );
    }
    if let Some(text) = text {
        write_clipboard(&text)?;
    }
    Ok(())
}

#[tauri::command]
async fn close_popup(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let snapshot = {
        let mut inner = state.inner.lock().map_err(lock_error)?;
        commit_popup_close(&mut inner, PopupCloseReason::ExplicitClose)
            .map_err(popup_control_error)?
    };
    hide_committed_popup(
        &app,
        &state.inner,
        &snapshot,
        PopupCloseReason::ExplicitClose,
    );
    Ok(())
}

#[tauri::command]
async fn get_popup_snapshot(state: State<'_, AppState>) -> Result<PopupState, String> {
    state
        .inner
        .lock()
        .map(|inner| inner.popup_controller.snapshot().clone())
        .map_err(lock_error)
}

#[tauri::command]
async fn toggle_pin(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let next = {
        let mut inner = state.inner.lock().map_err(lock_error)?;
        inner
            .popup_controller
            .toggle_pin()
            .map_err(popup_control_error)?
    };
    send_popup_state(&app, &next);
    Ok(())
}

#[tauri::command]
async fn translate_selection(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let (cleaned, target_language, mode, selection_revision) = {
        let mut inner = state.inner.lock().map_err(lock_error)?;
        let is_current_selection = inner.popup_controller.snapshot().visible
            && inner.popup_controller.snapshot().status == PopupStatus::SelectionReady;
        let cleaned = is_current_selection
            .then(|| inner.popup_controller.snapshot().cleaned_text.clone())
            .flatten();
        let target_language =
            normalize_target_language(&inner.settings.target_language, DEFAULT_TARGET_LANGUAGE);
        let mode = inner.settings.mode.clone();
        let selection_revision = inner.popup_controller.snapshot().selection_revision;
        if let Some(cleaned) = &cleaned {
            inner.last_cleaned_text = cleaned.clone();
        }
        (cleaned, target_language, mode, selection_revision)
    };
    if let Some(cleaned) = cleaned {
        #[cfg(target_os = "windows")]
        if let Some(window) = app.get_webview_window("popup") {
            let _ = window.set_focusable(true);
        }
        translate_text(
            app,
            TranslateContext {
                inner: state.inner.clone(),
                data_dir: state.data_dir.clone(),
                http: state.http.clone(),
            },
            TranslateTask {
                cleaned_text: cleaned,
                bypass_cache: false,
                mode,
                target_language,
                selection_revision,
            },
        )
        .await?;
    }
    Ok(())
}

#[tauri::command]
async fn retry_translation(
    app: AppHandle,
    state: State<'_, AppState>,
    target_language: Option<String>,
) -> Result<(), String> {
    let (cleaned, mode, target_language, selection_revision) = {
        let inner = state.inner.lock().map_err(lock_error)?;
        let cleaned = inner
            .popup_controller
            .snapshot()
            .visible
            .then(|| inner.popup_controller.snapshot().cleaned_text.clone())
            .flatten()
            .unwrap_or_default();
        (
            cleaned,
            inner.settings.mode.clone(),
            resolve_popup_target_language(&inner, target_language.as_deref()),
            inner.popup_controller.snapshot().selection_revision,
        )
    };
    if !cleaned.is_empty() {
        translate_text(
            app,
            TranslateContext {
                inner: state.inner.clone(),
                data_dir: state.data_dir.clone(),
                http: state.http.clone(),
            },
            TranslateTask {
                cleaned_text: cleaned,
                bypass_cache: true,
                mode,
                target_language,
                selection_revision,
            },
        )
        .await?;
    }
    Ok(())
}

#[tauri::command]
async fn explain_terms(
    app: AppHandle,
    state: State<'_, AppState>,
    target_language: Option<String>,
) -> Result<(), String> {
    let (cleaned, target_language, selection_revision) = {
        let inner = state.inner.lock().map_err(lock_error)?;
        let cleaned = inner
            .popup_controller
            .snapshot()
            .visible
            .then(|| inner.popup_controller.snapshot().cleaned_text.clone())
            .flatten()
            .unwrap_or_default();
        (
            cleaned,
            resolve_popup_target_language(&inner, target_language.as_deref()),
            inner.popup_controller.snapshot().selection_revision,
        )
    };
    if !cleaned.is_empty() {
        translate_text(
            app,
            TranslateContext {
                inner: state.inner.clone(),
                data_dir: state.data_dir.clone(),
                http: state.http.clone(),
            },
            TranslateTask {
                cleaned_text: cleaned,
                bypass_cache: true,
                mode: TranslateMode::Terminology,
                target_language,
                selection_revision,
            },
        )
        .await?;
    }
    Ok(())
}

#[tauri::command]
async fn resize_popup(
    app: AppHandle,
    state: State<'_, AppState>,
    height: f64,
) -> Result<(), String> {
    resize_popup_to_content(&app, &state.inner, height);
    Ok(())
}

async fn translate_text(
    app: AppHandle,
    context: TranslateContext,
    task: TranslateTask,
) -> Result<(), String> {
    let TranslateContext {
        inner,
        data_dir,
        http,
    } = context;
    let TranslateTask {
        cleaned_text,
        bypass_cache,
        mode,
        target_language,
        selection_revision,
    } = task;
    let target_language = normalize_target_language(&target_language, DEFAULT_TARGET_LANGUAGE);
    let Some(translation_guard) = begin_translation(
        &app,
        &inner,
        selection_revision,
        &cleaned_text,
        &target_language,
    )?
    else {
        return Ok(());
    };

    let api_key = match get_api_key(&data_dir) {
        Ok(Some(value)) => value,
        Ok(None) => {
            update_popup_state_for_translation(
                &app,
                &inner,
                translation_guard,
                PopupState {
                    status: PopupStatus::Error,
                    source_text: Some(cleaned_text.clone()),
                    cleaned_text: Some(cleaned_text),
                    target_language: Some(target_language),
                    error: Some("请先在设置页保存 DeepSeek API Key。".to_string()),
                    error_kind: Some(PopupErrorKind::Configuration),
                    retryable: Some(false),
                    recovery_action: Some(PopupRecoveryAction::OpenSettings),
                    ..PopupState::default()
                },
            );
            let _ = open_settings(app).await;
            return Ok(());
        }
        Err(error) => {
            update_popup_state_for_translation(
                &app,
                &inner,
                translation_guard,
                PopupState {
                    status: PopupStatus::Error,
                    source_text: Some(cleaned_text.clone()),
                    cleaned_text: Some(cleaned_text),
                    target_language: Some(target_language),
                    error: Some(error.clone()),
                    error_kind: Some(PopupErrorKind::Configuration),
                    retryable: Some(true),
                    recovery_action: Some(PopupRecoveryAction::OpenSettings),
                    ..PopupState::default()
                },
            );
            return Err(error);
        }
    };

    let (settings, glossary, glossary_version, cache_key) = {
        let guard = inner.lock().map_err(lock_error)?;
        let settings = guard.settings.clone();
        drop(guard);
        let glossary = load_glossary(&data_dir);
        let glossary_version = create_glossary_version(&glossary);
        let cache_key = create_cache_key(
            &settings.model,
            &mode,
            &target_language,
            &glossary_version,
            &cleaned_text,
        );
        (settings, glossary, glossary_version, cache_key)
    };

    if settings.enable_cache && !bypass_cache {
        let mut cache = read_cache(&data_dir);
        if let Some(cached) = cache.entries.remove(&cache_key) {
            update_popup_state_for_translation(
                &app,
                &inner,
                translation_guard,
                PopupState {
                    status: PopupStatus::Translated,
                    source_text: Some(cleaned_text.clone()),
                    cleaned_text: Some(cleaned_text),
                    translation: Some(cached.translation),
                    cached: Some(true),
                    target_language: Some(target_language),
                    ..PopupState::default()
                },
            );
            return Ok(());
        }
    }

    let mut streamed_translation = String::new();
    let mut last_stream_update = Instant::now();
    let translation = match request_deepseek_stream(
        &http,
        DeepSeekStreamRequest {
            api_key: &api_key,
            cleaned_text: &cleaned_text,
            model: &settings.model,
            mode: &mode,
            target_language: &target_language,
            glossary: &glossary,
        },
        |delta| {
            streamed_translation.push_str(delta);
            if last_stream_update.elapsed() >= Duration::from_millis(STREAM_UPDATE_THROTTLE_MS) {
                update_popup_state_for_translation(
                    &app,
                    &inner,
                    translation_guard,
                    PopupState {
                        status: PopupStatus::Translating,
                        source_text: Some(cleaned_text.clone()),
                        cleaned_text: Some(cleaned_text.clone()),
                        translation: Some(streamed_translation.clone()),
                        cached: Some(false),
                        target_language: Some(target_language.clone()),
                        ..PopupState::default()
                    },
                );
                last_stream_update = Instant::now();
            }
        },
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            update_popup_state_for_translation(
                &app,
                &inner,
                translation_guard,
                PopupState {
                    status: PopupStatus::Error,
                    source_text: Some(cleaned_text.clone()),
                    cleaned_text: Some(cleaned_text),
                    target_language: Some(target_language),
                    error: Some(error),
                    error_kind: Some(PopupErrorKind::Translation),
                    retryable: Some(true),
                    recovery_action: Some(PopupRecoveryAction::RetryTranslation),
                    ..PopupState::default()
                },
            );
            return Ok(());
        }
    };

    if settings.enable_cache
        && !translation.is_empty()
        && translation_guard_is_current(&inner, translation_guard)
    {
        let _ = insert_cache_entry(
            &data_dir,
            cache_key.clone(),
            CacheEntry {
                key: cache_key,
                cleaned_text: cleaned_text.clone(),
                translation: translation.clone(),
                model: settings.model,
                mode: mode.clone(),
                target_language: target_language.clone(),
                glossary_version,
                created_at: cache_timestamp_now(),
            },
        );
    }

    update_popup_state_for_translation(
        &app,
        &inner,
        translation_guard,
        PopupState {
            status: PopupStatus::Translated,
            source_text: Some(cleaned_text.clone()),
            cleaned_text: Some(cleaned_text),
            translation: Some(translation),
            cached: Some(false),
            target_language: Some(target_language),
            ..PopupState::default()
        },
    );
    Ok(())
}

/// Serialize acceptance Q snapshot operations with every settings-driven
/// watcher transition. The synchronous watcher lifecycle gate separately
/// orders native start/stop submissions against process shutdown and is never
/// held across an await. Acceptance commands retain transition -> Q snapshot
/// -> acceptance lock ordering.
async fn with_watcher_transition_ordered<T, F>(
    transition: &tauri::async_runtime::Mutex<()>,
    operation: F,
) -> T
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    let _transition = transition.lock().await;
    operation()
}

fn start_platform_watchers(
    app: AppHandle,
    inner: Arc<Mutex<InnerState>>,
    readiness: Arc<WatcherReadiness>,
) -> Result<u64, String> {
    let generation = readiness.begin()?;

    #[cfg(target_os = "macos")]
    {
        macos_native::start(app, inner, readiness, generation);
    }

    #[cfg(target_os = "windows")]
    {
        let registered = platform::windows_selection_shortcut_registered();
        let (enabled, automatic_selection_enabled) = {
            let guard = inner.lock().map_err(lock_error)?;
            (
                guard.settings.enable_selection_popup,
                guard.settings.enable_automatic_selection,
            )
        };
        platform::set_windows_input_monitor_enabled(&app, enabled);
        let input_running = platform::windows_input_monitor_running(&app);
        let automatic_requested = enabled && automatic_selection_enabled;
        let automatic_configuration =
            platform::set_windows_automatic_selection_enabled(&app, automatic_requested);
        if let Err(error) = &automatic_configuration {
            eprintln!("windows_selection.automatic_configuration_failed: {error}");
        }
        let automatic_running = automatic_configuration.unwrap_or(false)
            && platform::windows_automatic_selection_running(&app);
        if let Ok(mut guard) = inner.lock() {
            let enabled = guard.settings.enable_selection_popup;
            guard.double_copy_status = WatcherStatus::with_code(
                false,
                false,
                format!(
                    "Windows 双复制兜底尚未启用；当前请使用 {}。",
                    platform::WINDOWS_SELECTION_SHORTCUT_LABEL
                ),
                "windows_double_copy_pending",
            );
            guard.selection_status = if !enabled {
                WatcherStatus::with_code(
                    true,
                    false,
                    "Windows 快捷键取词已在设置中关闭。",
                    "selection_disabled",
                )
            } else if registered && automatic_requested && automatic_running {
                WatcherStatus::with_code(
                    true,
                    true,
                    format!(
                        "Windows 鼠标划词与快捷键取词已就绪；快捷键为 {}。",
                        platform::WINDOWS_SELECTION_SHORTCUT_LABEL
                    ),
                    "windows_auto_selection_ready",
                )
            } else if registered && automatic_requested {
                WatcherStatus::with_code(
                    true,
                    true,
                    format!(
                        "自动划词监听未能完整启动；仍可使用 {} 与 Ctrl+C+C。",
                        platform::WINDOWS_SELECTION_SHORTCUT_LABEL
                    ),
                    "windows_auto_selection_degraded",
                )
            } else if registered {
                WatcherStatus::with_code(
                    true,
                    true,
                    format!(
                        "Windows 快捷键取词已就绪：选中文字后按 {}。",
                        platform::WINDOWS_SELECTION_SHORTCUT_LABEL
                    ),
                    "windows_uia_shortcut_ready",
                )
            } else {
                WatcherStatus::with_code(
                    false,
                    false,
                    format!(
                        "无法注册 {}；该快捷键可能已被其他应用占用。",
                        platform::WINDOWS_SELECTION_SHORTCUT_LABEL
                    ),
                    "windows_shortcut_registration_failed",
                )
            };
            guard.double_copy_status = windows_double_copy_status(enabled, input_running);
            guard.capability_snapshot = windows_shortcut_capability_snapshot(
                enabled,
                registered,
                input_running,
                automatic_selection_enabled,
                automatic_running,
            );
        }
        readiness.observe(generation, WatcherSource::Pasteboard)?;
        readiness.observe(generation, WatcherSource::Selection)?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = app;
        if let Ok(mut guard) = inner.lock() {
            guard.double_copy_status = WatcherStatus::with_code(
                false,
                false,
                "双复制监听目前仅支持 macOS。",
                "unsupported_os",
            );
            guard.selection_status = WatcherStatus::with_code(
                false,
                false,
                "自动选区浮窗目前仅支持 macOS。",
                "unsupported_os",
            );
        }
        readiness.observe(generation, WatcherSource::Pasteboard)?;
        readiness.observe(generation, WatcherSource::Selection)?;
    }

    Ok(generation)
}

fn restart_platform_watchers(
    app: AppHandle,
    inner: Arc<Mutex<InnerState>>,
    readiness: Arc<WatcherReadiness>,
    quitting: &AtomicBool,
    watcher_lifecycle: &Mutex<()>,
) -> Result<u64, String> {
    execute_watcher_restart_if_running(
        quitting,
        watcher_lifecycle,
        || start_platform_watchers(app, inner, readiness),
        stop_platform_watchers_native,
    )
}

fn execute_watcher_restart_if_running<T, Restart, Stop>(
    quitting: &AtomicBool,
    watcher_lifecycle: &Mutex<()>,
    restart: Restart,
    fail_safe_stop: Stop,
) -> Result<T, String>
where
    Restart: FnOnce() -> Result<T, String>,
    Stop: FnOnce(),
{
    if quitting.load(Ordering::Acquire) {
        return Err("应用正在退出，拒绝重新启动监听器。".to_owned());
    }

    let _lifecycle = match watcher_lifecycle.lock() {
        Ok(lifecycle) => lifecycle,
        Err(poisoned) => {
            let _lifecycle = poisoned.into_inner();
            quitting.store(true, Ordering::Release);
            fail_safe_stop();
            return Err("监听生命周期门已损坏；已停止监听器并进入不可逆退出状态。".to_owned());
        }
    };

    // The first check avoids needless blocking after shutdown. This second
    // check is the linearization point: shutdown stores `true` before taking
    // the same gate, so no native start can be submitted after shutdown wins.
    if quitting.load(Ordering::Acquire) {
        return Err("应用正在退出，拒绝重新启动监听器。".to_owned());
    }

    restart()
}

fn execute_watcher_stop_ordered<Stop>(
    quitting: &AtomicBool,
    watcher_lifecycle: &Mutex<()>,
    stop: Stop,
) where
    Stop: FnOnce(),
{
    let _lifecycle = match watcher_lifecycle.lock() {
        Ok(lifecycle) => lifecycle,
        Err(poisoned) => {
            // Recover the guard only to perform a final fail-safe stop. A
            // poisoned lifecycle gate permanently disables later restarts.
            quitting.store(true, Ordering::Release);
            poisoned.into_inner()
        }
    };
    stop();
}

fn execute_watcher_shutdown<Stop>(quitting: &AtomicBool, watcher_lifecycle: &Mutex<()>, stop: Stop)
where
    Stop: FnOnce(),
{
    // Irreversible and deliberately before the gate: a restart already in the
    // critical section may finish submitting start, but this shutdown then
    // submits stop after it. A restart that has not won the gate sees `true`.
    quitting.store(true, Ordering::Release);
    execute_watcher_stop_ordered(quitting, watcher_lifecycle, stop);
}

fn apply_watcher_refresh_timeout(inner: &mut InnerState, missing: MissingWatcherSources) {
    if missing.pasteboard {
        inner.double_copy_status = WatcherStatus::with_code(
            true,
            false,
            "Cmd+C+C 监听刷新超时，原生监听器尚未返回启动状态；请再次刷新诊断。",
            "pasteboard_watcher_refresh_timeout",
        );
        inner.capability_snapshot.key_tap = RuntimeCapabilityState::with_status(
            CapabilityHealth::Degraded,
            "pasteboard_watcher_refresh_timeout",
        );
        inner.capability_snapshot.clipboard_double_copy_fallback =
            RuntimeCapabilityState::with_status(
                CapabilityHealth::Degraded,
                "pasteboard_watcher_refresh_timeout",
            );
    }

    if missing.selection
        && inner.selection_status.code.as_deref()
            != Some("selection_multi_click_quiet_window_invalid")
    {
        inner.selection_status = WatcherStatus::with_code(
            true,
            false,
            "自动选区监听刷新超时，原生监听器尚未返回启动状态；请再次刷新诊断。",
            "selection_watcher_refresh_timeout",
        );
        inner.capability_snapshot.mouse_tap = RuntimeCapabilityState::with_status(
            CapabilityHealth::Degraded,
            "selection_watcher_refresh_timeout",
        );
        inner.capability_snapshot.ax_selected_text_observer = RuntimeCapabilityState::with_status(
            CapabilityHealth::Degraded,
            "selection_watcher_refresh_timeout",
        );
        inner.capability_snapshot.direct_selection_read = RuntimeCapabilityState::with_status(
            CapabilityHealth::Degraded,
            "selection_watcher_refresh_timeout",
        );
    }
}

fn stop_platform_watchers_ordered(quitting: &AtomicBool, watcher_lifecycle: &Mutex<()>) {
    execute_watcher_stop_ordered(quitting, watcher_lifecycle, stop_platform_watchers_native);
}

fn shutdown_platform_watchers(quitting: &AtomicBool, watcher_lifecycle: &Mutex<()>) {
    execute_watcher_shutdown(quitting, watcher_lifecycle, stop_platform_watchers_native);
}

fn stop_platform_watchers_native() {
    #[cfg(target_os = "macos")]
    {
        macos_native::stop();
    }
}

fn current_external_focus_pid() -> Option<i32> {
    #[cfg(target_os = "macos")]
    {
        macos_native::frontmost_external_pid()
    }

    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn restore_external_focus(pid: i32) {
    #[cfg(target_os = "macos")]
    {
        macos_native::restore_application_focus(pid);
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
    }
}

fn prompt_accessibility_authorization() {
    #[cfg(target_os = "macos")]
    {
        macos_native::prompt_accessibility();
    }
}

fn permission_diagnostics(
    app: &AppHandle,
    selection_read: Option<SelectionReadDiagnostics>,
) -> Option<PermissionDiagnostics> {
    #[cfg(target_os = "macos")]
    {
        Some(macos_permission_diagnostics(app, selection_read))
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, selection_read);
        None
    }
}

fn acceptance_runtime_metadata(app: &AppHandle) -> Result<AcceptanceRuntimeMetadata, String> {
    #[cfg(target_os = "macos")]
    {
        let executable_path =
            std::env::current_exe().map_err(|error| format!("读取验收构建路径失败：{error}"))?;
        let signed_path = macos_bundle_path(&executable_path).unwrap_or(executable_path);
        let build_kind = match classify_signature(&read_codesign_diagnostics(&signed_path)) {
            SignatureKind::Adhoc => AcceptanceBuildKind::AdHocAcceptance,
            SignatureKind::AppleDevelopment => AcceptanceBuildKind::Development,
            SignatureKind::DeveloperId => AcceptanceBuildKind::DeveloperId,
            SignatureKind::Unknown => AcceptanceBuildKind::Unknown,
        };
        #[cfg(target_arch = "aarch64")]
        let architecture = AcceptanceArchitecture::Arm64;
        #[cfg(target_arch = "x86_64")]
        let architecture = AcceptanceArchitecture::X86_64;
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        return Err("当前 CPU 架构不属于固定验收基线。".to_owned());
        let multi_click_quiet_window_ms = current_native_multi_click_quiet_window_ms()?;

        Ok(AcceptanceRuntimeMetadata {
            app: AcceptanceAppMetadata {
                version: app.package_info().version.to_string(),
                bundle_identifier: app.config().identifier.clone(),
                build_kind,
            },
            system: AcceptanceSystemMetadata {
                operating_system: AcceptanceOperatingSystem::MacOs,
                version: macos_product_version()?,
                architecture,
            },
            baseline_applications: vec![
                BaselineApplicationMetadata {
                    application: BaselineApplication::TextEdit,
                    version: macos_application_version(&[
                        "/System/Applications/TextEdit.app/Contents/Info.plist",
                    ])?,
                },
                BaselineApplicationMetadata {
                    application: BaselineApplication::Safari,
                    version: macos_application_version(&[
                        "/Applications/Safari.app/Contents/Info.plist",
                        "/System/Applications/Safari.app/Contents/Info.plist",
                    ])?,
                },
                BaselineApplicationMetadata {
                    application: BaselineApplication::Preview,
                    version: macos_application_version(&[
                        "/System/Applications/Preview.app/Contents/Info.plist",
                    ])?,
                },
            ],
            multi_click_quiet_window_ms,
        })
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("固定验收运行时仅支持 macOS。".to_owned())
    }
}

#[cfg(target_os = "macos")]
fn current_native_multi_click_quiet_window_ms() -> Result<u64, String> {
    acceptance_runtime::multi_click_quiet_window_ms_from_seconds(
        macos_native::multi_click_quiet_window_seconds(),
    )
    .map_err(|error| format!("读取原生多击静默窗口失败：{error}"))
}

#[cfg(target_os = "macos")]
fn macos_product_version() -> Result<String, String> {
    let output = Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .map_err(|error| format!("读取 macOS 版本失败：{error}"))?;
    bounded_metadata_output(output, "macOS 版本")
}

#[cfg(target_os = "macos")]
fn macos_application_version(candidates: &[&str]) -> Result<String, String> {
    for candidate in candidates {
        if !Path::new(candidate).is_file() {
            continue;
        }
        let output = Command::new("/usr/bin/plutil")
            .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
            .arg(candidate)
            .output()
            .map_err(|error| format!("读取验收基线应用版本失败：{error}"))?;
        if output.status.success() {
            return bounded_metadata_output(output, "基线应用版本");
        }
    }
    Err("无法读取固定验收基线应用的实际版本。".to_owned())
}

#[cfg(target_os = "macos")]
fn bounded_metadata_output(output: std::process::Output, label: &str) -> Result<String, String> {
    if !output.status.success() {
        return Err(format!("{label}命令返回失败状态。"));
    }
    let value = String::from_utf8(output.stdout).map_err(|_| format!("{label}不是有效 UTF-8。"))?;
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
    {
        return Err(format!("{label}不符合内容无关的版本标识约束。"));
    }
    Ok(value.to_owned())
}

#[cfg(target_os = "macos")]
fn macos_permission_diagnostics(
    app: &AppHandle,
    selection_read: Option<SelectionReadDiagnostics>,
) -> PermissionDiagnostics {
    let executable_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::new());
    let bundle_path =
        macos_bundle_path(&executable_path).unwrap_or_else(|| executable_path.clone());
    let codesign = read_codesign_diagnostics(&bundle_path);
    let team_identifier =
        parse_codesign_field(&codesign, "TeamIdentifier").filter(|value| value != "not set");
    let cd_hash = parse_codesign_field(&codesign, "CDHash");
    let signature_kind = classify_signature(&codesign);

    PermissionDiagnostics {
        bundle_identifier: app.config().identifier.clone(),
        app_version: app.package_info().version.to_string(),
        bundle_path: display_path(&bundle_path),
        executable_path: display_path(&executable_path),
        team_identifier,
        cd_hash,
        signature_kind,
        accessibility_trusted: macos_native::accessibility_trusted(),
        selection_read,
    }
}

#[cfg(target_os = "macos")]
fn macos_bundle_path(executable_path: &Path) -> Option<PathBuf> {
    executable_path
        .ancestors()
        .find(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        })
        .map(Path::to_path_buf)
}

#[cfg(target_os = "macos")]
fn read_codesign_diagnostics(bundle_path: &Path) -> String {
    Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4"])
        .arg(bundle_path)
        .output()
        .map(|output| {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            text
        })
        .unwrap_or_default()
}

fn parse_codesign_field(text: &str, field: &str) -> Option<String> {
    let prefix = format!("{field}=");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix).map(str::trim))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn classify_signature(codesign_output: &str) -> SignatureKind {
    if parse_codesign_field(codesign_output, "Signature").as_deref() == Some("adhoc") {
        return SignatureKind::Adhoc;
    }

    if codesign_output
        .lines()
        .any(|line| line.starts_with("Authority=Developer ID Application"))
    {
        return SignatureKind::DeveloperId;
    }

    if codesign_output
        .lines()
        .any(|line| line.starts_with("Authority=Apple Development"))
    {
        return SignatureKind::AppleDevelopment;
    }

    SignatureKind::Unknown
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Reclaims ownership transferred through the native watcher callback.
///
/// # Safety
///
/// `context` must be null or a pointer returned exactly once by
/// `Box::into_raw` for `T`. No other code may reclaim the same allocation.
#[cfg(any(target_os = "macos", test))]
unsafe fn release_native_context_owner<T, Observe>(
    context: *mut T,
    native_generation: i64,
    observe: Observe,
) where
    Observe: FnOnce(&T, i64),
{
    if context.is_null() {
        return;
    }

    let owner = unsafe { Box::from_raw(context) };
    if native_generation > 0 {
        observe(&owner, native_generation);
    }
}

#[cfg(all(not(feature = "local-api-key-file"), any(target_os = "macos", test)))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeychainReadDisposition {
    Missing,
    Present,
}

#[cfg(all(not(feature = "local-api-key-file"), any(target_os = "macos", test)))]
fn classify_keychain_read_status(
    status: i32,
    value_present: bool,
) -> Result<KeychainReadDisposition, String> {
    match (status, value_present) {
        (0, true) => Ok(KeychainReadDisposition::Present),
        (KEYCHAIN_ITEM_NOT_FOUND, false) => Ok(KeychainReadDisposition::Missing),
        (0, false) => Err("系统钥匙串返回成功，但没有返回 API Key 数据。".to_string()),
        (KEYCHAIN_ITEM_NOT_FOUND, true) => {
            Err("系统钥匙串报告 API Key 不存在，但同时返回了数据。".to_string())
        }
        (status, _) => Err(format!("读取系统钥匙串失败（状态 {status}）。")),
    }
}

#[cfg(all(not(feature = "local-api-key-file"), any(target_os = "macos", test)))]
fn classify_keychain_presence_status(status: i32) -> Result<bool, String> {
    match status {
        0 => Ok(true),
        KEYCHAIN_ITEM_NOT_FOUND => Ok(false),
        status => Err(format!(
            "检查系统钥匙串中的 API Key 失败（状态 {status}）。"
        )),
    }
}

#[cfg(target_os = "macos")]
mod macos_native {
    use super::*;
    #[cfg(not(feature = "local-api-key-file"))]
    use std::ffi::CString;
    use std::{
        ffi::CStr,
        os::raw::{c_char, c_double, c_int, c_longlong, c_void},
    };

    const EVENT_PASTEBOARD_STATUS: c_int = 1;
    const EVENT_SELECTION_STATUS: c_int = 2;
    const EVENT_PASTEBOARD_CHANGE: c_int = 3;
    const EVENT_DOUBLE_COPY: c_int = 4;
    const EVENT_MOUSE_DOWN: c_int = 5;
    const EVENT_SELECTION: c_int = 6;
    const EVENT_WATCHER_CONTEXT_RELEASE: c_int = 7;
    const EVENT_POPUP_ESCAPE: c_int = 8;
    const EVENT_POPUP_FOCUS_REQUEST: c_int = 9;

    #[repr(C)]
    #[derive(Default)]
    struct NativeResourceSnapshot {
        version: u32,
        selection_observer_count: u32,
        selection_observer_source_count: u32,
        key_event_tap_count: u32,
        key_event_tap_source_count: u32,
        mouse_event_tap_count: u32,
        mouse_event_tap_source_count: u32,
        pasteboard_timer_count: u32,
        workspace_activation_observer_count: u32,
        callback_count: u32,
        context_count: u32,
        effective_source_set_count: u32,
        total_source_count: u32,
        watcher_lifecycle_token: c_longlong,
    }

    struct NativeWatcherContext {
        app: AppHandle,
        inner: Arc<Mutex<InnerState>>,
        readiness: Arc<WatcherReadiness>,
        generation: u64,
    }

    unsafe extern "C" {
        fn paper_float_accessibility_is_trusted() -> c_int;
        fn paper_float_listen_event_access_preflight() -> c_int;
        fn paper_float_accessibility_prompt();
        #[cfg(not(feature = "local-api-key-file"))]
        fn paper_float_keychain_get(
            service: *const c_char,
            account: *const c_char,
            out_value: *mut *mut c_char,
        ) -> c_int;
        #[cfg(not(feature = "local-api-key-file"))]
        fn paper_float_keychain_contains(service: *const c_char, account: *const c_char) -> c_int;
        #[cfg(not(feature = "local-api-key-file"))]
        fn paper_float_keychain_set(
            service: *const c_char,
            account: *const c_char,
            secret: *const c_char,
        ) -> c_int;
        #[cfg(not(feature = "local-api-key-file"))]
        fn paper_float_keychain_delete(service: *const c_char, account: *const c_char) -> c_int;
        #[cfg(not(feature = "local-api-key-file"))]
        fn paper_float_keychain_free(value: *mut c_char);
        fn paper_float_frontmost_external_pid() -> c_int;
        fn paper_float_restore_application_focus(pid: c_int);
        fn paper_float_watchers_start(
            context: *mut c_void,
            callback: extern "C" fn(
                *mut c_void,
                c_int,
                *const c_char,
                c_double,
                c_double,
                c_longlong,
                c_int,
            ),
            selection_enabled: c_int,
        );
        fn paper_float_watchers_stop();
        fn paper_float_multi_click_quiet_window_seconds() -> c_double;
        fn paper_float_native_resource_snapshot(snapshot: *mut NativeResourceSnapshot) -> c_int;
        #[cfg(feature = "acceptance-testing")]
        fn paper_float_test_inject_tap_disabled(
            tap_kind: c_int,
            timeout: c_int,
            reenable_succeeds: c_int,
            observer_ready: c_int,
        ) -> c_int;
    }

    pub(super) fn accessibility_trusted() -> bool {
        unsafe { paper_float_accessibility_is_trusted() != 0 }
    }

    pub(super) fn listen_event_access_grant() -> PermissionGrant {
        match unsafe { paper_float_listen_event_access_preflight() } {
            1 => PermissionGrant::Granted,
            0 => PermissionGrant::Denied,
            _ => PermissionGrant::Unknown,
        }
    }

    pub(super) fn prompt_accessibility() {
        unsafe {
            paper_float_accessibility_prompt();
        }
    }

    #[cfg(not(feature = "local-api-key-file"))]
    pub(super) fn get_keychain_secret(
        service: &str,
        account: &str,
    ) -> Result<Option<String>, String> {
        let service = CString::new(service).map_err(|_| "钥匙串服务名无效。".to_string())?;
        let account = CString::new(account).map_err(|_| "钥匙串账户名无效。".to_string())?;
        let mut raw_value: *mut c_char = std::ptr::null_mut();
        let status =
            unsafe { paper_float_keychain_get(service.as_ptr(), account.as_ptr(), &mut raw_value) };
        let disposition = classify_keychain_read_status(status, !raw_value.is_null());
        if disposition.is_err() && !raw_value.is_null() {
            unsafe {
                paper_float_keychain_free(raw_value);
            }
        }
        match disposition? {
            KeychainReadDisposition::Missing => return Ok(None),
            KeychainReadDisposition::Present => {}
        }
        let value = unsafe { CStr::from_ptr(raw_value) }
            .to_str()
            .map(str::to_owned)
            .map_err(|_| "系统钥匙串中的 API Key 不是有效 UTF-8。".to_string());
        unsafe {
            paper_float_keychain_free(raw_value);
        }
        let value = value?;
        Ok((!value.is_empty()).then_some(value))
    }

    #[cfg(not(feature = "local-api-key-file"))]
    pub(super) fn keychain_secret_exists(service: &str, account: &str) -> Result<bool, String> {
        let service = CString::new(service).map_err(|_| "钥匙串服务名无效。".to_string())?;
        let account = CString::new(account).map_err(|_| "钥匙串账户名无效。".to_string())?;
        classify_keychain_presence_status(unsafe {
            paper_float_keychain_contains(service.as_ptr(), account.as_ptr())
        })
    }

    #[cfg(not(feature = "local-api-key-file"))]
    pub(super) fn set_keychain_secret(
        service: &str,
        account: &str,
        secret: &str,
    ) -> Result<(), String> {
        let service = CString::new(service).map_err(|_| "钥匙串服务名无效。".to_string())?;
        let account = CString::new(account).map_err(|_| "钥匙串账户名无效。".to_string())?;
        let secret = CString::new(secret).map_err(|_| "API Key 包含无效字符。".to_string())?;
        let status = unsafe {
            paper_float_keychain_set(service.as_ptr(), account.as_ptr(), secret.as_ptr())
        };
        if status == 0 {
            Ok(())
        } else {
            Err(format!("保存 API Key 到系统钥匙串失败（状态 {status}）。"))
        }
    }

    #[cfg(not(feature = "local-api-key-file"))]
    pub(super) fn delete_keychain_secret(service: &str, account: &str) -> Result<(), String> {
        let service = CString::new(service).map_err(|_| "钥匙串服务名无效。".to_string())?;
        let account = CString::new(account).map_err(|_| "钥匙串账户名无效。".to_string())?;
        let status = unsafe { paper_float_keychain_delete(service.as_ptr(), account.as_ptr()) };
        if status == 0 {
            Ok(())
        } else {
            Err(format!("从系统钥匙串清除 API Key 失败（状态 {status}）。"))
        }
    }

    pub(super) fn frontmost_external_pid() -> Option<i32> {
        let pid = unsafe { paper_float_frontmost_external_pid() };
        (pid > 0).then_some(pid)
    }

    pub(super) fn restore_application_focus(pid: i32) {
        if pid > 0 {
            unsafe {
                paper_float_restore_application_focus(pid);
            }
        }
    }

    pub(super) fn start(
        app: AppHandle,
        inner: Arc<Mutex<InnerState>>,
        readiness: Arc<WatcherReadiness>,
        generation: u64,
    ) {
        let selection_enabled = inner
            .lock()
            .ok()
            .is_some_and(|state| state.settings.enable_selection_popup)
            as c_int;
        let context = Box::new(NativeWatcherContext {
            app,
            inner,
            readiness,
            generation,
        });
        let context_ptr = Box::into_raw(context).cast::<c_void>();

        unsafe {
            paper_float_watchers_start(context_ptr, native_watcher_callback, selection_enabled);
        }
    }

    pub(super) fn stop() {
        unsafe {
            paper_float_watchers_stop();
        }
    }

    pub(super) fn multi_click_quiet_window_seconds() -> f64 {
        unsafe { paper_float_multi_click_quiet_window_seconds() }
    }

    #[cfg(feature = "acceptance-testing")]
    pub(super) fn inject_acceptance_mouse_tap_disabled() -> Result<(), String> {
        // Fixed, argument-free fault profile: mouse tap, timeout disable,
        // successful re-enable, and a ready AX fallback observer.
        if unsafe { paper_float_test_inject_tap_disabled(2, 1, 1, 1) } == 1 {
            Ok(())
        } else {
            Err("原生 selection watcher 未运行，tap-disabled 注入已拒绝。".to_owned())
        }
    }

    pub(super) fn resource_snapshot() -> Result<(u64, WatcherResourceCounts), String> {
        let mut snapshot = NativeResourceSnapshot::default();
        let read = unsafe { paper_float_native_resource_snapshot(&mut snapshot) };
        if read != 1 {
            return Err("读取原生监听资源快照失败。".to_owned());
        }
        let generation = u64::try_from(snapshot.watcher_lifecycle_token)
            .ok()
            .filter(|generation| *generation > 0)
            .ok_or_else(|| "原生监听资源快照缺少有效生命周期标识。".to_owned())?;
        Ok((
            generation,
            WatcherResourceCounts {
                snapshot_version: snapshot.version,
                selection_observers: snapshot.selection_observer_count,
                selection_observer_sources: snapshot.selection_observer_source_count,
                key_event_taps: snapshot.key_event_tap_count,
                key_event_tap_sources: snapshot.key_event_tap_source_count,
                mouse_event_taps: snapshot.mouse_event_tap_count,
                mouse_event_tap_sources: snapshot.mouse_event_tap_source_count,
                pasteboard_timers: snapshot.pasteboard_timer_count,
                workspace_activation_observers: snapshot.workspace_activation_observer_count,
                callbacks: snapshot.callback_count,
                contexts: snapshot.context_count,
                effective_source_sets: snapshot.effective_source_set_count,
                total_sources: snapshot.total_source_count,
            },
        ))
    }

    extern "C" fn native_watcher_callback(
        context: *mut c_void,
        event_type: c_int,
        text: *const c_char,
        x: c_double,
        y: c_double,
        delta: c_longlong,
        target_pid: c_int,
    ) {
        if event_type == EVENT_WATCHER_CONTEXT_RELEASE {
            unsafe {
                release_native_context_owner(
                    context.cast::<NativeWatcherContext>(),
                    delta,
                    |context, native_generation| {
                        observe_acceptance_watcher_release(&context.app, native_generation);
                    },
                );
            }
            return;
        }

        if context.is_null() {
            return;
        }

        let context = unsafe { &*(context.cast::<NativeWatcherContext>()) };
        if !context
            .readiness
            .is_current(context.generation)
            .unwrap_or(false)
        {
            return;
        }
        let text = c_string(text);
        let anchor = Point { x, y };

        match event_type {
            EVENT_PASTEBOARD_STATUS => {
                let handled = handle_double_copy_status(&context.inner, &text);
                if handled {
                    let _ = context
                        .readiness
                        .observe(context.generation, WatcherSource::Pasteboard);
                }
                ensure_selection_popup_keyboard_access(&context.app, &context.inner);
            }
            EVENT_SELECTION_STATUS => {
                let completes_readiness = selection_status_completes_watcher_readiness(&text);
                if let Ok(mut guard) = context.inner.lock() {
                    handle_selection_status_for_target(&mut guard, &text, target_pid);
                    drop(guard);
                    if completes_readiness {
                        let _ = context
                            .readiness
                            .observe(context.generation, WatcherSource::Selection);
                    }
                }
                observe_acceptance_native_status(
                    &context.app,
                    &text,
                    target_pid,
                    delta,
                    context.generation,
                );
            }
            EVENT_PASTEBOARD_CHANGE => handle_native_pasteboard_change(&context.inner, delta),
            EVENT_DOUBLE_COPY => handle_confirmed_double_copy(
                context.app.clone(),
                context.inner.clone(),
                text,
                anchor,
            ),
            EVENT_MOUSE_DOWN => hide_popup_if_unpinned(&context.app, &context.inner, Some(anchor)),
            EVENT_SELECTION => show_selection_popup(
                &context.app,
                &context.inner,
                text,
                anchor,
                delta,
                target_pid,
                SelectionTriggerKind::Automatic,
            ),
            EVENT_POPUP_ESCAPE => close_popup_from_global_escape(&context.app, &context.inner),
            EVENT_POPUP_FOCUS_REQUEST => {
                focus_selection_popup_from_global_tab(&context.app, &context.inner, delta < 0)
            }
            _ => {}
        }
    }

    fn c_string(value: *const c_char) -> String {
        if value.is_null() {
            return String::new();
        }

        unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned()
    }

    fn handle_native_pasteboard_change(inner: &Arc<Mutex<InnerState>>, delta: c_longlong) {
        if let Ok(mut guard) = inner.lock() {
            let current_code = guard.double_copy_status.code.clone();
            let ambiguous_fallback_delta = delta >= 2
                && current_code
                    .as_deref()
                    .is_some_and(is_double_copy_fallback_status);
            if ambiguous_fallback_delta {
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(
                        CapabilityHealth::Degraded,
                        "pasteboard_fallback_delta_ambiguous",
                    );
            } else if current_code
                .as_deref()
                .is_some_and(is_double_copy_fallback_status)
            {
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(
                        CapabilityHealth::Ready,
                        "pasteboard_fallback_change_observed",
                    );
            }
            guard.double_copy_status =
                double_copy_pasteboard_change_status(current_code.as_deref(), delta);
        }
    }
}

#[cfg(target_os = "macos")]
fn observe_acceptance_watcher_release(app: &AppHandle, native_generation: i64) {
    let state = app.state::<AppState>();
    let should_observe = match state.acceptance.lock() {
        Ok(acceptance) => acceptance
            .status()
            .armed
            .is_some_and(|armed| armed.scenario == AcceptanceScenario::WatcherRestart),
        Err(_) => return,
    };
    if !should_observe {
        return;
    }
    let Ok((generation, resources)) = macos_native::resource_snapshot() else {
        if let Ok(mut acceptance) = state.acceptance.lock() {
            let _ = acceptance.reject_native_observation();
        }
        return;
    };
    if u64::try_from(native_generation).ok() != Some(generation) {
        if let Ok(mut acceptance) = state.acceptance.lock() {
            let _ = acceptance.reject_native_observation();
        }
        return;
    }
    if let Ok(mut acceptance) = state.acceptance.lock() {
        if let Err(error) = acceptance.observe_watcher_resources(
            generation,
            AcceptanceLifecycleEvent::WatcherReleased,
            resources,
        ) {
            eprintln!("acceptance_diagnostics.watcher_release: {error}");
        }
    };
}

#[cfg(target_os = "macos")]
fn observe_acceptance_native_status(
    app: &AppHandle,
    status: &str,
    target_pid: i32,
    delta: i64,
    watcher_generation: u64,
) {
    let state = app.state::<AppState>();
    let observed_multi_click_quiet_window_ms =
        current_native_multi_click_quiet_window_ms().unwrap_or(0);
    let mut acceptance = match state.acceptance.lock() {
        Ok(acceptance) => acceptance,
        Err(_) => {
            eprintln!("acceptance_diagnostics.lock_poisoned");
            return;
        }
    };
    if let Err(error) =
        acceptance.validate_runtime_multi_click_quiet_window(observed_multi_click_quiet_window_ms)
    {
        eprintln!("acceptance_diagnostics.multi_click_window: {error}");
    }

    if status.starts_with("selection_read_") {
        match parse_selection_read_diagnostics(status) {
            Some(diagnostics) => {
                let complete_protocol = diagnostics
                    .generation
                    .zip(diagnostics.attempt)
                    .zip(diagnostics.trigger_to_read_ms)
                    .zip(diagnostics.terminal);
                let Some((((generation, attempt), trigger_to_read_ms), terminal)) =
                    complete_protocol
                else {
                    // Legacy seven-field diagnostics remain visible in settings, but
                    // cannot become acceptance evidence because they have no explicit
                    // native generation or terminal marker.
                    if let Err(error) = acceptance.reject_native_observation() {
                        eprintln!("acceptance_diagnostics.native_protocol_tail: {error}");
                    }
                    return;
                };
                if let Err(error) = acceptance.observe_native_read(NativeReadObservation {
                    status: &diagnostics.status,
                    generation,
                    attempt,
                    trigger_to_read_ms,
                    reason: &diagnostics.reason,
                    source_bundle_id: &diagnostics.source_bundle_id,
                    source_pid: target_pid,
                    found_text: diagnostics.found_text,
                    terminal,
                }) {
                    eprintln!("acceptance_diagnostics.native_read: {error}");
                }
            }
            None => {
                if let Err(error) = acceptance.reject_native_observation() {
                    eprintln!("acceptance_diagnostics.native_protocol: {error}");
                }
            }
        }
        return;
    }

    if status.starts_with("selection_mouse_tap_disabled_") {
        let elapsed_micros = u64::try_from(delta).ok().filter(|elapsed| *elapsed > 0);
        if let Err(error) =
            acceptance.observe_tap_status(status, watcher_generation, elapsed_micros)
        {
            eprintln!("acceptance_diagnostics.tap_status: {error}");
        }
    }
}

fn handle_confirmed_double_copy(
    app: AppHandle,
    inner: Arc<Mutex<InnerState>>,
    raw_text: String,
    anchor: Point,
) {
    let mut should_translate: Option<(String, String, TranslateMode, String)> = None;
    let mut should_show_empty = false;
    let finish_raw_text: String;
    {
        let mut guard = match inner.lock() {
            Ok(value) => value,
            Err(_) => return,
        };
        let (source_text, cleaned) = resolve_double_copy_source_text(&guard, &raw_text);
        let copied_at = Instant::now();
        accept_double_copy_trigger(
            &mut guard,
            source_text.clone(),
            cleaned,
            anchor,
            copied_at,
            &mut should_translate,
            &mut should_show_empty,
        );
        finish_raw_text = source_text;
    }
    finish_double_copy(
        app,
        inner,
        finish_raw_text,
        should_show_empty,
        should_translate,
    );
}

fn resolve_double_copy_source_text(inner: &InnerState, raw_text: &str) -> (String, String) {
    let cleaned = clean_selected_text(raw_text, inner.settings.clean_pdf_text);
    if !cleaned.is_empty() {
        return (raw_text.to_string(), cleaned);
    }

    if let Some(fallback) = selection_text_fallback_for_double_copy(inner) {
        let fallback_cleaned = clean_selected_text(&fallback, inner.settings.clean_pdf_text);
        if !fallback_cleaned.is_empty() {
            return (fallback, fallback_cleaned);
        }
    }

    (raw_text.to_string(), cleaned)
}

fn selection_text_fallback_for_double_copy(inner: &InnerState) -> Option<String> {
    if !inner.settings.enable_selection_popup
        || !inner.popup_controller.snapshot().visible
        || inner.popup_controller.snapshot().status != PopupStatus::SelectionReady
    {
        return None;
    }

    inner
        .active_selection_text
        .clone()
        .or_else(|| inner.popup_controller.snapshot().cleaned_text.clone())
        .or_else(|| inner.popup_controller.snapshot().selected_text.clone())
        .filter(|text| !text.trim().is_empty())
}

fn finish_double_copy(
    app: AppHandle,
    inner: Arc<Mutex<InnerState>>,
    raw_text: String,
    should_show_empty: bool,
    should_translate: Option<(String, String, TranslateMode, String)>,
) {
    if should_show_empty {
        if let Err(error) = show_popup(
            &app,
            &inner,
            PopupState {
                status: PopupStatus::Error,
                source_text: Some(raw_text),
                cleaned_text: Some(String::new()),
                error: Some(build_empty_clipboard_message()),
                error_kind: Some(PopupErrorKind::Selection),
                retryable: Some(false),
                recovery_action: Some(PopupRecoveryAction::Reselect),
                ..PopupState::default()
            },
        ) {
            eprintln!("{}", error.code());
        }
        return;
    }

    if let Some((raw, cleaned, mode, target_language)) = should_translate {
        let selection_revision = match show_selection_pending(&app, &inner, raw, cleaned.clone()) {
            Ok(Some(revision)) => revision,
            Ok(None) => return,
            Err(error) => {
                eprintln!("{}", error.code());
                return;
            }
        };
        let app_clone = app.clone();
        let inner_clone = inner.clone();
        let data_dir = app.state::<AppState>().data_dir.clone();
        let http = app.state::<AppState>().http.clone();
        tauri::async_runtime::spawn(async move {
            if translate_text(
                app_clone,
                TranslateContext {
                    inner: inner_clone,
                    data_dir,
                    http,
                },
                TranslateTask {
                    cleaned_text: cleaned,
                    bypass_cache: false,
                    mode,
                    target_language,
                    selection_revision,
                },
            )
            .await
            .is_err()
            {
                eprintln!("popup_control.background_translation_failed");
            }
        });
    }
}

fn handle_double_copy_status(inner: &Arc<Mutex<InnerState>>, status: &str) -> bool {
    if let Ok(mut guard) = inner.lock() {
        let status_code = status.split('\t').next().unwrap_or(status);
        match status_code {
            "ready" => {
                guard.capability_snapshot.key_tap =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status_code);
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(
                        CapabilityHealth::Disabled,
                        "inactive_key_tap_ready",
                    );
                guard.double_copy_status =
                    WatcherStatus::with_code(true, true, "Cmd+C+C 双复制按键监听已启用。", "ready");
            }
            "pasteboard_fallback_ready" | "key_tap_unavailable_fallback_ready" => {
                guard.capability_snapshot.key_tap =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status_code);
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status_code);
                guard.double_copy_status = WatcherStatus::with_code(
                    true,
                    true,
                    "Cmd+C+C 剪贴板备用监听已启用；即使输入监听未授权，也可快速复制同一段文本两次触发翻译。",
                    status_code,
                );
            }
            "key_input_monitoring_unavailable" | "key_tap_unavailable" => {
                guard.capability_snapshot.key_tap =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status_code);
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status_code);
                guard.double_copy_status = WatcherStatus::with_code(
                    true,
                    true,
                    "Cmd+C+C 按键监听不可用，已启用剪贴板双复制回退。",
                    status_code,
                );
            }
            "pasteboard_fallback_delta_ambiguous" => {
                let delta = status
                    .split('\t')
                    .nth(1)
                    .and_then(|value| value.parse::<i64>().ok());
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, status_code);
                guard.double_copy_status = WatcherStatus::with_code(
                    true,
                    true,
                    delta.map_or_else(
                        || {
                            "剪贴板版本一次跨过多步，无法确认双复制；请重新按两次 Cmd+C。"
                                .to_string()
                        },
                        |value| {
                            format!(
                                "剪贴板版本一次变化 +{value}，无法确认双复制；请重新按两次 Cmd+C。"
                            )
                        },
                    ),
                    status_code,
                );
            }
            _ if parse_disabled_tap_outcome(status_code, "key_tap_disabled_")
                == Some(TapDisabledOutcome::Recovered) =>
            {
                guard.capability_snapshot.key_tap =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status_code);
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(
                        CapabilityHealth::Disabled,
                        "inactive_key_tap_recovered",
                    );
                guard.double_copy_status = WatcherStatus::with_code(
                    true,
                    true,
                    "Cmd+C+C 按键监听曾被系统禁用，现已自动恢复。",
                    status_code,
                );
            }
            _ if parse_disabled_tap_outcome(status_code, "key_tap_disabled_")
                == Some(TapDisabledOutcome::Fallback) =>
            {
                guard.capability_snapshot.key_tap =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Disabled, status_code);
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status_code);
                guard.double_copy_status = WatcherStatus::with_code(
                    true,
                    true,
                    "Cmd+C+C 按键监听被系统禁用，已切换到剪贴板双复制回退。",
                    status_code,
                );
            }
            _ => {
                guard.capability_snapshot.key_tap =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Unknown, status_code);
                guard.capability_snapshot.clipboard_double_copy_fallback =
                    RuntimeCapabilityState::with_status(CapabilityHealth::Unknown, status_code);
                guard.double_copy_status = WatcherStatus::with_code(
                    false,
                    false,
                    format!("收到无法识别的原生 Cmd+C+C 状态：{status_code}"),
                    "unknown_native_status",
                );
            }
        }
        true
    } else {
        false
    }
}

fn double_copy_pasteboard_change_status(current_code: Option<&str>, delta: i64) -> WatcherStatus {
    if delta >= 2 && current_code.is_some_and(is_double_copy_fallback_status) {
        return WatcherStatus::with_code(
            true,
            true,
            format!("剪贴板版本一次变化 +{delta}，无法确认双复制；请重新按两次 Cmd+C。"),
            "pasteboard_fallback_delta_ambiguous",
        );
    }

    if current_code.is_some_and(is_double_copy_fallback_status) {
        return WatcherStatus::with_code(
            true,
            true,
            format!("Cmd+C+C 剪贴板备用监听已启用；最近检测到剪贴板变更 +{delta}。"),
            current_code.unwrap_or("pasteboard_fallback_ready"),
        );
    }

    WatcherStatus::with_code(
        true,
        true,
        format!("Cmd+C+C 监听已启用；最近检测到剪贴板变更 +{delta}。"),
        "ready",
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TapDisabledOutcome {
    Recovered,
    Fallback,
    FallbackAx,
    Unavailable,
}

fn parse_disabled_tap_outcome(status: &str, prefix: &str) -> Option<TapDisabledOutcome> {
    let remainder = status.strip_prefix(prefix)?;
    if !remainder.starts_with("timeout_") && !remainder.starts_with("user_input_") {
        return None;
    }
    if remainder.ends_with("_recovered") {
        Some(TapDisabledOutcome::Recovered)
    } else if remainder.ends_with("_fallback_ax") {
        Some(TapDisabledOutcome::FallbackAx)
    } else if remainder.ends_with("_fallback") {
        Some(TapDisabledOutcome::Fallback)
    } else if remainder.ends_with("_unavailable") {
        Some(TapDisabledOutcome::Unavailable)
    } else {
        None
    }
}

fn is_double_copy_fallback_status(status: &str) -> bool {
    status == "pasteboard_fallback_ready"
        || status == "key_tap_unavailable_fallback_ready"
        || status == "pasteboard_fallback_delta_ambiguous"
        || parse_disabled_tap_outcome(status, "key_tap_disabled_")
            == Some(TapDisabledOutcome::Fallback)
}

fn accept_double_copy_trigger(
    inner: &mut InnerState,
    raw_text: String,
    cleaned: String,
    anchor: Point,
    copied_at: Instant,
    should_translate: &mut Option<(String, String, TranslateMode, String)>,
    should_show_empty: &mut bool,
) {
    inner.last_cursor_point = Some(anchor);
    if cleaned.is_empty() {
        *should_show_empty = true;
    } else if !was_recently_triggered(inner, &cleaned, copied_at) {
        inner.last_double_copy_trigger = Some(TriggerRecord {
            text: cleaned.clone(),
            triggered_at: copied_at,
        });
        inner.last_cleaned_text = cleaned.clone();
        inner.active_selection_text = None;
        *should_translate = Some((
            raw_text,
            cleaned,
            inner.settings.mode.clone(),
            normalize_target_language(&inner.settings.target_language, DEFAULT_TARGET_LANGUAGE),
        ));
    }
}

fn show_selection_popup(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    raw_text: String,
    anchor: Point,
    native_generation: i64,
    source_pid: i32,
    trigger: SelectionTriggerKind,
) {
    let acceptance_text = raw_text.clone();
    let prepared = {
        let mut guard = match inner.lock() {
            Ok(value) => value,
            Err(_) => return,
        };
        prepare_selection_popup_locked(
            &mut guard,
            raw_text,
            anchor,
            native_generation,
            source_pid,
            trigger,
        )
    };
    match prepared {
        Ok(Some(state)) => {
            let rapid_probe_epoch = observe_acceptance_popup_commit(
                app,
                native_generation,
                source_pid,
                &acceptance_text,
                &state,
                anchor,
            );
            show_committed_popup(app, inner, &state);
            if let Some(epoch) = rapid_probe_epoch {
                start_acceptance_rapid_probe(app, inner, state.selection_revision, epoch);
            }
        }
        Ok(None) => observe_acceptance_popup_failure(
            app,
            native_generation,
            source_pid,
            Some(&acceptance_text),
            anchor,
            AcceptanceSelectionOutcome::NoSelection,
        ),
        Err(error) => {
            eprintln!("{}", error.code());
            observe_acceptance_popup_failure(
                app,
                native_generation,
                source_pid,
                Some(&acceptance_text),
                anchor,
                AcceptanceSelectionOutcome::StaleWriteRejected,
            );
        }
    }
}

fn observe_acceptance_popup_commit(
    app: &AppHandle,
    native_generation: i64,
    source_pid: i32,
    selected_text: &str,
    snapshot: &PopupState,
    anchor: Point,
) -> Option<u64> {
    let Ok(generation) = u64::try_from(native_generation) else {
        observe_acceptance_invalid_popup(app);
        return None;
    };
    let state = app.state::<AppState>();
    let mut acceptance = match state.acceptance.lock() {
        Ok(acceptance) => acceptance,
        Err(_) => {
            eprintln!("acceptance_diagnostics.lock_poisoned");
            return None;
        }
    };
    let disposition = match acceptance.record_popup_commit(
        generation,
        selected_text,
        source_pid,
        snapshot.selection_revision,
        snapshot.revision,
        MouseUpAnchor {
            x: anchor.x,
            y: anchor.y,
        },
    ) {
        Ok(disposition) => disposition,
        Err(error) => {
            eprintln!("acceptance_diagnostics.popup_commit: {error}");
            return None;
        }
    };
    if disposition != HookDisposition::Recorded {
        return None;
    }
    match acceptance.claim_rapid_probe(snapshot.selection_revision) {
        Ok(true) => Some(state.acceptance_probe_epoch.load(Ordering::SeqCst)),
        Ok(false) => None,
        Err(error) => {
            eprintln!("acceptance_diagnostics.rapid_probe_claim: {error}");
            None
        }
    }
}

fn start_acceptance_rapid_probe(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    selection_revision: u64,
    epoch: u64,
) {
    let app_state = app.state::<AppState>();
    if app_state.acceptance_probe_epoch.load(Ordering::SeqCst) != epoch {
        return;
    }
    let outcome = {
        let mut inner = match inner.lock() {
            Ok(inner) => inner,
            Err(_) => {
                reject_acceptance_rapid_probe(app, selection_revision);
                return;
            }
        };
        if app_state.acceptance_probe_epoch.load(Ordering::SeqCst) != epoch
            || inner.acceptance_rapid_probe.is_some()
        {
            drop(inner);
            reject_acceptance_rapid_probe(app, selection_revision);
            return;
        }
        let snapshot = inner.popup_controller.snapshot();
        let cleaned_text = if snapshot.selection_revision == selection_revision
            && snapshot.status == PopupStatus::SelectionReady
        {
            snapshot.cleaned_text.clone()
        } else {
            None
        };
        let Some(cleaned_text) = cleaned_text else {
            drop(inner);
            reject_acceptance_rapid_probe(app, selection_revision);
            return;
        };
        let outcome = inner.popup_controller.begin_translation(
            selection_revision,
            &cleaned_text,
            "acceptance-probe",
        );
        if let Ok(BeginTranslation::Started { guard, .. }) = &outcome {
            inner.acceptance_rapid_probe = Some(AcceptanceRapidProbe {
                epoch,
                guard: *guard,
            });
            inner.last_acceptance_probe_commit = None;
            clear_active_selection(&mut inner);
        }
        outcome
    };
    let BeginTranslation::Started { guard, snapshot } = (match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("{}", error.code());
            reject_acceptance_rapid_probe(app, selection_revision);
            return;
        }
    }) else {
        reject_acceptance_rapid_probe(app, selection_revision);
        return;
    };
    show_committed_popup(app, inner, &snapshot);

    let probe_app = app.clone();
    let probe_inner = Arc::clone(inner);
    let spawn_result = std::thread::Builder::new()
        .name("acceptance-rapid-probe".to_owned())
        .spawn(move || {
            std::thread::sleep(Duration::from_millis(RAPID_PROBE_DELAY_MS));
            update_acceptance_rapid_probe(
                &probe_app,
                &probe_inner,
                epoch,
                guard,
                PopupState {
                    status: PopupStatus::Translated,
                    translation: Some("Acceptance stale-write probe".to_owned()),
                    cached: Some(false),
                    target_language: Some("acceptance-probe".to_owned()),
                    ..PopupState::default()
                },
            );
        });
    if let Err(error) = spawn_result {
        eprintln!("acceptance_diagnostics.rapid_probe_spawn: {error}");
        let hidden_snapshot = inner
            .lock()
            .ok()
            .and_then(|mut inner| cancel_acceptance_rapid_probe_locked(&mut inner).ok())
            .flatten();
        if let Some(snapshot) = hidden_snapshot {
            hide_committed_popup(app, inner, &snapshot, PopupCloseReason::SelectionHandled);
        }
        reject_acceptance_rapid_probe(app, selection_revision);
    }
}

fn update_acceptance_rapid_probe(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    epoch: u64,
    guard: TranslationGuard,
    state: PopupState,
) -> bool {
    let app_state = app.state::<AppState>();
    let (outcome, controller_snapshot) = {
        let mut inner = match inner.lock() {
            Ok(inner) => inner,
            Err(_) => return false,
        };
        if app_state.acceptance_probe_epoch.load(Ordering::SeqCst) != epoch
            || inner.acceptance_rapid_probe != Some(AcceptanceRapidProbe { epoch, guard })
        {
            return false;
        }
        inner.acceptance_rapid_probe = None;
        let outcome = inner.popup_controller.commit_translation(guard, state);
        let snapshot = inner.popup_controller.snapshot().clone();
        if matches!(outcome, Ok(TranslationCommit::Applied(_))) {
            inner.last_acceptance_probe_commit = Some((epoch, snapshot.revision));
        }
        (outcome, snapshot)
    };
    let state = match outcome {
        Ok(TranslationCommit::Applied(state)) => {
            observe_acceptance_translation_commit(
                app,
                guard,
                TranslationCommitObservation::Applied,
                &state,
            );
            state
        }
        Ok(TranslationCommit::StaleGuard) => {
            observe_acceptance_translation_commit(
                app,
                guard,
                TranslationCommitObservation::StaleGuard,
                &controller_snapshot,
            );
            return false;
        }
        Err(error) => {
            eprintln!("{}", error.code());
            reject_acceptance_rapid_probe(app, guard.selection_revision);
            return false;
        }
    };
    if app_state.acceptance_probe_epoch.load(Ordering::SeqCst) != epoch {
        return false;
    }
    send_popup_state(app, &state);
    true
}

fn reject_acceptance_rapid_probe(app: &AppHandle, selection_revision: u64) {
    let state = app.state::<AppState>();
    let mut acceptance = match state.acceptance.lock() {
        Ok(acceptance) => acceptance,
        Err(_) => {
            eprintln!("acceptance_diagnostics.lock_poisoned");
            return;
        }
    };
    if let Err(error) = acceptance.reject_rapid_probe(selection_revision) {
        eprintln!("acceptance_diagnostics.rapid_probe_rejected: {error}");
    }
}

fn observe_acceptance_popup_failure(
    app: &AppHandle,
    native_generation: i64,
    source_pid: i32,
    selected_text: Option<&str>,
    anchor: Point,
    outcome: AcceptanceSelectionOutcome,
) {
    let Ok(generation) = u64::try_from(native_generation) else {
        observe_acceptance_invalid_popup(app);
        return;
    };
    let state = app.state::<AppState>();
    let mut acceptance = match state.acceptance.lock() {
        Ok(acceptance) => acceptance,
        Err(_) => {
            eprintln!("acceptance_diagnostics.lock_poisoned");
            return;
        }
    };
    if let Err(error) = acceptance.record_popup_failure(
        generation,
        selected_text,
        source_pid,
        Some(MouseUpAnchor {
            x: anchor.x,
            y: anchor.y,
        }),
        outcome,
    ) {
        eprintln!("acceptance_diagnostics.popup_failure: {error}");
    }
}

fn observe_acceptance_invalid_popup(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(mut acceptance) = state.acceptance.lock() {
        if let Err(error) = acceptance.reject_native_observation() {
            eprintln!("acceptance_diagnostics.popup_generation: {error}");
        }
    };
}

fn prepare_selection_popup_locked(
    inner: &mut InnerState,
    raw_text: String,
    anchor: Point,
    native_generation: i64,
    source_pid: i32,
    trigger: SelectionTriggerKind,
) -> Result<Option<PopupState>, PopupControlError> {
    if !inner.settings.enable_selection_popup {
        inner
            .popup_controller
            .cancel_translation_for_native_generation(native_generation)?;
        clear_active_selection(inner);
        return Ok(None);
    }
    let cleaned = clean_selected_text(&raw_text, inner.settings.clean_pdf_text);
    if cleaned.is_empty() {
        if trigger == SelectionTriggerKind::Automatic {
            inner.dismissed_automatic_selection = None;
        }
        inner
            .popup_controller
            .accept_native_generation(native_generation)?;
        return Ok(None);
    }
    let identity = SelectionIdentity::new(source_pid, &cleaned);
    if trigger == SelectionTriggerKind::Automatic
        && inner.dismissed_automatic_selection.as_ref() == Some(&identity)
    {
        inner
            .popup_controller
            .accept_native_generation(native_generation)?;
        return Ok(None);
    }
    inner.dismissed_automatic_selection = None;
    let next = PopupState {
        status: PopupStatus::SelectionReady,
        source_text: Some(raw_text),
        selected_text: Some(cleaned.clone()),
        cleaned_text: Some(cleaned.clone()),
        pinned: false,
        ..PopupState::default()
    };
    let state = inner
        .popup_controller
        .commit_native_selection(native_generation, next)?;
    let selection_revision = state.selection_revision;
    inner.last_cleaned_text = cleaned.clone();
    inner.active_selection_text = Some(cleaned);
    inner.active_popup_selection = Some(ActivePopupSelection {
        identity,
        selection_revision,
    });
    inner.last_cursor_point = Some(anchor);
    inner.popup_focus_return_target = (source_pid > 0).then_some(PopupFocusReturnTarget {
        selection_revision,
        pid: source_pid,
    });
    Ok(Some(state))
}

fn show_selection_pending(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    raw_text: String,
    cleaned_text: String,
) -> Result<Option<u64>, PopupControlError> {
    let state = {
        let mut guard = match inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                eprintln!("popup_control.lock_poisoned");
                return Ok(None);
            }
        };
        let next = PopupState {
            status: PopupStatus::SelectionPending,
            source_text: Some(raw_text),
            selected_text: Some(cleaned_text.clone()),
            cleaned_text: Some(cleaned_text.clone()),
            pinned: false,
            ..PopupState::default()
        };
        let state = guard.popup_controller.commit_new_selection(next)?;
        guard.last_cleaned_text = cleaned_text;
        guard.active_selection_text = None;
        guard.active_popup_selection = None;
        guard.dismissed_automatic_selection = None;
        guard.popup_focus_return_target = None;
        state
    };
    let selection_revision = state.selection_revision;
    show_committed_popup(app, inner, &state);
    Ok(Some(selection_revision))
}

fn show_popup(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    state: PopupState,
) -> Result<(), PopupControlError> {
    let state = {
        let mut guard = match inner.lock() {
            Ok(value) => value,
            Err(_) => {
                eprintln!("popup_control.lock_poisoned");
                return Ok(());
            }
        };
        let state = guard.popup_controller.commit_new_selection(state)?;
        guard.popup_focus_return_target = None;
        guard.active_popup_selection = None;
        clear_active_selection(&mut guard);
        state
    };
    show_committed_popup(app, inner, &state);
    Ok(())
}

fn show_committed_popup(app: &AppHandle, inner: &Arc<Mutex<InnerState>>, state: &PopupState) {
    let should_focus = inner
        .lock()
        .map(|current| should_focus_popup_on_show(state.status, &current.capability_snapshot))
        .unwrap_or(true);
    send_popup_state(app, state);
    if !popup_snapshot_is_current(inner, state) {
        return;
    }
    mark_popup_needs_initial_position(inner);
    resize_popup_for_state(app, inner, state);
    if !position_popup(app, inner) {
        return;
    }
    mark_popup_positioned(inner);
    let Ok(current) = inner.lock() else {
        return;
    };
    if current.popup_controller.snapshot().revision != state.revision
        || !current.popup_controller.snapshot().visible
    {
        return;
    }
    drop(current);
    remember_current_external_focus_target(inner, state);
    if let Some(window) = app.get_webview_window("popup") {
        #[cfg(target_os = "windows")]
        {
            let _ = window.set_focusable(!state.status.is_selection_prompt());
        }
        let _ = window.show();
        if should_focus {
            if state.status.is_selection_prompt() {
                focus_selection_popup_for_keyboard_entry(
                    app,
                    inner,
                    state,
                    PopupFocusDirection::Forward,
                );
            } else {
                let _ = window.set_focus();
            }
        }
    }
}

fn should_focus_popup_on_show(status: PopupStatus, capabilities: &CapabilitySnapshot) -> bool {
    !status.is_selection_prompt() || capabilities.key_tap.health != CapabilityHealth::Ready
}

fn remember_current_external_focus_target(
    inner: &Arc<Mutex<InnerState>>,
    expected_state: &PopupState,
) {
    let Some(pid) = current_external_focus_pid() else {
        return;
    };
    let Ok(mut current) = inner.lock() else {
        return;
    };
    if current.popup_controller.snapshot().revision != expected_state.revision
        || !current.popup_controller.snapshot().visible
        || current.popup_controller.snapshot().selection_revision
            != expected_state.selection_revision
    {
        return;
    }
    if current
        .popup_focus_return_target
        .is_some_and(|target| target.selection_revision == expected_state.selection_revision)
    {
        return;
    }
    current.popup_focus_return_target = Some(PopupFocusReturnTarget {
        selection_revision: expected_state.selection_revision,
        pid,
    });
}

fn focus_selection_popup_for_keyboard_entry(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    expected_state: &PopupState,
    direction: PopupFocusDirection,
) {
    let is_current_selection = inner.lock().ok().is_some_and(|current| {
        current.popup_controller.snapshot().revision == expected_state.revision
            && current.popup_controller.snapshot().visible
            && current
                .popup_controller
                .snapshot()
                .status
                .is_selection_prompt()
    });
    if !is_current_selection {
        return;
    }

    remember_current_external_focus_target(inner, expected_state);
    let Some(window) = app.get_webview_window("popup") else {
        return;
    };
    if window.is_focused().unwrap_or(false) {
        return;
    }
    if window.set_focus().is_err() || !popup_snapshot_is_current(inner, expected_state) {
        return;
    }
    let _ = app.emit_to(
        "popup",
        "popup-keyboard-entry",
        PopupKeyboardEntry {
            revision: expected_state.revision,
            selection_revision: expected_state.selection_revision,
            direction,
        },
    );
}

fn popup_snapshot_is_current(inner: &Arc<Mutex<InnerState>>, state: &PopupState) -> bool {
    inner.lock().ok().is_some_and(|inner| {
        inner.popup_controller.snapshot().revision == state.revision
            && inner.popup_controller.snapshot().visible
    })
}

fn begin_translation(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    selection_revision: u64,
    cleaned_text: &str,
    target_language: &str,
) -> Result<Option<TranslationGuard>, String> {
    let outcome = {
        let mut inner = inner.lock().map_err(lock_error)?;
        let outcome = inner
            .popup_controller
            .begin_translation(selection_revision, cleaned_text, target_language)
            .map_err(popup_control_error)?;
        if matches!(outcome, BeginTranslation::Started { .. }) {
            clear_active_selection(&mut inner);
        }
        outcome
    };
    match outcome {
        BeginTranslation::Started { guard, snapshot } => {
            show_committed_popup(app, inner, &snapshot);
            Ok(Some(guard))
        }
        BeginTranslation::StaleSelection => Ok(None),
    }
}

fn update_popup_state_for_translation(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    translation_guard: TranslationGuard,
    state: PopupState,
) -> bool {
    let (outcome, controller_snapshot) = {
        let mut inner = match inner.lock() {
            Ok(value) => value,
            Err(_) => return false,
        };
        let outcome = inner
            .popup_controller
            .commit_translation(translation_guard, state);
        (outcome, inner.popup_controller.snapshot().clone())
    };
    let state = match outcome {
        Ok(TranslationCommit::Applied(state)) => {
            observe_acceptance_translation_commit(
                app,
                translation_guard,
                TranslationCommitObservation::Applied,
                &state,
            );
            state
        }
        Ok(TranslationCommit::StaleGuard) => {
            observe_acceptance_translation_commit(
                app,
                translation_guard,
                TranslationCommitObservation::StaleGuard,
                &controller_snapshot,
            );
            return false;
        }
        Err(error) => {
            eprintln!("{}", error.code());
            return false;
        }
    };
    send_popup_state(app, &state);
    true
}

fn observe_acceptance_translation_commit(
    app: &AppHandle,
    guard: TranslationGuard,
    observation: TranslationCommitObservation,
    controller_snapshot: &PopupState,
) {
    let state = app.state::<AppState>();
    let mut acceptance = match state.acceptance.lock() {
        Ok(acceptance) => acceptance,
        Err(_) => {
            eprintln!("acceptance_diagnostics.lock_poisoned");
            return;
        }
    };
    if let Err(error) = acceptance.observe_translation_commit(
        guard.selection_revision,
        observation,
        controller_snapshot.selection_revision,
        controller_snapshot.revision,
    ) {
        eprintln!("acceptance_diagnostics.translation_commit: {error}");
    }
}

fn translation_guard_is_current(
    inner: &Arc<Mutex<InnerState>>,
    translation_guard: TranslationGuard,
) -> bool {
    inner.lock().ok().is_some_and(|inner| {
        inner
            .popup_controller
            .translation_is_current(translation_guard)
    })
}

fn send_popup_state(app: &AppHandle, state: &PopupState) {
    let _ = app.emit_to("popup", "popup-state", state);
}

fn resize_popup_for_state(app: &AppHandle, inner: &Arc<Mutex<InnerState>>, state: &PopupState) {
    if let Some(window) = app.get_webview_window("popup") {
        if state.status.is_selection_prompt() {
            let _ = window.set_size(LogicalSize::new(
                SELECTION_POPUP_WIDTH,
                SELECTION_POPUP_HEIGHT,
            ));
        } else {
            resize_existing_popup_to_settings(app, inner);
        }
    }
}

fn resize_existing_popup_to_settings(app: &AppHandle, inner: &Arc<Mutex<InnerState>>) {
    let width = inner
        .lock()
        .ok()
        .map(|guard| guard.settings.popup_width as f64)
        .unwrap_or(420.0);
    if let Some(window) = app.get_webview_window("popup") {
        let _ = window.set_size(LogicalSize::new(width, POPUP_DEFAULT_HEIGHT));
    }
}

fn resize_popup_to_content(app: &AppHandle, inner: &Arc<Mutex<InnerState>>, requested_height: f64) {
    let (status, width, should_position) = inner
        .lock()
        .ok()
        .map(|guard| {
            (
                guard.popup_controller.snapshot().status,
                guard.settings.popup_width as f64,
                should_position_popup_on_resize(&guard),
            )
        })
        .unwrap_or((PopupStatus::Hidden, 420.0, false));

    if let Some(window) = app.get_webview_window("popup") {
        if status.is_selection_prompt() {
            let _ = window.set_size(LogicalSize::new(
                SELECTION_POPUP_WIDTH,
                SELECTION_POPUP_HEIGHT,
            ));
        } else {
            let height = requested_height.round().clamp(180.0, 520.0);
            let _ = window.set_size(LogicalSize::new(width, height));
        }
        if should_position && position_popup(app, inner) {
            mark_popup_positioned(inner);
        }
    }
}

fn mark_popup_needs_initial_position(inner: &Arc<Mutex<InnerState>>) {
    if let Ok(mut guard) = inner.lock() {
        guard.popup_positioned = false;
        guard.popup_dragging = false;
    }
}

fn mark_popup_positioned(inner: &Arc<Mutex<InnerState>>) {
    if let Ok(mut guard) = inner.lock() {
        guard.popup_positioned = true;
    }
}

fn should_position_popup_on_resize(inner: &InnerState) -> bool {
    inner.last_cursor_point.is_some() && !inner.popup_positioned
}

fn position_popup(app: &AppHandle, inner: &Arc<Mutex<InnerState>>) -> bool {
    let positioned = try_position_popup(app, inner);
    if !positioned {
        hide_popup(app);
    }
    positioned
}

fn try_position_popup(app: &AppHandle, inner: &Arc<Mutex<InnerState>>) -> bool {
    let Some(anchor) = inner.lock().ok().and_then(|guard| guard.last_cursor_point) else {
        return false;
    };
    let Some(window) = app.get_webview_window("popup") else {
        return false;
    };
    let logical_anchor = LogicalPoint {
        x: anchor.x,
        y: anchor.y,
    };
    let Some(monitor) = monitor_for_logical_point(&window, logical_anchor) else {
        return false;
    };
    let Some(monitor) = monitor_geometry(&monitor, logical_anchor) else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };
    let Ok(position) = place_popup(PopupPlacement {
        anchor: logical_anchor,
        monitor,
        popup_size: GeometryPhysicalSize {
            width: size.width,
            height: size.height,
        },
        offset_logical: POPUP_OFFSET,
        edge_padding_logical: POPUP_HIT_TEST_PADDING,
    }) else {
        return false;
    };
    window
        .set_position(PhysicalPosition::new(position.x, position.y))
        .is_ok()
}

fn monitor_for_logical_point(
    window: &WebviewWindow,
    point: LogicalPoint,
) -> Option<tauri::Monitor> {
    #[cfg(target_os = "windows")]
    {
        window
            .available_monitors()
            .ok()?
            .into_iter()
            .find(|monitor| {
                let scale_factor = monitor.scale_factor();
                logical_point_is_inside_monitor(
                    point,
                    LogicalPoint {
                        x: monitor.position().x as f64 / scale_factor,
                        y: monitor.position().y as f64 / scale_factor,
                    },
                    GeometryPhysicalSize {
                        width: monitor.size().width,
                        height: monitor.size().height,
                    },
                    scale_factor,
                )
            })
    }

    #[cfg(not(target_os = "windows"))]
    {
        window.monitor_from_point(point.x, point.y).ok().flatten()
    }
}

fn monitor_geometry(
    monitor: &tauri::Monitor,
    native_anchor: LogicalPoint,
) -> Option<MonitorGeometry> {
    let scale_factor = monitor.scale_factor();
    if !scale_factor.is_finite() || scale_factor <= 0.0 {
        return None;
    }
    let position = monitor.position();
    let size = monitor.size();
    let logical_origin = LogicalPoint {
        x: position.x as f64 / scale_factor,
        y: position.y as f64 / scale_factor,
    };
    let physical_origin = PhysicalPoint {
        x: position.x,
        y: position.y,
    };
    let physical_size = GeometryPhysicalSize {
        width: size.width,
        height: size.height,
    };
    if !logical_point_is_inside_monitor(native_anchor, logical_origin, physical_size, scale_factor)
    {
        return None;
    }

    let physical_work_area = {
        #[cfg(target_os = "macos")]
        {
            let logical_work_area = native_work_area::visible_work_area_for_anchor(native_anchor)?;
            logical_work_area_to_physical(
                logical_origin,
                physical_origin,
                physical_size,
                scale_factor,
                logical_work_area,
            )
            .ok()?
        }

        #[cfg(not(target_os = "macos"))]
        {
            let work_area = monitor.work_area();
            PhysicalRect {
                origin: PhysicalPoint {
                    x: work_area.position.x,
                    y: work_area.position.y,
                },
                size: GeometryPhysicalSize {
                    width: work_area.size.width,
                    height: work_area.size.height,
                },
            }
        }
    };

    Some(MonitorGeometry {
        logical_origin,
        physical_origin,
        physical_size,
        physical_work_area,
        scale_factor,
    })
}

fn logical_point_is_inside_monitor(
    point: LogicalPoint,
    logical_origin: LogicalPoint,
    physical_size: GeometryPhysicalSize,
    scale_factor: f64,
) -> bool {
    if !point.x.is_finite()
        || !point.y.is_finite()
        || !logical_origin.x.is_finite()
        || !logical_origin.y.is_finite()
        || !scale_factor.is_finite()
        || scale_factor <= 0.0
        || physical_size.width == 0
        || physical_size.height == 0
    {
        return false;
    }
    let logical_width = physical_size.width as f64 / scale_factor;
    let logical_height = physical_size.height as f64 / scale_factor;
    point.x >= logical_origin.x
        && point.x < logical_origin.x + logical_width
        && point.y >= logical_origin.y
        && point.y < logical_origin.y + logical_height
}

fn monitor_logical_center(monitor: &tauri::Monitor) -> Option<LogicalPoint> {
    let position = monitor.position();
    let size = monitor.size();
    logical_monitor_center_from_geometry(
        PhysicalPoint {
            x: position.x,
            y: position.y,
        },
        GeometryPhysicalSize {
            width: size.width,
            height: size.height,
        },
        monitor.scale_factor(),
    )
}

fn logical_monitor_center_from_geometry(
    physical_origin: PhysicalPoint,
    physical_size: GeometryPhysicalSize,
    scale_factor: f64,
) -> Option<LogicalPoint> {
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || physical_size.width == 0
        || physical_size.height == 0
    {
        return None;
    }
    let center = LogicalPoint {
        x: physical_origin.x as f64 / scale_factor
            + physical_size.width as f64 / scale_factor / 2.0,
        y: physical_origin.y as f64 / scale_factor
            + physical_size.height as f64 / scale_factor / 2.0,
    };
    (center.x.is_finite() && center.y.is_finite()).then_some(center)
}

fn hide_popup_if_unpinned(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    mouse_point: Option<Point>,
) {
    let is_inside_popup = mouse_point.is_some_and(|point| is_point_inside_popup(app, point));
    let hidden_snapshot = {
        let mut guard = match inner.lock() {
            Ok(value) => value,
            Err(_) => return,
        };
        if !should_hide_popup_for_mouse_down(&guard, is_inside_popup) {
            return;
        }
        match commit_popup_close(&mut guard, PopupCloseReason::OutsideClick) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("{}", error.code());
                return;
            }
        }
    };
    hide_committed_popup(app, inner, &hidden_snapshot, PopupCloseReason::OutsideClick);
}

fn close_popup_from_global_escape(app: &AppHandle, inner: &Arc<Mutex<InnerState>>) {
    let hidden_snapshot = {
        let mut guard = match inner.lock() {
            Ok(value) => value,
            Err(_) => return,
        };
        if !should_close_popup_from_global_escape(&guard) {
            return;
        }
        match commit_popup_close(&mut guard, PopupCloseReason::ExplicitClose) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("{}", error.code());
                return;
            }
        }
    };
    hide_committed_popup(
        app,
        inner,
        &hidden_snapshot,
        PopupCloseReason::ExplicitClose,
    );
}

fn focus_selection_popup_from_global_tab(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    reverse: bool,
) {
    let state = inner.lock().ok().and_then(|guard| {
        should_focus_popup_from_global_tab(&guard)
            .then(|| guard.popup_controller.snapshot().clone())
    });
    let Some(state) = state else {
        return;
    };
    focus_selection_popup_for_keyboard_entry(
        app,
        inner,
        &state,
        if reverse {
            PopupFocusDirection::Backward
        } else {
            PopupFocusDirection::Forward
        },
    );
}

fn ensure_selection_popup_keyboard_access(app: &AppHandle, inner: &Arc<Mutex<InnerState>>) {
    let state = inner.lock().ok().and_then(|guard| {
        (guard.popup_controller.snapshot().visible
            && guard
                .popup_controller
                .snapshot()
                .status
                .is_selection_prompt()
            && should_focus_popup_on_show(
                guard.popup_controller.snapshot().status,
                &guard.capability_snapshot,
            ))
        .then(|| guard.popup_controller.snapshot().clone())
    });
    let Some(state) = state else {
        return;
    };
    focus_selection_popup_for_keyboard_entry(app, inner, &state, PopupFocusDirection::Forward);
}

fn should_close_popup_from_global_escape(inner: &InnerState) -> bool {
    inner.popup_controller.snapshot().visible
}

fn should_focus_popup_from_global_tab(inner: &InnerState) -> bool {
    inner.popup_controller.snapshot().visible
        && inner
            .popup_controller
            .snapshot()
            .status
            .is_selection_prompt()
}

fn should_hide_popup_for_mouse_down(inner: &InnerState, is_inside_popup: bool) -> bool {
    inner.popup_controller.snapshot().visible
        && !inner.popup_controller.snapshot().pinned
        && !inner.popup_dragging
        && !is_inside_popup
}

fn hide_popup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("popup") {
        let _ = window.hide();
    }
}

fn hide_committed_popup(
    app: &AppHandle,
    inner: &Arc<Mutex<InnerState>>,
    state: &PopupState,
    reason: PopupCloseReason,
) {
    send_popup_state(app, state);
    let popup_was_focused = app
        .get_webview_window("popup")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);
    let Ok(mut current) = inner.lock() else {
        return;
    };
    if current.popup_controller.snapshot().revision != state.revision
        || current.popup_controller.snapshot().visible
    {
        return;
    }
    let focus_return_target = current.popup_focus_return_target.take();
    let focus_return_pid = focus_return_pid_for_close(
        reason,
        popup_was_focused,
        state.selection_revision,
        focus_return_target,
    );
    drop(current);
    hide_popup(app);
    if let Some(pid) = focus_return_pid {
        restore_external_focus(pid);
    }
}

fn focus_return_pid_for_close(
    reason: PopupCloseReason,
    popup_was_focused: bool,
    selection_revision: u64,
    target: Option<PopupFocusReturnTarget>,
) -> Option<i32> {
    if !popup_was_focused
        || matches!(
            reason,
            PopupCloseReason::OutsideClick | PopupCloseReason::ConfigurationChanged
        )
    {
        return None;
    }
    target
        .filter(|value| value.pid > 0 && value.selection_revision == selection_revision)
        .map(|value| value.pid)
}

fn is_point_inside_popup(app: &AppHandle, point: Point) -> bool {
    let Some(window) = app.get_webview_window("popup") else {
        return false;
    };
    let Ok(position) = window.outer_position() else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };
    let logical_point = LogicalPoint {
        x: point.x,
        y: point.y,
    };
    let Some(point_monitor_handle) = monitor_for_logical_point(&window, logical_point) else {
        return false;
    };
    let Some(point_monitor) = monitor_geometry(&point_monitor_handle, logical_point) else {
        return false;
    };
    let Some(popup_monitor_handle) = window.current_monitor().ok().flatten() else {
        return false;
    };
    let Some(popup_anchor) = monitor_logical_center(&popup_monitor_handle) else {
        return false;
    };
    let Some(popup_monitor) = monitor_geometry(&popup_monitor_handle, popup_anchor) else {
        return false;
    };

    popup_contains_point(PopupHitTest {
        point: logical_point,
        point_monitor,
        popup: PhysicalRect {
            origin: PhysicalPoint {
                x: position.x,
                y: position.y,
            },
            size: GeometryPhysicalSize {
                width: size.width,
                height: size.height,
            },
        },
        popup_monitor,
        padding_logical: POPUP_HIT_TEST_PADDING,
    })
    .unwrap_or(false)
}

fn commit_popup_close(
    inner: &mut InnerState,
    reason: PopupCloseReason,
) -> Result<PopupState, PopupControlError> {
    let dismissed_automatic_selection = matches!(
        reason,
        PopupCloseReason::ExplicitClose | PopupCloseReason::OutsideClick
    )
    .then(|| inner.active_popup_selection.clone())
    .flatten()
    .filter(|selection| {
        selection.selection_revision == inner.popup_controller.snapshot().selection_revision
    })
    .map(|selection| selection.identity);
    let snapshot = inner.popup_controller.close()?;
    inner.popup_dragging = false;
    match reason {
        PopupCloseReason::ExplicitClose | PopupCloseReason::OutsideClick => {
            if let Some(identity) = dismissed_automatic_selection {
                inner.dismissed_automatic_selection = Some(identity);
            }
            inner.active_popup_selection = None;
            clear_active_selection(inner);
        }
        PopupCloseReason::SelectionHandled | PopupCloseReason::ConfigurationChanged => {
            inner.active_popup_selection = None;
            inner.dismissed_automatic_selection = None;
            clear_active_selection(inner);
        }
    }
    Ok(snapshot)
}

fn cancel_acceptance_rapid_probe_locked(
    inner: &mut InnerState,
) -> Result<Option<PopupState>, String> {
    let probe = inner.acceptance_rapid_probe.take();
    let committed = inner.last_acceptance_probe_commit.take();
    let active_probe_is_current =
        probe.is_some_and(|probe| inner.popup_controller.translation_is_current(probe.guard));
    let committed_probe_is_current = committed.is_some_and(|(_, popup_revision)| {
        inner.popup_controller.snapshot().revision == popup_revision
    });
    if !active_probe_is_current && !committed_probe_is_current {
        return Ok(None);
    }
    commit_popup_close(inner, PopupCloseReason::SelectionHandled)
        .map(Some)
        .map_err(popup_control_error)
}

fn clear_active_selection(inner: &mut InnerState) {
    inner.active_selection_text = None;
}

#[cfg(test)]
fn handle_selection_status(inner: &mut InnerState, status: &str) {
    handle_selection_status_for_target(inner, status, 0);
}

fn is_mouse_selection_read_reason(reason: &str) -> bool {
    reason.starts_with("mouse_up")
}

fn apply_latched_selection_baseline_degradation(inner: &mut InnerState) {
    let Some(error) = inner.latched_selection_baseline_error.as_deref() else {
        return;
    };
    if !inner.settings.enable_selection_popup
        || inner.selection_status.code.as_deref() == Some("selection_disabled_by_setting")
        || matches!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Unavailable | CapabilityHealth::Disabled
        )
    {
        return;
    }
    inner.capability_snapshot.direct_selection_read =
        RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, error);
    inner.selection_status = WatcherStatus::with_code(
        true,
        true,
        "已监听到鼠标拖选，但上一次直接读取未能建立安全基线；该降级会保留到一次真实鼠标划词成功。",
        "selection_direct_baseline_degraded",
    );
}

fn handle_selection_status_for_target(inner: &mut InnerState, status: &str, target_pid: i32) {
    if let Some(diagnostics) = parse_selection_read_diagnostics(status) {
        if target_pid > 0 && u32::try_from(target_pid).ok() == Some(std::process::id()) {
            // Returning to Settings necessarily produces another global
            // mouse-up. Keep that self interaction from replacing the Preview
            // failure the user is trying to copy.
            return;
        }
        let is_empty = diagnostics.status == "selection_read_empty";
        let is_mouse_read = is_mouse_selection_read_reason(&diagnostics.reason);
        let is_terminal = diagnostics.terminal != Some(false);
        let is_baseline_failure = is_empty
            && is_mouse_read
            && is_terminal
            && diagnostics.ax_error.starts_with("selection_baseline_");
        let mouse_read_recovered = diagnostics.status == "selection_read_found" && is_mouse_read;
        if is_empty && diagnostics.terminal == Some(false) {
            inner.last_selection_read = Some(diagnostics);
            return;
        }
        let ax_error = diagnostics.ax_error.clone();
        inner.last_selection_read = Some(diagnostics);
        if is_baseline_failure {
            inner.latched_selection_baseline_error = Some(ax_error.clone());
        } else if mouse_read_recovered {
            inner.latched_selection_baseline_error = None;
            inner.capability_snapshot.direct_selection_read = RuntimeCapabilityState::with_status(
                CapabilityHealth::Ready,
                "selection_read_found",
            );
            if inner.selection_status.code.as_deref() == Some("selection_direct_baseline_degraded")
            {
                inner.selection_status = WatcherStatus::with_code(
                    true,
                    true,
                    "鼠标划词已完成一次真实选区读取，直接读取链路已恢复。",
                    "selection_direct_read_recovered",
                );
            }
        }
        if is_empty && ax_error == "not_trusted" {
            // Unlike an app-specific empty selection, an AX not-trusted error
            // is a process-level capability failure and may update readiness.
            inner.capability_snapshot.accessibility = PermissionCapabilityState::from_grant(
                PermissionGrant::Denied,
                "accessibility_read",
            );
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(
                    CapabilityHealth::Unavailable,
                    "accessibility_read_not_trusted",
                );
            inner.capability_snapshot.direct_selection_read = RuntimeCapabilityState::with_status(
                CapabilityHealth::Unavailable,
                "accessibility_read_not_trusted",
            );
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "已触发拖选，但 macOS 没有允许当前进程读取选区文本；自动划词受限，Cmd+C+C 仍可用。",
                "selection_accessibility_limited",
            );
        } else if is_empty && ax_error.starts_with("selection_baseline_") {
            // The active mouse-down tap could not establish its bounded,
            // pre-delivery AX snapshot. A direct-only result cannot be proven
            // to belong to this gesture, so native code fails closed. Expose
            // that degradation instead of continuing to claim direct reads
            // are ready while ordinary Preview drags are silently ignored.
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, &ax_error);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "已监听到鼠标拖选，但未能在系统时限内建立选区读取基线；本次直接读取已安全停止，详细原因见诊断。",
                "selection_direct_baseline_degraded",
            );
        }
        apply_latched_selection_baseline_degradation(inner);
        // Found/empty/duplicate are outcomes of one app-specific read. They
        // remain in last_selection_read for diagnosis but must not overwrite
        // the independently reported mouse-tap, AX-observer, direct-read, or
        // watcher readiness state. In particular, an observer_context_unavailable
        // event from Codex during Refresh is not evidence that direct reads are
        // globally degraded.
        return;
    }

    match status {
        "selection_multi_click_quiet_window_invalid" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                false,
                "macOS 多击时间设置无法安全读取，自动划词监听已停止；请恢复系统默认双击速度后刷新诊断，Cmd+C+C 仍可用。",
                status,
            );
        }
        "selection_disabled_by_setting" => {
            apply_selection_disabled_by_setting(inner);
        }
        "accessibility_not_trusted" => {
            inner.capability_snapshot.accessibility = PermissionCapabilityState::from_grant(
                PermissionGrant::Denied,
                "accessibility_status",
            );
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "macOS 当前未允许本进程读取选区文本；这只限制自动划词读取，Cmd+C+C 仍可用。",
                "selection_accessibility_limited",
            );
        }
        "accessibility_denied" | "app_accessibility_denied" => {
            inner.capability_snapshot.accessibility = PermissionCapabilityState::from_grant(
                PermissionGrant::Denied,
                "accessibility_status",
            );
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.selection_status = WatcherStatus::with_code(
                false,
                false,
                "Paper Float Translator 未获得 macOS 辅助功能权限。请在系统设置中重新授权 /Applications 中的 app。",
                "app_accessibility_denied",
            );
        }
        "ready" => {
            inner.capability_snapshot.mouse_tap = RuntimeCapabilityState::unknown();
            inner.capability_snapshot.ax_selected_text_observer = RuntimeCapabilityState::unknown();
            inner.capability_snapshot.direct_selection_read = RuntimeCapabilityState::unknown();
            inner.selection_status = WatcherStatus::with_code(
                false,
                false,
                "原生监听只返回了旧版 ready，无法判断具体输入源是否可用；请刷新诊断。",
                "selection_legacy_ready_ambiguous",
            );
        }
        "selection_sources_ready" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "鼠标选区监听、AX selectedText 通知与直接读取均已启用。",
                status,
            );
        }
        "selection_ax_timeout_unavailable" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                false,
                "AX 消息超时未能安全恢复，已停止本次自动选区读取以避免持续误判；请退出并重新打开应用，Cmd+C+C 仍可用。",
                status,
            );
        }
        "selection_mouse_ready_ax_observer_limited" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "鼠标选区与 AX 直接读取可用；当前前台应用未注册 selectedText 通知。",
                status,
            );
        }
        "selection_ax_observer_ready_mouse_unavailable" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "鼠标事件监听不可用，已由 AX selectedText 通知继续提供自动选区。",
                status,
            );
        }
        "selection_event_ready_accessibility_limited" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "鼠标拖选事件监听已启用，但选区文本读取受 macOS AX 校验限制；Cmd+C+C 仍可用。",
                "selection_event_ready_accessibility_limited",
            );
        }
        "selection_clipboard_only" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                false,
                "自动划词监听没有可用的读取路径；Cmd+C+C 仍可用，刷新诊断不会触发系统授权弹窗。",
                "selection_clipboard_only",
            );
        }
        "accessibility_selection_ready" => {
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Degraded, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "自动选区辅助功能备用监听已启用；拖选后若当前应用暴露选中文本，会自动显示浮窗。",
                "accessibility_selection_ready",
            );
        }
        "accessibility_selection_observer_ready" => {
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "AXObserver 选区备用监听已启用；前台应用选区变化后会自动显示浮窗。",
                "accessibility_selection_observer_ready",
            );
        }
        "accessibility_selection_input_limited" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                false,
                "辅助功能已通过，但全局鼠标监听和 AXObserver 都没有启动；请使用 Cmd+C+C 兜底。",
                "accessibility_selection_input_limited",
            );
        }
        "selection_empty" => {
            clear_active_selection(inner);
            // Legacy empty-selection notifications are read outcomes, not
            // watcher health. Keep the latest capability handshake intact.
        }
        "input_monitoring_unavailable" | "mouse_tap_unavailable" => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                false,
                "自动选区鼠标监听启动失败；请在系统设置中允许 Paper Float Translator 的输入监听权限，或继续使用 Cmd+C+C。",
                "input_monitoring_unavailable",
            );
        }
        _ if parse_disabled_tap_outcome(status, "selection_mouse_tap_disabled_")
            == Some(TapDisabledOutcome::Recovered) =>
        {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "鼠标事件监听曾被系统禁用，现已自动恢复。",
                status,
            );
        }
        _ if parse_disabled_tap_outcome(status, "selection_mouse_tap_disabled_")
            == Some(TapDisabledOutcome::FallbackAx) =>
        {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Disabled, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Ready, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                true,
                "鼠标事件监听被系统禁用，已切换到 AX selectedText 通知。",
                status,
            );
        }
        _ if parse_disabled_tap_outcome(status, "selection_mouse_tap_disabled_")
            == Some(TapDisabledOutcome::Unavailable) =>
        {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Disabled, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unavailable, status);
            inner.selection_status = WatcherStatus::with_code(
                true,
                false,
                "鼠标事件监听被系统禁用且 AX selectedText 通知不可用；请刷新诊断或使用 Cmd+C+C。",
                status,
            );
        }
        _ => {
            inner.capability_snapshot.mouse_tap =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unknown, status);
            inner.capability_snapshot.ax_selected_text_observer =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unknown, status);
            inner.capability_snapshot.direct_selection_read =
                RuntimeCapabilityState::with_status(CapabilityHealth::Unknown, status);
            inner.selection_status = WatcherStatus::with_code(
                false,
                false,
                format!("收到无法识别的原生选区状态：{status}"),
                "unknown_native_status",
            );
        }
    }
    apply_latched_selection_baseline_degradation(inner);
}

fn selection_status_completes_watcher_readiness(status: &str) -> bool {
    // Only the one capability status emitted at the end of native watcher
    // startup may satisfy the refresh barrier. A selection-read diagnostic can
    // race with startup (for example from the frontmost Codex AX observer) and
    // must never make Refresh finish with an app-specific empty-read status.
    matches!(
        status,
        "selection_disabled_by_setting"
            | "selection_sources_ready"
            | "selection_mouse_ready_ax_observer_limited"
            | "selection_ax_timeout_unavailable"
            | "selection_event_ready_accessibility_limited"
            | "selection_ax_observer_ready_mouse_unavailable"
            | "accessibility_selection_input_limited"
            | "selection_clipboard_only"
    )
}

fn parse_selection_read_diagnostics(status: &str) -> Option<SelectionReadDiagnostics> {
    let mut parts = status.split('\t');
    let status = parts.next()?;
    if !matches!(
        status,
        "selection_read_found" | "selection_read_empty" | "selection_read_duplicate"
    ) {
        return None;
    }

    let reason = parts.next()?.to_string();
    let found_text = match parts.next()? {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    let candidate_count = parts.next()?.parse().ok()?;
    if found_text != matches!(status, "selection_read_found" | "selection_read_duplicate") {
        return None;
    }
    let duration_ms = parts.next()?.parse::<f64>().ok()?;
    if !duration_ms.is_finite() || duration_ms < 0.0 {
        return None;
    }
    let source_bundle_id = parts.next()?.to_string();
    let ax_error = parts.next()?.to_string();
    let tail = parts.collect::<Vec<_>>();
    let (generation, attempt, trigger_to_read_ms, terminal) = match tail.as_slice() {
        [] => (None, None, None, None),
        [generation, attempt, trigger_to_read_ms, terminal] => {
            let generation = generation
                .parse::<i64>()
                .ok()
                .and_then(|value| u64::try_from(value).ok())?;
            let attempt = attempt.parse::<u16>().ok()?;
            let trigger_to_read_ms = trigger_to_read_ms.parse::<f64>().ok()?;
            let terminal = match *terminal {
                "true" => true,
                "false" => false,
                _ => return None,
            };
            (
                Some(generation),
                Some(attempt),
                Some(trigger_to_read_ms),
                Some(terminal),
            )
        }
        _ => return None,
    };

    Some(SelectionReadDiagnostics {
        status: status.to_string(),
        reason,
        found_text,
        candidate_count,
        duration_ms,
        source_bundle_id,
        ax_error,
        generation,
        attempt,
        trigger_to_read_ms,
        terminal,
    })
}

fn initial_double_copy_status() -> WatcherStatus {
    if cfg!(target_os = "macos") {
        WatcherStatus::new(true, false, "Cmd+C+C 双复制监听未启用。")
    } else if cfg!(target_os = "windows") {
        WatcherStatus::with_code(
            false,
            false,
            "正在初始化 Windows Ctrl+C+C 双复制取词。",
            "windows_double_copy_pending",
        )
    } else {
        WatcherStatus::with_code(
            false,
            false,
            "双复制监听目前仅支持 macOS。",
            "unsupported_os",
        )
    }
}

fn initial_selection_status() -> WatcherStatus {
    if cfg!(target_os = "macos") {
        WatcherStatus::new(true, false, "自动选区浮窗监听未启用。")
    } else if cfg!(target_os = "windows") {
        WatcherStatus::new(true, false, "正在初始化 Windows 快捷键取词。")
    } else {
        WatcherStatus::with_code(
            false,
            false,
            "自动选区浮窗目前仅支持 macOS。",
            "unsupported_os",
        )
    }
}

fn initial_capability_snapshot() -> CapabilitySnapshot {
    if cfg!(any(target_os = "macos", target_os = "windows")) {
        CapabilitySnapshot::unknown()
    } else {
        CapabilitySnapshot::unsupported()
    }
}

fn current_permission_grants() -> (PermissionGrant, PermissionGrant) {
    #[cfg(target_os = "macos")]
    {
        (
            if macos_native::accessibility_trusted() {
                PermissionGrant::Granted
            } else {
                PermissionGrant::Denied
            },
            macos_native::listen_event_access_grant(),
        )
    }

    #[cfg(target_os = "windows")]
    {
        (PermissionGrant::Granted, PermissionGrant::Granted)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        (PermissionGrant::Unsupported, PermissionGrant::Unsupported)
    }
}

fn was_recently_triggered(inner: &InnerState, text: &str, copied_at: Instant) -> bool {
    inner.last_double_copy_trigger.as_ref().is_some_and(|last| {
        last.text == text
            && copied_at.duration_since(last.triggered_at).as_millis() <= DOUBLE_COPY_COOLDOWN_MS
    })
}

struct DeepSeekStreamRequest<'a> {
    api_key: &'a str,
    cleaned_text: &'a str,
    model: &'a DeepSeekModel,
    mode: &'a TranslateMode,
    target_language: &'a str,
    glossary: &'a Glossary,
}

async fn request_deepseek_stream(
    http: &reqwest::Client,
    request: DeepSeekStreamRequest<'_>,
    mut on_delta: impl FnMut(&str),
) -> Result<String, String> {
    let DeepSeekStreamRequest {
        api_key,
        cleaned_text,
        model,
        mode,
        target_language,
        glossary,
    } = request;
    let body = serde_json::json!({
      "model": model_name(model),
      "thinking": { "type": "disabled" },
      "stream": true,
      "messages": [
        {
          "role": "system",
          "content": build_system_prompt(mode, target_language, glossary)
        },
        {
          "role": "user",
          "content": cleaned_text
        }
      ]
    });
    let response = http
        .post(format!("{DEEPSEEK_BASE_URL}/chat/completions"))
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| readable_deepseek_transport_error(&error))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(readable_deepseek_error(status.as_u16(), &body));
    }

    let mut buffer = String::new();
    let mut translation = String::new();
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| readable_deepseek_transport_error(&error))?
    {
        let text = String::from_utf8_lossy(&chunk);
        let done = parse_deepseek_stream_chunk(&mut buffer, &text, |delta| {
            translation.push_str(delta);
            on_delta(delta);
        })?;
        if done {
            break;
        }
    }
    parse_deepseek_stream_chunk(&mut buffer, "\n", |delta| {
        translation.push_str(delta);
        on_delta(delta);
    })?;

    let raw = translation.trim();
    if raw.is_empty() {
        return Err("DeepSeek 响应中没有可用译文。".to_string());
    }
    if *mode == TranslateMode::Terminology {
        Ok(normalize_terminology_output(raw))
    } else {
        Ok(raw.to_string())
    }
}

fn parse_deepseek_stream_chunk(
    buffer: &mut String,
    chunk: &str,
    mut on_delta: impl FnMut(&str),
) -> Result<bool, String> {
    buffer.push_str(chunk);
    let mut done = false;

    while let Some(line_end) = buffer.find('\n') {
        let line = buffer[..line_end].trim_end_matches('\r').to_string();
        buffer.drain(..=line_end);
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() {
            continue;
        }
        if payload == "[DONE]" {
            done = true;
            continue;
        }
        if let Some(delta) = deepseek_stream_delta(payload)? {
            on_delta(&delta);
        }
    }

    Ok(done)
}

fn deepseek_stream_delta(payload: &str) -> Result<Option<String>, String> {
    let data: serde_json::Value = serde_json::from_str(payload).map_err(to_string)?;
    Ok(data["choices"][0]["delta"]["content"]
        .as_str()
        .or_else(|| data["choices"][0]["message"]["content"].as_str())
        .map(str::to_string))
}

fn build_system_prompt(mode: &TranslateMode, target_language: &str, glossary: &Glossary) -> String {
    let target_language = normalize_target_language(target_language, DEFAULT_TARGET_LANGUAGE);
    let mut lines = vec![
    "你是专业的学术内容翻译助手。".to_string(),
    format!("自动识别用户提供的原文语言，并将其翻译为{target_language}。"),
    "不要要求用户重新提供英文文本，也不要因为原文不是英文而拒绝翻译。".to_string(),
    "保留公式、变量名、引用编号、专有名词。".to_string(),
    format!("必要时在{target_language}译名后用括号保留原文术语。"),
    match mode {
      TranslateMode::AcademicZh => format!("请将用户提供的原文翻译为准确、自然、适合{target_language}学术阅读的{target_language}。只输出译文，不要解释。"),
      TranslateMode::Bilingual => format!("请输出{target_language}译文与原文对照。先给{target_language}译文，再保留用户提供的原文。格式保持简洁，适合论文精读。"),
      TranslateMode::Terminology => [
        format!("请解释原文中的关键学术术语，并给出推荐{target_language}译名。"),
        "最多输出 8 条。".to_string(),
        "不要使用 Markdown、标题、加粗、编号、项目符号或表格。".to_string(),
        "不要输出开头说明、结尾总结或“推荐译法”等额外内容。".to_string(),
        format!("每行固定使用格式：原文术语：{target_language}译名。说明：用{target_language}一句话解释。"),
        "没有关键术语时只输出：未发现需要解释的关键术语。".to_string(),
      ].join("\n"),
    },
  ];
    let glossary_block = build_glossary_block(glossary);
    if !glossary_block.is_empty() {
        lines.push(glossary_block);
    }
    lines.join("\n")
}

fn build_glossary_block(glossary: &Glossary) -> String {
    let mut entries = glossary
        .iter()
        .filter(|(source, target)| !source.trim().is_empty() && !target.trim().is_empty())
        .map(|(source, target)| format!("- {}: {}", source.trim(), target.trim()))
        .collect::<Vec<_>>();
    entries.sort();
    if entries.is_empty() {
        String::new()
    } else {
        ["请优先使用以下术语表：".to_string(), entries.join("\n")].join("\n")
    }
}

fn normalize_terminology_output(output: &str) -> String {
    output
        .lines()
        .filter_map(normalize_terminology_line)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn normalize_terminology_line(line: &str) -> Option<String> {
    let cleaned = strip_markdown_syntax(line);
    if cleaned.is_empty() || is_terminology_wrapper_line(&cleaned) {
        return None;
    }
    Some(cleaned)
}

fn strip_markdown_syntax(line: &str) -> String {
    let mut value = line.trim().replace("**", "").replace(['`', '_'], "");
    while value.starts_with('#')
        || value.starts_with('-')
        || value.starts_with('*')
        || value.starts_with('+')
    {
        value = value[1..].trim_start().to_string();
    }
    let numbered = value
        .chars()
        .skip_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if numbered.starts_with('.') || numbered.starts_with(')') || numbered.starts_with('、') {
        value = numbered[1..].trim_start().to_string();
    }
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_terminology_wrapper_line(line: &str) -> bool {
    let normalized = line
        .chars()
        .filter(|ch| !matches!(ch, '：' | ':' | '。' | '.' | ' '))
        .collect::<String>();
    normalized.is_empty()
        || normalized.contains("关键术语")
        || normalized.contains("推荐译法")
        || normalized.contains("推荐译名")
        || normalized.contains("上述术语")
        || normalized.contains("可直用")
        || normalized.contains("总结")
}

fn collapse_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_target_language(value: &str, fallback: &str) -> String {
    let normalized = collapse_spaces(&value.replace(['\r', '\n', '\t'], " "));
    if normalized.is_empty() {
        collapse_spaces(fallback).if_empty(DEFAULT_TARGET_LANGUAGE)
    } else {
        normalized
    }
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

fn create_cache_key(
    model: &DeepSeekModel,
    mode: &TranslateMode,
    target_language: &str,
    glossary_version: &str,
    cleaned_text: &str,
) -> String {
    let payload = serde_json::json!({
      "model": model_name(model),
      "mode": mode_name(mode),
      "targetLanguage": normalize_target_language(target_language, DEFAULT_TARGET_LANGUAGE),
      "glossaryVersion": glossary_version,
      "cleanedText": cleaned_text
    });
    sha256(&payload.to_string())
}

fn create_glossary_version(glossary: &Glossary) -> String {
    let mut entries = glossary
        .iter()
        .filter(|(source, target)| !source.trim().is_empty() && !target.trim().is_empty())
        .map(|(source, target)| (source.trim().to_string(), target.trim().to_string()))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    sha256(&serde_json::to_string(&entries).unwrap_or_default())
}

fn sha256(payload: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload.as_bytes());
    hex::encode(hasher.finalize())
}

fn ensure_data_dir(data_dir: &Path) -> Result<(), String> {
    #[cfg(feature = "acceptance-testing")]
    validate_acceptance_storage_namespace(data_dir)?;
    fs::create_dir_all(data_dir).map_err(|error| {
        format!(
            "访问应用数据目录失败（{}）：{error}",
            display_path(data_dir)
        )
    })?;
    #[cfg(feature = "acceptance-testing")]
    validate_acceptance_storage_namespace(data_dir)?;
    Ok(())
}

fn load_settings(data_dir: &Path) -> Result<AppSettings, String> {
    let path = settings_path(data_dir);
    let content = match read_managed_text(data_dir, &path) {
        Ok(Some(content)) => content,
        Ok(None) => return Ok(AppSettings::default()),
        Err(error) => {
            return Err(format!(
                "读取设置文件失败（{}）：{error}",
                display_path(&path)
            ));
        }
    };
    let value = serde_json::from_str::<serde_json::Value>(&content).map_err(|error| {
        format!(
            "设置文件已损坏或不是有效 JSON（{}）：{error}",
            display_path(&path)
        )
    })?;
    Ok(normalize_settings(settings_from_value(&value)))
}

fn settings_from_value(value: &serde_json::Value) -> AppSettings {
    let defaults = AppSettings::default();
    AppSettings {
        model: value
            .get("model")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_model)
            .unwrap_or(defaults.model),
        mode: value
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .and_then(parse_mode)
            .unwrap_or(defaults.mode),
        clean_pdf_text: value
            .get("cleanPdfText")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(defaults.clean_pdf_text),
        enable_cache: value
            .get("enableCache")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(defaults.enable_cache),
        enable_selection_popup: value
            .get("enableSelectionPopup")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(defaults.enable_selection_popup),
        enable_automatic_selection: value
            .get("enableAutomaticSelection")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(defaults.enable_automatic_selection),
        target_language: value
            .get("targetLanguage")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&defaults.target_language)
            .to_string(),
        popup_width: value
            .get("popupWidth")
            .and_then(parse_u32_value)
            .unwrap_or(defaults.popup_width),
    }
}

fn parse_model(value: &str) -> Option<DeepSeekModel> {
    match value {
        "deepseek-v4-flash" => Some(DeepSeekModel::Flash),
        "deepseek-v4-pro" => Some(DeepSeekModel::Pro),
        _ => None,
    }
}

fn parse_mode(value: &str) -> Option<TranslateMode> {
    match value {
        "academic_zh" => Some(TranslateMode::AcademicZh),
        "bilingual" => Some(TranslateMode::Bilingual),
        "literal" | "natural" => Some(TranslateMode::AcademicZh),
        "terminology" => Some(TranslateMode::Terminology),
        _ => None,
    }
}

fn parse_u32_value(value: &serde_json::Value) -> Option<u32> {
    if let Some(number) = value.as_u64() {
        return u32::try_from(number).ok();
    }
    value.as_str()?.trim().parse::<u32>().ok()
}

fn normalize_settings(settings: AppSettings) -> AppSettings {
    AppSettings {
        popup_width: settings.popup_width.clamp(320, 640),
        target_language: normalize_target_language(
            &settings.target_language,
            DEFAULT_TARGET_LANGUAGE,
        ),
        ..settings
    }
}

fn read_cache(data_dir: &Path) -> CacheFile {
    let Ok(_guard) = CACHE_IO_LOCK.lock() else {
        return CacheFile::default();
    };
    let mut cache = read_cache_unlocked(data_dir);
    if prune_expired_cache_entries(&mut cache, cache_epoch_seconds_now()) > 0 {
        if let Err(error) = save_managed_json(data_dir, &cache_path(data_dir), &cache) {
            eprintln!("{error}");
        }
    }
    cache
}

fn read_cache_unlocked(data_dir: &Path) -> CacheFile {
    match read_managed_json::<CacheFile>(data_dir, &cache_path(data_dir)) {
        Ok(Some(cache)) => cache,
        Ok(None) => CacheFile::default(),
        Err(error) => {
            eprintln!("{error}");
            CacheFile::default()
        }
    }
}

fn insert_cache_entry(data_dir: &Path, key: String, entry: CacheEntry) -> Result<(), String> {
    let _guard = CACHE_IO_LOCK.lock().map_err(lock_error)?;
    #[cfg(feature = "acceptance-testing")]
    validate_acceptance_managed_path(data_dir, &cache_path(data_dir))?;
    let mut cache = read_cache_unlocked(data_dir);
    prune_expired_cache_entries(&mut cache, cache_epoch_seconds_now());
    cache.entries.insert(key, entry);
    trim_cache_entries(&mut cache);
    save_managed_json(data_dir, &cache_path(data_dir), &cache)
}

fn clear_cache_file(data_dir: &Path) -> Result<(), String> {
    let _guard = CACHE_IO_LOCK.lock().map_err(lock_error)?;
    save_managed_json(data_dir, &cache_path(data_dir), &CacheFile::default())
}

fn trim_cache_entries(cache: &mut CacheFile) {
    while cache.entries.len() > CACHE_MAX_ENTRIES {
        let oldest_key = cache
            .entries
            .iter()
            .min_by(|(left_key, left), (right_key, right)| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left_key.cmp(right_key))
            })
            .map(|(key, _)| key.clone());
        let Some(oldest_key) = oldest_key else {
            break;
        };
        cache.entries.remove(&oldest_key);
    }
}

fn prune_expired_cache_entries(cache: &mut CacheFile, now_epoch_seconds: u64) -> usize {
    let before = cache.entries.len();
    let latest_allowed = now_epoch_seconds.saturating_add(CACHE_FUTURE_SKEW_SECS);
    cache.entries.retain(|_, entry| {
        let Some(created_at) = parse_cache_timestamp(&entry.created_at) else {
            return false;
        };
        created_at <= latest_allowed
            && now_epoch_seconds.saturating_sub(created_at) <= CACHE_MAX_AGE_SECS
    });
    before.saturating_sub(cache.entries.len())
}

fn parse_cache_timestamp(value: &str) -> Option<u64> {
    (value.len() == 20 && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<u64>().ok())
        .flatten()
}

fn load_glossary(data_dir: &Path) -> Glossary {
    match read_managed_json::<Glossary>(data_dir, &glossary_path(data_dir)) {
        Ok(Some(glossary)) => glossary,
        Ok(None) => Glossary::default(),
        Err(error) => {
            eprintln!("{error}");
            Glossary::default()
        }
    }
}

#[cfg(test)]
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn read_managed_json<T: for<'de> Deserialize<'de>>(
    data_dir: &Path,
    path: &Path,
) -> Result<Option<T>, String> {
    let Some(content) = read_managed_text(data_dir, path)? else {
        return Ok(None);
    };
    Ok(serde_json::from_str(&content).ok())
}

fn read_managed_text(data_dir: &Path, path: &Path) -> Result<Option<String>, String> {
    #[cfg(feature = "acceptance-testing")]
    validate_acceptance_managed_path(data_dir, path)?;
    #[cfg(not(feature = "acceptance-testing"))]
    let _ = data_dir;

    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(all(feature = "acceptance-testing", unix))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NOFOLLOW closes the final-component replacement race before the
        // opened descriptor is checked again below. We intentionally do not
        // walk intermediate components with openat/dirfd: this namespace guard
        // protects against stale or accidental links, while a same-UID actor
        // able to replace an app-data ancestor can already mutate both stores
        // directly. The descriptor check still prevents reading a raced final
        // symlink or a multiply-linked managed file.
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    #[cfg(feature = "acceptance-testing")]
    validate_opened_acceptance_file(&file, path)?;

    let mut content = String::new();
    file.read_to_string(&mut content).map_err(to_string)?;
    Ok(Some(content))
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_string)?;
    }
    let content = serde_json::to_string_pretty(value).map_err(to_string)?;
    write_atomic_contents(path, content.as_bytes())
}

fn write_atomic_contents(path: &Path, content: &[u8]) -> Result<(), String> {
    let _write_guard = ATOMIC_FILE_WRITE_LOCK.lock().map_err(lock_error)?;
    let parent = path
        .parent()
        .ok_or_else(|| "持久化路径缺少父目录。".to_owned())?;
    fs::create_dir_all(parent).map_err(to_string)?;

    // Keep the existing fail-closed link policy. AtomicWriteFile also replaces
    // a destination symlink instead of following it, so a final-component race
    // cannot redirect these bytes into the symlink target.
    validate_atomic_write_destination(path)?;
    let mut file = AtomicWriteFile::open(path).map_err(to_string)?;
    file.write_all(content).map_err(to_string)?;
    validate_atomic_write_destination(path)?;
    file.commit().map_err(to_string)
}

#[cfg(test)]
fn write_atomic_file(path: &Path, temporary_path: &Path, content: &[u8]) -> Result<(), String> {
    let _write_guard = ATOMIC_FILE_WRITE_LOCK.lock().map_err(lock_error)?;
    let parent = path
        .parent()
        .ok_or_else(|| "持久化路径缺少父目录。".to_string())?;
    if temporary_path.parent() != Some(parent) {
        return Err("原子写入临时文件必须与目标文件位于同一目录。".to_owned());
    }

    let file = match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temporary_path)
    {
        Ok(file) => file,
        // Do not clean up here: create_new did not give this invocation
        // ownership, so the pre-existing file or symlink belongs to somebody
        // else and must remain untouched.
        Err(error) => return Err(error.to_string()),
    };
    let mut file = Some(file);
    let mut temporary_owned = true;
    let result = (|| {
        let writer = file
            .as_mut()
            .ok_or_else(|| "原子写入临时文件已意外关闭。".to_owned())?;
        writer.write_all(content).map_err(to_string)?;
        writer.sync_all().map_err(to_string)?;
        // Windows cannot replace a destination while the temporary file is
        // still open, so close it explicitly before rename on every platform.
        drop(file.take());

        validate_atomic_write_destination(path)?;
        // A same-UID actor can still replace the final directory entry between
        // this recheck and rename. rename replaces that entry without following
        // a symlink or writing through a hard link, so such a race cannot redirect
        // these bytes outside the target directory. Protecting every ancestor
        // against a malicious same-UID actor would require a platform-specific
        // openat/dirfd walk; that actor can already mutate both app data stores.
        fs::rename(temporary_path, path).map_err(to_string)?;
        temporary_owned = false;

        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(to_string)
    })();
    drop(file.take());
    if result.is_err() && temporary_owned {
        let _ = fs::remove_file(temporary_path);
    }
    result
}

fn validate_atomic_write_destination(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "原子写入目标不得是符号链接（{}）。",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!("原子写入目标不是普通文件（{}）。", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(format!(
                "原子写入目标不得有多个硬链接（{}）。",
                path.display()
            ));
        }
    }
    Ok(())
}

fn save_managed_json<T: Serialize>(data_dir: &Path, path: &Path, value: &T) -> Result<(), String> {
    #[cfg(feature = "acceptance-testing")]
    validate_acceptance_managed_path(data_dir, path)?;
    #[cfg(not(feature = "acceptance-testing"))]
    let _ = data_dir;
    save_json(path, value)
}

fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}

fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("cache.json")
}

fn glossary_path(data_dir: &Path) -> PathBuf {
    data_dir.join("glossary.json")
}

fn validate_compiled_runtime_identity(
    bundle_identifier: &str,
    product_name: &str,
) -> Result<(), String> {
    if bundle_identifier != EXPECTED_BUNDLE_IDENTIFIER || product_name != EXPECTED_PRODUCT_NAME {
        return Err(format!(
            "当前编译制品身份不匹配：必须使用 {EXPECTED_PRODUCT_NAME} / {EXPECTED_BUNDLE_IDENTIFIER}，已在访问用户数据和启动监听前终止。"
        ));
    }
    Ok(())
}

#[cfg(feature = "acceptance-testing")]
fn expected_acceptance_data_dir() -> Result<PathBuf, String> {
    dirs::data_dir()
        .map(|path| path.join(EXPECTED_BUNDLE_IDENTIFIER))
        .ok_or_else(|| "无法解析平台应用数据根目录，已阻止验收构建访问任何用户状态。".to_owned())
}

#[cfg(feature = "acceptance-testing")]
fn prepare_acceptance_data_dir(data_dir: &Path) -> Result<(), String> {
    validate_acceptance_data_dir(data_dir)?;
    ensure_data_dir(data_dir)?;
    validate_acceptance_data_dir(data_dir)
}

#[cfg(feature = "acceptance-testing")]
fn validate_acceptance_data_dir(data_dir: &Path) -> Result<(), String> {
    let expected = expected_acceptance_data_dir()?;
    validate_acceptance_data_dir_against(data_dir, &expected)
}

#[cfg(feature = "acceptance-testing")]
fn validate_acceptance_data_dir_against(
    data_dir: &Path,
    expected_data_dir: &Path,
) -> Result<(), String> {
    if data_dir != expected_data_dir {
        return Err(format!(
            "验收数据目录不匹配平台专用根：期望 {}，实际 {}，已阻止继续运行。",
            expected_data_dir.display(),
            data_dir.display(),
        ));
    }
    validate_acceptance_storage_namespace(data_dir)
}

#[cfg(feature = "acceptance-testing")]
fn validate_acceptance_storage_namespace(data_dir: &Path) -> Result<(), String> {
    validate_acceptance_path_kind(data_dir, true, "验收数据根目录")?;

    let report_path = acceptance_runtime::export_path(data_dir);
    let report_dir = report_path
        .parent()
        .ok_or_else(|| "验收报告路径缺少父目录，已阻止继续运行。".to_owned())?;
    validate_acceptance_path_kind(report_dir, true, "验收报告目录")?;
    for (path, label) in [
        (settings_path(data_dir), "验收设置文件"),
        (cache_path(data_dir), "验收缓存文件"),
        (glossary_path(data_dir), "验收术语表文件"),
        (report_path, "验收报告文件"),
    ] {
        validate_acceptance_path_kind(&path, false, label)?;
    }
    Ok(())
}

#[cfg(feature = "acceptance-testing")]
fn validate_acceptance_managed_path(data_dir: &Path, path: &Path) -> Result<(), String> {
    let report_path = acceptance_runtime::export_path(data_dir);
    let allowed_paths = [
        settings_path(data_dir),
        cache_path(data_dir),
        glossary_path(data_dir),
        report_path,
    ];
    if !allowed_paths.iter().any(|allowed| allowed == path) {
        return Err(format!(
            "验收受管路径越出专用名称空间（{}），已阻止访问。",
            path.display()
        ));
    }
    validate_acceptance_storage_namespace(data_dir)
}

#[cfg(feature = "acceptance-testing")]
fn validate_acceptance_path_kind(path: &Path, directory: bool, label: &str) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "检查{label}失败（{}）：{error}，已阻止继续运行。",
                path.display()
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "{label}不得是符号链接（{}），已阻止名称空间逃逸。",
            path.display()
        ));
    }
    let kind_matches = if directory {
        metadata.is_dir()
    } else {
        metadata.is_file()
    };
    if !kind_matches {
        return Err(format!(
            "{label}类型异常（{}），已阻止继续运行。",
            path.display()
        ));
    }
    #[cfg(unix)]
    if !directory {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(format!(
                "{label}不得有多个硬链接（{}），已阻止名称空间逃逸。",
                path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "acceptance-testing")]
fn validate_opened_acceptance_file(file: &fs::File, path: &Path) -> Result<(), String> {
    let metadata = file.metadata().map_err(|error| {
        format!(
            "检查已打开的验收受管文件失败（{}）：{error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "已打开的验收受管路径不是普通文件（{}），已阻止读取。",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(format!(
                "已打开的验收受管文件不得有多个硬链接（{}），已阻止读取。",
                path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(not(feature = "acceptance-testing"))]
fn fallback_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Paper Float Translator")
}

fn compiled_api_key_storage() -> ApiKeyStorage {
    #[cfg(feature = "local-api-key-file")]
    {
        // Keep a feature-specific byte sequence in stripped release binaries.
        // Release verification rejects this marker before a local diagnostic
        // artifact can be signed or distributed as the production build.
        std::hint::black_box(LOCAL_API_KEY_ARTIFACT_MARKER.as_bytes());
        ApiKeyStorage::LocalFile
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "windows"))]
    {
        ApiKeyStorage::WindowsCredentialManager
    }

    #[cfg(all(not(feature = "local-api-key-file"), not(target_os = "windows")))]
    {
        ApiKeyStorage::SystemKeychain
    }
}

fn compiled_runtime_platform() -> RuntimePlatform {
    #[cfg(target_os = "macos")]
    {
        RuntimePlatform::Macos
    }

    #[cfg(target_os = "windows")]
    {
        RuntimePlatform::Windows
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        RuntimePlatform::Other
    }
}

fn get_api_key(data_dir: &Path) -> Result<Option<String>, String> {
    #[cfg(feature = "local-api-key-file")]
    {
        read_local_api_key(data_dir)
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "macos"))]
    {
        let _ = data_dir;
        macos_native::get_keychain_secret(KEYCHAIN_SERVICE, DEEPSEEK_ACCOUNT)
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "windows"))]
    {
        let _ = data_dir;
        platform::get_windows_secret(KEYCHAIN_SERVICE, DEEPSEEK_ACCOUNT)
    }

    #[cfg(all(
        not(feature = "local-api-key-file"),
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = data_dir;
        Ok(None)
    }
}

#[cfg(not(feature = "local-api-key-file"))]
fn summarize_api_key_presence_for_settings(
    result: Result<bool, String>,
) -> (ApiKeyStatus, Option<String>) {
    match result {
        Ok(true) => (ApiKeyStatus::Configured, None),
        Ok(false) => (ApiKeyStatus::Missing, None),
        Err(error) => (
            ApiKeyStatus::Unavailable,
            Some(format!(
                "当前构建无法在后台安全检查新的 API Key 钥匙串条目：{error} 为避免系统密码弹窗，读取已停止。请在下方重新保存 API Key；应用不会回退读取旧签名留下的钥匙串条目。"
            )),
        ),
    }
}

fn get_api_key_status_for_settings(data_dir: &Path) -> (ApiKeyStatus, Option<String>) {
    #[cfg(feature = "local-api-key-file")]
    {
        match read_local_api_key(data_dir) {
            Ok(Some(_)) => (ApiKeyStatus::Configured, None),
            Ok(None) => (ApiKeyStatus::Missing, None),
            Err(error) => (
                ApiKeyStatus::Unavailable,
                Some(format!(
                    "读取当前用户的本地 API Key 文件失败：{error} 请在下方重新保存 API Key；本地构建不会访问系统钥匙串。"
                )),
            ),
        }
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "macos"))]
    {
        let _ = data_dir;
        summarize_api_key_presence_for_settings(macos_native::keychain_secret_exists(
            KEYCHAIN_SERVICE,
            DEEPSEEK_ACCOUNT,
        ))
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "windows"))]
    {
        let _ = data_dir;
        summarize_api_key_presence_for_settings(platform::windows_secret_exists(
            KEYCHAIN_SERVICE,
            DEEPSEEK_ACCOUNT,
        ))
    }

    #[cfg(all(
        not(feature = "local-api-key-file"),
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = data_dir;
        summarize_api_key_presence_for_settings(Ok(false))
    }
}

fn set_api_key(data_dir: &Path, value: &str) -> Result<(), String> {
    #[cfg(feature = "local-api-key-file")]
    {
        write_local_api_key(data_dir, value)
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "macos"))]
    {
        let _ = data_dir;
        macos_native::set_keychain_secret(KEYCHAIN_SERVICE, DEEPSEEK_ACCOUNT, value)
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "windows"))]
    {
        let _ = data_dir;
        platform::set_windows_secret(KEYCHAIN_SERVICE, DEEPSEEK_ACCOUNT, value)
    }

    #[cfg(all(
        not(feature = "local-api-key-file"),
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = (data_dir, value);
        Err("当前操作系统没有可用的系统安全凭据后端。".to_owned())
    }
}

fn delete_api_key(data_dir: &Path) -> Result<(), String> {
    #[cfg(feature = "local-api-key-file")]
    {
        delete_local_api_key(data_dir)
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "macos"))]
    {
        let _ = data_dir;
        macos_native::delete_keychain_secret(KEYCHAIN_SERVICE, DEEPSEEK_ACCOUNT)
    }

    #[cfg(all(not(feature = "local-api-key-file"), target_os = "windows"))]
    {
        let _ = data_dir;
        platform::delete_windows_secret(KEYCHAIN_SERVICE, DEEPSEEK_ACCOUNT)
    }

    #[cfg(all(
        not(feature = "local-api-key-file"),
        not(any(target_os = "macos", target_os = "windows"))
    ))]
    {
        let _ = data_dir;
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn windows_double_copy_status(enabled: bool, input_running: bool) -> WatcherStatus {
    if !enabled {
        WatcherStatus::with_code(
            true,
            false,
            "Ctrl+C+C 双复制取词已在设置中关闭。",
            "selection_disabled",
        )
    } else if input_running {
        WatcherStatus::with_code(
            true,
            true,
            "Ctrl+C+C 双复制取词已就绪；只确认用户主动执行的复制。",
            "windows_double_copy_ready",
        )
    } else {
        WatcherStatus::with_code(
            false,
            false,
            "Windows 后台输入监听未能启动；仍可使用 Ctrl+Alt+T 取词。",
            "windows_raw_input_unavailable",
        )
    }
}

#[cfg(target_os = "windows")]
fn windows_shortcut_capability_snapshot(
    selection_enabled: bool,
    shortcut_registered: bool,
    input_running: bool,
    automatic_selection_enabled: bool,
    automatic_running: bool,
) -> CapabilitySnapshot {
    let no_permission_required = PermissionCapabilityState {
        grant: PermissionGrant::Granted,
        health: CapabilityHealth::Ready,
        status_code: Some("windows_uia_no_consent_required".to_owned()),
    };
    let listen_event = PermissionCapabilityState {
        grant: PermissionGrant::Granted,
        health: CapabilityHealth::Ready,
        status_code: Some("windows_raw_input_no_consent_required".to_owned()),
    };
    let shortcut_health = if !selection_enabled {
        CapabilityHealth::Disabled
    } else if shortcut_registered {
        CapabilityHealth::Ready
    } else {
        CapabilityHealth::Unavailable
    };
    let shortcut_code = if !selection_enabled {
        "selection_disabled"
    } else if shortcut_registered {
        "windows_global_shortcut_ready"
    } else {
        "windows_shortcut_registration_failed"
    };
    let input_health = if !selection_enabled {
        CapabilityHealth::Disabled
    } else if input_running {
        CapabilityHealth::Ready
    } else {
        CapabilityHealth::Unavailable
    };
    let input_code = if !selection_enabled {
        "selection_disabled"
    } else if input_running {
        "windows_raw_input_ready"
    } else {
        "windows_raw_input_unavailable"
    };
    let mouse_tap = if !selection_enabled {
        RuntimeCapabilityState::with_status(CapabilityHealth::Disabled, "selection_disabled")
    } else if !automatic_selection_enabled {
        RuntimeCapabilityState::with_status(
            CapabilityHealth::Disabled,
            "windows_auto_selection_disabled_by_setting",
        )
    } else if input_running {
        RuntimeCapabilityState::with_status(CapabilityHealth::Ready, "windows_raw_mouse_ready")
    } else {
        RuntimeCapabilityState::with_status(
            CapabilityHealth::Degraded,
            "windows_raw_mouse_unavailable",
        )
    };
    let selection_observer = if !selection_enabled {
        RuntimeCapabilityState::with_status(CapabilityHealth::Disabled, "selection_disabled")
    } else if !automatic_selection_enabled {
        RuntimeCapabilityState::with_status(
            CapabilityHealth::Disabled,
            "windows_auto_selection_disabled_by_setting",
        )
    } else if automatic_running {
        RuntimeCapabilityState::with_status(CapabilityHealth::Ready, "windows_uia_observer_ready")
    } else {
        RuntimeCapabilityState::with_status(
            CapabilityHealth::Degraded,
            "windows_uia_observer_unavailable",
        )
    };

    CapabilitySnapshot {
        accessibility: no_permission_required,
        listen_event,
        mouse_tap,
        key_tap: RuntimeCapabilityState::with_status(input_health, input_code),
        ax_selected_text_observer: selection_observer,
        direct_selection_read: RuntimeCapabilityState::with_status(
            if shortcut_registered && selection_enabled {
                CapabilityHealth::Unknown
            } else {
                shortcut_health
            },
            if shortcut_registered && selection_enabled {
                "windows_uia_not_probed"
            } else {
                shortcut_code
            },
        ),
        clipboard_double_copy_fallback: RuntimeCapabilityState::with_status(
            input_health,
            if input_running && selection_enabled {
                "windows_double_copy_ready"
            } else {
                input_code
            },
        ),
    }
}

#[cfg(feature = "local-api-key-file")]
fn local_api_key_path(data_dir: &Path) -> PathBuf {
    data_dir.join(LOCAL_API_KEY_FILE_NAME)
}

#[cfg(all(feature = "local-api-key-file", unix))]
fn local_api_key_component(value: &std::ffi::OsStr) -> Result<std::ffi::CString, String> {
    use std::os::unix::ffi::OsStrExt;

    std::ffi::CString::new(value.as_bytes())
        .map_err(|_| "本地 API Key 路径包含无效的空字符。".to_owned())
}

#[cfg(all(feature = "local-api-key-file", unix))]
fn open_private_local_api_key_directory(data_dir: &Path) -> Result<fs::File, String> {
    use std::{
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        },
        path::Component,
    };

    // macOS publishes these root aliases as Apple-managed symlinks. Resolve
    // only those fixed aliases before the no-follow walk; every application-
    // controlled component remains subject to O_NOFOLLOW.
    #[cfg(target_os = "macos")]
    let normalized_data_dir = [
        (Path::new("/var"), Path::new("/private/var")),
        (Path::new("/tmp"), Path::new("/private/tmp")),
        (Path::new("/etc"), Path::new("/private/etc")),
    ]
    .into_iter()
    .find_map(|(alias, destination)| {
        data_dir
            .strip_prefix(alias)
            .ok()
            .map(|suffix| destination.join(suffix))
    });
    #[cfg(not(target_os = "macos"))]
    let normalized_data_dir: Option<PathBuf> = None;
    let traversal_data_dir = normalized_data_dir.as_deref().unwrap_or(data_dir);

    let mut components = Vec::new();
    for component in traversal_data_dir.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(value) => components.push(value.to_owned()),
            Component::ParentDir | Component::Prefix(_) => {
                return Err(format!(
                    "本地 API Key 数据目录不能包含父级跳转（{}）。",
                    data_dir.display()
                ));
            }
        }
    }
    if components.is_empty() {
        return Err(format!(
            "本地 API Key 数据目录不能是文件系统根目录（{}）。",
            data_dir.display()
        ));
    }

    let starting_directory = if traversal_data_dir.is_absolute() {
        Path::new("/")
    } else {
        Path::new(".")
    };
    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut current = options.open(starting_directory).map_err(|error| {
        format!(
            "打开本地 API Key 目录起点失败（{}）：{error}",
            starting_directory.display()
        )
    })?;

    for (index, component) in components.iter().enumerate() {
        let component = local_api_key_component(component.as_os_str())?;
        let mut descriptor = unsafe {
            libc::openat(
                current.as_raw_fd(),
                component.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            let open_error = std::io::Error::last_os_error();
            if open_error.raw_os_error() == Some(libc::ENOENT) {
                let created =
                    unsafe { libc::mkdirat(current.as_raw_fd(), component.as_ptr(), 0o700) };
                if created != 0 {
                    let create_error = std::io::Error::last_os_error();
                    if create_error.raw_os_error() != Some(libc::EEXIST) {
                        return Err(format!(
                            "创建本地 API Key 数据目录失败（{}）：{create_error}",
                            data_dir.display()
                        ));
                    }
                }
                descriptor = unsafe {
                    libc::openat(
                        current.as_raw_fd(),
                        component.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                };
            }
        }
        if descriptor < 0 {
            return Err(format!(
                "本地 API Key 数据目录含有不可安全跟随的路径组件（{}）：{}",
                data_dir.display(),
                std::io::Error::last_os_error()
            ));
        }
        current = unsafe { fs::File::from_raw_fd(descriptor) };

        if index + 1 == components.len() {
            let metadata = current.metadata().map_err(|error| {
                format!(
                    "检查本地 API Key 数据目录失败（{}）：{error}",
                    data_dir.display()
                )
            })?;
            if !metadata.is_dir() {
                return Err(format!(
                    "本地 API Key 数据根不是目录（{}）。",
                    data_dir.display()
                ));
            }
            if metadata.uid() != unsafe { libc::geteuid() } {
                return Err(format!(
                    "本地 API Key 数据根不属于当前用户（{}）。",
                    data_dir.display()
                ));
            }
            if metadata.permissions().mode() & 0o077 != 0 {
                let chmod_status = unsafe { libc::fchmod(current.as_raw_fd(), 0o700) };
                if chmod_status != 0 {
                    return Err(format!(
                        "无法将本地 API Key 数据根权限限制为当前用户（{}）：{}",
                        data_dir.display(),
                        std::io::Error::last_os_error()
                    ));
                }
                let tightened = current.metadata().map_err(|error| {
                    format!(
                        "复核本地 API Key 数据根权限失败（{}）：{error}",
                        data_dir.display()
                    )
                })?;
                if tightened.permissions().mode() & 0o077 != 0 {
                    return Err(format!(
                        "本地 API Key 数据根权限仍对其他用户开放（{}）。",
                        data_dir.display()
                    ));
                }
            }
        }
    }

    Ok(current)
}

#[cfg(all(feature = "local-api-key-file", unix))]
fn validate_opened_local_api_key_file(file: &fs::File, path: &Path) -> Result<(), String> {
    let metadata = file
        .metadata()
        .map_err(|error| format!("检查本地 API Key 文件失败（{}）：{error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "本地 API Key 路径不是普通文件（{}）。",
            path.display()
        ));
    }
    if metadata.len() > LOCAL_API_KEY_MAX_BYTES {
        return Err(format!("本地 API Key 文件大小异常（{}）。", path.display()));
    }
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if metadata.nlink() != 1 {
        return Err(format!(
            "本地 API Key 文件不得有多个硬链接（{}）。",
            path.display()
        ));
    }
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(format!(
            "本地 API Key 文件不属于当前用户（{}）。",
            path.display()
        ));
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "本地 API Key 文件权限必须仅限当前用户（{}）。",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(all(feature = "local-api-key-file", unix))]
fn validate_local_api_key_destination_at(
    directory: &fs::File,
    name: &std::ffi::CStr,
    path: &Path,
) -> Result<bool, String> {
    use std::os::fd::AsRawFd;

    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    let status = unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            metadata.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if status != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            return Ok(false);
        }
        return Err(format!(
            "检查本地 API Key 目录项失败（{}）：{error}",
            path.display()
        ));
    }
    let metadata = unsafe { metadata.assume_init() };
    if metadata.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Err(format!(
            "本地 API Key 目录项不是普通文件（{}）。",
            path.display()
        ));
    }
    if metadata.st_nlink != 1 {
        return Err(format!(
            "本地 API Key 目录项不得有多个硬链接（{}）。",
            path.display()
        ));
    }
    if metadata.st_uid != unsafe { libc::geteuid() } {
        return Err(format!(
            "本地 API Key 目录项不属于当前用户（{}）。",
            path.display()
        ));
    }
    Ok(true)
}

#[cfg(all(feature = "local-api-key-file", unix))]
fn read_local_api_key_unix(data_dir: &Path) -> Result<Option<String>, String> {
    use std::os::fd::{AsRawFd, FromRawFd};

    let directory = open_private_local_api_key_directory(data_dir)?;
    let path = local_api_key_path(data_dir);
    let name = local_api_key_component(std::ffi::OsStr::new(LOCAL_API_KEY_FILE_NAME))?;
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            return Ok(None);
        }
        return Err(format!(
            "打开本地 API Key 文件失败（{}）：{error}",
            path.display()
        ));
    }
    let mut file = unsafe { fs::File::from_raw_fd(descriptor) };
    validate_opened_local_api_key_file(&file, &path)?;
    let mut value = String::new();
    file.read_to_string(&mut value)
        .map_err(|error| format!("读取本地 API Key 文件失败（{}）：{error}", path.display()))?;
    let value = value.trim().to_owned();
    Ok((!value.is_empty()).then_some(value))
}

#[cfg(feature = "local-api-key-file")]
fn read_local_api_key(data_dir: &Path) -> Result<Option<String>, String> {
    #[cfg(unix)]
    {
        read_local_api_key_unix(data_dir)
    }
    #[cfg(not(unix))]
    {
        let _ = data_dir;
        Err("本地 API Key 文件模式仅支持 Unix 系统。".to_owned())
    }
}

#[cfg(all(feature = "local-api-key-file", unix))]
fn write_local_api_key_unix(data_dir: &Path, value: &str) -> Result<(), String> {
    use std::os::fd::{AsRawFd, FromRawFd};

    let value = value.trim();
    if value.is_empty() {
        return delete_local_api_key_unix(data_dir);
    }
    if value.len() as u64 > LOCAL_API_KEY_MAX_BYTES {
        return Err("API Key 长度异常，已拒绝保存。".to_owned());
    }

    let path = local_api_key_path(data_dir);
    let sequence = ATOMIC_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary_name = format!(
        ".{LOCAL_API_KEY_FILE_NAME}.tmp-{}-{sequence}",
        std::process::id()
    );
    let temporary_path = data_dir.join(&temporary_name);
    let target_name = local_api_key_component(std::ffi::OsStr::new(LOCAL_API_KEY_FILE_NAME))?;
    let temporary_name = local_api_key_component(std::ffi::OsStr::new(&temporary_name))?;
    let _write_guard = ATOMIC_FILE_WRITE_LOCK.lock().map_err(lock_error)?;
    let directory = open_private_local_api_key_directory(data_dir)?;
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            temporary_name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if descriptor < 0 {
        return Err(format!(
            "创建本地 API Key 临时文件失败（{}）：{}",
            temporary_path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let file = unsafe { fs::File::from_raw_fd(descriptor) };
    let mut file = Some(file);
    let mut temporary_owned = true;
    let result = (|| {
        let writer = file
            .as_mut()
            .ok_or_else(|| "本地 API Key 临时文件已意外关闭。".to_owned())?;
        writer.write_all(value.as_bytes()).map_err(|error| {
            format!(
                "写入本地 API Key 临时文件失败（{}）：{error}",
                temporary_path.display()
            )
        })?;
        writer.sync_all().map_err(|error| {
            format!(
                "同步本地 API Key 临时文件失败（{}）：{error}",
                temporary_path.display()
            )
        })?;
        validate_opened_local_api_key_file(writer, &temporary_path)?;
        drop(file.take());

        validate_local_api_key_destination_at(&directory, &target_name, &path)?;
        let rename_status = unsafe {
            libc::renameat(
                directory.as_raw_fd(),
                temporary_name.as_ptr(),
                directory.as_raw_fd(),
                target_name.as_ptr(),
            )
        };
        if rename_status != 0 {
            return Err(format!(
                "提交本地 API Key 文件失败（{}）：{}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        temporary_owned = false;
        directory.sync_all().map_err(|error| {
            format!(
                "同步本地 API Key 目录失败（{}）：{error}",
                data_dir.display()
            )
        })
    })();
    drop(file.take());
    if result.is_err() && temporary_owned {
        let cleanup_status =
            unsafe { libc::unlinkat(directory.as_raw_fd(), temporary_name.as_ptr(), 0) };
        if cleanup_status != 0 {
            let cleanup_error = std::io::Error::last_os_error();
            if cleanup_error.raw_os_error() != Some(libc::ENOENT) {
                return Err(format!(
                    "{}；清理本地 API Key 临时文件失败（{}）：{cleanup_error}",
                    result.expect_err("cleanup runs only after a write failure"),
                    temporary_path.display()
                ));
            }
        }
    }
    result
}

#[cfg(feature = "local-api-key-file")]
fn write_local_api_key(data_dir: &Path, value: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        write_local_api_key_unix(data_dir, value)
    }
    #[cfg(not(unix))]
    {
        let _ = (data_dir, value);
        Err("本地 API Key 文件模式仅支持 Unix 系统。".to_owned())
    }
}

#[cfg(all(feature = "local-api-key-file", unix))]
fn delete_local_api_key_unix(data_dir: &Path) -> Result<(), String> {
    use std::os::fd::AsRawFd;

    let path = local_api_key_path(data_dir);
    let _write_guard = ATOMIC_FILE_WRITE_LOCK.lock().map_err(lock_error)?;
    let directory = open_private_local_api_key_directory(data_dir)?;
    let name = local_api_key_component(std::ffi::OsStr::new(LOCAL_API_KEY_FILE_NAME))?;
    if !validate_local_api_key_destination_at(&directory, &name, &path)? {
        return Ok(());
    }
    let status = unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
    if status != 0 {
        return Err(format!(
            "清除本地 API Key 文件失败（{}）：{}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    directory.sync_all().map_err(|error| {
        format!(
            "同步已清除的本地 API Key 目录失败（{}）：{error}",
            data_dir.display()
        )
    })
}

#[cfg(feature = "local-api-key-file")]
fn delete_local_api_key(data_dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        delete_local_api_key_unix(data_dir)
    }
    #[cfg(not(unix))]
    {
        let _ = data_dir;
        Err("本地 API Key 文件模式仅支持 Unix 系统。".to_owned())
    }
}

fn write_clipboard(text: &str) -> Result<(), String> {
    Clipboard::new()
        .map_err(to_string)?
        .set_text(text.to_string())
        .map_err(to_string)
}

fn open_url(url: &str) -> Result<(), String> {
    Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(to_string)
}

fn resolve_popup_target_language(inner: &InnerState, value: Option<&str>) -> String {
    normalize_target_language(
        value.unwrap_or_else(|| {
            inner
                .popup_controller
                .snapshot()
                .target_language
                .as_deref()
                .unwrap_or(&inner.settings.target_language)
        }),
        &inner.settings.target_language,
    )
}

fn model_name(model: &DeepSeekModel) -> &'static str {
    match model {
        DeepSeekModel::Flash => "deepseek-v4-flash",
        DeepSeekModel::Pro => "deepseek-v4-pro",
    }
}

fn mode_name(mode: &TranslateMode) -> &'static str {
    match mode {
        TranslateMode::AcademicZh => "academic_zh",
        TranslateMode::Bilingual => "bilingual",
        TranslateMode::Terminology => "terminology",
    }
}

fn readable_deepseek_error(status: u16, body: &str) -> String {
    match status {
        401 | 403 => "DeepSeek API Key 无效或没有权限。".to_string(),
        429 => "DeepSeek 请求过于频繁，请稍后再试。".to_string(),
        500..=599 => "DeepSeek 服务暂时不可用，请稍后再试。".to_string(),
        _ if !body.is_empty() => format!("DeepSeek 请求失败：{body}"),
        _ => format!("DeepSeek 请求失败，状态码 {status}。"),
    }
}

fn readable_deepseek_transport_error(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return format!(
            "DeepSeek 请求超时；连接超时 {DEEPSEEK_CONNECT_TIMEOUT_SECS}s，总请求超时 {DEEPSEEK_REQUEST_TIMEOUT_SECS}s。"
        );
    }
    format!("DeepSeek 请求失败：{error}")
}

fn build_empty_clipboard_message() -> String {
    "剪贴板中没有可翻译文本。".to_string()
}

fn cache_epoch_seconds_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cache_timestamp_now() -> String {
    format!("{:020}", cache_epoch_seconds_now())
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn popup_control_error(error: PopupControlError) -> String {
    error.code().to_string()
}

fn log_popup_control_error<T>(result: Result<T, PopupControlError>) {
    if let Err(error) = result {
        eprintln!("{}", error.code());
    }
}

fn lock_error<T>(error: std::sync::PoisonError<T>) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NativeOwnerDropProbe {
        drop_count: Arc<AtomicU64>,
    }

    impl Drop for NativeOwnerDropProbe {
        fn drop(&mut self) {
            self.drop_count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn startup_gate_times_out_then_remains_ready() {
        let gate = StartupGate::default();
        assert!(gate.wait(Duration::from_millis(1)).is_err());
        gate.mark_ready().expect("startup gate should become ready");
        gate.mark_ready()
            .expect("marking ready should be idempotent");
        gate.wait(Duration::ZERO)
            .expect("a ready gate should never wait or time out");
    }

    #[test]
    fn startup_gate_wakes_a_waiting_settings_request() {
        let gate = Arc::new(StartupGate::default());
        let waiting_gate = gate.clone();
        let waiter = std::thread::spawn(move || waiting_gate.wait(Duration::from_secs(1)));
        gate.mark_ready().expect("startup gate should become ready");
        waiter
            .join()
            .expect("settings waiter should not panic")
            .expect("settings waiter should be released");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_cursor_conversion_respects_negative_origins_scale_and_half_open_bounds() {
        let point = windows_physical_to_monitor_logical(
            -960.0,
            540.0,
            PhysicalPoint { x: -1920, y: 0 },
            GeometryPhysicalSize {
                width: 1920,
                height: 1080,
            },
            1.25,
        )
        .expect("point should be inside the negative-origin monitor");

        assert!((point.x + 768.0).abs() < f64::EPSILON);
        assert!((point.y - 432.0).abs() < f64::EPSILON);
        assert!(windows_physical_to_monitor_logical(
            0.0,
            540.0,
            PhysicalPoint { x: -1920, y: 0 },
            GeometryPhysicalSize {
                width: 1920,
                height: 1080,
            },
            1.25,
        )
        .is_none());
        assert!(windows_physical_to_monitor_logical(
            -960.0,
            1080.0,
            PhysicalPoint { x: -1920, y: 0 },
            GeometryPhysicalSize {
                width: 1920,
                height: 1080,
            },
            1.25,
        )
        .is_none());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_shortcut_capabilities_distinguish_ready_disabled_and_conflict() {
        let ready = windows_shortcut_capability_snapshot(true, true, true, false, false);
        assert_eq!(ready.key_tap.health, CapabilityHealth::Ready);
        assert_eq!(
            ready.key_tap.status_code.as_deref(),
            Some("windows_raw_input_ready")
        );
        assert_eq!(
            ready.direct_selection_read.health,
            CapabilityHealth::Unknown
        );
        assert_eq!(
            ready.clipboard_double_copy_fallback.health,
            CapabilityHealth::Ready
        );
        assert_eq!(ready.mouse_tap.health, CapabilityHealth::Disabled);
        assert_eq!(
            ready.mouse_tap.status_code.as_deref(),
            Some("windows_auto_selection_disabled_by_setting")
        );

        let automatic = windows_shortcut_capability_snapshot(true, true, true, true, true);
        assert_eq!(automatic.mouse_tap.health, CapabilityHealth::Ready);
        assert_eq!(
            automatic.ax_selected_text_observer.health,
            CapabilityHealth::Ready
        );

        let disabled = windows_shortcut_capability_snapshot(false, true, true, true, true);
        assert_eq!(disabled.key_tap.health, CapabilityHealth::Disabled);
        assert_eq!(
            disabled.key_tap.status_code.as_deref(),
            Some("selection_disabled")
        );

        let conflict = windows_shortcut_capability_snapshot(true, false, true, false, false);
        assert_eq!(conflict.key_tap.health, CapabilityHealth::Ready);
        assert_eq!(
            conflict.direct_selection_read.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(
            conflict.direct_selection_read.status_code.as_deref(),
            Some("windows_shortcut_registration_failed")
        );

        let input_unavailable = windows_shortcut_capability_snapshot(true, true, false, true, true);
        assert_eq!(
            input_unavailable.key_tap.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(
            input_unavailable.clipboard_double_copy_fallback.health,
            CapabilityHealth::Unavailable
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_permission_model_does_not_claim_a_global_input_consent() {
        assert_eq!(
            current_permission_grants(),
            (PermissionGrant::Granted, PermissionGrant::Granted)
        );
    }

    #[test]
    fn cleans_pdf_hyphenation() {
        assert_eq!(
            clean_selected_text("The pro-\nposed method", true),
            "The proposed method"
        );
    }

    #[test]
    fn prompt_supports_any_source_language() {
        let prompt = build_system_prompt(&TranslateMode::AcademicZh, "日文", &Glossary::new());
        assert!(prompt.contains("自动识别用户提供的原文语言"));
        assert!(prompt.contains("翻译为日文"));
        assert!(!prompt.contains("英文论文内容"));
    }

    #[test]
    fn cache_key_changes_by_target_language() {
        let zh = create_cache_key(
            &DeepSeekModel::Flash,
            &TranslateMode::AcademicZh,
            "中文",
            "v1",
            "hello",
        );
        let ja = create_cache_key(
            &DeepSeekModel::Flash,
            &TranslateMode::AcademicZh,
            "日文",
            "v1",
            "hello",
        );
        assert_ne!(zh, ja);
    }

    #[test]
    fn cache_is_bounded_and_evicts_the_oldest_entry() {
        let mut cache = CacheFile::default();
        for index in 0..=CACHE_MAX_ENTRIES {
            let key = format!("key-{index:04}");
            cache
                .entries
                .insert(key.clone(), test_cache_entry(key, format!("{index:08}")));
        }

        trim_cache_entries(&mut cache);

        assert_eq!(cache.entries.len(), CACHE_MAX_ENTRIES);
        assert!(!cache.entries.contains_key("key-0000"));
        assert!(cache
            .entries
            .contains_key(&format!("key-{CACHE_MAX_ENTRIES:04}")));
    }

    #[test]
    fn cache_pruning_enforces_ttl_and_rejects_legacy_or_implausible_timestamps() {
        let now = 2_000_000_000_u64;
        let mut cache = CacheFile::default();
        for (key, created_at) in [
            ("fresh", now),
            ("boundary", now - CACHE_MAX_AGE_SECS),
            ("expired", now - CACHE_MAX_AGE_SECS - 1),
            ("future-skew", now + CACHE_FUTURE_SKEW_SECS),
            ("future-invalid", now + CACHE_FUTURE_SKEW_SECS + 1),
        ] {
            cache.entries.insert(
                key.to_owned(),
                test_cache_entry(key.to_owned(), format!("{created_at:020}")),
            );
        }
        cache.entries.insert(
            "legacy".to_owned(),
            test_cache_entry(
                "legacy".to_owned(),
                "SystemTime { tv_sec: 1, tv_nsec: 0 }".to_owned(),
            ),
        );

        assert_eq!(prune_expired_cache_entries(&mut cache, now), 3);
        assert_eq!(
            cache
                .entries
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([
                "boundary".to_owned(),
                "fresh".to_owned(),
                "future-skew".to_owned(),
            ])
        );
    }

    #[test]
    fn concurrent_cache_updates_preserve_every_entry() {
        let directory = unique_test_directory("cache-rmw");
        fs::create_dir_all(&directory).expect("test cache directory should be created");
        let mut workers = Vec::new();
        for index in 0..50 {
            let directory = directory.clone();
            workers.push(std::thread::spawn(move || {
                let key = format!("concurrent-{index:02}");
                insert_cache_entry(
                    &directory,
                    key.clone(),
                    test_cache_entry(key, cache_timestamp_now()),
                )
            }));
        }
        for worker in workers {
            worker
                .join()
                .expect("cache worker should not panic")
                .expect("cache update should succeed");
        }

        assert_eq!(read_cache(&directory).entries.len(), 50);
        fs::remove_dir_all(directory).expect("test cache directory should be removable");
    }

    #[test]
    fn atomic_json_writes_never_leave_a_partial_document() {
        let directory = unique_test_directory("atomic-json");
        fs::create_dir_all(&directory).expect("test data directory should be created");
        let path = Arc::new(directory.join("state.json"));
        let mut workers = Vec::new();
        for index in 0..32 {
            let path = path.clone();
            workers.push(std::thread::spawn(move || {
                save_json(&path, &serde_json::json!({ "index": index }))
            }));
        }
        for worker in workers {
            worker
                .join()
                .expect("json worker should not panic")
                .expect("atomic json write should succeed");
        }

        let value = read_json::<serde_json::Value>(&path).expect("final JSON should be valid");
        assert!(value["index"].as_u64().is_some_and(|index| index < 32));
        assert_eq!(
            fs::read_dir(&directory)
                .expect("test data directory should be readable")
                .count(),
            1
        );
        fs::remove_dir_all(directory).expect("test data directory should be removable");
    }

    #[test]
    fn atomic_json_write_preserves_a_preexisting_temporary_file() {
        let directory = unique_test_directory("atomic-preexisting-temp-file");
        fs::create_dir_all(&directory).expect("test directory should be created");
        let path = directory.join("state.json");
        let temporary_path = directory.join(".state.json.tmp-controlled");
        fs::write(&temporary_path, b"preexisting-temporary-sentinel")
            .expect("temporary sentinel should be written");

        write_atomic_file(&path, &temporary_path, br#"{"replacement":true}"#)
            .expect_err("create_new must reject a pre-existing temporary file");

        assert_eq!(
            fs::read(&temporary_path).expect("pre-existing temporary file must remain"),
            b"preexisting-temporary-sentinel"
        );
        assert!(!path.exists());
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_json_write_preserves_a_preexisting_temporary_symlink_and_outside_sentinel() {
        use std::os::unix::fs::symlink;

        let directory = unique_test_directory("atomic-preexisting-temp-symlink");
        fs::create_dir_all(&directory).expect("test directory should be created");
        let path = directory.join("state.json");
        let temporary_path = directory.join(".state.json.tmp-controlled");
        let outside = directory.join("outside-sentinel.json");
        fs::write(&outside, b"outside-sentinel").expect("outside sentinel should be written");
        symlink(&outside, &temporary_path).expect("temporary symlink should be created");

        write_atomic_file(&path, &temporary_path, br#"{"replacement":true}"#)
            .expect_err("create_new must reject a pre-existing temporary symlink");

        assert_eq!(
            fs::read(&outside).expect("outside sentinel must remain readable"),
            b"outside-sentinel"
        );
        assert!(fs::symlink_metadata(&temporary_path)
            .expect("pre-existing symlink must not be removed")
            .file_type()
            .is_symlink());
        assert!(!path.exists());
        fs::remove_file(&temporary_path).expect("temporary symlink should be removable");
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_json_write_rejects_destination_symlink_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let directory = unique_test_directory("atomic-destination-symlink");
        fs::create_dir_all(&directory).expect("test directory should be created");
        let path = directory.join("state.json");
        let temporary_path = directory.join(".state.json.tmp-controlled");
        let outside = directory.join("outside-sentinel.json");
        fs::write(&outside, b"outside-sentinel").expect("outside sentinel should be written");
        symlink(&outside, &path).expect("destination symlink should be created");

        write_atomic_file(&path, &temporary_path, br#"{"replacement":true}"#)
            .expect_err("a destination symlink must be rejected");

        assert_eq!(
            fs::read(&outside).expect("outside sentinel must remain readable"),
            b"outside-sentinel"
        );
        assert!(fs::symlink_metadata(&path)
            .expect("destination symlink must remain")
            .file_type()
            .is_symlink());
        assert!(!temporary_path.exists());
        fs::remove_file(&path).expect("destination symlink should be removable");
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_json_write_rejects_destination_hard_link_without_touching_its_inode() {
        let directory = unique_test_directory("atomic-destination-hard-link");
        fs::create_dir_all(&directory).expect("test directory should be created");
        let path = directory.join("state.json");
        let temporary_path = directory.join(".state.json.tmp-controlled");
        let outside = directory.join("outside-sentinel.json");
        fs::write(&outside, b"outside-sentinel").expect("outside sentinel should be written");
        fs::hard_link(&outside, &path).expect("destination hard link should be created");

        write_atomic_file(&path, &temporary_path, br#"{"replacement":true}"#)
            .expect_err("a multiply-linked destination must be rejected");

        assert_eq!(
            fs::read(&outside).expect("outside sentinel must remain readable"),
            b"outside-sentinel"
        );
        assert_eq!(
            fs::read(&path).expect("destination hard link must remain readable"),
            b"outside-sentinel"
        );
        assert!(!temporary_path.exists());
        fs::remove_file(&path).expect("destination hard link should be removable");
        assert_eq!(
            fs::read(&outside).expect("outside sentinel must survive link removal"),
            b"outside-sentinel"
        );
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[test]
    fn settings_from_legacy_value_fills_defaults() {
        let raw = serde_json::json!({
            "model": "deepseek-v4-pro",
            "cleanPdfText": false,
            "enableCache": false,
            "popupWidth": "520",
            "shortcut": "CommandOrControl+Shift+Y",
            "triggerMode": "clipboard_shortcut"
        });

        let settings = normalize_settings(settings_from_value(&raw));

        assert_eq!(settings.model, DeepSeekModel::Pro);
        assert_eq!(settings.mode, TranslateMode::AcademicZh);
        assert!(!settings.clean_pdf_text);
        assert!(!settings.enable_cache);
        assert!(settings.enable_selection_popup);
        assert_eq!(settings.target_language, "中文");
        assert_eq!(settings.popup_width, 520);
    }

    #[test]
    fn missing_settings_file_is_the_only_load_case_that_uses_defaults() {
        let directory = unique_test_directory("settings-missing");
        assert!(!directory.exists());

        let settings = load_settings(&directory).expect("missing settings should use defaults");

        assert_eq!(settings.model, DeepSeekModel::Flash);
        assert_eq!(settings.mode, TranslateMode::AcademicZh);
        assert!(settings.clean_pdf_text);
        assert!(settings.enable_cache);
        assert!(settings.enable_selection_popup);
        assert_eq!(settings.target_language, DEFAULT_TARGET_LANGUAGE);
        assert_eq!(settings.popup_width, 420);
    }

    #[test]
    fn valid_settings_file_loads_and_normalizes_production_values() {
        let directory = unique_test_directory("settings-valid");
        fs::create_dir_all(&directory).expect("test settings directory should be created");
        save_json(
            &settings_path(&directory),
            &serde_json::json!({
                "model": "deepseek-v4-pro",
                "mode": "bilingual",
                "cleanPdfText": false,
                "enableCache": false,
                "enableSelectionPopup": false,
                "targetLanguage": "  日文  ",
                "popupWidth": 900
            }),
        )
        .expect("valid settings fixture should be saved");

        let settings = load_settings(&directory).expect("valid settings should load");

        assert_eq!(settings.model, DeepSeekModel::Pro);
        assert_eq!(settings.mode, TranslateMode::Bilingual);
        assert!(!settings.clean_pdf_text);
        assert!(!settings.enable_cache);
        assert!(!settings.enable_selection_popup);
        assert_eq!(settings.target_language, "日文");
        assert_eq!(settings.popup_width, 640);
        fs::remove_dir_all(directory).expect("test settings directory should be removable");
    }

    #[test]
    fn corrupt_settings_file_returns_a_visible_error_instead_of_defaults() {
        let directory = unique_test_directory("settings-corrupt");
        fs::create_dir_all(&directory).expect("test settings directory should be created");
        fs::write(settings_path(&directory), b"{ not-valid-json")
            .expect("corrupt settings fixture should be written");

        let error = load_settings(&directory).expect_err("corrupt settings must fail");

        assert!(error.contains("设置文件已损坏或不是有效 JSON"));
        assert!(error.contains("settings.json"));
        fs::remove_dir_all(directory).expect("test settings directory should be removable");
    }

    #[test]
    fn settings_io_failure_returns_a_visible_error_instead_of_defaults() {
        let directory = unique_test_directory("settings-io-error");
        fs::create_dir_all(settings_path(&directory))
            .expect("directory-shaped settings fixture should be created");

        let error = load_settings(&directory).expect_err("settings I/O failure must fail");

        assert!(error.contains("读取设置文件失败") || error.contains("验收设置文件类型异常"));
        assert!(error.contains("settings.json"));
        fs::remove_dir_all(directory).expect("test settings directory should be removable");
    }

    #[test]
    fn inaccessible_data_directory_returns_a_visible_error_before_settings_load() {
        let path = unique_test_directory("settings-data-dir-error");
        fs::write(&path, b"not a directory")
            .expect("file-shaped data directory fixture should be written");

        let error = ensure_data_dir(&path).expect_err("data directory failure must stay visible");

        assert!(error.contains("访问应用数据目录失败") || error.contains("验收数据根目录类型异常"));
        assert!(error.contains("settings-data-dir-error"));
        fs::remove_file(path).expect("data directory fixture should be removable");
    }

    #[cfg(not(feature = "local-api-key-file"))]
    #[test]
    fn keychain_read_status_distinguishes_missing_from_system_failures() {
        assert_eq!(
            classify_keychain_read_status(0, true),
            Ok(KeychainReadDisposition::Present)
        );
        assert_eq!(
            classify_keychain_read_status(KEYCHAIN_ITEM_NOT_FOUND, false),
            Ok(KeychainReadDisposition::Missing)
        );

        let system_error = classify_keychain_read_status(-25308, false)
            .expect_err("a Keychain OSStatus must stay visible");
        assert!(system_error.contains("-25308"));
        assert!(classify_keychain_read_status(0, false)
            .expect_err("success without data must fail")
            .contains("没有返回 API Key 数据"));
        assert!(classify_keychain_read_status(KEYCHAIN_ITEM_NOT_FOUND, true)
            .expect_err("missing with data must fail")
            .contains("同时返回了数据"));
    }

    #[cfg(not(feature = "local-api-key-file"))]
    #[test]
    fn keychain_presence_status_distinguishes_missing_from_system_failures() {
        assert_eq!(classify_keychain_presence_status(0), Ok(true));
        assert_eq!(
            classify_keychain_presence_status(KEYCHAIN_ITEM_NOT_FOUND),
            Ok(false)
        );

        let system_error = classify_keychain_presence_status(-25308)
            .expect_err("a Keychain presence OSStatus must stay visible");
        assert!(system_error.contains("-25308"));
        assert!(system_error.contains("检查系统钥匙串"));
    }

    #[cfg(not(feature = "local-api-key-file"))]
    #[test]
    fn settings_keychain_summary_keeps_read_failures_visible_without_blocking_settings() {
        assert_eq!(
            serde_json::to_value(ApiKeyStatus::Configured).expect("status should serialize"),
            serde_json::json!("configured")
        );
        assert_eq!(
            serde_json::to_value(ApiKeyStatus::Unavailable).expect("status should serialize"),
            serde_json::json!("unavailable")
        );
        assert_eq!(
            summarize_api_key_presence_for_settings(Ok(true)),
            (ApiKeyStatus::Configured, None)
        );
        assert_eq!(
            summarize_api_key_presence_for_settings(Ok(false)),
            (ApiKeyStatus::Missing, None)
        );

        let (api_key_status, warning) = summarize_api_key_presence_for_settings(Err(
            "读取系统钥匙串失败（状态 -25308）。".to_owned(),
        ));
        assert_eq!(api_key_status, ApiKeyStatus::Unavailable);
        assert!(!api_key_status.has_api_key());
        let warning = warning.expect("Keychain read failure must remain visible in Settings");
        assert!(warning.contains("-25308"));
        assert!(warning.contains("避免系统密码弹窗"));
        assert!(warning.contains("重新保存"));
        assert!(!warning.contains("secret"));
    }

    #[cfg(not(feature = "local-api-key-file"))]
    #[test]
    fn keychain_namespace_matches_the_compiled_artifact_class() {
        #[cfg(not(feature = "acceptance-testing"))]
        {
            assert_eq!(KEYCHAIN_SERVICE, "Paper Float Translator API Key v2");
            assert_eq!(DEEPSEEK_ACCOUNT, "deepseek-api-key-v2");
        }
        #[cfg(feature = "acceptance-testing")]
        {
            assert_eq!(
                KEYCHAIN_SERVICE,
                "Paper Float Translator Acceptance API Key v2"
            );
            assert_eq!(DEEPSEEK_ACCOUNT, "deepseek-api-key-acceptance-v2");
        }
    }

    #[cfg(all(feature = "local-api-key-file", unix))]
    #[test]
    fn local_api_key_round_trip_uses_private_permissions_and_never_needs_keychain() {
        let directory = unique_test_directory("local-api-key-round-trip");
        write_local_api_key(&directory, "  test-key-value  ")
            .expect("local API key write should succeed");

        assert_eq!(compiled_api_key_storage(), ApiKeyStorage::LocalFile);
        assert_eq!(
            read_local_api_key(&directory).expect("local API key read should succeed"),
            Some("test-key-value".to_owned())
        );
        assert_eq!(
            get_api_key_status_for_settings(&directory),
            (ApiKeyStatus::Configured, None)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let directory_mode = fs::metadata(&directory)
                .expect("local API key directory metadata should exist")
                .permissions()
                .mode();
            let mode = fs::metadata(local_api_key_path(&directory))
                .expect("local API key metadata should exist")
                .permissions()
                .mode();
            assert_eq!(directory_mode & 0o077, 0);
            assert_eq!(mode & 0o077, 0);
        }

        delete_local_api_key(&directory).expect("local API key deletion should succeed");
        assert_eq!(
            read_local_api_key(&directory).expect("deleted local API key should be missing"),
            None
        );
        fs::remove_dir_all(directory).expect("test directory should be removable");
    }

    #[cfg(all(feature = "local-api-key-file", unix))]
    #[test]
    fn local_api_key_rejects_symlinked_data_root_and_ancestor() {
        use std::os::unix::fs::symlink;

        let outside = unique_test_directory("local-api-key-outside-root");
        fs::create_dir_all(&outside).expect("outside data root should exist");

        let symlinked_root = unique_test_directory("local-api-key-symlinked-root");
        symlink(&outside, &symlinked_root).expect("symlinked data root should exist");
        let root_error = write_local_api_key(&symlinked_root, "must-not-escape")
            .expect_err("symlinked data root must be rejected");
        assert!(root_error.contains("不可安全跟随"));
        assert!(!local_api_key_path(&outside).exists());
        fs::remove_file(&symlinked_root).expect("symlinked data root should be removable");

        let parent = unique_test_directory("local-api-key-symlinked-ancestor");
        fs::create_dir_all(&parent).expect("ancestor test parent should exist");
        let linked_ancestor = parent.join("linked");
        symlink(&outside, &linked_ancestor).expect("symlinked ancestor should exist");
        let nested_root = linked_ancestor.join("app-data");
        let ancestor_error = read_local_api_key(&nested_root)
            .expect_err("symlinked data-root ancestor must be rejected");
        assert!(ancestor_error.contains("不可安全跟随"));
        assert!(!outside.join("app-data").exists());

        fs::remove_dir_all(parent).expect("ancestor fixture should be removable");
        fs::remove_dir_all(outside).expect("outside data root should be removable");
    }

    #[cfg(all(feature = "local-api-key-file", unix))]
    #[test]
    fn local_api_key_read_rejects_symlink_and_hard_link_aliases() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let symlink_directory = unique_test_directory("local-api-key-symlink");
        fs::create_dir_all(&symlink_directory).expect("symlink test directory should exist");
        let outside = symlink_directory.with_extension("outside-secret");
        fs::write(&outside, "outside-secret").expect("outside sentinel should exist");
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o600))
            .expect("outside sentinel permissions should be private");
        symlink(&outside, local_api_key_path(&symlink_directory))
            .expect("managed symlink should be created");
        assert!(read_local_api_key(&symlink_directory)
            .expect_err("symlink API key must be rejected")
            .contains("打开本地 API Key 文件失败"));
        assert_eq!(
            fs::read_to_string(&outside).expect("outside sentinel should remain readable"),
            "outside-secret"
        );
        fs::remove_dir_all(&symlink_directory).expect("symlink directory should be removable");
        fs::remove_file(&outside).expect("outside sentinel should be removable");

        let hardlink_directory = unique_test_directory("local-api-key-hardlink");
        fs::create_dir_all(&hardlink_directory).expect("hard-link test directory should exist");
        let source = hardlink_directory.join("source");
        fs::write(&source, "hard-linked-secret").expect("hard-link source should exist");
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600))
            .expect("hard-link source permissions should be private");
        fs::hard_link(&source, local_api_key_path(&hardlink_directory))
            .expect("managed hard link should be created");
        assert!(read_local_api_key(&hardlink_directory)
            .expect_err("hard-linked API key must be rejected")
            .contains("多个硬链接"));
        assert_eq!(
            fs::read_to_string(&source).expect("hard-link source should remain readable"),
            "hard-linked-secret"
        );
        fs::remove_dir_all(hardlink_directory).expect("hard-link directory should be removable");
    }

    #[cfg(all(feature = "local-api-key-file", unix))]
    #[test]
    fn local_api_key_read_rejects_group_or_world_access() {
        use std::os::unix::fs::PermissionsExt;

        let directory = unique_test_directory("local-api-key-permissions");
        fs::create_dir_all(&directory).expect("permissions test directory should exist");
        let path = local_api_key_path(&directory);
        fs::write(&path, "insecure-secret").expect("insecure test file should exist");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
            .expect("insecure permissions should be installed");
        assert!(read_local_api_key(&directory)
            .expect_err("group-readable API key must be rejected")
            .contains("权限必须仅限当前用户"));
        fs::remove_dir_all(directory).expect("permissions test directory should be removable");
    }

    #[cfg(all(feature = "local-api-key-file", unix))]
    #[test]
    fn local_api_key_read_rejects_fifo_without_blocking() {
        use std::os::unix::ffi::OsStrExt;

        let directory = unique_test_directory("local-api-key-fifo");
        fs::create_dir_all(&directory).expect("FIFO test directory should exist");
        let path = local_api_key_path(&directory);
        let raw_path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .expect("FIFO path should not contain NUL");
        assert_eq!(unsafe { libc::mkfifo(raw_path.as_ptr(), 0o600) }, 0);
        let error = read_local_api_key(&directory)
            .expect_err("FIFO API key entry must be rejected without waiting for a writer");
        assert!(error.contains("不是普通文件"));
        fs::remove_dir_all(directory).expect("FIFO test directory should be removable");
    }

    #[test]
    fn compiled_artifact_identity_is_bidirectional_and_fail_closed() {
        assert!(validate_compiled_runtime_identity(
            EXPECTED_BUNDLE_IDENTIFIER,
            EXPECTED_PRODUCT_NAME
        )
        .is_ok());
        assert!(validate_compiled_runtime_identity(
            "com.example.wrong-artifact",
            EXPECTED_PRODUCT_NAME
        )
        .is_err());
        assert!(validate_compiled_runtime_identity(
            EXPECTED_BUNDLE_IDENTIFIER,
            "Wrong Artifact Name"
        )
        .is_err());
        #[cfg(not(feature = "acceptance-testing"))]
        assert!(validate_compiled_runtime_identity(
            "com.paperfloat.translator.acceptance",
            "Paper Float Translator Acceptance"
        )
        .is_err());
        #[cfg(feature = "acceptance-testing")]
        assert!(validate_compiled_runtime_identity(
            "com.paperfloat.translator",
            "Paper Float Translator"
        )
        .is_err());
    }

    #[test]
    fn acceptance_self_interaction_window_is_derived_only_from_known_backend_labels() {
        assert_eq!(
            acceptance_self_interaction_window_from_label("settings"),
            Ok(AcceptanceSelfInteractionWindow::Settings)
        );
        assert_eq!(
            acceptance_self_interaction_window_from_label("popup"),
            Ok(AcceptanceSelfInteractionWindow::Popup)
        );
        for unknown in ["", "main", "settings-child", "popup-preview"] {
            assert!(acceptance_self_interaction_window_from_label(unknown).is_err());
        }
    }

    #[cfg(feature = "acceptance-testing")]
    #[test]
    fn acceptance_data_directory_is_fail_closed() {
        let fixture = unique_test_directory("acceptance-expected-root");
        let isolated = fixture.join(EXPECTED_BUNDLE_IDENTIFIER);
        let production = fixture.join("com.paperfloat.translator");
        assert!(validate_acceptance_data_dir_against(&isolated, &isolated).is_ok());
        assert!(validate_acceptance_data_dir_against(&production, &isolated).is_err());
        assert!(validate_acceptance_data_dir_against(Path::new("/"), &isolated).is_err());
    }

    #[cfg(feature = "acceptance-testing")]
    #[test]
    fn acceptance_managed_path_escape_is_fail_closed() {
        let root = unique_test_directory("acceptance-path-escape").join(EXPECTED_BUNDLE_IDENTIFIER);
        let escaped = root
            .join("..")
            .join("com.paperfloat.translator")
            .join("settings.json");

        let error = validate_acceptance_managed_path(&root, &escaped)
            .expect_err("a lexical path escape must be rejected");

        assert!(error.contains("越出专用名称空间"));
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_data_root_symlink_is_fail_closed() {
        use std::os::unix::fs::symlink;

        let fixture = unique_test_directory("acceptance-root-symlink");
        let data_root = fixture.join(EXPECTED_BUNDLE_IDENTIFIER);
        let outside = fixture.join("formal-data");
        fs::create_dir_all(&outside).expect("outside root should be created");
        symlink(&outside, &data_root).expect("root symlink fixture should be created");

        let error = validate_acceptance_data_dir_against(&data_root, &data_root)
            .expect_err("the acceptance root must never be a symlink");

        assert!(error.contains("验收数据根目录不得是符号链接"));
        fs::remove_file(&data_root).expect("root symlink should be removable");
        fs::remove_dir_all(fixture).expect("root symlink fixture should be removable");
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_report_directory_symlink_is_fail_closed() {
        assert_acceptance_managed_symlink_rejected(Path::new("acceptance"), true);
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_settings_file_symlink_is_fail_closed() {
        assert_acceptance_managed_symlink_rejected(Path::new("settings.json"), false);
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_cache_file_symlink_is_fail_closed() {
        assert_acceptance_managed_symlink_rejected(Path::new("cache.json"), false);
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_glossary_file_symlink_is_fail_closed() {
        assert_acceptance_managed_symlink_rejected(Path::new("glossary.json"), false);
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_report_file_symlink_is_fail_closed() {
        assert_acceptance_managed_symlink_rejected(
            Path::new("acceptance")
                .join(acceptance_runtime::ACCEPTANCE_EXPORT_FILE)
                .as_path(),
            false,
        );
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_settings_hard_link_is_fail_closed() {
        assert_acceptance_managed_hard_link_rejected(Path::new("settings.json"));
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_cache_hard_link_is_fail_closed() {
        assert_acceptance_managed_hard_link_rejected(Path::new("cache.json"));
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_glossary_hard_link_is_fail_closed() {
        assert_acceptance_managed_hard_link_rejected(Path::new("glossary.json"));
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    #[test]
    fn acceptance_report_hard_link_is_fail_closed() {
        assert_acceptance_managed_hard_link_rejected(
            Path::new("acceptance")
                .join(acceptance_runtime::ACCEPTANCE_EXPORT_FILE)
                .as_path(),
        );
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    fn assert_acceptance_managed_symlink_rejected(relative_path: &Path, target_is_directory: bool) {
        use std::os::unix::fs::symlink;

        let fixture = unique_test_directory("acceptance-managed-symlink");
        let data_root = fixture.join(EXPECTED_BUNDLE_IDENTIFIER);
        fs::create_dir_all(&data_root).expect("acceptance root should be created");
        let outside = fixture.join("formal-target");
        if target_is_directory {
            fs::create_dir_all(&outside).expect("outside directory should be created");
        } else {
            fs::write(&outside, b"formal-sentinel").expect("outside file should be created");
        }
        let link = data_root.join(relative_path);
        fs::create_dir_all(link.parent().expect("managed path parent"))
            .expect("managed path parent should be created");
        symlink(&outside, &link).expect("managed symlink fixture should be created");

        let error = validate_acceptance_storage_namespace(&data_root)
            .expect_err("a managed symlink must be rejected");

        assert!(
            error.contains("不得是符号链接"),
            "unexpected error: {error}"
        );
        assert!(fs::symlink_metadata(&link)
            .expect("rejected managed symlink must remain in place")
            .file_type()
            .is_symlink());
        if !target_is_directory {
            assert_eq!(
                fs::read(&outside).expect("outside sentinel should remain readable"),
                b"formal-sentinel"
            );
        }
        fs::remove_file(&link).expect("managed symlink should be removable");
        fs::remove_dir_all(fixture).expect("managed symlink fixture should be removable");
    }

    #[cfg(all(feature = "acceptance-testing", unix))]
    fn assert_acceptance_managed_hard_link_rejected(relative_path: &Path) {
        let fixture = unique_test_directory("acceptance-managed-hard-link");
        let data_root = fixture.join(EXPECTED_BUNDLE_IDENTIFIER);
        fs::create_dir_all(&data_root).expect("acceptance root should be created");
        let outside = fixture.join("formal-sentinel");
        fs::write(&outside, b"formal-sentinel").expect("outside sentinel should be written");
        let link = data_root.join(relative_path);
        fs::create_dir_all(link.parent().expect("managed path parent"))
            .expect("managed path parent should be created");
        fs::hard_link(&outside, &link).expect("managed hard-link fixture should be created");

        let error = validate_acceptance_managed_path(&data_root, &link)
            .expect_err("a multiply-linked managed file must be rejected");
        let opened = fs::File::open(&link).expect("hard-link fixture should open for recheck");
        let opened_error = validate_opened_acceptance_file(&opened, &link)
            .expect_err("descriptor metadata must independently reject multiple links");

        assert!(
            error.contains("不得有多个硬链接"),
            "unexpected error: {error}"
        );
        assert!(
            opened_error.contains("不得有多个硬链接"),
            "unexpected opened-file error: {opened_error}"
        );
        assert_eq!(
            fs::read(&outside).expect("outside sentinel should remain readable"),
            b"formal-sentinel"
        );
        assert_eq!(
            fs::read(&link).expect("rejected hard link must remain in place"),
            b"formal-sentinel"
        );
        fs::remove_file(&link).expect("managed hard link should be removable");
        assert_eq!(
            fs::read(&outside).expect("outside sentinel should survive link removal"),
            b"formal-sentinel"
        );
        fs::remove_dir_all(fixture).expect("managed hard-link fixture should be removable");
    }

    #[cfg(feature = "acceptance-testing")]
    #[test]
    fn acceptance_storage_operations_preserve_production_sentinels() {
        let root = unique_test_directory("acceptance-storage-isolation");
        let production_root = root.join("com.paperfloat.translator");
        let acceptance_root = root.join(EXPECTED_BUNDLE_IDENTIFIER);
        let production_paths = [
            settings_path(&production_root),
            cache_path(&production_root),
            glossary_path(&production_root),
            acceptance_runtime::export_path(&production_root),
        ];
        for (index, path) in production_paths.iter().enumerate() {
            fs::create_dir_all(path.parent().expect("sentinel parent"))
                .expect("production sentinel directory should be created");
            fs::write(path, format!("production-sentinel-{index}").as_bytes())
                .expect("production sentinel should be written");
        }
        let production_before = production_paths
            .iter()
            .map(|path| fs::read(path).expect("production sentinel should be readable"))
            .collect::<Vec<_>>();

        save_managed_json(
            &acceptance_root,
            &settings_path(&acceptance_root),
            &AppSettings::default(),
        )
        .expect("acceptance settings should be isolated");
        clear_cache_file(&acceptance_root).expect("acceptance cache should be isolated");
        let mut glossary = Glossary::new();
        glossary.insert("acceptance".to_owned(), "isolated".to_owned());
        save_managed_json(
            &acceptance_root,
            &glossary_path(&acceptance_root),
            &glossary,
        )
        .expect("acceptance glossary should be isolated");
        assert!(load_settings(&acceptance_root).is_ok());
        assert!(read_cache(&acceptance_root).entries.is_empty());
        assert_eq!(load_glossary(&acceptance_root), glossary);

        let mut acceptance =
            AcceptanceRuntime::new(true, 64).expect("acceptance runtime should initialize");
        acceptance
            .start_full_baseline(test_acceptance_metadata(530))
            .expect("acceptance session should start");
        acceptance
            .end()
            .expect("incomplete baseline should produce a rejected report");
        let acceptance_report = acceptance_runtime::export_path(&acceptance_root);
        validate_acceptance_managed_path(&acceptance_root, &acceptance_report)
            .expect("acceptance report path should remain in the isolated namespace");
        acceptance
            .export_atomic(&acceptance_report)
            .expect("acceptance report should export only to the isolated root");
        validate_acceptance_managed_path(&acceptance_root, &acceptance_report)
            .expect("exported acceptance report should remain a regular managed file");
        fs::remove_file(&acceptance_report)
            .expect("acceptance report clear should remain isolated");

        for (index, path) in production_paths.iter().enumerate() {
            assert_eq!(
                fs::read(path).expect("production sentinel should remain readable"),
                production_before[index]
            );
        }
        fs::remove_dir_all(root).expect("isolation fixture should be removable");
    }

    #[test]
    fn settings_close_hides_during_background_operation_but_not_during_quit() {
        assert!(should_hide_settings_window_on_close(false));
        assert!(!should_hide_settings_window_on_close(true));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn user_exit_request_keeps_windows_background_agent_alive() {
        assert!(should_keep_background_agent_running(None, false));
        assert!(!should_keep_background_agent_running(None, true));
        assert!(!should_keep_background_agent_running(Some(0), false));
    }

    #[test]
    fn removed_translate_modes_normalize_to_academic() {
        assert_eq!(parse_mode("literal"), Some(TranslateMode::AcademicZh));
        assert_eq!(parse_mode("natural"), Some(TranslateMode::AcademicZh));
    }

    #[test]
    fn selection_permission_status_identifies_app_denial() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "accessibility_denied");

        assert!(!inner.selection_status.available);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("app_accessibility_denied")
        );
    }

    #[test]
    fn selection_not_trusted_status_is_diagnostic_only() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "accessibility_not_trusted");

        assert!(inner.selection_status.available);
        assert!(inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_accessibility_limited")
        );
    }

    #[test]
    fn selection_read_empty_updates_only_diagnostics_and_preserves_ready_capabilities() {
        let mut inner = test_inner_state("idle", None);
        handle_selection_status(&mut inner, "selection_sources_ready");
        let ready_capabilities = inner.capability_snapshot.clone();

        handle_selection_status(
            &mut inner,
            "selection_read_empty\tax_observer\tfalse\t0\t0.0\tcom.openai.codex\tobserver_context_unavailable\t102\t1\t3.1\ttrue",
        );

        assert!(inner.selection_status.available);
        assert!(inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_sources_ready")
        );
        assert_eq!(inner.capability_snapshot, ready_capabilities);
        let diagnostics = inner
            .last_selection_read
            .as_ref()
            .expect("selection read diagnostics should be stored");
        assert_eq!(diagnostics.reason, "ax_observer");
        assert_eq!(diagnostics.candidate_count, 0);
        assert_eq!(diagnostics.source_bundle_id, "com.openai.codex");
        assert_eq!(diagnostics.ax_error, "observer_context_unavailable");
        assert_eq!(diagnostics.generation, Some(102));
        assert_eq!(diagnostics.terminal, Some(true));

        handle_selection_status(&mut inner, "selection_empty");
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_sources_ready")
        );
        assert_eq!(inner.capability_snapshot, ready_capabilities);
    }

    #[test]
    fn selection_baseline_failure_degrades_direct_read_with_explicit_reason() {
        let mut inner = test_inner_state("idle", None);
        handle_selection_status(&mut inner, "selection_sources_ready");

        handle_selection_status(
            &mut inner,
            "selection_read_empty\tmouse_up_drag\tfalse\t0\t18.0\tcom.apple.Preview\tselection_baseline_element_timeout_setup_failed_-25205\t44\t1\t20.0\ttrue",
        );

        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_direct_baseline_degraded")
        );
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Degraded
        );
        assert_eq!(
            inner
                .capability_snapshot
                .direct_selection_read
                .status_code
                .as_deref(),
            Some("selection_baseline_element_timeout_setup_failed_-25205")
        );
        assert_eq!(
            inner
                .last_selection_read
                .as_ref()
                .map(|diagnostics| diagnostics.source_bundle_id.as_str()),
            Some("com.apple.Preview")
        );
        assert_eq!(
            inner.latched_selection_baseline_error.as_deref(),
            Some("selection_baseline_element_timeout_setup_failed_-25205")
        );
    }

    #[test]
    fn selection_baseline_degradation_survives_refresh_until_real_mouse_read_found() {
        let mut inner = test_inner_state("idle", None);
        handle_selection_status(&mut inner, "selection_sources_ready");
        handle_selection_status(
            &mut inner,
            "selection_read_empty\tmouse_up_drag\tfalse\t0\t18.0\tcom.apple.Preview\tselection_baseline_anchor_read_failed_-25204\t44\t1\t20.0\ttrue",
        );

        // Refresh resets native capabilities and replays its startup status.
        // The preserved terminal external failure must still win.
        inner.capability_snapshot = initial_capability_snapshot();
        inner.selection_status = initial_selection_status();
        handle_selection_status(&mut inner, "selection_mouse_ready_ax_observer_limited");
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_direct_baseline_degraded")
        );
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Degraded
        );

        handle_selection_status(
            &mut inner,
            "selection_read_found\tmouse_up_drag\ttrue\t1\t7.0\tcom.apple.Preview\tnone\t45\t1\t8.0\ttrue",
        );
        assert!(inner.latched_selection_baseline_error.is_none());
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_direct_read_recovered")
        );
    }

    #[test]
    fn disabling_selection_clears_latched_baseline_degradation() {
        let mut inner = test_inner_state("idle", None);
        inner.latched_selection_baseline_error =
            Some("selection_baseline_anchor_read_failed_-25204".to_owned());

        handle_selection_status(&mut inner, "selection_disabled_by_setting");

        assert!(inner.latched_selection_baseline_error.is_none());
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Disabled
        );
    }

    #[test]
    fn settings_window_mouse_up_cannot_replace_external_selection_diagnostics() {
        let mut inner = test_inner_state("idle", None);
        handle_selection_status(
            &mut inner,
            "selection_read_empty\tmouse_up_drag\tfalse\t0\t10.0\tcom.apple.Preview\tno_selected_text\t42\t3\t650.0\ttrue",
        );
        let external = inner
            .last_selection_read
            .clone()
            .expect("external Preview diagnostics should be stored");

        handle_selection_status_for_target(
            &mut inner,
            "selection_read_empty\tmouse_up_drag\tfalse\t0\t0.0\tcom.paperfloat.translator\tself_process_filtered\t43\t1\t0.0\ttrue",
            i32::try_from(std::process::id()).expect("the test PID should fit i32"),
        );

        assert_eq!(inner.last_selection_read, Some(external));
    }

    #[test]
    fn selection_read_not_trusted_remains_diagnostic_only() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(
            &mut inner,
            "selection_read_empty\tmouse_up\tfalse\t0\t0.0\tcom.apple.Safari\tnot_trusted",
        );

        assert!(inner.selection_status.available);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_accessibility_limited")
        );
        let diagnostics = inner
            .last_selection_read
            .expect("selection read diagnostics should be stored");
        assert_eq!(diagnostics.ax_error, "not_trusted");
    }

    #[test]
    fn selection_read_protocol_parses_explicit_generation_attempt_latency_and_terminal() {
        let diagnostics = parse_selection_read_diagnostics(
            "selection_read_empty\tmouse_up_drag\tfalse\t2\t650.0\tcom.apple.TextEdit\tax_timeout\t42\t3\t651.2\ttrue",
        )
        .expect("current native diagnostics protocol should parse");

        assert_eq!(diagnostics.generation, Some(42));
        assert_eq!(diagnostics.attempt, Some(3));
        assert_eq!(diagnostics.trigger_to_read_ms, Some(651.2));
        assert_eq!(diagnostics.terminal, Some(true));
        assert_eq!(diagnostics.source_bundle_id, "com.apple.TextEdit");
    }

    #[test]
    fn selection_read_protocol_rejects_missing_or_implicit_terminal_tail() {
        assert!(parse_selection_read_diagnostics(
            "selection_read_empty\tmouse_up_drag\tfalse\t2\t300.0\tcom.apple.TextEdit\tno_selected_text\t42\t2\t300.1"
        )
        .is_none());
        assert!(parse_selection_read_diagnostics(
            "selection_read_empty\tmouse_up_drag\tfalse\t2\t650.0\tcom.apple.TextEdit\tno_selected_text\t42\t3\t650.1\tmaybe"
        )
        .is_none());
    }

    #[test]
    fn selection_mouse_tap_status_identifies_input_monitoring_path() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "mouse_tap_unavailable");

        assert!(inner.selection_status.available);
        assert!(!inner.selection_status.running);
        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("input_monitoring_unavailable")
        );
    }

    #[test]
    fn selection_input_monitoring_status_identifies_input_monitoring_path() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "input_monitoring_unavailable");

        assert!(inner.selection_status.available);
        assert!(!inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("input_monitoring_unavailable")
        );
    }

    #[test]
    fn selection_accessibility_fallback_status_is_available() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "accessibility_selection_ready");

        assert!(inner.selection_status.available);
        assert!(inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("accessibility_selection_ready")
        );
    }

    #[test]
    fn selection_accessibility_fallback_input_limited_status_is_available() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "accessibility_selection_input_limited");

        assert!(inner.selection_status.available);
        assert!(!inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("accessibility_selection_input_limited")
        );
    }

    #[test]
    fn selection_accessibility_observer_status_is_available() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "accessibility_selection_observer_ready");

        assert!(inner.selection_status.available);
        assert!(inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("accessibility_selection_observer_ready")
        );
    }

    #[test]
    fn selection_event_ready_with_ax_limit_is_available() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "selection_event_ready_accessibility_limited");

        assert!(inner.selection_status.available);
        assert!(inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_event_ready_accessibility_limited")
        );
    }

    #[test]
    fn selection_clipboard_only_status_keeps_fallback_available() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "selection_clipboard_only");

        assert!(inner.selection_status.available);
        assert!(!inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_clipboard_only")
        );
    }

    #[test]
    fn deepseek_stream_parser_handles_split_sse_chunks() {
        let mut buffer = String::new();
        let mut output = String::new();

        let done = parse_deepseek_stream_chunk(
            &mut buffer,
            "data: {\"choices\":[{\"delta\":{\"content\":\"你",
            |delta| output.push_str(delta),
        )
        .expect("partial chunk should parse");
        assert!(!done);
        let done =
            parse_deepseek_stream_chunk(&mut buffer, "好\"}}]}\n\ndata: [DONE]\n\n", |delta| {
                output.push_str(delta)
            })
            .expect("complete chunk should parse");

        assert!(done);
        assert_eq!(output, "你好");
    }

    #[test]
    fn translation_token_invalidation_increments_monotonically() {
        let mut inner = test_inner_state("idle", None);

        assert_eq!(
            inner.popup_controller.cancel_translation().expect("cancel"),
            1
        );
        assert_eq!(
            inner.popup_controller.cancel_translation().expect("cancel"),
            2
        );
    }

    #[test]
    fn codesign_parser_detects_adhoc_identity() {
        let output = "Signature=adhoc\nTeamIdentifier=not set\nCDHash=abc123\n";

        assert_eq!(
            parse_codesign_field(output, "CDHash").as_deref(),
            Some("abc123")
        );
        assert_eq!(classify_signature(output), SignatureKind::Adhoc);
    }

    #[test]
    fn codesign_parser_detects_developer_id_identity() {
        let output =
            "Authority=Developer ID Application: Example (TEAMID)\nTeamIdentifier=TEAMID\n";

        assert_eq!(
            parse_codesign_field(output, "TeamIdentifier").as_deref(),
            Some("TEAMID")
        );
        assert_eq!(classify_signature(output), SignatureKind::DeveloperId);
    }

    #[test]
    fn codesign_parser_does_not_treat_any_team_signature_as_developer_id() {
        let apple_development =
            "Authority=Apple Development: Developer (TEAMID)\nTeamIdentifier=TEAMID\n";
        let team_only = "TeamIdentifier=TEAMID\n";

        assert_eq!(
            classify_signature(apple_development),
            SignatureKind::AppleDevelopment
        );
        assert_eq!(classify_signature(team_only), SignatureKind::Unknown);
    }

    #[test]
    fn double_copy_key_tap_status_identifies_input_monitoring_path() {
        let inner = Arc::new(Mutex::new(test_inner_state("idle", None)));

        handle_double_copy_status(&inner, "key_tap_unavailable");

        let guard = inner.lock().expect("test state should lock");
        assert!(guard.double_copy_status.available);
        assert!(guard.double_copy_status.running);
        assert_eq!(
            guard.double_copy_status.code.as_deref(),
            Some("key_tap_unavailable")
        );
        assert_eq!(
            guard
                .capability_snapshot
                .clipboard_double_copy_fallback
                .health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            guard.capability_snapshot.key_tap.health,
            CapabilityHealth::Unavailable
        );
    }

    #[test]
    fn double_copy_pasteboard_fallback_status_is_available() {
        let inner = Arc::new(Mutex::new(test_inner_state("idle", None)));

        handle_double_copy_status(&inner, "pasteboard_fallback_ready");

        let guard = inner.lock().expect("test state should lock");
        assert!(guard.double_copy_status.available);
        assert!(guard.double_copy_status.running);
        assert_eq!(
            guard.double_copy_status.code.as_deref(),
            Some("pasteboard_fallback_ready")
        );
    }

    #[test]
    fn watcher_refresh_timeout_marks_only_missing_source_as_degraded() {
        let mut inner = test_inner_state("idle", None);
        inner.capability_snapshot.key_tap =
            RuntimeCapabilityState::with_status(CapabilityHealth::Ready, "ready");

        apply_watcher_refresh_timeout(
            &mut inner,
            MissingWatcherSources {
                pasteboard: false,
                selection: true,
            },
        );

        assert_eq!(
            inner.double_copy_status.code.as_deref(),
            None,
            "the observed pasteboard source must not be overwritten"
        );
        assert_eq!(
            inner.capability_snapshot.key_tap.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_watcher_refresh_timeout")
        );
        assert!(!inner.selection_status.running);
        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Degraded
        );
        assert_eq!(
            inner.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Degraded
        );
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Degraded
        );
    }

    #[test]
    fn watcher_refresh_timeout_never_leaves_missing_sources_unknown() {
        let mut inner = test_inner_state("idle", None);

        apply_watcher_refresh_timeout(
            &mut inner,
            MissingWatcherSources {
                pasteboard: true,
                selection: true,
            },
        );

        assert_eq!(
            inner.double_copy_status.code.as_deref(),
            Some("pasteboard_watcher_refresh_timeout")
        );
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_watcher_refresh_timeout")
        );
        assert_eq!(
            inner
                .capability_snapshot
                .clipboard_double_copy_fallback
                .health,
            CapabilityHealth::Degraded
        );
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Degraded
        );
    }

    #[test]
    fn pasteboard_change_preserves_double_copy_fallback_status() {
        let status = double_copy_pasteboard_change_status(Some("pasteboard_fallback_ready"), 1);

        assert!(status.available);
        assert!(status.running);
        assert_eq!(status.code.as_deref(), Some("pasteboard_fallback_ready"));
    }

    #[test]
    fn permission_grants_are_independent_from_runtime_capabilities() {
        let mut snapshot = CapabilitySnapshot::unknown();
        snapshot.mouse_tap = RuntimeCapabilityState::with_status(
            CapabilityHealth::Disabled,
            "selection_mouse_tap_disabled_timeout_fallback_ax",
        );

        snapshot.apply_permission_grants(PermissionGrant::Granted, PermissionGrant::Granted);

        assert_eq!(snapshot.accessibility.grant, PermissionGrant::Granted);
        assert_eq!(snapshot.listen_event.grant, PermissionGrant::Granted);
        assert_eq!(snapshot.mouse_tap.health, CapabilityHealth::Disabled);

        snapshot.apply_permission_grants(PermissionGrant::Denied, PermissionGrant::Denied);

        assert_eq!(
            snapshot.direct_selection_read.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(
            snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(snapshot.mouse_tap.health, CapabilityHealth::Unavailable);
    }

    #[test]
    fn selection_sources_ready_maps_every_selection_source() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "selection_sources_ready");

        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            inner.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_sources_ready")
        );
    }

    #[test]
    fn selection_mouse_ready_observer_limited_is_degraded_not_ready() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "selection_mouse_ready_ax_observer_limited");

        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            inner.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Degraded
        );
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Ready
        );
    }

    #[test]
    fn selection_ax_observer_ready_keeps_mouse_failure_explicit() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "selection_ax_observer_ready_mouse_unavailable");

        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(
            inner.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Ready
        );
        assert!(inner.selection_status.running);
    }

    #[test]
    fn selection_mouse_tap_disabled_recovered_restores_health() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "selection_mouse_tap_disabled_timeout_recovered");

        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Ready
        );
        assert!(inner.selection_status.running);
    }

    #[test]
    fn selection_mouse_tap_disabled_uses_ax_fallback() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(
            &mut inner,
            "selection_mouse_tap_disabled_user_input_fallback_ax",
        );

        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Disabled
        );
        assert_eq!(
            inner.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Ready
        );
        assert!(inner.selection_status.running);
    }

    #[test]
    fn selection_mouse_tap_disabled_without_fallback_is_not_running() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(
            &mut inner,
            "selection_mouse_tap_disabled_timeout_unavailable",
        );

        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Disabled
        );
        assert_eq!(
            inner.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Unavailable
        );
        assert!(!inner.selection_status.running);
    }

    #[test]
    fn key_tap_unavailable_uses_explicit_fallback_ready_status() {
        let inner = Arc::new(Mutex::new(test_inner_state("idle", None)));

        handle_double_copy_status(&inner, "key_tap_unavailable_fallback_ready");

        let guard = inner.lock().expect("test state should lock");
        assert_eq!(
            guard.double_copy_status.code.as_deref(),
            Some("key_tap_unavailable_fallback_ready")
        );
        assert_eq!(
            guard
                .capability_snapshot
                .clipboard_double_copy_fallback
                .health,
            CapabilityHealth::Ready
        );
    }

    #[test]
    fn key_tap_disabled_recovered_is_visible_and_running() {
        let inner = Arc::new(Mutex::new(test_inner_state("idle", None)));

        handle_double_copy_status(&inner, "key_tap_disabled_user_input_recovered");

        let guard = inner.lock().expect("test state should lock");
        assert_eq!(
            guard.double_copy_status.code.as_deref(),
            Some("key_tap_disabled_user_input_recovered")
        );
        assert_eq!(
            guard.capability_snapshot.key_tap.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            guard
                .capability_snapshot
                .clipboard_double_copy_fallback
                .health,
            CapabilityHealth::Disabled
        );
        assert!(guard.double_copy_status.running);
    }

    #[test]
    fn key_tap_disabled_fallback_keeps_clipboard_source_ready() {
        let inner = Arc::new(Mutex::new(test_inner_state("idle", None)));

        handle_double_copy_status(&inner, "key_tap_disabled_timeout_fallback");

        let guard = inner.lock().expect("test state should lock");
        assert_eq!(
            guard
                .capability_snapshot
                .clipboard_double_copy_fallback
                .health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            guard.capability_snapshot.key_tap.health,
            CapabilityHealth::Disabled
        );
        assert!(guard.double_copy_status.running);
    }

    #[test]
    fn ambiguous_pasteboard_delta_is_never_reported_ready() {
        let inner = Arc::new(Mutex::new(test_inner_state("idle", None)));

        handle_double_copy_status(&inner, "pasteboard_fallback_delta_ambiguous\t3");

        let guard = inner.lock().expect("test state should lock");
        assert_eq!(
            guard
                .capability_snapshot
                .clipboard_double_copy_fallback
                .health,
            CapabilityHealth::Degraded
        );
        assert_eq!(
            guard.double_copy_status.code.as_deref(),
            Some("pasteboard_fallback_delta_ambiguous")
        );
        drop(guard);

        let status =
            double_copy_pasteboard_change_status(Some("key_tap_unavailable_fallback_ready"), 3);
        assert_eq!(
            status.code.as_deref(),
            Some("pasteboard_fallback_delta_ambiguous")
        );

        let primary_status = double_copy_pasteboard_change_status(Some("ready"), 3);
        assert_eq!(primary_status.code.as_deref(), Some("ready"));
    }

    #[test]
    fn unknown_native_selection_status_is_visible_and_not_ready() {
        let mut inner = test_inner_state("idle", None);

        handle_selection_status(&mut inner, "future_selection_status");

        assert!(!inner.selection_status.available);
        assert!(!inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("unknown_native_status")
        );
        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Unknown
        );
    }

    #[test]
    fn invalid_multi_click_window_fails_closed_without_completing_readiness() {
        let mut inner = test_inner_state("idle", None);
        let status = "selection_multi_click_quiet_window_invalid";

        assert!(!selection_status_completes_watcher_readiness(status));
        handle_selection_status(&mut inner, status);

        assert!(inner.selection_status.available);
        assert!(!inner.selection_status.running);
        assert_eq!(inner.selection_status.code.as_deref(), Some(status));
        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(
            inner.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Unavailable
        );

        apply_watcher_refresh_timeout(
            &mut inner,
            MissingWatcherSources {
                pasteboard: false,
                selection: true,
            },
        );
        assert_eq!(inner.selection_status.code.as_deref(), Some(status));
        assert!(!inner.selection_status.running);
    }

    #[test]
    fn unrecoverable_ax_timeout_is_explicitly_unavailable_and_completes_readiness() {
        let mut inner = test_inner_state("idle", None);
        let status = "selection_ax_timeout_unavailable";

        assert!(selection_status_completes_watcher_readiness(status));
        handle_selection_status(&mut inner, status);

        assert_eq!(inner.selection_status.code.as_deref(), Some(status));
        assert!(!inner.selection_status.running);
        assert_eq!(
            inner.capability_snapshot.mouse_tap.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            inner.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Unavailable
        );
        assert_eq!(
            inner.capability_snapshot.direct_selection_read.health,
            CapabilityHealth::Unavailable
        );
    }

    #[test]
    fn only_native_startup_handshakes_complete_selection_watcher_refresh() {
        for status in [
            "selection_disabled_by_setting",
            "selection_sources_ready",
            "selection_mouse_ready_ax_observer_limited",
            "selection_ax_timeout_unavailable",
            "selection_event_ready_accessibility_limited",
            "selection_ax_observer_ready_mouse_unavailable",
            "accessibility_selection_input_limited",
            "selection_clipboard_only",
        ] {
            assert!(
                selection_status_completes_watcher_readiness(status),
                "startup handshake should complete readiness: {status}"
            );
        }

        for status in [
            "selection_read_empty\tax_observer\tfalse\t0\t0.0\tcom.openai.codex\tobserver_context_unavailable\t102\t1\t3.1\ttrue",
            "selection_read_found\tmouse_up_drag\ttrue\t1\t4.0\tcom.google.Chrome\tnone\t103\t1\t5.0\ttrue",
            "selection_empty",
            "selection_mouse_tap_disabled_timeout_recovered",
            "selection_multi_click_quiet_window_invalid",
            "future_selection_status",
        ] {
            assert!(
                !selection_status_completes_watcher_readiness(status),
                "runtime outcome must not complete startup readiness: {status}"
            );
        }
    }

    #[test]
    fn closing_an_automatic_popup_suppresses_the_same_selection_identity() {
        let mut inner = test_inner_state("selection", Some("same text"));
        inner.active_popup_selection = Some(ActivePopupSelection {
            identity: SelectionIdentity::new(9001, "same text"),
            selection_revision: 1,
        });

        let hidden = commit_popup_close(&mut inner, PopupCloseReason::OutsideClick)
            .expect("outside click should close");

        assert_eq!(inner.active_selection_text, None);
        assert_eq!(
            inner.dismissed_automatic_selection,
            Some(SelectionIdentity::new(9001, "same text"))
        );
        assert_eq!(hidden.status, PopupStatus::Hidden);
        assert!(!hidden.visible);
        assert_eq!(inner.popup_controller.translation_generation(), 1);
    }

    #[test]
    fn suppressed_automatic_selection_stays_closed_until_the_identity_changes() {
        let mut inner = test_inner_state("idle", None);
        let anchor = Point { x: 10.0, y: 20.0 };

        let first = prepare_selection_popup_locked(
            &mut inner,
            "same text".to_string(),
            anchor,
            41,
            9001,
            SelectionTriggerKind::Automatic,
        )
        .expect("first native generation should be valid")
        .expect("first native generation should be accepted");
        assert_eq!(
            inner.popup_focus_return_target,
            Some(PopupFocusReturnTarget {
                selection_revision: first.selection_revision,
                pid: 9001,
            })
        );
        let hidden = commit_popup_close(&mut inner, PopupCloseReason::ExplicitClose)
            .expect("explicit close should suppress the automatic selection");
        assert!(!hidden.visible);
        assert!(prepare_selection_popup_locked(
            &mut inner,
            "same text".to_string(),
            anchor,
            42,
            9001,
            SelectionTriggerKind::Automatic,
        )
        .expect("new automatic generation should be consumed")
        .is_none());
        assert!(!inner.popup_controller.snapshot().visible);

        let second = prepare_selection_popup_locked(
            &mut inner,
            "different text".to_string(),
            anchor,
            43,
            9001,
            SelectionTriggerKind::Automatic,
        )
        .expect("changed automatic selection should be valid")
        .expect("changed automatic selection should open");

        assert!(second.selection_revision > first.selection_revision);
        assert_eq!(
            inner.popup_controller.snapshot().cleaned_text.as_deref(),
            Some("different text")
        );
        assert_eq!(
            inner.popup_focus_return_target,
            Some(PopupFocusReturnTarget {
                selection_revision: second.selection_revision,
                pid: 9001,
            })
        );
    }

    #[test]
    fn deliberate_selection_overrides_an_automatic_dismissal() {
        let mut inner = test_inner_state("idle", None);
        let anchor = Point { x: 10.0, y: 20.0 };
        inner.dismissed_automatic_selection = Some(SelectionIdentity::new(9001, "same text"));

        let state = prepare_selection_popup_locked(
            &mut inner,
            "same text".to_string(),
            anchor,
            51,
            9001,
            SelectionTriggerKind::Deliberate,
        )
        .expect("deliberate selection should be valid")
        .expect("deliberate selection should override dismissal");

        assert!(state.visible);
        assert_eq!(inner.dismissed_automatic_selection, None);
    }

    #[test]
    fn closing_a_deliberate_popup_still_blocks_automatic_reopen() {
        let mut inner = test_inner_state("idle", None);
        let anchor = Point { x: 10.0, y: 20.0 };

        let deliberate = prepare_selection_popup_locked(
            &mut inner,
            "same text".to_string(),
            anchor,
            56,
            9001,
            SelectionTriggerKind::Deliberate,
        )
        .expect("deliberate selection should be valid")
        .expect("deliberate selection should open");
        assert!(deliberate.visible);

        commit_popup_close(&mut inner, PopupCloseReason::ExplicitClose)
            .expect("user close should take priority");

        assert!(prepare_selection_popup_locked(
            &mut inner,
            "same text".to_string(),
            anchor,
            57,
            9001,
            SelectionTriggerKind::Automatic,
        )
        .expect("automatic generation should be consumed")
        .is_none());
        assert!(!inner.popup_controller.snapshot().visible);
    }

    #[test]
    fn copying_source_keeps_the_same_automatic_selection_suppressed() {
        let mut inner = test_inner_state("idle", None);
        let anchor = Point { x: 10.0, y: 20.0 };

        prepare_selection_popup_locked(
            &mut inner,
            "same text".to_string(),
            anchor,
            58,
            9001,
            SelectionTriggerKind::Automatic,
        )
        .expect("automatic selection should be valid")
        .expect("automatic selection should open");
        commit_popup_close(&mut inner, PopupCloseReason::ExplicitClose)
            .expect("copy-source close should take priority");

        assert_eq!(
            inner.dismissed_automatic_selection,
            Some(SelectionIdentity::new(9001, "same text"))
        );
    }

    #[test]
    fn empty_automatic_selection_clears_dismissal_without_opening() {
        let mut inner = test_inner_state("idle", None);
        let anchor = Point { x: 10.0, y: 20.0 };
        inner.dismissed_automatic_selection = Some(SelectionIdentity::new(9001, "same text"));

        assert!(prepare_selection_popup_locked(
            &mut inner,
            "   ".to_string(),
            anchor,
            61,
            9001,
            SelectionTriggerKind::Automatic,
        )
        .expect("empty selection generation should be accepted")
        .is_none());
        assert_eq!(inner.dismissed_automatic_selection, None);
    }

    #[test]
    fn self_process_deselection_cannot_clear_user_dismissal() {
        let translator_pid = std::process::id() as i32;

        assert!(!should_clear_automatic_dismissal_for_focus(
            translator_pid,
            translator_pid
        ));
        assert!(!should_clear_automatic_dismissal_for_focus(
            0,
            translator_pid
        ));
        assert!(should_clear_automatic_dismissal_for_focus(
            translator_pid.saturating_add(1),
            translator_pid
        ));
    }

    #[test]
    fn empty_double_copy_uses_active_selection_fallback() {
        let inner = test_inner_state("selection", Some("fallback text"));

        let (source, cleaned) = resolve_double_copy_source_text(&inner, "");

        assert_eq!(source, "fallback text");
        assert_eq!(cleaned, "fallback text");
    }

    #[test]
    fn empty_double_copy_without_selection_stays_empty() {
        let inner = test_inner_state("idle", None);

        let (source, cleaned) = resolve_double_copy_source_text(&inner, "");

        assert_eq!(source, "");
        assert_eq!(cleaned, "");
    }

    #[test]
    fn hidden_popup_never_supplies_double_copy_fallback() {
        let mut inner = test_inner_state("selection", Some("stale text"));
        commit_popup_close(&mut inner, PopupCloseReason::ExplicitClose)
            .expect("close should succeed");

        assert_eq!(selection_text_fallback_for_double_copy(&inner), None);
    }

    #[test]
    fn slow_translation_a_cannot_override_fast_translation_b() {
        let mut inner = test_inner_state("idle", None);
        let revision_a = commit_test_selection(&mut inner, "A");
        let guard_a = begin_test_translation(&mut inner, revision_a, "A");
        let revision_b = commit_test_selection(&mut inner, "B");
        let guard_b = begin_test_translation(&mut inner, revision_b, "B");

        assert!(matches!(
            inner
                .popup_controller
                .commit_translation(guard_b, translated_test_state("B-result"))
                .expect("B commit should be valid"),
            TranslationCommit::Applied(_)
        ));
        assert_eq!(
            inner
                .popup_controller
                .commit_translation(guard_a, translated_test_state("A-result"))
                .expect("stale A is not an error"),
            TranslationCommit::StaleGuard
        );
        assert_eq!(
            inner.popup_controller.snapshot().translation.as_deref(),
            Some("B-result")
        );
        assert_eq!(
            inner.popup_controller.snapshot().selection_revision,
            revision_b
        );
    }

    #[test]
    fn closing_while_translating_keeps_popup_hidden() {
        let mut inner = test_inner_state("idle", None);
        let selection_revision = commit_test_selection(&mut inner, "A");
        let translation_guard = begin_test_translation(&mut inner, selection_revision, "A");

        let hidden = commit_popup_close(&mut inner, PopupCloseReason::ExplicitClose)
            .expect("close should succeed");

        assert_eq!(hidden.status, PopupStatus::Hidden);
        assert_eq!(
            inner
                .popup_controller
                .commit_translation(translation_guard, translated_test_state("late"))
                .expect("late commit should cancel normally"),
            TranslationCommit::StaleGuard
        );
        assert_eq!(
            inner.popup_controller.snapshot().status,
            PopupStatus::Hidden
        );
        assert!(!inner.popup_controller.snapshot().visible);
    }

    #[test]
    fn a_new_selection_cancels_the_previous_translation_task() {
        let mut inner = test_inner_state("idle", None);
        let old_revision = commit_test_selection(&mut inner, "old");
        let old_guard = begin_test_translation(&mut inner, old_revision, "old");

        let new_revision = commit_test_selection(&mut inner, "new");

        assert!(new_revision > old_revision);
        assert!(!inner.popup_controller.translation_is_current(old_guard));
        assert_eq!(
            inner
                .popup_controller
                .commit_translation(old_guard, translated_test_state("late-old"))
                .expect("stale commit should cancel normally"),
            TranslationCommit::StaleGuard
        );
        assert_eq!(
            inner.popup_controller.snapshot().cleaned_text.as_deref(),
            Some("new")
        );
        assert_eq!(
            inner.popup_controller.snapshot().status,
            PopupStatus::SelectionReady
        );
    }

    #[test]
    fn one_thousand_out_of_order_translation_guards_cannot_corrupt_state() {
        let mut inner = test_inner_state("idle", None);
        let mut guards = Vec::with_capacity(1_000);

        for index in 0..1_000 {
            let text = format!("selection-{index}");
            let selection_revision = commit_test_selection(&mut inner, &text);
            let translation_guard = begin_test_translation(&mut inner, selection_revision, &text);
            guards.push(translation_guard);
        }

        let mut accepted = 0;
        for step in 0..1_000 {
            let index = (step * 997) % 1_000;
            let result = format!("result-{index}");
            if matches!(
                inner
                    .popup_controller
                    .commit_translation(guards[index], translated_test_state(&result))
                    .expect("translation commit should be valid"),
                TranslationCommit::Applied(_)
            ) {
                accepted += 1;
            }
        }

        assert_eq!(accepted, 1);
        assert_eq!(
            inner.popup_controller.snapshot().translation.as_deref(),
            Some("result-999")
        );
        assert_eq!(
            inner.popup_controller.snapshot().status,
            PopupStatus::Translated
        );
    }

    #[test]
    fn popup_snapshot_serializes_versioned_strong_state() {
        let mut inner = test_inner_state("idle", None);
        commit_test_selection(&mut inner, "snapshot");

        let value = serde_json::to_value(inner.popup_controller.snapshot())
            .expect("snapshot should serialize");

        assert_eq!(value["protocolVersion"], POPUP_PROTOCOL_VERSION);
        assert_eq!(value["status"], "selection_ready");
        assert_eq!(value["visible"], true);
        assert_eq!(value["revision"], 1);
        assert_eq!(value["selectionRevision"], 1);
    }

    #[test]
    fn popup_resize_does_not_reposition_after_initial_positioning() {
        let mut inner = test_inner_state("success", Some("text"));
        inner.last_cursor_point = Some(Point { x: 20.0, y: 30.0 });

        assert!(should_position_popup_on_resize(&inner));

        inner.popup_positioned = true;

        assert!(!should_position_popup_on_resize(&inner));
    }

    #[test]
    fn selection_setting_restart_is_required_only_when_the_toggle_changes() {
        assert!(!selection_setting_requires_restart(false, false));
        assert!(!selection_setting_requires_restart(true, true));
        assert!(selection_setting_requires_restart(false, true));
        assert!(selection_setting_requires_restart(true, false));
    }

    #[test]
    fn disabled_selection_resource_barrier_allows_only_non_selection_sources() {
        let released_with_double_copy_alive = WatcherResourceCounts {
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
        assert!(selection_resources_released(
            released_with_double_copy_alive
        ));

        for leaked in [
            WatcherResourceCounts {
                mouse_event_taps: 1,
                ..released_with_double_copy_alive
            },
            WatcherResourceCounts {
                mouse_event_tap_sources: 1,
                ..released_with_double_copy_alive
            },
            WatcherResourceCounts {
                selection_observers: 1,
                ..released_with_double_copy_alive
            },
            WatcherResourceCounts {
                selection_observer_sources: 1,
                ..released_with_double_copy_alive
            },
            WatcherResourceCounts {
                workspace_activation_observers: 1,
                ..released_with_double_copy_alive
            },
            WatcherResourceCounts {
                snapshot_version: 2,
                ..released_with_double_copy_alive
            },
        ] {
            assert!(!selection_resources_released(leaked));
        }
    }

    #[test]
    fn disabled_setting_status_clears_selection_evidence_and_marks_all_paths_disabled() {
        let mut inner = test_inner_state("selection", Some("selected"));
        inner.last_selection_read = parse_selection_read_diagnostics(
            "selection_read_found\tmouse_up\ttrue\t1\t2.5\tcom.example.Editor\tnone\t7\t1\t12.0\ttrue",
        );

        handle_selection_status(&mut inner, "selection_disabled_by_setting");

        assert!(inner.active_selection_text.is_none());
        assert!(inner.last_selection_read.is_none());
        assert!(!inner.selection_status.running);
        assert_eq!(
            inner.selection_status.code.as_deref(),
            Some("selection_disabled_by_setting")
        );
        for capability in [
            &inner.capability_snapshot.mouse_tap,
            &inner.capability_snapshot.ax_selected_text_observer,
            &inner.capability_snapshot.direct_selection_read,
        ] {
            assert_eq!(capability.health, CapabilityHealth::Disabled);
            assert_eq!(
                capability.status_code.as_deref(),
                Some("selection_disabled_by_setting")
            );
        }
    }

    #[test]
    fn logical_monitor_helpers_cover_retina_negative_origins_and_half_open_edges() {
        let origin = PhysicalPoint { x: -2880, y: -1800 };
        let size = GeometryPhysicalSize {
            width: 2880,
            height: 1800,
        };
        let logical_origin = LogicalPoint {
            x: -1440.0,
            y: -900.0,
        };
        assert_eq!(
            logical_monitor_center_from_geometry(origin, size, 2.0),
            Some(LogicalPoint {
                x: -720.0,
                y: -450.0,
            })
        );
        assert!(logical_point_is_inside_monitor(
            LogicalPoint {
                x: -1440.0,
                y: -900.0,
            },
            logical_origin,
            size,
            2.0,
        ));
        assert!(logical_point_is_inside_monitor(
            LogicalPoint { x: -0.5, y: -0.5 },
            logical_origin,
            size,
            2.0,
        ));
        assert!(!logical_point_is_inside_monitor(
            LogicalPoint { x: 0.0, y: 0.0 },
            logical_origin,
            size,
            2.0,
        ));
        assert!(!logical_point_is_inside_monitor(
            LogicalPoint {
                x: f64::NAN,
                y: -10.0,
            },
            logical_origin,
            size,
            2.0,
        ));
    }

    #[test]
    fn dragging_popup_ignores_outside_mouse_down() {
        let mut inner = test_inner_state("success", Some("text"));
        inner.popup_dragging = true;

        assert!(!should_hide_popup_for_mouse_down(&inner, false));
    }

    #[test]
    fn outside_mouse_down_hides_unpinned_popup_when_not_dragging() {
        let inner = test_inner_state("success", Some("text"));

        assert!(should_hide_popup_for_mouse_down(&inner, false));
        assert!(!should_hide_popup_for_mouse_down(&inner, true));
    }

    #[test]
    fn global_escape_and_tab_have_explicit_popup_focus_rules() {
        let hidden = test_inner_state("idle", None);
        assert!(!should_close_popup_from_global_escape(&hidden));
        assert!(!should_focus_popup_from_global_tab(&hidden));

        let selection = test_inner_state("selection", Some("text"));
        assert!(should_close_popup_from_global_escape(&selection));
        assert!(should_focus_popup_from_global_tab(&selection));

        let translated = test_inner_state("success", Some("text"));
        assert!(should_close_popup_from_global_escape(&translated));
        assert!(!should_focus_popup_from_global_tab(&translated));
    }

    #[test]
    fn popup_show_focus_policy_covers_every_state_and_key_tap_health() {
        let statuses = [
            PopupStatus::Hidden,
            PopupStatus::SelectionPending,
            PopupStatus::SelectionReady,
            PopupStatus::Translating,
            PopupStatus::Translated,
            PopupStatus::Error,
        ];
        let healths = [
            CapabilityHealth::Ready,
            CapabilityHealth::Degraded,
            CapabilityHealth::Disabled,
            CapabilityHealth::Unavailable,
            CapabilityHealth::Unknown,
        ];

        for status in statuses {
            for health in healths {
                let mut capabilities = CapabilitySnapshot::unknown();
                capabilities.key_tap = RuntimeCapabilityState::with_status(health, "test");
                assert_eq!(
                    should_focus_popup_on_show(status, &capabilities),
                    !status.is_selection_prompt() || health != CapabilityHealth::Ready,
                    "unexpected focus policy for {status:?} with {health:?}"
                );
            }
        }
    }

    #[test]
    fn ax_ready_with_key_tap_unavailable_forces_the_selection_keyboard_fallback() {
        let mut state = test_inner_state("selection", Some("text"));
        state.capability_snapshot.accessibility = PermissionCapabilityState::from_grant(
            PermissionGrant::Granted,
            "accessibility_preflight",
        );
        state.capability_snapshot.ax_selected_text_observer =
            RuntimeCapabilityState::with_status(CapabilityHealth::Ready, "selection_ax_ready");
        let inner = Arc::new(Mutex::new(state));

        handle_double_copy_status(&inner, "key_tap_unavailable_fallback_ready");

        let state = inner
            .lock()
            .expect("focus fallback state should remain readable");
        assert_eq!(
            state.capability_snapshot.accessibility.grant,
            PermissionGrant::Granted
        );
        assert_eq!(
            state.capability_snapshot.ax_selected_text_observer.health,
            CapabilityHealth::Ready
        );
        assert_eq!(
            state.capability_snapshot.key_tap.health,
            CapabilityHealth::Unavailable
        );
        assert!(should_focus_popup_on_show(
            state.popup_controller.snapshot().status,
            &state.capability_snapshot
        ));
    }

    #[test]
    fn key_tap_disabled_then_recovered_preserves_the_focus_return_target() {
        let mut state = test_inner_state("selection", Some("text"));
        state.popup_focus_return_target = Some(PopupFocusReturnTarget {
            selection_revision: state.popup_controller.snapshot().selection_revision,
            pid: 4242,
        });
        let inner = Arc::new(Mutex::new(state));

        handle_double_copy_status(&inner, "key_tap_disabled_timeout_fallback");
        {
            let state = inner.lock().expect("disabled state should remain readable");
            assert!(should_focus_popup_on_show(
                state.popup_controller.snapshot().status,
                &state.capability_snapshot
            ));
            assert_eq!(
                state.popup_focus_return_target.map(|target| target.pid),
                Some(4242)
            );
        }

        handle_double_copy_status(&inner, "key_tap_disabled_timeout_recovered");
        let state = inner
            .lock()
            .expect("recovered state should remain readable");
        assert!(!should_focus_popup_on_show(
            state.popup_controller.snapshot().status,
            &state.capability_snapshot
        ));
        assert_eq!(
            state.popup_focus_return_target.map(|target| target.pid),
            Some(4242)
        );
    }

    #[test]
    fn popup_focus_return_requires_a_focused_popup_and_matching_revision() {
        let target = Some(PopupFocusReturnTarget {
            selection_revision: 7,
            pid: 4242,
        });

        assert_eq!(
            focus_return_pid_for_close(PopupCloseReason::ExplicitClose, true, 7, target),
            Some(4242)
        );
        assert_eq!(
            focus_return_pid_for_close(PopupCloseReason::SelectionHandled, true, 7, target),
            Some(4242)
        );
        assert_eq!(
            focus_return_pid_for_close(PopupCloseReason::OutsideClick, true, 7, target),
            None
        );
        assert_eq!(
            focus_return_pid_for_close(PopupCloseReason::ExplicitClose, false, 7, target),
            None
        );
        assert_eq!(
            focus_return_pid_for_close(PopupCloseReason::ExplicitClose, true, 8, target),
            None
        );
        assert_eq!(
            focus_return_pid_for_close(PopupCloseReason::ExplicitClose, true, 7, None),
            None
        );
    }

    #[test]
    fn popup_keyboard_entry_serializes_revision_and_tab_direction() {
        let forward = serde_json::to_value(PopupKeyboardEntry {
            revision: 9,
            selection_revision: 4,
            direction: PopupFocusDirection::Forward,
        })
        .expect("forward focus entry should serialize");
        let backward = serde_json::to_value(PopupKeyboardEntry {
            revision: 9,
            selection_revision: 4,
            direction: PopupFocusDirection::Backward,
        })
        .expect("backward focus entry should serialize");

        assert_eq!(forward["revision"], 9);
        assert_eq!(forward["selectionRevision"], 4);
        assert_eq!(forward["direction"], "forward");
        assert_eq!(backward["direction"], "backward");
    }

    #[test]
    fn popup_permission_recovery_serializes_the_exact_system_pane() {
        let state = PopupState {
            revision: 1,
            selection_revision: 1,
            visible: true,
            status: PopupStatus::Error,
            error: Some("permission".to_string()),
            error_kind: Some(PopupErrorKind::Permission),
            retryable: Some(false),
            recovery_action: Some(PopupRecoveryAction::OpenInputMonitoring),
            ..PopupState::default()
        };

        let value = serde_json::to_value(state).expect("permission error should serialize");
        assert_eq!(value["errorKind"], "permission");
        assert_eq!(value["retryable"], false);
        assert_eq!(value["recoveryAction"], "open_input_monitoring");

        let actions = [
            PopupRecoveryAction::RetryTranslation,
            PopupRecoveryAction::OpenSettings,
            PopupRecoveryAction::OpenAccessibility,
            PopupRecoveryAction::OpenInputMonitoring,
            PopupRecoveryAction::Reselect,
        ];
        assert_eq!(actions.len(), 5);

        let kinds = [
            PopupErrorKind::Configuration,
            PopupErrorKind::Permission,
            PopupErrorKind::Selection,
            PopupErrorKind::Translation,
            PopupErrorKind::Protocol,
            PopupErrorKind::Unknown,
        ];
        assert_eq!(kinds.len(), 6);
    }

    fn test_inner_state(status: &str, cleaned_text: Option<&str>) -> InnerState {
        let status = match status {
            "idle" => PopupStatus::Hidden,
            "selection" => PopupStatus::SelectionReady,
            "loading" => PopupStatus::Translating,
            "success" => PopupStatus::Translated,
            "error" => PopupStatus::Error,
            other => panic!("unknown popup test status: {other}"),
        };
        let selection_revision = u64::from(cleaned_text.is_some());
        let popup_controller = PopupController::from_snapshot(PopupState {
            revision: u64::from(status.is_visible()),
            selection_revision,
            visible: status.is_visible(),
            status,
            cleaned_text: cleaned_text.map(str::to_string),
            pinned: false,
            ..PopupState::default()
        });
        InnerState {
            settings: AppSettings::default(),
            popup_controller,
            last_cleaned_text: cleaned_text.unwrap_or_default().to_string(),
            last_cursor_point: None,
            double_copy_status: WatcherStatus::new(true, true, ""),
            selection_status: WatcherStatus::new(true, true, ""),
            capability_snapshot: CapabilitySnapshot::unknown(),
            last_double_copy_trigger: None,
            active_selection_text: cleaned_text.map(str::to_string),
            active_popup_selection: None,
            dismissed_automatic_selection: None,
            last_selection_read: None,
            latched_selection_baseline_error: None,
            popup_positioned: false,
            popup_dragging: false,
            popup_focus_return_target: None,
            acceptance_rapid_probe: None,
            last_acceptance_probe_commit: None,
        }
    }

    #[test]
    fn clearing_acceptance_probe_invalidates_guard_before_return() {
        let mut inner = test_inner_state("selection", Some("fixed rapid A"));
        let BeginTranslation::Started { guard, .. } = inner
            .popup_controller
            .begin_translation(1, "fixed rapid A", "acceptance-probe")
            .unwrap()
        else {
            panic!("fixed probe should start")
        };
        inner.acceptance_rapid_probe = Some(AcceptanceRapidProbe { epoch: 9, guard });
        let hidden = cancel_acceptance_rapid_probe_locked(&mut inner)
            .unwrap()
            .expect("active acceptance probe should be closed");
        assert_eq!(hidden.status, PopupStatus::Hidden);
        assert!(inner.acceptance_rapid_probe.is_none());
        assert_eq!(
            inner
                .popup_controller
                .commit_translation(
                    guard,
                    PopupState {
                        status: PopupStatus::Translated,
                        translation: Some("must not appear".to_owned()),
                        ..PopupState::default()
                    }
                )
                .unwrap(),
            TranslationCommit::StaleGuard
        );
        assert_eq!(
            inner.popup_controller.snapshot().status,
            PopupStatus::Hidden
        );
    }

    #[cfg(feature = "acceptance-testing")]
    #[test]
    fn acceptance_tap_injection_requires_the_single_armed_recovery_scenario() {
        let mut status = AcceptanceRuntimeStatus {
            enabled: true,
            tap_injection_available: true,
            preset: "fixed-full-baseline-v1",
            phase: AcceptanceSessionPhase::Running,
            armed: Some(acceptance_runtime::ArmedScenarioStatus {
                scenario: AcceptanceScenario::TapDisabledRecovery,
                ordinal: 1,
            }),
            self_isolation_active: false,
            pending_generations: 0,
            pending_rapid_probes: 0,
            recorded_generations: 0,
            duplicate_generations_suppressed: 0,
        };
        assert!(acceptance_tap_injection_authorized(&status));
        assert!(acceptance_tap_injection_authorized_at(&status, 7, 7));
        assert!(!acceptance_tap_injection_authorized_at(&status, 7, 8));

        status.armed.as_mut().unwrap().ordinal = 2;
        assert!(!acceptance_tap_injection_authorized(&status));
        status.armed.as_mut().unwrap().ordinal = 1;
        status.phase = AcceptanceSessionPhase::Ended;
        assert!(!acceptance_tap_injection_authorized(&status));
        status.phase = AcceptanceSessionPhase::Running;
        status.tap_injection_available = false;
        assert!(!acceptance_tap_injection_authorized(&status));
    }

    #[cfg(feature = "acceptance-testing")]
    #[test]
    fn queued_tap_injection_rejects_clear_and_same_scenario_rearm() {
        let acceptance = Arc::new(Mutex::new(
            AcceptanceRuntime::new(true, 32).expect("acceptance runtime"),
        ));
        {
            let mut acceptance = acceptance.lock().expect("acceptance lock");
            acceptance
                .start_full_baseline(test_acceptance_metadata(530))
                .expect("start baseline");
            acceptance
                .arm(AcceptanceScenario::TapDisabledRecovery, 1)
                .expect("arm tap recovery");
        }
        let injection_epoch = Arc::new(AtomicU64::new(7));
        let injection_executed = Arc::new(AtomicBool::new(false));
        let queued_execution = {
            let acceptance = Arc::clone(&acceptance);
            let injection_epoch = Arc::clone(&injection_epoch);
            let injection_executed = Arc::clone(&injection_executed);
            move || {
                execute_acceptance_tap_injection_at_epoch(&acceptance, &injection_epoch, 7, || {
                    injection_executed.store(true, Ordering::SeqCst);
                    Ok(())
                })
            }
        };

        // Model an injection already queued to the main thread. Clear, start a
        // new session, and re-arm the identical visible scenario before the
        // queued closure runs. Status-only checking would accept this; epoch
        // revalidation must reject it without executing the native hook.
        {
            let mut acceptance = acceptance.lock().expect("acceptance lock");
            acceptance.clear();
            acceptance
                .start_full_baseline(test_acceptance_metadata(530))
                .expect("restart baseline");
            acceptance
                .arm(AcceptanceScenario::TapDisabledRecovery, 1)
                .expect("re-arm tap recovery");
            injection_epoch.store(8, Ordering::SeqCst);
        }

        assert!(queued_execution().is_err());
        assert!(!injection_executed.load(Ordering::SeqCst));
    }

    #[test]
    fn watcher_transition_gate_orders_acceptance_start_and_end_q_snapshots() {
        tauri::async_runtime::block_on(async {
            let transition = Arc::new(tauri::async_runtime::Mutex::new(()));
            let native_q = Arc::new(AtomicU64::new(530));
            let acceptance = Arc::new(Mutex::new(
                AcceptanceRuntime::new(true, 32).expect("acceptance runtime"),
            ));

            // Queue start while a simulated restart owns the transition. The
            // restart changes Q before releasing; start must capture the new Q.
            let restart = transition.lock().await;
            let (start_queued_tx, mut start_queued_rx) = tauri::async_runtime::channel(1);
            let start_task = tauri::async_runtime::spawn({
                let transition = Arc::clone(&transition);
                let native_q = Arc::clone(&native_q);
                let acceptance = Arc::clone(&acceptance);
                async move {
                    start_queued_tx.send(()).await.expect("queue start");
                    with_watcher_transition_ordered(&transition, || {
                        let captured = native_q.load(Ordering::SeqCst);
                        acceptance
                            .lock()
                            .expect("acceptance lock")
                            .start_full_baseline(test_acceptance_metadata(captured))
                            .expect("start baseline");
                        captured
                    })
                    .await
                }
            });
            start_queued_rx.recv().await.expect("start queued");
            native_q.store(531, Ordering::SeqCst);
            drop(restart);
            assert_eq!(start_task.await.expect("start task"), 531);

            // Recreate the rejected E/R interleaving: end is queued while a
            // restart owns the gate, then the restart captures another Q. End
            // must observe 532 (not stale 531) and invalidate the report.
            let restart = transition.lock().await;
            let (end_queued_tx, mut end_queued_rx) = tauri::async_runtime::channel(1);
            let end_task = tauri::async_runtime::spawn({
                let transition = Arc::clone(&transition);
                let native_q = Arc::clone(&native_q);
                let acceptance = Arc::clone(&acceptance);
                async move {
                    end_queued_tx.send(()).await.expect("queue end");
                    with_watcher_transition_ordered(&transition, || {
                        let observed = native_q.load(Ordering::SeqCst);
                        let mut acceptance = acceptance.lock().expect("acceptance lock");
                        let _ = acceptance.validate_runtime_multi_click_quiet_window(observed);
                        acceptance.end().expect("end baseline")
                    })
                    .await
                }
            });
            end_queued_rx.recv().await.expect("end queued");
            native_q.store(532, Ordering::SeqCst);
            drop(restart);
            let report = end_task.await.expect("end task");
            assert_eq!(report.multi_click_quiet_window_ms, 531);
            assert_eq!(report.rejected_records, 1);
            assert!(!report.status.integrity_valid);
            assert!(!report.status.valid);
        });
    }

    #[test]
    fn sentinel_native_release_only_drops_owner_and_preserves_report_integrity() {
        let acceptance = Arc::new(Mutex::new(
            AcceptanceRuntime::new(true, 64).expect("acceptance runtime"),
        ));
        {
            let mut acceptance = acceptance.lock().expect("acceptance lock");
            acceptance
                .start_full_baseline(test_acceptance_metadata(530))
                .expect("start baseline");
            acceptance
                .arm(AcceptanceScenario::WatcherRestart, 1)
                .expect("arm watcher restart");
        }

        let drop_count = Arc::new(AtomicU64::new(0));
        let state_access_count = Arc::new(AtomicU64::new(0));
        let owner = Box::into_raw(Box::new(NativeOwnerDropProbe {
            drop_count: Arc::clone(&drop_count),
        }));
        unsafe {
            release_native_context_owner(owner, 0, {
                let acceptance = Arc::clone(&acceptance);
                let state_access_count = Arc::clone(&state_access_count);
                move |_, _| {
                    state_access_count.fetch_add(1, Ordering::SeqCst);
                    let _ = acceptance
                        .lock()
                        .expect("acceptance lock")
                        .reject_native_observation();
                }
            });
        }
        assert_eq!(drop_count.load(Ordering::SeqCst), 1);
        assert_eq!(state_access_count.load(Ordering::SeqCst), 0);

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
        let report = {
            let mut acceptance = acceptance.lock().expect("acceptance lock");
            acceptance
                .observe_watcher_resources(70, AcceptanceLifecycleEvent::WatcherReleased, released)
                .expect("observe released resources");
            acceptance
                .observe_watcher_started(70)
                .expect("observe watcher start");
            acceptance
                .observe_watcher_resources(70, AcceptanceLifecycleEvent::WatcherStarted, running)
                .expect("observe started resources");
            acceptance
                .observe_watcher_resolution(70, false)
                .expect("observe watcher ready");
            acceptance
                .observe_watcher_resources(70, AcceptanceLifecycleEvent::WatcherReady, running)
                .expect("observe ready resources");
            acceptance.end().expect("end baseline")
        };
        assert!(report.status.integrity_valid);
    }

    #[test]
    fn positive_native_release_observes_once_and_drops_owner_once() {
        let drop_count = Arc::new(AtomicU64::new(0));
        let observe_count = Arc::new(AtomicU64::new(0));
        let observed_generation = Arc::new(AtomicU64::new(0));
        let owner = Box::into_raw(Box::new(NativeOwnerDropProbe {
            drop_count: Arc::clone(&drop_count),
        }));

        unsafe {
            release_native_context_owner(owner, 73, {
                let observe_count = Arc::clone(&observe_count);
                let observed_generation = Arc::clone(&observed_generation);
                move |_, native_generation| {
                    observe_count.fetch_add(1, Ordering::SeqCst);
                    observed_generation.store(
                        u64::try_from(native_generation).expect("positive generation"),
                        Ordering::SeqCst,
                    );
                }
            });
        }

        assert_eq!(observe_count.load(Ordering::SeqCst), 1);
        assert_eq!(observed_generation.load(Ordering::SeqCst), 73);
        assert_eq!(drop_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn watcher_restart_then_exit_finishes_with_ordered_stop_and_zero_sources() {
        let quitting = Arc::new(AtomicBool::new(false));
        let watcher_lifecycle = Arc::new(Mutex::new(()));
        let simulated_sources = Arc::new(AtomicU64::new(0));
        let events = Arc::new(Mutex::new(Vec::new()));
        let (restart_entered_tx, restart_entered_rx) = std::sync::mpsc::channel();
        let (release_restart_tx, release_restart_rx) = std::sync::mpsc::channel();

        let restart_thread = std::thread::spawn({
            let quitting = Arc::clone(&quitting);
            let watcher_lifecycle = Arc::clone(&watcher_lifecycle);
            let simulated_sources = Arc::clone(&simulated_sources);
            let events = Arc::clone(&events);
            move || {
                execute_watcher_restart_if_running(
                    &quitting,
                    &watcher_lifecycle,
                    || {
                        simulated_sources.store(5, Ordering::SeqCst);
                        events.lock().expect("events lock").push("start");
                        restart_entered_tx.send(()).expect("signal restart");
                        release_restart_rx.recv().expect("release restart");
                        Ok(())
                    },
                    || panic!("healthy lifecycle gate must not invoke fail-safe stop"),
                )
            }
        });
        restart_entered_rx.recv().expect("restart entered gate");

        let (exit_started_tx, exit_started_rx) = std::sync::mpsc::channel();
        let exit_thread = std::thread::spawn({
            let quitting = Arc::clone(&quitting);
            let watcher_lifecycle = Arc::clone(&watcher_lifecycle);
            let simulated_sources = Arc::clone(&simulated_sources);
            let events = Arc::clone(&events);
            move || {
                exit_started_tx.send(()).expect("signal exit task");
                execute_watcher_shutdown(&quitting, &watcher_lifecycle, || {
                    simulated_sources.store(0, Ordering::SeqCst);
                    events.lock().expect("events lock").push("stop");
                });
            }
        });
        exit_started_rx.recv().expect("exit task started");
        let deadline = Instant::now() + Duration::from_secs(1);
        while !quitting.load(Ordering::Acquire) {
            assert!(Instant::now() < deadline, "shutdown must mark quitting");
            std::thread::yield_now();
        }

        release_restart_tx.send(()).expect("release restart");
        restart_thread
            .join()
            .expect("restart thread")
            .expect("restart that won the gate");
        exit_thread.join().expect("exit thread");

        assert_eq!(*events.lock().expect("events lock"), vec!["start", "stop"]);
        assert_eq!(simulated_sources.load(Ordering::SeqCst), 0);
        assert!(quitting.load(Ordering::Acquire));
    }

    #[test]
    fn watcher_exit_then_restart_rejects_start_and_keeps_sources_zero() {
        let quitting = AtomicBool::new(false);
        let watcher_lifecycle = Mutex::new(());
        let simulated_sources = AtomicU64::new(5);
        let stop_count = AtomicU64::new(0);
        let start_count = AtomicU64::new(0);

        execute_watcher_shutdown(&quitting, &watcher_lifecycle, || {
            simulated_sources.store(0, Ordering::SeqCst);
            stop_count.fetch_add(1, Ordering::SeqCst);
        });
        let result = execute_watcher_restart_if_running(
            &quitting,
            &watcher_lifecycle,
            || {
                start_count.fetch_add(1, Ordering::SeqCst);
                simulated_sources.store(5, Ordering::SeqCst);
                Ok(())
            },
            || {
                stop_count.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(result.is_err());
        assert_eq!(start_count.load(Ordering::SeqCst), 0);
        assert_eq!(stop_count.load(Ordering::SeqCst), 1);
        assert_eq!(simulated_sources.load(Ordering::SeqCst), 0);
        assert!(quitting.load(Ordering::Acquire));
    }

    #[test]
    fn poisoned_watcher_lifecycle_rejects_restart_and_still_stops() {
        let watcher_lifecycle = Arc::new(Mutex::new(()));
        let poison_thread = std::thread::spawn({
            let watcher_lifecycle = Arc::clone(&watcher_lifecycle);
            move || {
                let _lifecycle = watcher_lifecycle.lock().expect("lifecycle lock");
                panic!("poison watcher lifecycle gate");
            }
        });
        assert!(poison_thread.join().is_err());
        assert!(watcher_lifecycle.is_poisoned());

        let quitting = AtomicBool::new(false);
        let start_count = AtomicU64::new(0);
        let stop_count = AtomicU64::new(0);
        let result = execute_watcher_restart_if_running(
            &quitting,
            &watcher_lifecycle,
            || {
                start_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            || {
                stop_count.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert!(result.is_err());
        assert_eq!(start_count.load(Ordering::SeqCst), 0);
        assert_eq!(stop_count.load(Ordering::SeqCst), 1);
        assert!(quitting.load(Ordering::Acquire));

        execute_watcher_shutdown(&quitting, &watcher_lifecycle, || {
            stop_count.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(stop_count.load(Ordering::SeqCst), 2);
    }

    fn test_acceptance_metadata(multi_click_quiet_window_ms: u64) -> AcceptanceRuntimeMetadata {
        AcceptanceRuntimeMetadata {
            app: AcceptanceAppMetadata {
                version: "0.1.0".to_owned(),
                bundle_identifier: "com.paperfloat.translator".to_owned(),
                build_kind: AcceptanceBuildKind::AdHocAcceptance,
            },
            system: AcceptanceSystemMetadata {
                operating_system: AcceptanceOperatingSystem::MacOs,
                version: "15.5".to_owned(),
                architecture: AcceptanceArchitecture::Arm64,
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
            multi_click_quiet_window_ms,
        }
    }

    fn test_cache_entry(key: String, created_at: String) -> CacheEntry {
        CacheEntry {
            key,
            cleaned_text: "source".to_string(),
            translation: "translation".to_string(),
            model: DeepSeekModel::Flash,
            mode: TranslateMode::AcademicZh,
            target_language: "中文".to_string(),
            glossary_version: "v1".to_string(),
            created_at,
        }
    }

    fn unique_test_directory(label: &str) -> PathBuf {
        let sequence = ATOMIC_WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "paper-float-{label}-{}-{sequence}",
            std::process::id()
        ))
    }

    fn commit_test_selection(inner: &mut InnerState, text: &str) -> u64 {
        let next = PopupState {
            status: PopupStatus::SelectionReady,
            source_text: Some(text.to_string()),
            selected_text: Some(text.to_string()),
            cleaned_text: Some(text.to_string()),
            pinned: false,
            ..PopupState::default()
        };
        let state = inner
            .popup_controller
            .commit_new_selection(next)
            .expect("test selection should commit");
        inner.active_selection_text = Some(text.to_string());
        inner.popup_focus_return_target = None;
        state.selection_revision
    }

    fn begin_test_translation(
        inner: &mut InnerState,
        selection_revision: u64,
        text: &str,
    ) -> TranslationGuard {
        let outcome = inner
            .popup_controller
            .begin_translation(selection_revision, text, "中文")
            .expect("translation begin should be valid");
        match outcome {
            BeginTranslation::Started { guard, .. } => {
                clear_active_selection(inner);
                guard
            }
            BeginTranslation::StaleSelection => panic!("translation should begin"),
        }
    }

    fn translated_test_state(translation: &str) -> PopupState {
        PopupState {
            status: PopupStatus::Translated,
            translation: Some(translation.to_string()),
            ..PopupState::default()
        }
    }
}
