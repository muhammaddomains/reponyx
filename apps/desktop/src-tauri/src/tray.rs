//! System tray: agent health at a glance + quick actions.
//!
//! Requires `tauri` with the `tray-icon` feature (already enabled in the
//! workspace). Menu actions talk to the background agent over IPC, and
//! "Sync due now" materializes the due schedules (the same operation
//! the Schedules page offers) and reports the result as a system
//! notification. Stop/Pause/Resume are state-aware: a background task
//! polls agent health every 30 s and enables only the action that
//! makes sense right now (none when the agent is down). Every action
//! reports its outcome as a notification, so a click is never silent.

use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

/// How often the tray refreshes Pause/Resume from agent health.
const HEALTH_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let Some(icon) = app.default_window_icon() else {
        tracing::warn!("no default window icon; skipping tray setup");
        return Ok(());
    };
    let show = MenuItem::with_id(app, "show", "Show RepoNyx", true, None::<&str>)?;
    let health = MenuItem::with_id(app, "health", "Agent status…", true, None::<&str>)?;
    let sync_now = MenuItem::with_id(app, "sync_now", "Sync due now", true, None::<&str>)?;
    let ensure = MenuItem::with_id(app, "ensure", "Start agent if needed", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop agent", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause agent", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume agent", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &show, &health, &sync_now, &ensure, &stop, &pause, &resume, &separator, &quit,
        ],
    )?;

    // Keep Stop/Pause/Resume honest: poll agent health and enable
    // only the action that applies. When the agent is down all
    // three are off ("Start agent if needed" is the way back);
    // when paused only Resume is on; otherwise Stop and Pause are
    // on.
    {
        let stop = stop.clone();
        let pause = pause.clone();
        let resume = resume.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let (stop_enabled, pause_enabled, resume_enabled) =
                    match crate::agent::agent_health_command().await {
                        Ok(view) => match view.status {
                            Some(status) => (true, !status.paused, status.paused),
                            None => (false, false, false),
                        },
                        Err(_) => (false, false, false),
                    };
                let _ = stop.set_enabled(stop_enabled);
                let _ = pause.set_enabled(pause_enabled);
                let _ = resume.set_enabled(resume_enabled);
                tokio::time::sleep(HEALTH_POLL_INTERVAL).await;
            }
        });
    }

    let _tray = TrayIconBuilder::new()
        .icon(icon.clone())
        .menu(&menu)
        .tooltip("RepoNyx")
        .on_menu_event(|app, event| match event.id.as_ref() {
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
            "sync_now" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_notification::NotificationExt;
                    let outcome = async {
                        crate::agent::ensure_agent_running_command().await?;
                        let db = handle.state::<Option<reponyx_database::Database>>();
                        let report = crate::schedules::materialize_due_command(db).await?;
                        Ok::<_, crate::auth::CommandError>(report)
                    }
                    .await;
                    let (title, body) = match outcome {
                        Ok(report) => {
                            let created = report.created_job_ids.len();
                            if created > 0 {
                                (
                                    "Sync queued",
                                    format!(
                                        "Queued {created} sync job{} for due repositories.",
                                        if created == 1 { "" } else { "s" }
                                    ),
                                )
                            } else {
                                (
                                    "Nothing due",
                                    "No repository is due for a sync right now.".to_string(),
                                )
                            }
                        }
                        Err(e) => (
                            "Sync failed",
                            format!("Could not queue due syncs: {}", e.message),
                        ),
                    };
                    let _ = handle
                        .notification()
                        .builder()
                        .title(title)
                        .body(body)
                        .show();
                });
            }
            "ensure" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_notification::NotificationExt;
                    let outcome = crate::agent::ensure_agent_running_command().await;
                    let (title, body) = match outcome {
                        Ok(view) if view.reachable => (
                            "Agent running",
                            "The background agent is running.".to_string(),
                        ),
                        Ok(view) => ("Agent did not start", view.message),
                        Err(e) => ("Agent did not start", e.message),
                    };
                    let _ = handle
                        .notification()
                        .builder()
                        .title(title)
                        .body(body)
                        .show();
                });
            }
            "stop" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_notification::NotificationExt;
                    let outcome = crate::agent::stop_agent_command().await;
                    let (title, body) = match outcome {
                        Ok(message) => ("Agent", message),
                        Err(e) => ("Agent did not stop", e.message),
                    };
                    let _ = handle
                        .notification()
                        .builder()
                        .title(title)
                        .body(body)
                        .show();
                });
            }
            "pause" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_notification::NotificationExt;
                    let outcome = crate::agent::agent_pause_command().await;
                    let (title, body) = match outcome {
                        Ok(()) => ("Agent paused", "Scheduled sync is paused.".to_string()),
                        Err(e) => ("Agent did not pause", e.message),
                    };
                    let _ = handle
                        .notification()
                        .builder()
                        .title(title)
                        .body(body)
                        .show();
                });
            }
            "resume" => {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri_plugin_notification::NotificationExt;
                    let outcome = crate::agent::agent_resume_command().await;
                    let (title, body) = match outcome {
                        Ok(()) => (
                            "Agent resumed",
                            "Scheduled sync is running again.".to_string(),
                        ),
                        Err(e) => ("Agent did not resume", e.message),
                    };
                    let _ = handle
                        .notification()
                        .builder()
                        .title(title)
                        .body(body)
                        .show();
                });
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
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
