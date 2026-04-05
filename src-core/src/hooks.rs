use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::models::{
    PromptSource, SessionEvent, SessionSource, SessionStatus, SubmitTarget, TerminalApp,
};

pub const SOCKET_PATH: &str = "/tmp/codex-island.sock";
const HOOK_EVENTS: [&str; 5] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
];
const MANAGED_HOOK_COMMAND: &str = "python3 ~/.codex/hooks/codex-island-state.py";
const EVENT_CACHE_TTL_MS: i64 = 30_000;

pub fn managed_hook_command() -> &'static str {
    MANAGED_HOOK_COMMAND
}

pub fn install_managed_hooks() -> Result<(), String> {
    let hook_dir = codex_hook_dir()?;
    fs::create_dir_all(&hook_dir).map_err(|error| error.to_string())?;

    let script_path = hook_dir.join("codex-island-state.py");
    let script = include_str!("../assets/codex-island-state.py");
    fs::write(&script_path, script).map_err(|error| error.to_string())?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&script_path)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).map_err(|error| error.to_string())?;
    }

    let hooks_path = codex_home()?.join("hooks.json");
    let current = if hooks_path.exists() {
        let content = fs::read_to_string(&hooks_path).map_err(|error| error.to_string())?;
        serde_json::from_str::<Value>(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };
    let merged = merge_managed_hook_command(&current, MANAGED_HOOK_COMMAND);
    let rendered = serde_json::to_string_pretty(&merged).map_err(|error| error.to_string())?;
    fs::write(hooks_path, format!("{rendered}\n")).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn merge_managed_hook_command(current: &Value, command: &str) -> Value {
    let mut merged = current.clone();
    if !merged.is_object() {
        merged = json!({});
    }

    if merged.get("hooks").and_then(Value::as_object).is_none() {
        merged["hooks"] = json!({});
    }

    for event in HOOK_EVENTS {
        if merged["hooks"].get(event).and_then(Value::as_array).is_none() {
            merged["hooks"][event] = json!([{ "hooks": [] }]);
        }

        let groups = merged["hooks"][event].as_array_mut().expect("hooks groups");
        if groups.is_empty() {
            groups.push(json!({ "hooks": [] }));
        }

        let hooks = groups[0]["hooks"].as_array_mut().expect("hook commands");
        let exists = hooks
            .iter()
            .any(|hook| hook.get("command").and_then(Value::as_str) == Some(command));
        if !exists {
            hooks.push(json!({
                "type": "command",
                "command": command,
                "timeout": 30
            }));
        }
    }

    merged
}

pub fn parse_hook_event_value(value: &Value) -> Option<SessionEvent> {
    let session_id = value.get("session_id")?.as_str()?.to_string();
    let status = match value.get("status").and_then(Value::as_str)? {
        "waiting_for_input" => SessionStatus::WaitingInput,
        "processing" | "running_tool" | "notification" => SessionStatus::Running,
        "idle" => SessionStatus::Idle,
        "completed" => SessionStatus::Completed,
        "failed" => SessionStatus::Failed,
        _ => SessionStatus::Running,
    };
    let source = match value.get("provider").and_then(Value::as_str) {
        Some("desktop") => SessionSource::Desktop,
        _ => SessionSource::Cli,
    };
    let prompt_text = value
        .get("prompt")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let user_prompt = value
        .get("user_prompt")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let activity_label = value
        .get("tool")
        .and_then(Value::as_str)
        .map(|tool| format!("Running {tool}"))
        .or_else(|| match status {
            SessionStatus::WaitingInput => Some("Waiting for input".into()),
            SessionStatus::Completed => Some("Completed".into()),
            SessionStatus::Failed => Some("Failed".into()),
            _ => Some("Working".into()),
        });
    let happened_at_unix_ms = value
        .get("timestamp")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
    let terminal_app = value
        .get("terminal_name")
        .and_then(Value::as_str)
        .and_then(parse_terminal_app);

    Some(SessionEvent {
        session_id: session_id.clone(),
        thread_id: Some(session_id.clone()),
        source,
        pid: value
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok()),
        cwd: value.get("cwd").and_then(Value::as_str).map(ToOwned::to_owned),
        tty: value.get("tty").and_then(Value::as_str).map(ToOwned::to_owned),
        terminal_app,
        title: value
            .get("cwd")
            .and_then(Value::as_str)
            .and_then(|cwd| Path::new(cwd).file_name().map(|name| name.to_string_lossy().into())),
        project_name: value
            .get("cwd")
            .and_then(Value::as_str)
            .and_then(|cwd| Path::new(cwd).file_name().map(|name| name.to_string_lossy().into())),
        activity_label,
        prompt_text,
        user_prompt,
        prompt_actions: vec![],
        prompt_source: Some(PromptSource::Thread),
        submit_target: Some(SubmitTarget::ThreadId(session_id)),
        status,
        transcript_path: value
            .get("transcript_path")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        happened_at_unix_ms,
    })
}

pub fn read_cached_events() -> Vec<SessionEvent> {
    let path = event_cache_path();
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let cutoff = chrono::Utc::now().timestamp_millis() - EVENT_CACHE_TTL_MS;
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter_map(|value| parse_hook_event_value(&value))
        .filter(|event| event.happened_at_unix_ms >= cutoff)
        .collect()
}

pub fn start_hook_event_server(handler: Arc<dyn Fn(SessionEvent) + Send + Sync>) -> Result<(), String> {
    if Path::new(SOCKET_PATH).exists() {
        let _ = fs::remove_file(SOCKET_PATH);
    }

    let listener = UnixListener::bind(SOCKET_PATH).map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        for connection in listener.incoming() {
            let Ok(mut stream) = connection else {
                continue;
            };
            let mut buffer = String::new();
            if stream.read_to_string(&mut buffer).is_err() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(&buffer) else {
                continue;
            };
            if let Some(event) = parse_hook_event_value(&value) {
                handler(event);
            }
        }
    });

    Ok(())
}

fn parse_terminal_app(value: &str) -> Option<TerminalApp> {
    let normalized = value.to_lowercase();
    if normalized.contains("iterm") {
        return Some(TerminalApp::ITerm);
    }
    if normalized.contains("terminal") {
        return Some(TerminalApp::Terminal);
    }
    if value.is_empty() {
        return None;
    }

    Some(TerminalApp::Unsupported(value.to_string()))
}

fn codex_home() -> Result<PathBuf, String> {
    let home = std::env::var("HOME").map_err(|error| error.to_string())?;
    Ok(Path::new(&home).join(".codex"))
}

fn codex_hook_dir() -> Result<PathBuf, String> {
    Ok(codex_home()?.join("hooks"))
}

fn event_cache_path() -> PathBuf {
    std::env::var("HOME")
        .map(|home| Path::new(&home).join(".codex/hooks/codex-island-events.jsonl"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/codex-island-events.jsonl"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::models::{SessionIngestionMode, SessionStatus};

    use super::{merge_managed_hook_command, parse_hook_event_value};

    #[test]
    fn parses_waiting_hook_event_into_session_event() {
        let event = parse_hook_event_value(&json!({
            "session_id": "thread-123",
            "cwd": "/tmp/alpha",
            "tty": "/dev/ttys001",
            "terminal_name": "iTerm.app",
            "event": "Stop",
            "status": "waiting_for_input",
            "pid": 101,
            "prompt": "Continue?",
            "timestamp": 2000
        }))
        .expect("event");

        assert_eq!(event.status, SessionStatus::WaitingInput);
        assert_eq!(event.prompt_text.as_deref(), Some("Continue?"));
    }

    #[test]
    fn merges_managed_hook_command_without_removing_existing_hooks() {
        let merged = merge_managed_hook_command(
            &json!({
                "hooks": {
                    "SessionStart": [
                        {
                            "hooks": [
                                {
                                    "type": "command",
                                    "command": "echo existing",
                                    "timeout": 5
                                }
                            ]
                        }
                    ]
                }
            }),
            "python3 ~/.codex/hooks/codex-island-state.py",
        );

        let commands = merged["hooks"]["SessionStart"][0]["hooks"]
            .as_array()
            .expect("commands");
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0]["command"], "echo existing");
        assert_eq!(commands[1]["command"], "python3 ~/.codex/hooks/codex-island-state.py");
        assert_eq!(SessionIngestionMode::Hooks.as_str(), "hooks");
    }
}
