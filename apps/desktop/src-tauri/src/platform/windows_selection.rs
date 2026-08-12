use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tauri::AppHandle;
use uiautomation::{
    events::{CustomEventHandlerFn, UIEventHandler, UIEventType},
    patterns::UITextPattern,
    types::TreeScope,
    UIAutomation, UIElement,
};

const MAX_SELECTION_CHARS: usize = 32_000;
const MAX_PARENT_DEPTH: usize = 8;
const AUTOMATIC_SELECTION_DEBOUNCE: Duration = Duration::from_millis(120);
const AUTOMATIC_SELECTION_MAX_SETTLE: Duration = Duration::from_millis(350);
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(25);
const WORKER_STOP_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowsSelection {
    pub(crate) generation: i64,
    pub(crate) text: String,
    pub(crate) source_pid: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowsSelectionError {
    Busy,
    Timeout,
    UiAutomationUnavailable,
    PatternUnavailable,
    EmptySelection,
    SelectionTooLarge,
    SecureField,
    WorkerStopped,
}

impl WindowsSelectionError {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Busy => "windows_uia_busy",
            Self::Timeout => "windows_uia_timeout",
            Self::UiAutomationUnavailable => "windows_uia_unavailable",
            Self::PatternUnavailable => "windows_uia_pattern_unavailable",
            Self::EmptySelection => "windows_uia_empty_selection",
            Self::SelectionTooLarge => "windows_uia_selection_too_large",
            Self::SecureField => "windows_uia_secure_field_ignored",
            Self::WorkerStopped => "windows_uia_worker_stopped",
        }
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Busy => "上一次 Windows 取词仍在处理中，请稍后重试。",
            Self::Timeout => "目标应用响应取词超时；已停止等待，避免拖慢翻译器。",
            Self::UiAutomationUnavailable => "Windows UI Automation 当前不可用。",
            Self::PatternUnavailable => "当前应用未提供可读取的选中文本；后续将使用复制兜底。",
            Self::EmptySelection => "没有读到选中文本，请先选中文字再按快捷键。",
            Self::SelectionTooLarge => "选中文本过长，请缩小选区后重试。",
            Self::SecureField => "已忽略密码或受保护输入框中的选区。",
            Self::WorkerStopped => "Windows 取词工作线程已停止，请重启应用。",
        }
    }
}

#[derive(Clone)]
pub(crate) struct WindowsSelectionReader {
    inner: Arc<ReaderInner>,
}

struct ReaderInner {
    commands: Sender<WorkerCommand>,
    automatic_events: SyncSender<()>,
    busy: Arc<AtomicBool>,
    automatic_enabled: Arc<AtomicBool>,
    automatic_running: Arc<AtomicBool>,
    interaction_epoch: Arc<AtomicU64>,
    stopping: AtomicBool,
    worker: Mutex<Option<JoinHandle<()>>>,
}

struct ReadRequest {
    reply: SyncSender<Result<SelectionPayload, WindowsSelectionError>>,
}

enum WorkerCommand {
    Read(ReadRequest),
    AutomaticGesture {
        source_pid: i32,
        interaction_epoch: u64,
    },
    CancelAutomaticGesture,
    ConfigureAutomatic {
        enabled: bool,
        reply: SyncSender<Result<bool, String>>,
    },
    Stop {
        reply: SyncSender<()>,
    },
}

struct SelectionPayload {
    text: String,
    source_pid: i32,
}

#[derive(Clone, Copy, Debug)]
struct PendingAutomaticRead {
    source_pid: i32,
    interaction_epoch: u64,
    deadline: Instant,
    expires_at: Instant,
}

fn apply_automatic_command(pending: &mut Option<PendingAutomaticRead>, command: AutomaticCommand) {
    match command {
        AutomaticCommand::Schedule(next) => *pending = next,
        AutomaticCommand::Cancel => *pending = None,
    }
}

enum AutomaticCommand {
    Schedule(Option<PendingAutomaticRead>),
    Cancel,
}

struct AutomaticRegistration {
    root: UIElement,
    handler: UIEventHandler,
}

struct AutomaticWorkerState {
    enabled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    interaction_epoch: Arc<AtomicU64>,
}

impl WindowsSelectionReader {
    pub(crate) fn new(app: AppHandle) -> Result<Self, String> {
        let (commands, command_receiver) = mpsc::channel();
        let (automatic_events, automatic_event_receiver) = mpsc::sync_channel(1);
        let busy = Arc::new(AtomicBool::new(false));
        let worker_busy = busy.clone();
        let automatic_enabled = Arc::new(AtomicBool::new(false));
        let worker_automatic_enabled = automatic_enabled.clone();
        let automatic_running = Arc::new(AtomicBool::new(false));
        let worker_automatic_running = automatic_running.clone();
        let interaction_epoch = Arc::new(AtomicU64::new(0));
        let worker_interaction_epoch = interaction_epoch.clone();
        let event_sender = automatic_events.clone();
        let worker = thread::Builder::new()
            .name("paper-float-windows-uia".to_owned())
            .spawn(move || {
                worker_loop(
                    app,
                    command_receiver,
                    automatic_event_receiver,
                    event_sender,
                    worker_busy,
                    AutomaticWorkerState {
                        enabled: worker_automatic_enabled,
                        running: worker_automatic_running,
                        interaction_epoch: worker_interaction_epoch,
                    },
                )
            })
            .map_err(|error| format!("无法启动 Windows UI Automation 工作线程：{error}"))?;

        Ok(Self {
            inner: Arc::new(ReaderInner {
                commands,
                automatic_events,
                busy,
                automatic_enabled,
                automatic_running,
                interaction_epoch,
                stopping: AtomicBool::new(false),
                worker: Mutex::new(Some(worker)),
            }),
        })
    }

    pub(crate) fn read(
        &self,
        generation: i64,
        timeout: Duration,
    ) -> Result<WindowsSelection, WindowsSelectionError> {
        if generation <= 0 {
            return Err(WindowsSelectionError::WorkerStopped);
        }
        if self
            .inner
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(WindowsSelectionError::Busy);
        }

        let (reply, response) = mpsc::sync_channel(1);
        if self
            .inner
            .commands
            .send(WorkerCommand::Read(ReadRequest { reply }))
            .is_err()
        {
            self.inner.busy.store(false, Ordering::Release);
            return Err(WindowsSelectionError::WorkerStopped);
        }

        match response.recv_timeout(timeout) {
            Ok(Ok(payload)) => Ok(WindowsSelection {
                generation,
                text: payload.text,
                source_pid: payload.source_pid,
            }),
            Ok(Err(error)) => Err(error),
            Err(RecvTimeoutError::Timeout) => Err(WindowsSelectionError::Timeout),
            Err(RecvTimeoutError::Disconnected) => {
                self.inner.busy.store(false, Ordering::Release);
                Err(WindowsSelectionError::WorkerStopped)
            }
        }
    }

    pub(crate) fn set_automatic_enabled(
        &self,
        enabled: bool,
        timeout: Duration,
    ) -> Result<bool, String> {
        if self.inner.stopping.load(Ordering::Acquire) {
            return Err("windows_uia_worker_stopped".to_owned());
        }
        self.inner
            .automatic_enabled
            .store(enabled, Ordering::Release);
        let (reply, response) = mpsc::sync_channel(1);
        self.inner
            .commands
            .send(WorkerCommand::ConfigureAutomatic { enabled, reply })
            .map_err(|_| "windows_uia_worker_stopped".to_owned())?;
        response
            .recv_timeout(timeout)
            .map_err(|error| format!("windows_uia_observer_configuration_timeout: {error}"))?
    }

    pub(crate) fn request_automatic_trigger(&self, source_pid: i32) -> bool {
        if !self.inner.automatic_enabled.load(Ordering::Acquire)
            || self.inner.stopping.load(Ordering::Acquire)
            || !is_external_selection_source(source_pid, std::process::id() as i32)
        {
            return false;
        }
        let interaction_epoch = self.inner.interaction_epoch.load(Ordering::Acquire);
        self.inner
            .commands
            .send(WorkerCommand::AutomaticGesture {
                source_pid,
                interaction_epoch,
            })
            .is_ok()
    }

    pub(crate) fn cancel_automatic_trigger(&self) {
        if !self.inner.stopping.load(Ordering::Acquire) {
            self.inner.interaction_epoch.fetch_add(1, Ordering::AcqRel);
            let _ = self
                .inner
                .commands
                .send(WorkerCommand::CancelAutomaticGesture);
        }
    }

    pub(crate) fn automatic_running(&self) -> bool {
        self.inner.automatic_running.load(Ordering::Acquire)
    }

    pub(crate) fn stop(&self) {
        self.inner.stop();
    }
}

impl ReaderInner {
    fn stop(&self) {
        if self.stopping.swap(true, Ordering::AcqRel) {
            return;
        }
        self.automatic_enabled.store(false, Ordering::Release);
        let (reply, stopped) = mpsc::sync_channel(1);
        let acknowledged = self.commands.send(WorkerCommand::Stop { reply }).is_ok()
            && stopped.recv_timeout(WORKER_STOP_TIMEOUT).is_ok();
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                if acknowledged {
                    let _ = worker.join();
                }
            }
        }
        self.automatic_running.store(false, Ordering::Release);
    }
}

impl Drop for ReaderInner {
    fn drop(&mut self) {
        self.stop();
    }
}

fn worker_loop(
    app: AppHandle,
    commands: Receiver<WorkerCommand>,
    automatic_events: Receiver<()>,
    automatic_event_sender: SyncSender<()>,
    busy: Arc<AtomicBool>,
    automatic: AutomaticWorkerState,
) {
    let mut automation = None;
    let mut automatic_registration: Option<AutomaticRegistration> = None;
    let mut pending_automatic_read: Option<PendingAutomaticRead> = None;
    let mut stop_reply = None;

    loop {
        drain_automatic_events(
            &automatic_events,
            automatic.enabled.load(Ordering::Acquire),
            &mut pending_automatic_read,
            Instant::now(),
        );
        let wait = pending_automatic_read
            .map(|pending| pending.deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(WORKER_POLL_INTERVAL)
            .min(WORKER_POLL_INTERVAL);

        match commands.recv_timeout(wait) {
            Ok(WorkerCommand::Read(request)) => {
                let result = read_with_recovery(&mut automation);
                busy.store(false, Ordering::Release);
                let _ = request.reply.send(result);
            }
            Ok(WorkerCommand::AutomaticGesture {
                source_pid,
                interaction_epoch: gesture_epoch,
            }) => {
                apply_automatic_command(
                    &mut pending_automatic_read,
                    AutomaticCommand::Schedule(schedule_automatic_gesture(
                        automatic.enabled.load(Ordering::Acquire),
                        source_pid,
                        std::process::id() as i32,
                        gesture_epoch,
                        Instant::now(),
                    )),
                );
            }
            Ok(WorkerCommand::CancelAutomaticGesture) => {
                apply_automatic_command(&mut pending_automatic_read, AutomaticCommand::Cancel);
            }
            Ok(WorkerCommand::ConfigureAutomatic { enabled, reply }) => {
                let result = configure_automatic_registration(
                    enabled,
                    &mut automation,
                    &mut automatic_registration,
                    &automatic_event_sender,
                );
                automatic
                    .running
                    .store(result.as_ref().copied().unwrap_or(false), Ordering::Release);
                if !enabled {
                    pending_automatic_read = None;
                }
                let _ = reply.send(result);
            }
            Ok(WorkerCommand::Stop { reply }) => {
                stop_reply = Some(reply);
                break;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        drain_automatic_events(
            &automatic_events,
            automatic.enabled.load(Ordering::Acquire),
            &mut pending_automatic_read,
            Instant::now(),
        );
        if pending_automatic_read.is_some_and(|pending| pending.deadline <= Instant::now()) {
            let pending = pending_automatic_read.take();
            if automatic.enabled.load(Ordering::Acquire) {
                if let Some(pending) = pending {
                    handle_automatic_read(
                        &app,
                        &mut automation,
                        pending.source_pid,
                        pending.interaction_epoch,
                        &automatic.interaction_epoch,
                    );
                }
            }
        }
    }

    if let (Some(automation), Some(registration)) =
        (automation.as_ref(), automatic_registration.take())
    {
        let _ = automation.remove_automation_event_handler(
            UIEventType::Text_TextSelectionChanged,
            &registration.root,
            &registration.handler,
        );
    }
    automatic.running.store(false, Ordering::Release);
    if let Some(reply) = stop_reply {
        let _ = reply.send(());
    }
}

fn drain_automatic_events(
    events: &Receiver<()>,
    enabled: bool,
    pending: &mut Option<PendingAutomaticRead>,
    now: Instant,
) {
    let mut selection_changed = false;
    loop {
        match events.try_recv() {
            Ok(()) if enabled => selection_changed = true,
            Ok(()) => {}
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
        }
    }

    // UI Automation reports only that a selection changed; it does not report
    // whether a mouse, keyboard shortcut, paste, or programmatic action caused
    // the change. It may stabilize an already-authorized mouse read, but must
    // never create permission to show the popup by itself.
    if selection_changed {
        if let Some(pending) = pending.as_mut() {
            pending.deadline = (now + AUTOMATIC_SELECTION_DEBOUNCE).min(pending.expires_at);
        }
    }
}

fn schedule_automatic_gesture(
    enabled: bool,
    source_pid: i32,
    translator_pid: i32,
    interaction_epoch: u64,
    now: Instant,
) -> Option<PendingAutomaticRead> {
    (enabled && is_external_selection_source(source_pid, translator_pid)).then_some(
        PendingAutomaticRead {
            source_pid,
            interaction_epoch,
            deadline: now + AUTOMATIC_SELECTION_DEBOUNCE,
            expires_at: now + AUTOMATIC_SELECTION_MAX_SETTLE,
        },
    )
}

fn is_external_selection_source(source_pid: i32, translator_pid: i32) -> bool {
    source_pid > 0 && source_pid != translator_pid
}

fn configure_automatic_registration(
    enabled: bool,
    automation: &mut Option<UIAutomation>,
    registration: &mut Option<AutomaticRegistration>,
    automatic_events: &SyncSender<()>,
) -> Result<bool, String> {
    if !enabled {
        if let (Some(automation), Some(existing)) = (automation.as_ref(), registration.take()) {
            automation
                .remove_automation_event_handler(
                    UIEventType::Text_TextSelectionChanged,
                    &existing.root,
                    &existing.handler,
                )
                .map_err(|_| "windows_uia_observer_removal_failed".to_owned())?;
        }
        return Ok(false);
    }
    if registration.is_some() {
        return Ok(true);
    }
    if automation.is_none() {
        *automation = UIAutomation::new().ok();
    }
    let automation = automation
        .as_ref()
        .ok_or_else(|| "windows_uia_unavailable".to_owned())?;
    let root = automation
        .get_root_element()
        .map_err(|_| "windows_uia_root_unavailable".to_owned())?;
    let signal = automatic_events.clone();
    let callback: Box<CustomEventHandlerFn> = Box::new(move |_sender, event_type| {
        if event_type == UIEventType::Text_TextSelectionChanged {
            let _ = signal.try_send(());
        }
        Ok(())
    });
    let handler = UIEventHandler::from(callback);
    automation
        .add_automation_event_handler(
            UIEventType::Text_TextSelectionChanged,
            &root,
            TreeScope::Subtree,
            None,
            &handler,
        )
        .map_err(|_| "windows_uia_observer_registration_failed".to_owned())?;
    *registration = Some(AutomaticRegistration { root, handler });
    Ok(true)
}

fn read_with_recovery(
    automation: &mut Option<UIAutomation>,
) -> Result<SelectionPayload, WindowsSelectionError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if automation.is_none() {
            *automation = UIAutomation::new().ok();
        }
        automation
            .as_ref()
            .ok_or(WindowsSelectionError::UiAutomationUnavailable)
            .and_then(read_focused_selection)
    }))
    .unwrap_or(Err(WindowsSelectionError::WorkerStopped))
}

fn handle_automatic_read(
    app: &AppHandle,
    automation: &mut Option<UIAutomation>,
    expected_source_pid: i32,
    expected_interaction_epoch: u64,
    interaction_epoch: &AtomicU64,
) {
    if interaction_epoch.load(Ordering::Acquire) != expected_interaction_epoch {
        return;
    }
    let result = read_with_recovery(automation);
    if interaction_epoch.load(Ordering::Acquire) != expected_interaction_epoch {
        return;
    }
    match result {
        Ok(payload) => {
            if payload.source_pid != expected_source_pid
                || !is_external_selection_source(payload.source_pid, std::process::id() as i32)
            {
                return;
            }
            crate::handle_windows_automatic_selection(payload.text, payload.source_pid, app);
        }
        Err(
            WindowsSelectionError::EmptySelection
            | WindowsSelectionError::PatternUnavailable
            | WindowsSelectionError::SecureField,
        ) => {
            if let Some(source_pid) = focused_process_id(automation) {
                if source_pid == expected_source_pid {
                    crate::handle_windows_automatic_selection_cleared(app, source_pid);
                }
            }
        }
        Err(error) => crate::handle_windows_automatic_selection_error(app, error),
    }
}

fn focused_process_id(automation: &mut Option<UIAutomation>) -> Option<i32> {
    if automation.is_none() {
        *automation = UIAutomation::new().ok();
    }
    automation
        .as_ref()?
        .get_focused_element()
        .ok()?
        .get_process_id()
        .ok()
        .and_then(|pid| i32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
}

fn read_focused_selection(
    automation: &UIAutomation,
) -> Result<SelectionPayload, WindowsSelectionError> {
    let focused = automation
        .get_focused_element()
        .map_err(|_| WindowsSelectionError::UiAutomationUnavailable)?;
    let source_pid = focused
        .get_process_id()
        .ok()
        .and_then(|pid| i32::try_from(pid).ok())
        .unwrap_or_default();
    if focused.is_password().unwrap_or(false) {
        return Err(WindowsSelectionError::SecureField);
    }
    let walker = automation
        .get_control_view_walker()
        .map_err(|_| WindowsSelectionError::UiAutomationUnavailable)?;

    let mut element = focused;
    let mut pattern_seen = false;
    for _ in 0..=MAX_PARENT_DEPTH {
        if let Ok(pattern) = element.get_pattern::<UITextPattern>() {
            pattern_seen = true;
            if let Ok(ranges) = pattern.get_selection() {
                let text = collect_selection_text(ranges)?;
                if !text.trim().is_empty() {
                    return Ok(SelectionPayload { text, source_pid });
                }
            }
        }
        element = match walker.get_parent(&element) {
            Ok(parent) => parent,
            Err(_) => break,
        };
    }

    Err(if pattern_seen {
        WindowsSelectionError::EmptySelection
    } else {
        WindowsSelectionError::PatternUnavailable
    })
}

fn collect_selection_text(
    ranges: Vec<uiautomation::patterns::UITextRange>,
) -> Result<String, WindowsSelectionError> {
    let mut output = String::new();
    for range in ranges {
        let remaining = MAX_SELECTION_CHARS.saturating_sub(output.chars().count());
        if remaining == 0 {
            return Err(WindowsSelectionError::SelectionTooLarge);
        }
        let request_length = i32::try_from(remaining.saturating_add(1)).unwrap_or(i32::MAX);
        let segment = range
            .get_text(request_length)
            .map_err(|_| WindowsSelectionError::UiAutomationUnavailable)?;
        append_bounded_segment(&mut output, &segment, MAX_SELECTION_CHARS)?;
    }
    Ok(output)
}

fn append_bounded_segment(
    output: &mut String,
    segment: &str,
    max_chars: usize,
) -> Result<(), WindowsSelectionError> {
    let segment = segment.replace('\0', "");
    if segment.trim().is_empty() {
        return Ok(());
    }
    let separator_chars = usize::from(!output.is_empty());
    if output
        .chars()
        .count()
        .saturating_add(separator_chars)
        .saturating_add(segment.chars().count())
        > max_chars
    {
        return Err(WindowsSelectionError::SelectionTooLarge);
    }
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(&segment);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_segments_join_without_exposing_partial_oversize_text() {
        let mut text = String::new();
        append_bounded_segment(&mut text, "first", 12).unwrap();
        append_bounded_segment(&mut text, "second", 12).unwrap();
        assert_eq!(text, "first\nsecond");

        let original = text.clone();
        assert_eq!(
            append_bounded_segment(&mut text, "x", 12),
            Err(WindowsSelectionError::SelectionTooLarge)
        );
        assert_eq!(text, original);
    }

    #[test]
    fn error_codes_are_stable_and_content_free() {
        assert_eq!(WindowsSelectionError::Timeout.code(), "windows_uia_timeout");
        assert_eq!(
            WindowsSelectionError::SecureField.code(),
            "windows_uia_secure_field_ignored"
        );
        assert!(!WindowsSelectionError::PatternUnavailable
            .message()
            .contains("selectedText"));
    }

    #[test]
    fn empty_and_nul_only_segments_are_ignored() {
        let mut text = String::new();
        append_bounded_segment(&mut text, "\0  \0", 8).unwrap();
        assert!(text.is_empty());
    }

    #[test]
    fn automatic_events_are_bounded_coalesced_and_discarded_while_disabled() {
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.try_send(()).unwrap();
        assert!(matches!(sender.try_send(()), Err(TrySendError::Full(()))));

        let start = Instant::now();
        let mut pending = schedule_automatic_gesture(true, 10, 99, 0, start);
        drain_automatic_events(&receiver, true, &mut pending, start);
        let first_deadline = pending
            .expect("a mouse-authorized read should retain its quiet window")
            .deadline;
        assert!(first_deadline > start);

        sender.try_send(()).unwrap();
        drain_automatic_events(
            &receiver,
            false,
            &mut pending,
            start + Duration::from_millis(1),
        );
        assert_eq!(pending.map(|value| value.deadline), Some(first_deadline));
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn selection_change_without_mouse_authorization_never_schedules_a_read() {
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.try_send(()).unwrap();
        let mut pending = None;

        drain_automatic_events(&receiver, true, &mut pending, Instant::now());

        assert!(
            pending.is_none(),
            "copy, paste, keyboard, and programmatic UIA events must stay silent"
        );
    }

    #[test]
    fn mouse_authorization_rejects_invalid_and_self_processes() {
        let now = Instant::now();

        assert!(schedule_automatic_gesture(true, 0, 99, 0, now).is_none());
        assert!(schedule_automatic_gesture(true, 99, 99, 0, now).is_none());
        assert!(schedule_automatic_gesture(false, 10, 99, 0, now).is_none());
        assert!(schedule_automatic_gesture(true, 10, 99, 0, now).is_some());
    }

    #[test]
    fn selection_events_only_settle_an_existing_mouse_authorization() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let start = Instant::now();
        let mut pending = schedule_automatic_gesture(true, 10, 99, 0, start);
        sender.try_send(()).unwrap();

        let later = start + Duration::from_millis(300);
        drain_automatic_events(&receiver, true, &mut pending, later);

        let pending = pending.expect("mouse authorization should remain pending");
        assert_eq!(pending.source_pid, 10);
        assert_eq!(pending.deadline, pending.expires_at);
    }

    #[test]
    fn newer_user_input_cancels_a_pending_mouse_authorization() {
        let start = Instant::now();
        let mut pending = schedule_automatic_gesture(true, 10, 99, 0, start);
        assert!(pending.is_some());

        apply_automatic_command(&mut pending, AutomaticCommand::Cancel);

        assert!(pending.is_none());
    }

    #[test]
    fn mouse_authorization_preserves_the_interaction_epoch_for_post_read_validation() {
        let pending = schedule_automatic_gesture(true, 10, 99, 42, Instant::now())
            .expect("external mouse gesture should be scheduled");

        assert_eq!(pending.interaction_epoch, 42);
    }
}
