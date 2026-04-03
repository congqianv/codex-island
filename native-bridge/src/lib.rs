use codex_island_core::discovery::{CliSessionMonitor, SessionMonitor};
use codex_island_core::focus::{
    focus_session, reply_to_session, submit_session_reply_with_transport, LocalCodexSubmitTransport,
};
use codex_island_core::models::PromptSource;
use codex_island_core::{AppSnapshot, CoreState};

enum CliCommand {
    GetSessions,
    FocusSession(String),
    SubmitSessionReply { session_id: String, reply: String },
}

pub fn run_cli(args: &[String]) -> Result<(), String> {
    match parse_command(args)? {
        CliCommand::GetSessions => {
            println!("{}", get_sessions_json()?);
            Ok(())
        }
        CliCommand::FocusSession(session_id) => focus_session_by_id(&session_id),
        CliCommand::SubmitSessionReply { session_id, reply } => {
            submit_session_reply_by_id(&session_id, &reply)
        }
    }
}

pub fn get_sessions_json() -> Result<String, String> {
    get_sessions_json_from_monitor(&CliSessionMonitor)
}

pub fn focus_session_by_id(session_id: &str) -> Result<(), String> {
    focus_session_from_monitor(&CliSessionMonitor, session_id)
}

pub fn submit_session_reply_by_id(session_id: &str, reply: &str) -> Result<(), String> {
    submit_session_reply_from_monitor(&CliSessionMonitor, session_id, reply)
}

pub fn snapshot_to_json(snapshot: &AppSnapshot) -> Result<String, String> {
    serde_json::to_string(snapshot).map_err(|error| error.to_string())
}

fn parse_command(args: &[String]) -> Result<CliCommand, String> {
    match args.get(1).map(String::as_str) {
        Some("get-sessions") => Ok(CliCommand::GetSessions),
        Some("focus-session") => {
            let session_id = args
                .get(2)
                .ok_or_else(|| usage("focus-session requires a session id"))?;
            Ok(CliCommand::FocusSession(session_id.clone()))
        }
        Some("submit-session-reply") => {
            let session_id = args
                .get(2)
                .ok_or_else(|| usage("submit-session-reply requires a session id"))?;
            let reply = args
                .get(3)
                .ok_or_else(|| usage("submit-session-reply requires reply text"))?;
            Ok(CliCommand::SubmitSessionReply {
                session_id: session_id.clone(),
                reply: reply.clone(),
            })
        }
        Some(other) => Err(usage(&format!("unknown command: {other}"))),
        None => Err(usage("missing command")),
    }
}

fn usage(reason: &str) -> String {
    format!(
        "{reason}\nusage:\n  native-bridge get-sessions\n  native-bridge focus-session <session_id>\n  native-bridge submit-session-reply <session_id> <reply>"
    )
}

fn get_sessions_json_from_monitor<M: SessionMonitor>(monitor: &M) -> Result<String, String> {
    let snapshot = snapshot_from_monitor(monitor);
    snapshot_to_json(&snapshot)
}

fn snapshot_from_monitor<M: SessionMonitor>(monitor: &M) -> AppSnapshot {
    let state = state_from_monitor(monitor);
    state.snapshot()
}

fn focus_session_from_monitor<M: SessionMonitor>(
    monitor: &M,
    session_id: &str,
) -> Result<(), String> {
    let state = state_from_monitor(monitor);
    let session = state
        .focusable_session(session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    focus_session(&session)
}

fn submit_session_reply_from_monitor<M: SessionMonitor>(
    monitor: &M,
    session_id: &str,
    reply: &str,
) -> Result<(), String> {
    let state = state_from_monitor(monitor);
    let session = state
        .focusable_session(session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    if matches!(session.prompt_source, Some(PromptSource::Terminal)) {
        return reply_to_session(&session, reply);
    }

    let transport = LocalCodexSubmitTransport::detect();
    submit_session_reply_with_transport(&session, reply, &transport)
        .map_err(|error| error.to_string())
}

fn state_from_monitor<M: SessionMonitor>(monitor: &M) -> CoreState {
    let state = CoreState::default();
    let observations = monitor.poll();
    let mut store = state.store.lock().expect("session store lock poisoned");
    store.ingest(observations);
    drop(store);
    state
}

#[cfg(test)]
mod tests {
    use codex_island_core::models::{
        DiscoveryObservation, SessionSource, SessionStatus, SessionSummary, SessionViewModel,
        TerminalApp,
    };

    use super::{
        focus_session_from_monitor, get_sessions_json_from_monitor, parse_command,
        snapshot_to_json, CliCommand, submit_session_reply_from_monitor,
    };

    #[derive(Default)]
    struct StaticMonitor {
        observations: Vec<DiscoveryObservation>,
    }

    impl codex_island_core::discovery::SessionMonitor for StaticMonitor {
        fn poll(&self) -> Vec<DiscoveryObservation> {
            self.observations.clone()
        }
    }

    #[test]
    fn parses_get_sessions_command() {
        let args = vec!["native-bridge".into(), "get-sessions".into()];

        assert!(matches!(parse_command(&args), Ok(CliCommand::GetSessions)));
    }

    #[test]
    fn renders_snapshot_as_compact_json() {
        let payload = codex_island_core::AppSnapshot {
            sessions: vec![SessionViewModel {
                session_id: "session-1".into(),
                title: "Agent".into(),
                project_name: "alpha".into(),
                status: SessionStatus::Running,
                needs_attention: false,
                can_reply: false,
                subtitle: "Working".into(),
                prompt_text: None,
                action_options: vec![],
                prompt_source: None,
                terminal_label: "Terminal".into(),
                relative_last_activity: "just now".into(),
                last_activity_unix_ms: 1_000,
            }],
            summary: SessionSummary {
                total: 1,
                running: 1,
                waiting: 0,
                completed: 0,
            },
        };

        let json = snapshot_to_json(&payload).expect("json");
        assert!(json.contains("\"session_id\":\"session-1\""));
        assert!(json.contains("\"summary\":{\"total\":1"));
    }

    #[test]
    fn snapshot_from_monitor_keeps_waiting_state() {
        let monitor = StaticMonitor {
            observations: vec![DiscoveryObservation {
                pid: 101,
                parent_pid: Some(100),
                tty: Some("/dev/ttys001".into()),
                cwd: Some("/tmp/alpha".into()),
                terminal_app: Some(TerminalApp::Terminal),
                title: "Agent".into(),
                project_name: Some("alpha".into()),
                source: SessionSource::Cli,
                activity_label: Some("Working".into()),
                interaction_hint: Some("Continue with file access approval?".into()),
                prompt_actions: vec![],
                prompt_source: None,
                submit_target: None,
                seen_at_unix_ms: 1_000,
            }],
        };

        let json = get_sessions_json_from_monitor(&monitor).expect("json");
        assert!(json.contains("\"waiting\":1"));
        assert!(json.contains("Continue with file access approval?"));
    }

    #[test]
    fn focus_session_errors_when_missing_session_is_requested() {
        let monitor = StaticMonitor::default();
        let error = focus_session_from_monitor(&monitor, "missing-session").unwrap_err();

        assert!(error.contains("missing-session"));
    }

    #[test]
    fn parses_submit_session_reply_command() {
        let args = vec![
            "native-bridge".into(),
            "submit-session-reply".into(),
            "session-1".into(),
            "hello".into(),
        ];

        assert!(matches!(
            parse_command(&args),
            Ok(CliCommand::SubmitSessionReply { session_id, reply })
                if session_id == "session-1" && reply == "hello"
        ));
    }

    #[test]
    fn submit_session_reply_errors_when_missing_session_is_requested() {
        let monitor = StaticMonitor::default();
        let error = submit_session_reply_from_monitor(&monitor, "missing-session", "hello")
            .unwrap_err();

        assert!(error.contains("missing-session"));
    }
}
