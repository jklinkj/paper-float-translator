use super::show_settings_window;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager, Wry,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use super::{windows_input::WindowsInputMonitor, windows_selection::WindowsSelectionReader};

const TRAY_ID: &str = "paper-float-tray";
const SHOW_SETTINGS_MENU_ID: &str = "show-settings";
const QUIT_MENU_ID: &str = "quit";
pub(super) const SELECTION_SHORTCUT_LABEL: &str = "Ctrl+Alt+T";
static SELECTION_SHORTCUT_REGISTERED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrayMenuAction {
    ShowSettings,
    Quit,
}

fn tray_menu_action(id: &str) -> Option<TrayMenuAction> {
    match id {
        SHOW_SETTINGS_MENU_ID => Some(TrayMenuAction::ShowSettings),
        QUIT_MENU_ID => Some(TrayMenuAction::Quit),
        _ => None,
    }
}

pub(super) fn configure_builder(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    #[cfg(not(feature = "acceptance-testing"))]
    {
        builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings_window(app);
        }))
    }

    #[cfg(feature = "acceptance-testing")]
    {
        builder
    }
}

pub(super) fn setup(app: &mut App<Wry>, quitting: Arc<AtomicBool>) -> tauri::Result<()> {
    let reader =
        WindowsSelectionReader::new(app.handle().clone()).map_err(std::io::Error::other)?;
    app.manage(reader);
    let input_monitor = match WindowsInputMonitor::start(app.handle().clone()) {
        Ok(monitor) => monitor,
        Err(error) => {
            eprintln!("windows_input.unavailable: {error}");
            WindowsInputMonitor::unavailable()
        }
    };
    app.manage(input_monitor);

    let selection_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyT);
    let handler_shortcut = selection_shortcut;
    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, shortcut, event| {
                if shortcut == &handler_shortcut && event.state == ShortcutState::Pressed {
                    crate::handle_windows_selection_shortcut(app);
                }
            })
            .build(),
    )?;
    let shortcut_registration = app.global_shortcut().register(selection_shortcut);
    match &shortcut_registration {
        Ok(()) => SELECTION_SHORTCUT_REGISTERED.store(true, Ordering::Release),
        Err(error) => {
            SELECTION_SHORTCUT_REGISTERED.store(false, Ordering::Release);
            eprintln!("windows_selection.shortcut_registration_failed: {error}");
        }
    }

    #[cfg(not(feature = "acceptance-testing"))]
    shortcut_registration.map_err(std::io::Error::other)?;

    let menu = MenuBuilder::new(app)
        .text(SHOW_SETTINGS_MENU_ID, "打开设置")
        .separator()
        .text(QUIT_MENU_ID, "退出")
        .build()?;

    let menu_quitting = quitting.clone();
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Paper Float Translator")
        .on_menu_event(
            move |app, event| match tray_menu_action(event.id().as_ref()) {
                Some(TrayMenuAction::ShowSettings) => show_settings_window(app),
                Some(TrayMenuAction::Quit) => {
                    menu_quitting.store(true, Ordering::Release);
                    app.exit(0);
                }
                None => {}
            },
        )
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                show_settings_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

pub(super) fn selection_shortcut_registered() -> bool {
    SELECTION_SHORTCUT_REGISTERED.load(Ordering::Acquire)
}

pub(super) fn input_monitor_running(app: &AppHandle) -> bool {
    app.try_state::<WindowsInputMonitor>()
        .is_some_and(|monitor| monitor.is_running())
}

pub(super) fn set_input_monitor_enabled(app: &AppHandle, enabled: bool) {
    if let Some(monitor) = app.try_state::<WindowsInputMonitor>() {
        monitor.set_enabled(enabled);
    }
}

pub(super) fn set_automatic_selection_enabled(
    app: &AppHandle,
    enabled: bool,
) -> Result<bool, String> {
    app.try_state::<WindowsSelectionReader>()
        .ok_or_else(|| "windows_uia_worker_stopped".to_owned())?
        .set_automatic_enabled(enabled, std::time::Duration::from_millis(750))
}

pub(super) fn automatic_selection_running(app: &AppHandle) -> bool {
    app.try_state::<WindowsSelectionReader>()
        .is_some_and(|reader| reader.automatic_running())
}

pub(super) fn request_automatic_selection(app: &AppHandle, source_pid: i32) -> bool {
    app.try_state::<WindowsSelectionReader>()
        .is_some_and(|reader| reader.request_automatic_trigger(source_pid))
}

pub(super) fn cancel_automatic_selection(app: &AppHandle) {
    if let Some(reader) = app.try_state::<WindowsSelectionReader>() {
        reader.cancel_automatic_trigger();
    }
}

pub(super) fn shutdown(app: &AppHandle) {
    if let Some(monitor) = app.try_state::<WindowsInputMonitor>() {
        monitor.stop();
    }
    if let Some(reader) = app.try_state::<WindowsSelectionReader>() {
        reader.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_only_owned_tray_menu_ids() {
        assert_eq!(
            tray_menu_action(SHOW_SETTINGS_MENU_ID),
            Some(TrayMenuAction::ShowSettings)
        );
        assert_eq!(tray_menu_action(QUIT_MENU_ID), Some(TrayMenuAction::Quit));
        assert_eq!(tray_menu_action("window-menu-quit"), None);
    }
}
