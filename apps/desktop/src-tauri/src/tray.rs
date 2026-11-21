//! System tray: agent health at a glance + quick actions.
//!
//! Requires `tauri` with the `tray-icon` feature (already enabled in the
//! workspace). Menu actions talk to the background agent over IPC.

use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show RepoNyx", true, None::<&str>)?;
    let health = MenuItem::with_id(app, "health", "Agent status…", true, None::<&str>)?;
    let ensure = MenuItem::with_id(app, "ensure", "Start agent if needed", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause agent", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume agent", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&show, &health, &ensure, &pause, &resume, &quit],
    )?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("RepoNyx")
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                "health" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                        let _ = w.eval("window.location.hash = '#/agent'");
                    }
                }
                "ensure" => {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::agent::ensure_agent_running_command().await;
                        let _ = handle;
                    });
                }
                "pause" => {
                    tauri::async_runtime::spawn(async {
                        let _ = crate::agent::agent_pause_command().await;
                    });
                }
                "resume" => {
                    tauri::async_runtime::spawn(async {
                        let _ = crate::agent::agent_resume_command().await;
                    });
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
