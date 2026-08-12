use std::sync::{atomic::AtomicBool, Arc};
use tauri::{App, AppHandle, Manager, Runtime, Wry};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
mod windows_credentials;
#[cfg(target_os = "windows")]
mod windows_input;
#[cfg(target_os = "windows")]
mod windows_selection;

#[cfg(target_os = "windows")]
pub(crate) use windows_credentials::{
    delete_secret as delete_windows_secret, get_secret as get_windows_secret,
    secret_exists as windows_secret_exists, set_secret as set_windows_secret,
};
#[cfg(target_os = "windows")]
pub(crate) use windows_input::WindowsDoubleCopySelection;
#[cfg(target_os = "windows")]
pub(crate) use windows_selection::{
    WindowsSelection, WindowsSelectionError, WindowsSelectionReader,
};

#[cfg(target_os = "windows")]
pub(crate) fn windows_selection_shortcut_registered() -> bool {
    windows::selection_shortcut_registered()
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_input_monitor_running(app: &AppHandle) -> bool {
    windows::input_monitor_running(app)
}

#[cfg(target_os = "windows")]
pub(crate) fn set_windows_input_monitor_enabled(app: &AppHandle, enabled: bool) {
    windows::set_input_monitor_enabled(app, enabled);
}

#[cfg(target_os = "windows")]
pub(crate) fn set_windows_automatic_selection_enabled(
    app: &AppHandle,
    enabled: bool,
) -> Result<bool, String> {
    windows::set_automatic_selection_enabled(app, enabled)
}

#[cfg(target_os = "windows")]
pub(crate) fn windows_automatic_selection_running(app: &AppHandle) -> bool {
    windows::automatic_selection_running(app)
}

#[cfg(target_os = "windows")]
pub(crate) fn request_windows_automatic_selection(app: &AppHandle, source_pid: i32) -> bool {
    windows::request_automatic_selection(app, source_pid)
}

#[cfg(target_os = "windows")]
pub(crate) fn cancel_windows_automatic_selection(app: &AppHandle) {
    windows::cancel_automatic_selection(app)
}

#[cfg(target_os = "windows")]
pub(crate) const WINDOWS_SELECTION_SHORTCUT_LABEL: &str = windows::SELECTION_SHORTCUT_LABEL;

pub(crate) fn configure_builder(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    #[cfg(target_os = "windows")]
    {
        windows::configure_builder(builder)
    }

    #[cfg(not(target_os = "windows"))]
    {
        builder
    }
}

pub(crate) fn setup(app: &mut App<Wry>, quitting: Arc<AtomicBool>) -> tauri::Result<()> {
    #[cfg(target_os = "windows")]
    {
        windows::setup(app, quitting)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, quitting);
        Ok(())
    }
}

pub(crate) fn show_settings_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn shutdown(app: &AppHandle) {
    #[cfg(target_os = "windows")]
    windows::shutdown(app);

    #[cfg(not(target_os = "windows"))]
    let _ = app;
}
