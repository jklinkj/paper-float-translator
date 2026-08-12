use std::{
    ffi::c_void,
    mem::size_of,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use arboard::Clipboard;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use windows::{
    core::w,
    Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener},
        UI::{
            Input::KeyboardAndMouse::{
                GetDoubleClickTime, VK_C, VK_CONTROL, VK_LCONTROL, VK_RCONTROL,
            },
            Input::{
                GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
                RAWINPUTHEADER, RAWKEYBOARD, RAWMOUSE, RIDEV_INPUTSINK, RIDEV_REMOVE, RID_INPUT,
                RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, DispatchMessageW, GetCursorPos,
                GetForegroundWindow, GetMessageW, GetSystemMetrics, GetWindowThreadProcessId,
                PostThreadMessageW, MSG, RI_KEY_BREAK, RI_KEY_E0, RI_MOUSE_LEFT_BUTTON_DOWN,
                RI_MOUSE_LEFT_BUTTON_UP, SM_CXDOUBLECLK, SM_CXDRAG, SM_CYDOUBLECLK, SM_CYDRAG,
                WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WM_INPUT,
            },
        },
    },
};

const STOP_MESSAGE: u32 = WM_APP + 0x2f;
const COPY_CONFIRM_TIMEOUT: Duration = Duration::from_millis(750);
const DOUBLE_COPY_WINDOW: Duration = Duration::from_millis(1_000);
const MAX_SELECTION_CHARS: usize = 32_000;
const KEYBOARD_USAGE_PAGE: u16 = 0x01;
const KEYBOARD_USAGE: u16 = 0x06;
const MOUSE_USAGE: u16 = 0x02;
const FALLBACK_MOUSE_DRAG_INTENT_THRESHOLD: u32 = 4;
const FALLBACK_MOUSE_CLICK_SEQUENCE_WINDOW: Duration = Duration::from_millis(500);
const FALLBACK_MOUSE_CLICK_SEQUENCE_RADIUS: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PhysicalAnchor {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy)]
enum RawInputEvent {
    Keyboard(RAWKEYBOARD),
    Mouse(RAWMOUSE),
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowsDoubleCopySelection {
    pub(crate) text: String,
    pub(crate) source_pid: i32,
    pub(crate) physical_anchor: Option<(i32, i32)>,
}

#[derive(Clone, Copy, Debug)]
struct CopyGesture {
    observed_at: Instant,
    source_pid: i32,
    anchor: Option<PhysicalAnchor>,
}

#[derive(Clone, Copy, Debug)]
struct MouseSelectionGesture {
    source_pid: i32,
    anchor: Option<PhysicalAnchor>,
    dragged: bool,
}

#[derive(Clone, Copy, Debug)]
struct CompletedMouseClick {
    source_pid: i32,
    completed_at: Instant,
    anchor: Option<PhysicalAnchor>,
    count: u8,
}

#[derive(Default)]
struct MouseSelectionTracker {
    active: Option<MouseSelectionGesture>,
    previous_click: Option<CompletedMouseClick>,
    thresholds: MouseGestureThresholds,
}

#[derive(Clone, Copy, Debug)]
struct MouseGestureThresholds {
    drag_x: u32,
    drag_y: u32,
    double_click_window: Duration,
    double_click_x: u32,
    double_click_y: u32,
}

impl Default for MouseGestureThresholds {
    fn default() -> Self {
        Self {
            drag_x: FALLBACK_MOUSE_DRAG_INTENT_THRESHOLD,
            drag_y: FALLBACK_MOUSE_DRAG_INTENT_THRESHOLD,
            double_click_window: FALLBACK_MOUSE_CLICK_SEQUENCE_WINDOW,
            double_click_x: FALLBACK_MOUSE_CLICK_SEQUENCE_RADIUS as u32,
            double_click_y: FALLBACK_MOUSE_CLICK_SEQUENCE_RADIUS as u32,
        }
    }
}

impl MouseSelectionTracker {
    fn with_system_thresholds() -> Self {
        Self {
            thresholds: system_mouse_gesture_thresholds(),
            ..Self::default()
        }
    }

    fn reset(&mut self) {
        self.active = None;
        self.previous_click = None;
    }

    fn begin(&mut self, source_pid: i32, translator_pid: i32, anchor: Option<PhysicalAnchor>) {
        self.active =
            (source_pid > 0 && source_pid != translator_pid).then_some(MouseSelectionGesture {
                source_pid,
                anchor,
                dragged: false,
            });
        if self.active.is_none() {
            self.previous_click = None;
        }
    }

    fn observe_position(&mut self, anchor: Option<PhysicalAnchor>) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        active.dragged |= points_are_separated(
            active.anchor,
            anchor,
            self.thresholds.drag_x,
            self.thresholds.drag_y,
        );
    }

    fn finish(
        &mut self,
        source_pid: i32,
        translator_pid: i32,
        at: Instant,
        anchor: Option<PhysicalAnchor>,
    ) -> Option<i32> {
        let Some(gesture) = self.active.take() else {
            self.previous_click = None;
            return None;
        };
        if source_pid <= 0 || source_pid == translator_pid || source_pid != gesture.source_pid {
            self.previous_click = None;
            return None;
        }

        let dragged = gesture.dragged
            || points_are_separated(
                gesture.anchor,
                anchor,
                self.thresholds.drag_x,
                self.thresholds.drag_y,
            );
        let click_count = if dragged {
            self.previous_click = None;
            0
        } else {
            let count = self
                .previous_click
                .filter(|previous| {
                    previous.source_pid == source_pid
                        && at.saturating_duration_since(previous.completed_at)
                            <= self.thresholds.double_click_window
                        && points_are_near(
                            previous.anchor,
                            anchor,
                            self.thresholds.double_click_x,
                            self.thresholds.double_click_y,
                        )
                })
                .map_or(1, |previous| previous.count.saturating_add(1));
            self.previous_click = Some(CompletedMouseClick {
                source_pid,
                completed_at: at,
                anchor,
                count,
            });
            count
        };

        (dragged || click_count >= 2).then_some(source_pid)
    }
}

fn points_are_near(
    left: Option<PhysicalAnchor>,
    right: Option<PhysicalAnchor>,
    radius_x: u32,
    radius_y: u32,
) -> bool {
    left.zip(right).is_some_and(|(left, right)| {
        left.x.abs_diff(right.x) <= radius_x && left.y.abs_diff(right.y) <= radius_y
    })
}

fn points_are_separated(
    left: Option<PhysicalAnchor>,
    right: Option<PhysicalAnchor>,
    threshold_x: u32,
    threshold_y: u32,
) -> bool {
    left.zip(right).is_some_and(|(left, right)| {
        left.x.abs_diff(right.x) >= threshold_x || left.y.abs_diff(right.y) >= threshold_y
    })
}

fn system_mouse_gesture_thresholds() -> MouseGestureThresholds {
    // SAFETY: these calls read process-independent Windows user settings and
    // do not retain pointers or handles.
    let (double_click_ms, drag_x, drag_y, double_click_x, double_click_y) = unsafe {
        (
            GetDoubleClickTime(),
            GetSystemMetrics(SM_CXDRAG),
            GetSystemMetrics(SM_CYDRAG),
            GetSystemMetrics(SM_CXDOUBLECLK),
            GetSystemMetrics(SM_CYDOUBLECLK),
        )
    };
    MouseGestureThresholds {
        drag_x: u32::try_from(drag_x)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(FALLBACK_MOUSE_DRAG_INTENT_THRESHOLD),
        drag_y: u32::try_from(drag_y)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(FALLBACK_MOUSE_DRAG_INTENT_THRESHOLD),
        double_click_window: if double_click_ms > 0 {
            Duration::from_millis(u64::from(double_click_ms))
        } else {
            FALLBACK_MOUSE_CLICK_SEQUENCE_WINDOW
        },
        double_click_x: u32::try_from(double_click_x)
            .ok()
            .filter(|value| *value > 0)
            .map(|value| (value / 2).max(1))
            .unwrap_or(FALLBACK_MOUSE_CLICK_SEQUENCE_RADIUS as u32),
        double_click_y: u32::try_from(double_click_y)
            .ok()
            .filter(|value| *value > 0)
            .map(|value| (value / 2).max(1))
            .unwrap_or(FALLBACK_MOUSE_CLICK_SEQUENCE_RADIUS as u32),
    }
}

#[derive(Clone, Debug)]
struct ConfirmedCopy {
    fingerprint: [u8; 32],
    confirmed_at: Instant,
    source_pid: i32,
}

#[derive(Default)]
struct DoubleCopyTracker {
    control_mask: u8,
    c_down: bool,
    pending: Option<CopyGesture>,
    confirmed: Option<ConfirmedCopy>,
}

impl DoubleCopyTracker {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn observe_raw_key(&mut self, virtual_key: u16, flags: u16) -> bool {
        let is_break = u32::from(flags) & RI_KEY_BREAK != 0;
        if let Some(mask) = control_key_mask(virtual_key, flags) {
            if is_break {
                self.control_mask &= !mask;
            } else {
                self.control_mask |= mask;
            }
            return false;
        }

        if virtual_key != VK_C.0 {
            return false;
        }
        if is_break {
            self.c_down = false;
            return false;
        }
        if self.c_down {
            return false;
        }
        self.c_down = true;
        self.control_mask != 0
    }

    fn record_copy_gesture(&mut self, gesture: CopyGesture) {
        self.pending = Some(gesture);
    }

    fn take_confirmable_gesture(&mut self, now: Instant) -> Option<CopyGesture> {
        let gesture = self.pending.take()?;
        (now.saturating_duration_since(gesture.observed_at) <= COPY_CONFIRM_TIMEOUT)
            .then_some(gesture)
    }

    fn confirm_text(
        &mut self,
        gesture: CopyGesture,
        text: String,
        now: Instant,
    ) -> Option<WindowsDoubleCopySelection> {
        let fingerprint: [u8; 32] = Sha256::digest(text.as_bytes()).into();
        let is_second_copy = self.confirmed.as_ref().is_some_and(|previous| {
            previous.fingerprint == fingerprint
                && previous.source_pid == gesture.source_pid
                && now.saturating_duration_since(previous.confirmed_at) <= DOUBLE_COPY_WINDOW
        });
        if is_second_copy {
            self.confirmed = None;
            return Some(WindowsDoubleCopySelection {
                text,
                source_pid: gesture.source_pid,
                physical_anchor: gesture.anchor.map(|anchor| (anchor.x, anchor.y)),
            });
        }

        self.confirmed = Some(ConfirmedCopy {
            fingerprint,
            confirmed_at: now,
            source_pid: gesture.source_pid,
        });
        None
    }
}

fn control_key_mask(virtual_key: u16, flags: u16) -> Option<u8> {
    if virtual_key == VK_LCONTROL.0 {
        Some(0b01)
    } else if virtual_key == VK_RCONTROL.0 {
        Some(0b10)
    } else if virtual_key == VK_CONTROL.0 {
        Some(if u32::from(flags) & RI_KEY_E0 != 0 {
            0b10
        } else {
            0b01
        })
    } else {
        None
    }
}

pub(crate) struct WindowsInputMonitor {
    enabled: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
    stopping: AtomicBool,
    thread_id: u32,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl WindowsInputMonitor {
    pub(crate) fn start(app: AppHandle) -> Result<Self, String> {
        let enabled = Arc::new(AtomicBool::new(false));
        let epoch = Arc::new(AtomicU64::new(0));
        let running = Arc::new(AtomicBool::new(false));
        let worker_enabled = enabled.clone();
        let worker_epoch = epoch.clone();
        let worker_running = running.clone();
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("paper-float-windows-input".to_owned())
            .spawn(move || {
                input_worker(
                    app,
                    worker_enabled,
                    worker_epoch,
                    worker_running,
                    startup_sender,
                )
            })
            .map_err(|error| format!("windows_input.thread_spawn_failed: {error}"))?;

        match startup_receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(thread_id)) => Ok(Self {
                enabled,
                epoch,
                running,
                stopping: AtomicBool::new(false),
                thread_id,
                worker: Mutex::new(Some(worker)),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(error) => Err(format!("windows_input.startup_timeout: {error}")),
        }
    }

    pub(crate) fn unavailable() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            epoch: Arc::new(AtomicU64::new(0)),
            running: Arc::new(AtomicBool::new(false)),
            stopping: AtomicBool::new(true),
            thread_id: 0,
            worker: Mutex::new(None),
        }
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        if self.enabled.swap(enabled, Ordering::AcqRel) != enabled {
            self.epoch.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub(crate) fn stop(&self) {
        if self.thread_id == 0 || self.stopping.swap(true, Ordering::AcqRel) {
            return;
        }
        self.enabled.store(false, Ordering::Release);
        self.epoch.fetch_add(1, Ordering::AcqRel);
        // SAFETY: thread_id belongs to the worker message queue created before startup succeeds.
        let posted =
            unsafe { PostThreadMessageW(self.thread_id, STOP_MESSAGE, WPARAM(0), LPARAM(0)) };
        if posted.is_err() && self.running.load(Ordering::Acquire) {
            self.stopping.store(false, Ordering::Release);
            return;
        }
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                let _ = worker.join();
            }
        }
    }
}

impl Drop for WindowsInputMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn input_worker(
    app: AppHandle,
    enabled: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    running: Arc<AtomicBool>,
    startup: mpsc::SyncSender<Result<u32, String>>,
) {
    let window = match create_listener_window() {
        Ok(window) => window,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    if let Err(error) = register_input_sources(window) {
        // SAFETY: window was created on this thread and is still valid.
        let _ = unsafe { DestroyWindow(window) };
        let _ = startup.send(Err(error));
        return;
    }

    // SAFETY: window is a valid HWND owned by the current thread.
    let thread_id = unsafe { GetWindowThreadProcessId(window, None) };
    running.store(true, Ordering::Release);
    if startup.send(Ok(thread_id)).is_err() {
        unregister_input_sources(window);
        running.store(false, Ordering::Release);
        return;
    }

    let mut tracker = DoubleCopyTracker::default();
    let mut mouse_selection = MouseSelectionTracker::with_system_thresholds();
    let translator_pid = std::process::id() as i32;
    let mut observed_epoch = epoch.load(Ordering::Acquire);
    loop {
        let mut message = MSG::default();
        // SAFETY: message points to valid storage for the duration of GetMessageW.
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 || message.message == STOP_MESSAGE {
            break;
        }

        let current_epoch = epoch.load(Ordering::Acquire);
        if current_epoch != observed_epoch {
            tracker.reset();
            mouse_selection.reset();
            observed_epoch = current_epoch;
        }
        let active = enabled.load(Ordering::Acquire);
        if !active {
            tracker.reset();
            mouse_selection.reset();
        } else if message.message == WM_INPUT {
            match read_raw_input(message.lParam) {
                Some(RawInputEvent::Keyboard(keyboard)) => {
                    crate::handle_windows_global_keyboard_input(&app);
                    if tracker.observe_raw_key(keyboard.VKey, keyboard.Flags) {
                        tracker.record_copy_gesture(capture_copy_gesture());
                    }
                }
                Some(RawInputEvent::Mouse(mouse)) => {
                    // SAFETY: RAWMOUSE documents this union member as the button flag/data pair.
                    let button_flags = unsafe { mouse.Anonymous.Anonymous.usButtonFlags };
                    let button_flags = u32::from(button_flags);
                    let now = Instant::now();
                    let anchor = cursor_anchor();
                    if button_flags & RI_MOUSE_LEFT_BUTTON_DOWN != 0 {
                        let source_pid = foreground_process_id();
                        mouse_selection.begin(source_pid, translator_pid, anchor);
                        crate::handle_windows_global_mouse_down(&app);
                    }
                    if button_flags & RI_MOUSE_LEFT_BUTTON_UP == 0 {
                        mouse_selection.observe_position(anchor);
                    }
                    if button_flags & RI_MOUSE_LEFT_BUTTON_UP != 0 {
                        let source_pid = foreground_process_id();
                        if let Some(source_pid) =
                            mouse_selection.finish(source_pid, translator_pid, now, anchor)
                        {
                            crate::handle_windows_automatic_selection_trigger(&app, source_pid);
                        }
                    }
                }
                Some(RawInputEvent::Other) | None => {}
            }
        } else if message.message == WM_CLIPBOARDUPDATE {
            let now = Instant::now();
            if let Some(gesture) = tracker.take_confirmable_gesture(now) {
                if let Some(text) = read_bounded_clipboard_text() {
                    if let Some(selection) = tracker.confirm_text(gesture, text, now) {
                        crate::handle_windows_double_copy_selection(&app, selection);
                    }
                }
            }
        }

        // SAFETY: dispatching preserves the required default processing for WM_INPUT cleanup.
        unsafe { DispatchMessageW(&message) };
    }

    unregister_input_sources(window);
    running.store(false, Ordering::Release);
}

fn create_listener_window() -> Result<HWND, String> {
    // SAFETY: STATIC is a system-provided window class; HWND_MESSAGE creates a nonvisual,
    // thread-owned message window with no external lifetime requirements.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("PaperFloatInputListener"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(windows::Win32::UI::WindowsAndMessaging::HWND_MESSAGE),
            None,
            None,
            None,
        )
        .map_err(|error| format!("windows_input.window_create_failed: {error}"))
    }
}

fn register_input_sources(window: HWND) -> Result<(), String> {
    let keyboard = RAWINPUTDEVICE {
        usUsagePage: KEYBOARD_USAGE_PAGE,
        usUsage: KEYBOARD_USAGE,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: window,
    };
    let mouse = RAWINPUTDEVICE {
        usUsagePage: KEYBOARD_USAGE_PAGE,
        usUsage: MOUSE_USAGE,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: window,
    };
    let device_size = u32::try_from(size_of::<RAWINPUTDEVICE>())
        .map_err(|_| "windows_input.device_size_invalid".to_owned())?;
    // SAFETY: both initialized usage registrations target the live message-only window. No
    // RIDEV_NOLEGACY flag is used, so normal mouse and keyboard delivery is never suppressed.
    unsafe { RegisterRawInputDevices(&[keyboard, mouse], device_size) }
        .map_err(|error| format!("windows_input.raw_registration_failed: {error}"))?;
    // SAFETY: window remains valid until unregister_input_sources runs on this thread.
    if let Err(error) = unsafe { AddClipboardFormatListener(window) } {
        let remove_keyboard = removal_device(KEYBOARD_USAGE);
        let remove_mouse = removal_device(MOUSE_USAGE);
        // SAFETY: this removes the registration just made by the current process.
        let _ = unsafe { RegisterRawInputDevices(&[remove_keyboard, remove_mouse], device_size) };
        return Err(format!(
            "windows_input.clipboard_registration_failed: {error}"
        ));
    }
    Ok(())
}

fn unregister_input_sources(window: HWND) {
    // SAFETY: cleanup is performed on the creating thread before the HWND is destroyed.
    let _ = unsafe { RemoveClipboardFormatListener(window) };
    if let Ok(device_size) = u32::try_from(size_of::<RAWINPUTDEVICE>()) {
        let remove_keyboard = removal_device(KEYBOARD_USAGE);
        let remove_mouse = removal_device(MOUSE_USAGE);
        // SAFETY: RIDEV_REMOVE with a null target unregisters this usage class.
        let _ = unsafe { RegisterRawInputDevices(&[remove_keyboard, remove_mouse], device_size) };
    }
    // SAFETY: window is valid and owned by this thread.
    let _ = unsafe { DestroyWindow(window) };
}

fn removal_device(usage: u16) -> RAWINPUTDEVICE {
    RAWINPUTDEVICE {
        usUsagePage: KEYBOARD_USAGE_PAGE,
        usUsage: usage,
        dwFlags: RIDEV_REMOVE,
        hwndTarget: HWND::default(),
    }
}

fn read_raw_input(lparam: LPARAM) -> Option<RawInputEvent> {
    let mut raw_input = RAWINPUT::default();
    let mut input_size = u32::try_from(size_of::<RAWINPUT>()).ok()?;
    let header_size = u32::try_from(size_of::<RAWINPUTHEADER>()).ok()?;
    let handle = HRAWINPUT(lparam.0 as *mut c_void);
    // SAFETY: raw_input is aligned writable storage and the handle came directly from WM_INPUT.
    let copied = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            Some((&mut raw_input as *mut RAWINPUT).cast()),
            &mut input_size,
            header_size,
        )
    };
    if copied == u32::MAX {
        return None;
    }
    if raw_input.header.dwType == RIM_TYPEKEYBOARD.0 {
        // SAFETY: the header identifies the active RAWINPUT union member as keyboard.
        return Some(RawInputEvent::Keyboard(unsafe { raw_input.data.keyboard }));
    }
    if raw_input.header.dwType == RIM_TYPEMOUSE.0 {
        // SAFETY: the header identifies the active RAWINPUT union member as mouse.
        let mouse = unsafe { raw_input.data.mouse };
        return Some(RawInputEvent::Mouse(mouse));
    }
    Some(RawInputEvent::Other)
}

fn capture_copy_gesture() -> CopyGesture {
    CopyGesture {
        observed_at: Instant::now(),
        source_pid: foreground_process_id(),
        anchor: cursor_anchor(),
    }
}

fn foreground_process_id() -> i32 {
    // SAFETY: calls inspect the current foreground HWND and write to a valid local PID.
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return 0;
        }
        let mut process_id = 0_u32;
        GetWindowThreadProcessId(window, Some(&mut process_id));
        i32::try_from(process_id).unwrap_or_default()
    }
}

fn cursor_anchor() -> Option<PhysicalAnchor> {
    let mut point = POINT::default();
    // SAFETY: point is valid writable storage.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    Some(PhysicalAnchor {
        x: point.x,
        y: point.y,
    })
}

fn read_bounded_clipboard_text() -> Option<String> {
    let mut clipboard = Clipboard::new().ok()?;
    let text = clipboard.get_text().ok()?.replace('\0', "");
    (!text.trim().is_empty() && text.chars().count() <= MAX_SELECTION_CHARS).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gesture(at: Instant, source_pid: i32) -> CopyGesture {
        CopyGesture {
            observed_at: at,
            source_pid,
            anchor: Some(PhysicalAnchor { x: -20, y: 40 }),
        }
    }

    #[test]
    fn raw_key_state_requires_control_and_suppresses_c_auto_repeat() {
        let mut tracker = DoubleCopyTracker::default();
        assert!(!tracker.observe_raw_key(VK_C.0, 0));
        tracker.observe_raw_key(VK_C.0, RI_KEY_BREAK as u16);

        assert!(!tracker.observe_raw_key(VK_CONTROL.0, 0));
        assert!(tracker.observe_raw_key(VK_C.0, 0));
        assert!(!tracker.observe_raw_key(VK_C.0, 0));
        assert!(!tracker.observe_raw_key(VK_C.0, RI_KEY_BREAK as u16));
        assert!(tracker.observe_raw_key(VK_C.0, 0));
    }

    #[test]
    fn clipboard_update_must_confirm_the_copy_gesture_promptly() {
        let start = Instant::now();
        let mut tracker = DoubleCopyTracker::default();
        tracker.record_copy_gesture(gesture(start, 10));
        assert!(tracker
            .take_confirmable_gesture(start + COPY_CONFIRM_TIMEOUT)
            .is_some());

        tracker.record_copy_gesture(gesture(start, 10));
        assert!(tracker
            .take_confirmable_gesture(start + COPY_CONFIRM_TIMEOUT + Duration::from_millis(1))
            .is_none());
    }

    #[test]
    fn two_matching_confirmed_copies_trigger_without_retaining_plaintext() {
        let start = Instant::now();
        let mut tracker = DoubleCopyTracker::default();
        assert!(tracker
            .confirm_text(gesture(start, 10), "same text".to_owned(), start)
            .is_none());
        let selection = tracker
            .confirm_text(
                gesture(start + Duration::from_millis(500), 10),
                "same text".to_owned(),
                start + Duration::from_millis(500),
            )
            .expect("matching second copy should trigger");
        assert_eq!(selection.text, "same text");
        assert_eq!(selection.physical_anchor, Some((-20, 40)));
        assert!(tracker.confirmed.is_none());
    }

    #[test]
    fn different_text_process_or_late_copy_starts_a_new_pair() {
        let start = Instant::now();
        let mut tracker = DoubleCopyTracker::default();
        assert!(tracker
            .confirm_text(gesture(start, 10), "a".to_owned(), start)
            .is_none());
        assert!(tracker
            .confirm_text(
                gesture(start + Duration::from_millis(100), 10),
                "b".to_owned(),
                start + Duration::from_millis(100),
            )
            .is_none());
        assert!(tracker
            .confirm_text(
                gesture(start + Duration::from_millis(200), 11),
                "b".to_owned(),
                start + Duration::from_millis(200),
            )
            .is_none());
        assert!(tracker
            .confirm_text(
                gesture(start + Duration::from_millis(1_201), 11),
                "b".to_owned(),
                start + Duration::from_millis(1_201),
            )
            .is_none());
    }

    #[test]
    fn removal_records_cover_keyboard_and_mouse_without_a_target_window() {
        let keyboard = removal_device(KEYBOARD_USAGE);
        let mouse = removal_device(MOUSE_USAGE);
        assert_eq!(keyboard.usUsage, KEYBOARD_USAGE);
        assert_eq!(mouse.usUsage, MOUSE_USAGE);
        assert_eq!(keyboard.dwFlags, RIDEV_REMOVE);
        assert_eq!(mouse.dwFlags, RIDEV_REMOVE);
        assert!(keyboard.hwndTarget.is_invalid());
        assert!(mouse.hwndTarget.is_invalid());
    }

    #[test]
    fn mouse_selection_requires_a_matching_external_down_and_up() {
        let translator_pid = 99;
        let mut tracker = MouseSelectionTracker::default();
        let start = Instant::now();
        let point = Some(PhysicalAnchor { x: 10, y: 10 });

        assert_eq!(tracker.finish(10, translator_pid, start, point), None);

        tracker.begin(translator_pid, translator_pid, point);
        assert_eq!(
            tracker.finish(translator_pid, translator_pid, start, point),
            None
        );

        tracker.begin(10, translator_pid, point);
        assert_eq!(tracker.finish(11, translator_pid, start, point), None);

        tracker.begin(10, translator_pid, point);
        assert_eq!(tracker.finish(10, translator_pid, start, point), None);
        assert_eq!(tracker.finish(10, translator_pid, start, point), None);
    }

    #[test]
    fn ordinary_click_is_silent_but_drag_selection_is_authorized() {
        let translator_pid = 99;
        let start = Instant::now();
        let origin = Some(PhysicalAnchor { x: 10, y: 10 });
        let dragged_to = Some(PhysicalAnchor { x: 30, y: 10 });
        let mut tracker = MouseSelectionTracker::default();

        tracker.begin(10, translator_pid, origin);
        assert_eq!(tracker.finish(10, translator_pid, start, origin), None);

        tracker.begin(10, translator_pid, origin);
        tracker.observe_position(dragged_to);
        assert_eq!(
            tracker.finish(10, translator_pid, start, dragged_to),
            Some(10)
        );
    }

    #[test]
    fn second_nearby_click_authorizes_word_selection_once() {
        let translator_pid = 99;
        let start = Instant::now();
        let point = Some(PhysicalAnchor { x: 10, y: 10 });
        let mut tracker = MouseSelectionTracker::default();

        tracker.begin(10, translator_pid, point);
        assert_eq!(tracker.finish(10, translator_pid, start, point), None);

        let second = start + Duration::from_millis(100);
        tracker.begin(10, translator_pid, point);
        assert_eq!(tracker.finish(10, translator_pid, second, point), Some(10));
    }

    #[test]
    fn late_or_distant_second_click_stays_silent() {
        let translator_pid = 99;
        let start = Instant::now();
        let point = Some(PhysicalAnchor { x: 10, y: 10 });
        let far_point = Some(PhysicalAnchor { x: 50, y: 50 });
        let mut tracker = MouseSelectionTracker::default();

        tracker.begin(10, translator_pid, point);
        assert_eq!(tracker.finish(10, translator_pid, start, point), None);
        let late = start + FALLBACK_MOUSE_CLICK_SEQUENCE_WINDOW + Duration::from_millis(1);
        tracker.begin(10, translator_pid, point);
        assert_eq!(tracker.finish(10, translator_pid, late, point), None);

        let next = late + Duration::from_millis(10);
        tracker.begin(10, translator_pid, far_point);
        assert_eq!(tracker.finish(10, translator_pid, next, far_point), None);
    }
}
