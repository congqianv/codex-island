mod window;

use chrono::Utc;
use codex_island_core::discovery::{CliSessionMonitor, SessionMonitor};
use codex_island_core::focus::{
    focus_session as focus_terminal_session, open_session_project as open_session_project_folder,
    reply_to_session,
};
use codex_island_core::hooks::{
    install_managed_hooks, read_cached_events, start_hook_event_server,
};
use codex_island_core::models::{
    CodexSession, SessionEvent, SessionStatus, SessionSummary, SessionViewModel,
};
use codex_island_core::notify::notify_attention;
use codex_island_core::{AppSnapshot, CoreState};
#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use window::{sync_island_window as sync_native_island_window, ExpandedView};

#[tauri::command]
fn get_sessions(state: tauri::State<'_, CoreState>) -> Result<AppSnapshot, String> {
    Ok(state.snapshot())
}

#[tauri::command]
fn focus_session(session_id: String, state: tauri::State<'_, CoreState>) -> Result<(), String> {
    let session = state
        .focusable_session(&session_id)
        .ok_or_else(|| "Session not found".to_string())?;

    focus_terminal_session(&session)
}

#[tauri::command]
fn reply_session(
    session_id: String,
    reply: String,
    state: tauri::State<'_, CoreState>,
) -> Result<(), String> {
    let session = state
        .focusable_session(&session_id)
        .ok_or_else(|| "Session not found".to_string())?;

    reply_to_session(&session, &reply)
}

#[tauri::command]
fn open_session_project(
    session_id: String,
    state: tauri::State<'_, CoreState>,
) -> Result<(), String> {
    let session = state
        .focusable_session(&session_id)
        .ok_or_else(|| "Session not found".to_string())?;

    open_session_project_folder(&session)
}

#[tauri::command]
fn sync_island_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, CoreState>,
    expanded: bool,
    expanded_view: String,
    session_count: usize,
) -> Result<(), String> {
    state.set_window_expanded(expanded);
    sync_native_island_window(
        &app,
        expanded,
        ExpandedView::from_wire(&expanded_view),
        session_count,
    )
    .map_err(|error| error.to_string())
}

fn start_monitor_loop<R, M>(app: AppHandle<R>, monitor: M)
where
    R: Runtime,
    M: SessionMonitor + 'static,
{
    let state = app.state::<CoreState>().store.clone();
    let hook_store = state.clone();
    let hook_app = app.clone();

    let _ = start_hook_event_server(std::sync::Arc::new(move |event: SessionEvent| {
        apply_session_update(&hook_store, &hook_app, |store| store.ingest_event(event));
    }));

    tauri::async_runtime::spawn(async move {
        loop {
            let observations = monitor.poll();
            for event in read_cached_events() {
                apply_session_update(&state, &app, |store| store.ingest_event(event));
            }
            apply_session_update(&state, &app, |store| store.ingest_observations(observations));

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
}

fn apply_session_update<R, F>(
    state: &std::sync::Arc<std::sync::Mutex<codex_island_core::session_store::SessionStore>>,
    app: &AppHandle<R>,
    update: F,
) where
    R: Runtime,
    F: FnOnce(&mut codex_island_core::session_store::SessionStore) -> bool,
{
    let (changed, payload, pending_notifications) = {
        let mut store = state.lock().expect("session store lock poisoned");
        let changed = update(&mut store);
        let sessions = store.sessions();
        let pending_notifications = sessions
            .iter()
            .filter(|session| session.needs_attention && session.notification_sent_at.is_none())
            .map(|session| {
                (
                    session.session_id.clone(),
                    session.title.clone(),
                    session.last_snapshot.clone(),
                )
            })
            .collect::<Vec<_>>();

        for session in store
            .sessions_mut()
            .filter(|session| session.needs_attention && session.notification_sent_at.is_none())
        {
            session.notification_sent_at = Some(Utc::now());
        }

        (changed, build_payload(sessions), pending_notifications)
    };

    for (_, title, subtitle) in pending_notifications {
        let body = subtitle.unwrap_or(title);
        notify_attention("Codex needs attention", &body);
    }

    if changed {
        let _ = app.emit("sessions:updated", payload);
    }
}

fn build_payload(sessions: Vec<CodexSession>) -> AppSnapshot {
    let summary = SessionSummary {
        total: sessions.len(),
        running: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::Running))
            .count(),
        idle: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::Idle))
            .count(),
        waiting: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::WaitingInput))
            .count(),
        discovering: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::Discovering))
            .count(),
        failed: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::Failed))
            .count(),
        completed: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::Completed))
            .count(),
    };

    AppSnapshot {
        sessions: sessions
            .iter()
            .take(5)
            .map(SessionViewModel::from)
            .collect(),
        summary,
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(CoreState::default())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let _ = install_managed_hooks();

            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(ActivationPolicy::Accessory)?;

            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                let _ = sync_native_island_window(&app_handle, false, ExpandedView::List, 0);
            });

            window::start_hover_monitor(app.handle().clone());
            start_monitor_loop(app.handle().clone(), CliSessionMonitor);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            focus_session,
            open_session_project,
            reply_session,
            sync_island_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn tauri_layer_can_read_core_snapshot_type() {
        let snapshot = codex_island_core::AppSnapshot::empty();
        assert_eq!(snapshot.summary.total, 0);
    }
}
