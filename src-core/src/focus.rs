use std::{
    io::{BufRead, BufReader, Read, Write},
    fs,
    os::unix::net::UnixStream,
    path::Path,
    process::{ChildStdout, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use serde_json::{json, Value};

use crate::models::{CodexSession, SubmitTarget, TerminalApp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitError {
    UnsupportedSessionTarget { session_id: String },
    SubmitTransportUnavailable,
    SubmitTransportFailed { detail: String },
}

impl SubmitError {
    pub fn unsupported_session_target(session_id: impl Into<String>) -> Self {
        Self::UnsupportedSessionTarget {
            session_id: session_id.into(),
        }
    }

    pub fn transport_unavailable() -> Self {
        Self::SubmitTransportUnavailable
    }

    pub fn transport_failed(detail: impl Into<String>) -> Self {
        Self::SubmitTransportFailed {
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSessionTarget { session_id } => {
                write!(
                    f,
                    "unsupported_session_target: session {session_id} has no submit target"
                )
            }
            Self::SubmitTransportUnavailable => {
                write!(
                    f,
                    "submit_transport_unavailable: no local submit transport found"
                )
            }
            Self::SubmitTransportFailed { detail } => {
                write!(f, "submit_transport_failed: {detail}")
            }
        }
    }
}

impl std::error::Error for SubmitError {}

pub trait SubmitTransport {
    fn submit(&self, target: &SubmitTarget, text: &str) -> Result<(), SubmitError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSubmitProbe {
    pub command: Vec<String>,
}

impl LocalSubmitProbe {
    pub fn detect() -> Option<Self> {
        let executable = "/Applications/Codex.app/Contents/Resources/codex";
        Path::new(executable).exists().then(|| Self {
            command: vec![
                executable.into(),
                "app-server".into(),
                "--listen".into(),
                "stdio://".into(),
            ],
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalCodexSubmitTransport {
    probe: Option<LocalSubmitProbe>,
}

impl LocalCodexSubmitTransport {
    pub fn detect() -> Self {
        Self::from_probe_result(LocalSubmitProbe::detect())
    }

    pub fn from_probe_result(probe: Option<LocalSubmitProbe>) -> Self {
        Self { probe }
    }
}

impl SubmitTransport for LocalCodexSubmitTransport {
    fn submit(&self, target: &SubmitTarget, text: &str) -> Result<(), SubmitError> {
        if let SubmitTarget::DesktopCommandApproval {
            conversation_id,
            request_id,
        } = target
        {
            let decision = map_desktop_approval_reply(text).ok_or_else(|| {
                SubmitError::transport_failed(format!(
                    "unsupported desktop approval reply: {text}"
                ))
            })?;
            return submit_desktop_command_approval(conversation_id, request_id, decision);
        }

        let probe = self
            .probe
            .as_ref()
            .ok_or_else(SubmitError::transport_unavailable)?;

        run_submit_probe_command(probe, target, text)
    }
}

pub fn focus_session(session: &CodexSession) -> Result<(), String> {
    if matches!(session.source, crate::models::SessionSource::Desktop) {
        return focus_codex_desktop();
    }

    let tty = session
        .tty
        .as_deref()
        .map(|value| value.trim_start_matches("/dev/"));

    let script = match (session.terminal_app.as_ref(), tty) {
        (Some(TerminalApp::Terminal), Some(tty)) => terminal_focus_script(tty),
        (Some(TerminalApp::ITerm), Some(tty)) => iterm_focus_script(tty),
        (Some(TerminalApp::Terminal), None) => return focus_named_application("Terminal"),
        (Some(TerminalApp::ITerm), None) => return focus_named_application("iTerm2"),
        (Some(TerminalApp::Unsupported(app_name)), _) => {
            return focus_named_application(app_name);
        }
        _ => return focus_project_directory(session),
    };

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        focus_terminal_application(session).or_else(|_| {
            focus_project_directory(session)
        }).map_err(|fallback_error| {
            let script_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if script_error.is_empty() {
                fallback_error
            } else {
                format!("{script_error}; fallback failed: {fallback_error}")
            }
        })
    }
}

pub fn open_session_project(session: &CodexSession) -> Result<(), String> {
    if matches!(session.source, crate::models::SessionSource::Desktop) {
        return focus_codex_desktop();
    }

    let cwd = session
        .cwd
        .as_deref()
        .ok_or_else(|| "Session has no project directory".to_string())?;

    let output = Command::new("open")
        .arg(cwd)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn terminal_focus_script(tty: &str) -> String {
    format!(
        r#"
tell application "Terminal"
  reopen
  activate
  repeat with w in windows
    repeat with t in tabs of w
      try
        if tty of t is "{tty}" then
          set miniaturized of w to false
          set selected of t to true
          set frontmost of w to true
          return
        end if
      end try
    end repeat
  end repeat
  error "TTY_NOT_FOUND"
end tell
"#
    )
}

fn iterm_focus_script(tty: &str) -> String {
    format!(
        r#"
tell application "iTerm2"
  reopen
  activate
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        try
          if tty of s is "{tty}" then
            set miniaturized of w to false
            select t
            tell w
              set current tab to t
              set current session of current tab to s
              set frontmost to true
            end tell
            return
          end if
        end try
      end repeat
    end repeat
  end repeat
  error "TTY_NOT_FOUND"
end tell
"#
    )
}

pub fn reply_to_session(session: &CodexSession, reply: &str) -> Result<(), String> {
    let tty = session
        .tty
        .as_deref()
        .map(|value| value.trim_start_matches("/dev/"));
    let escaped_reply = applescript_string(reply);

    let script = match (session.terminal_app.as_ref(), tty) {
        (Some(TerminalApp::Terminal), Some(tty)) => format!(
            r#"
tell application "Terminal"
  activate
  repeat with w in windows
    repeat with t in tabs of w
      try
        if tty of t is "{tty}" then
          do script "{escaped_reply}" in t
          return
        end if
      end try
    end repeat
  end repeat
end tell
"#
        ),
        (Some(TerminalApp::ITerm), Some(tty)) => format!(
            r#"
tell application "iTerm2"
  activate
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        try
          if tty of s is "{tty}" then
            write text "{escaped_reply}" to s
            return
          end if
        end try
      end repeat
    end repeat
  end repeat
end tell
"#
        ),
        (Some(TerminalApp::Unsupported(app_name)), _) => format!(
            r#"
tell application "{resolved_app_name}"
  activate
end tell
delay 0.1
tell application "System Events"
  keystroke "{escaped_reply}"
  key code 36
end tell
"#,
            resolved_app_name = applescript_app_name(app_name)
        ),
        _ => return Err("Session host does not support inline reply".to_string()),
    };

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn submit_session_reply_with_transport(
    session: &CodexSession,
    text: &str,
    transport: &dyn SubmitTransport,
) -> Result<(), SubmitError> {
    let target = session
        .submit_target
        .as_ref()
        .ok_or_else(|| SubmitError::unsupported_session_target(session.session_id.clone()))?;

    transport.submit(target, text)
}

fn run_submit_probe_command(
    probe: &LocalSubmitProbe,
    target: &SubmitTarget,
    text: &str,
) -> Result<(), SubmitError> {
    let (program, args) = probe
        .command
        .split_first()
        .ok_or_else(SubmitError::transport_unavailable)?;

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            SubmitError::transport_failed(format!("failed to spawn app-server: {error}"))
        })?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| SubmitError::transport_failed("app-server stdin unavailable"))?;

        for request in build_submit_requests(target, text) {
            write_json_rpc_line(stdin, &request)?;
        }
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| SubmitError::transport_failed("app-server stdout unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| SubmitError::transport_failed("app-server stderr unavailable"))?;

    let stdout_rx = spawn_stdout_reader(stdout);
    let stderr_handle = thread::spawn(move || -> String {
        let mut stderr = BufReader::new(stderr);
        let mut output = String::new();
        let _ = stderr.read_to_string(&mut output);
        output
    });

    let responses = match stdout_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(result) => result?,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = child.kill();
            let _ = child.wait();
            let stderr_output = stderr_handle.join().unwrap_or_default();
            let detail = if stderr_output.trim().is_empty() {
                "timed out waiting for app-server responses".to_string()
            } else {
                format!(
                    "timed out waiting for app-server responses; stderr: {}",
                    stderr_output.trim()
                )
            };
            return Err(SubmitError::transport_failed(detail));
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = child.kill();
            let _ = child.wait();
            let stderr_output = stderr_handle.join().unwrap_or_default();
            let detail = if stderr_output.trim().is_empty() {
                "app-server response channel closed unexpectedly".to_string()
            } else {
                format!(
                    "app-server response channel closed unexpectedly; stderr: {}",
                    stderr_output.trim()
                )
            };
            return Err(SubmitError::transport_failed(detail));
        }
    };

    let _ = child.kill();
    let _ = child.wait();
    let _stderr_output = stderr_handle.join().unwrap_or_default();

    ensure_json_rpc_success(&responses, 0, "initialize")?;
    ensure_json_rpc_success(&responses, 1, "thread/read")?;
    ensure_json_rpc_success(&responses, 2, "thread/resume")?;
    ensure_json_rpc_success(&responses, 3, "turn/start")?;

    if let Some(response) = find_response(&responses, 1) {
        let response_thread_id = response
            .get("result")
            .and_then(|result| result.get("threadId"))
            .and_then(Value::as_str);
        if let (SubmitTarget::ThreadId(thread_id), Some(response_thread_id)) =
            (target, response_thread_id)
        {
            if response_thread_id != thread_id {
                return Err(SubmitError::transport_failed(format!(
                    "thread/read returned unexpected thread id {response_thread_id}"
                )));
            }
        }
    }

    Ok(())
}

fn map_desktop_approval_reply(reply: &str) -> Option<&'static str> {
    match reply.trim() {
        "1" => Some("accept"),
        "2" => Some("acceptForSession"),
        "3" => Some("decline"),
        _ => None,
    }
}

fn submit_desktop_command_approval(
    conversation_id: &str,
    request_id: &str,
    decision: &str,
) -> Result<(), SubmitError> {
    let socket_path = detect_codex_ipc_socket()
        .ok_or_else(|| SubmitError::transport_failed("codex desktop ipc socket not found"))?;
    let mut stream = UnixStream::connect(&socket_path).map_err(|error| {
        SubmitError::transport_failed(format!(
            "failed to connect to codex desktop ipc socket {socket_path}: {error}"
        ))
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| SubmitError::transport_failed(format!("failed to set read timeout: {error}")))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| SubmitError::transport_failed(format!("failed to set write timeout: {error}")))?;

    let client_id = ipc_initialize(&mut stream)?;
    let request_id_value = uuid::Uuid::new_v4().to_string();
    write_ipc_message(
        &mut stream,
        &json!({
            "type": "request",
            "requestId": request_id_value,
            "sourceClientId": client_id,
            "version": 1,
            "method": "thread-follower-command-approval-decision",
            "params": {
                "conversationId": conversation_id,
                "requestId": request_id,
                "decision": decision,
            }
        }),
    )?;

    loop {
        let message = read_ipc_message(&mut stream)?;
        if message
            .get("type")
            .and_then(Value::as_str)
            != Some("response")
        {
            continue;
        }
        if message
            .get("requestId")
            .and_then(Value::as_str)
            != Some(request_id_value.as_str())
        {
            continue;
        }
        if message
            .get("resultType")
            .and_then(Value::as_str)
            == Some("success")
        {
            return Ok(());
        }

        let error = message
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown ipc error");
        return Err(SubmitError::transport_failed(format!(
            "desktop approval ipc failed: {error}"
        )));
    }
}

fn detect_codex_ipc_socket() -> Option<String> {
    let dir = std::env::temp_dir().join("codex-ipc");
    let mut sockets = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let is_socket = path
                .extension()
                .map(|extension| extension == "sock")
                .unwrap_or(false);
            is_socket.then_some(path)
        })
        .collect::<Vec<_>>();
    sockets.sort();
    sockets
        .pop()
        .map(|path| path.to_string_lossy().to_string())
}

fn ipc_initialize(stream: &mut UnixStream) -> Result<String, SubmitError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    write_ipc_message(
        stream,
        &json!({
            "type": "request",
            "requestId": request_id,
            "sourceClientId": "",
            "version": 0,
            "method": "initialize",
            "params": {
                "clientType": "codex-island",
            }
        }),
    )?;

    loop {
        let message = read_ipc_message(stream)?;
        if message
            .get("type")
            .and_then(Value::as_str)
            != Some("response")
        {
            continue;
        }
        if message
            .get("requestId")
            .and_then(Value::as_str)
            != Some(request_id.as_str())
        {
            continue;
        }
        if message
            .get("resultType")
            .and_then(Value::as_str)
            != Some("success")
        {
            let error = message
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("initialize failed");
            return Err(SubmitError::transport_failed(format!(
                "desktop ipc initialize failed: {error}"
            )));
        }

        let client_id = message
            .get("result")
            .and_then(|result| result.get("clientId"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                SubmitError::transport_failed("desktop ipc initialize returned no clientId")
            })?;
        return Ok(client_id.to_string());
    }
}

fn write_ipc_message(stream: &mut UnixStream, value: &Value) -> Result<(), SubmitError> {
    let payload = serde_json::to_vec(value).map_err(|error| {
        SubmitError::transport_failed(format!("failed to encode ipc message: {error}"))
    })?;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(&payload);
    stream.write_all(&frame).map_err(|error| {
        SubmitError::transport_failed(format!("failed to write ipc message: {error}"))
    })
}

fn read_ipc_message(stream: &mut UnixStream) -> Result<Value, SubmitError> {
    let mut header = [0_u8; 4];
    stream.read_exact(&mut header).map_err(|error| {
        SubmitError::transport_failed(format!("failed to read ipc frame header: {error}"))
    })?;
    let frame_len = u32::from_le_bytes(header) as usize;
    let mut payload = vec![0_u8; frame_len];
    stream.read_exact(&mut payload).map_err(|error| {
        SubmitError::transport_failed(format!("failed to read ipc frame payload: {error}"))
    })?;
    serde_json::from_slice(&payload).map_err(|error| {
        SubmitError::transport_failed(format!("invalid ipc message json: {error}"))
    })
}

fn build_submit_requests(target: &SubmitTarget, text: &str) -> [Value; 5] {
    let SubmitTarget::ThreadId(thread_id) = target else {
        unreachable!("desktop approvals do not use app-server submit requests");
    };

    [
        json!({
            "id": 0,
            "method": "initialize",
            "params": {
                "clientInfo": {
                    "name": "codex-island",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "capabilities": {
                    "experimentalApi": false,
                }
            }
        }),
        json!({
            "method": "initialized",
        }),
        json!({
            "id": 1,
            "method": "thread/read",
            "params": {
                "threadId": thread_id,
                "includeTurns": false,
            }
        }),
        json!({
            "id": 2,
            "method": "thread/resume",
            "params": {
                "threadId": thread_id,
            }
        }),
        json!({
            "id": 3,
            "method": "turn/start",
            "params": {
                "threadId": thread_id,
                "input": [
                    {
                        "type": "text",
                        "text": text,
                        "text_elements": [],
                    }
                ],
            }
        }),
    ]
}

fn write_json_rpc_line(writer: &mut dyn Write, value: &Value) -> Result<(), SubmitError> {
    serde_json::to_writer(&mut *writer, value).map_err(|error| {
        SubmitError::transport_failed(format!("failed to encode JSON-RPC request: {error}"))
    })?;
    writer
        .write_all(b"\n")
        .and_then(|_| writer.flush())
        .map_err(|error| {
            SubmitError::transport_failed(format!("failed to write JSON-RPC request: {error}"))
        })
}

fn spawn_stdout_reader(stdout: ChildStdout) -> mpsc::Receiver<Result<Vec<Value>, SubmitError>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = read_json_rpc_responses(stdout);
        let _ = tx.send(result);
    });
    rx
}

fn read_json_rpc_responses(stdout: ChildStdout) -> Result<Vec<Value>, SubmitError> {
    let mut responses = Vec::new();
    let mut seen_initialize = false;
    let mut seen_thread_read = false;
    let mut seen_thread_resume = false;
    let mut seen_turn_start = false;

    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|error| {
            SubmitError::transport_failed(format!("failed reading app-server stdout: {error}"))
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value = serde_json::from_str::<Value>(trimmed).map_err(|error| {
            SubmitError::transport_failed(format!(
                "invalid JSON-RPC response from app-server: {error}"
            ))
        })?;

        match response_id(&value) {
            Some(0) => seen_initialize = true,
            Some(1) => seen_thread_read = true,
            Some(2) => seen_thread_resume = true,
            Some(3) => seen_turn_start = true,
            _ => {}
        }

        let has_error = value.get("error").is_some();
        responses.push(value);

        if has_error || (seen_initialize && seen_thread_read && seen_thread_resume && seen_turn_start) {
            return Ok(responses);
        }
    }

    Err(SubmitError::transport_failed(
        "app-server exited before returning initialize/thread-read/thread-resume/turn-start responses",
    ))
}

fn response_id(value: &Value) -> Option<i64> {
    value.get("id").and_then(Value::as_i64)
}

fn find_response<'a>(responses: &'a [Value], request_id: i64) -> Option<&'a Value> {
    responses
        .iter()
        .find(|value| response_id(value) == Some(request_id))
}

fn ensure_json_rpc_success(
    responses: &[Value],
    request_id: i64,
    method: &str,
) -> Result<(), SubmitError> {
    let response = find_response(responses, request_id).ok_or_else(|| {
        SubmitError::transport_failed(format!("missing JSON-RPC response for {method}"))
    })?;

    if let Some(error) = response.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown JSON-RPC error");
        return Err(SubmitError::transport_failed(format!(
            "{method} failed with code {code}: {message}"
        )));
    }

    if response.get("result").is_none() {
        return Err(SubmitError::transport_failed(format!(
            "{method} returned no result"
        )));
    }

    Ok(())
}

fn applescript_string(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let mut escaped = String::from("\"");
    let mut first_segment = true;

    for segment in normalized.split('\n') {
        if !first_segment {
            escaped.push_str("\" & return & \"");
        }
        escaped.push_str(&segment.replace('\\', "\\\\").replace('"', "\\\""));
        first_segment = false;
    }

    escaped.push('"');
    escaped
}

fn applescript_app_name(value: &str) -> &str {
    match value {
        "VS Code" => "Visual Studio Code",
        other => other,
    }
}

fn focus_project_directory(session: &CodexSession) -> Result<(), String> {
    let cwd = session
        .cwd
        .as_deref()
        .ok_or_else(|| "Session has no project directory".to_string())?;

    let mut command = Command::new("open");
    let app_name = match session.terminal_app.as_ref() {
        Some(TerminalApp::ITerm) => "iTerm",
        _ => "Terminal",
    };
    command.args(["-a", app_name]);

    let output = command
        .arg(cwd)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        let _ = match session.terminal_app.as_ref() {
            Some(TerminalApp::ITerm) => focus_named_application("iTerm2"),
            _ => focus_named_application("Terminal"),
        };
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn focus_terminal_application(session: &CodexSession) -> Result<(), String> {
    match session.terminal_app.as_ref() {
        Some(TerminalApp::ITerm) => focus_named_application("iTerm2"),
        Some(TerminalApp::Terminal) => focus_named_application("Terminal"),
        Some(TerminalApp::Unsupported(app_name)) => focus_named_application(app_name),
        None => Err("Session host does not map to a known terminal application".to_string()),
    }
}

fn focus_codex_desktop() -> Result<(), String> {
    focus_named_application("Codex")
}

fn focus_named_application(app_name: &str) -> Result<(), String> {
    let resolved_app_name = applescript_app_name(app_name);
    let script = named_application_focus_script(resolved_app_name);
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| error.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn named_application_focus_script(app_name: &str) -> String {
    format!(
        r#"
tell application "{app_name}"
  reopen
  activate
  try
    repeat with w in windows
      try
        set miniaturized of w to false
      end try
    end repeat
  end try
end tell
"#
    )
}

#[cfg(test)]
mod tests {
    use super::{
        applescript_app_name, applescript_string, iterm_focus_script, named_application_focus_script,
        terminal_focus_script,
        SubmitTransport,
    };
    use crate::models::{
        CodexSession, SessionIngestionMode, SessionSource, SessionStatus, SubmitTarget,
        TerminalApp,
    };

    #[derive(Default)]
    struct FailingTransport;

    impl super::SubmitTransport for FailingTransport {
        fn submit(&self, _target: &SubmitTarget, _text: &str) -> Result<(), super::SubmitError> {
            unreachable!("transport should not be called for unsupported targets");
        }
    }

    #[derive(Default)]
    struct RecordingTransport {
        calls: std::cell::RefCell<Vec<(String, String)>>,
    }

    impl RecordingTransport {
        fn calls(&self) -> Vec<(String, String)> {
            self.calls.borrow().clone()
        }
    }

    impl super::SubmitTransport for RecordingTransport {
        fn submit(&self, target: &SubmitTarget, text: &str) -> Result<(), super::SubmitError> {
            match target {
                SubmitTarget::ThreadId(thread_id) => {
                    self.calls
                        .borrow_mut()
                        .push((thread_id.clone(), text.to_string()));
                    Ok(())
                }
                SubmitTarget::DesktopCommandApproval { .. } => Ok(()),
            }
        }
    }

    #[test]
    fn rejects_reply_when_session_has_no_submit_target() {
        let session = CodexSession {
            session_id: "s1".into(),
            source: SessionSource::Desktop,
            pid: 100,
            parent_pid: None,
            tty: None,
            cwd: Some("/tmp/playground".into()),
            terminal_app: None,
            title: "Desktop".into(),
            project_name: Some("playground".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: chrono::Utc::now(),
            activity_label: Some("Needs input".into()),
            last_snapshot: Some("continue?".into()),
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: None,
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(chrono::Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };

        let error = super::submit_session_reply_with_transport(&session, "yes", &FailingTransport)
            .unwrap_err()
            .to_string();

        assert!(error.contains("unsupported_session_target"));
    }

    #[test]
    fn delegates_reply_to_transport_when_submit_target_exists() {
        let session = CodexSession {
            session_id: "s1".into(),
            source: SessionSource::Cli,
            pid: 100,
            parent_pid: Some(99),
            tty: Some("/dev/ttys001".into()),
            cwd: Some("/tmp/playground".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "CLI".into(),
            project_name: Some("playground".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: chrono::Utc::now(),
            activity_label: Some("Needs input".into()),
            last_snapshot: Some("continue?".into()),
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(chrono::Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };

        let transport = RecordingTransport::default();
        super::submit_session_reply_with_transport(&session, "yes", &transport).unwrap();
        assert_eq!(transport.calls(), vec![("thread-123".into(), "yes".into())]);
    }

    #[test]
    fn returns_transport_unavailable_when_no_local_submit_channel_is_found() {
        let transport = super::LocalCodexSubmitTransport::from_probe_result(None);
        let error = transport
            .submit(&SubmitTarget::ThreadId("thread-123".into()), "yes")
            .unwrap_err()
            .to_string();

        assert!(error.contains("submit_transport_unavailable"));
    }

    #[test]
    fn desktop_sessions_do_not_require_project_directory_for_focus() {
        let session = CodexSession {
            session_id: "desktop:1".into(),
            source: SessionSource::Desktop,
            pid: 1,
            parent_pid: None,
            tty: None,
            cwd: None,
            terminal_app: None,
            title: "Desktop".into(),
            project_name: Some("codex-island".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: chrono::Utc::now(),
            activity_label: Some("Needs input".into()),
            last_snapshot: Some("Allow file access?".into()),
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: None,
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(chrono::Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };

        let result = super::focus_session(&session);
        assert!(
            !matches!(result, Err(message) if message == "Session has no project directory")
        );
    }

    #[test]
    fn vscode_sessions_do_not_require_project_directory_for_focus() {
        let session = CodexSession {
            session_id: "cli:/dev/ttys001".into(),
            source: SessionSource::Cli,
            pid: 1,
            parent_pid: None,
            tty: Some("/dev/ttys001".into()),
            cwd: None,
            terminal_app: Some(TerminalApp::Unsupported("VS Code".into())),
            title: "CLI".into(),
            project_name: Some("codex-island".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: chrono::Utc::now(),
            activity_label: Some("Needs input".into()),
            last_snapshot: Some("Allow file access?".into()),
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: None,
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(chrono::Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };

        let result = super::focus_session(&session);
        assert!(
            !matches!(result, Err(message) if message == "Session has no project directory")
        );
    }

    #[test]
    fn iterm_focus_script_restores_minimized_windows() {
        let script = iterm_focus_script("ttys001");

        assert!(script.contains("reopen"));
        assert!(script.contains("set miniaturized of w to false"));
        assert!(script.contains("set current tab to t"));
        assert!(script.contains("set frontmost to true"));
        assert!(script.contains("error \"TTY_NOT_FOUND\""));
    }

    #[test]
    fn terminal_focus_script_restores_minimized_windows() {
        let script = terminal_focus_script("ttys001");

        assert!(script.contains("reopen"));
        assert!(script.contains("set miniaturized of w to false"));
        assert!(script.contains("set frontmost of w to true"));
        assert!(script.contains("error \"TTY_NOT_FOUND\""));
    }

    #[test]
    fn named_application_focus_script_restores_minimized_windows() {
        let script = named_application_focus_script("Visual Studio Code");

        assert!(script.contains("reopen"));
        assert!(script.contains("set miniaturized of w to false"));
        assert!(script.contains("activate"));
    }

    #[test]
    fn submits_reply_over_stdio_app_server_transport() {
        let transport = super::LocalCodexSubmitTransport::from_probe_result(Some(
            super::LocalSubmitProbe {
                command: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    r#"python3 -c 'import json,sys
sys.stdout.write("{\"id\":0,\"result\":{\"userAgent\":\"Codex Desktop\",\"codexHome\":\"/Users/cong/.codex\",\"platformFamily\":\"unix\",\"platformOs\":\"macos\"}}\n")
sys.stdout.write("{\"id\":1,\"result\":{\"threadId\":\"thread-123\",\"turns\":[]}}\n")
sys.stdout.write("{\"id\":2,\"result\":{\"thread\":{\"id\":\"thread-123\",\"turns\":[]}}}\n")
sys.stdout.write("{\"id\":3,\"result\":{\"turnId\":\"turn-456\"}}\n")
sys.stdout.flush()
stdin_lines = [line.strip() for line in sys.stdin if line.strip()]
req0 = json.loads(stdin_lines[0])
req1 = json.loads(stdin_lines[2])
req2 = json.loads(stdin_lines[3])
req3 = json.loads(stdin_lines[4])
assert req0["method"] == "initialize", req0
assert req0["params"]["clientInfo"]["name"] == "codex-island", req0
assert stdin_lines[1] == "{\"method\":\"initialized\"}", stdin_lines[1]
assert req1["method"] == "thread/read", req1
assert req1["params"] == {"threadId":"thread-123","includeTurns":False}, req1
assert req2["method"] == "thread/resume", req2
assert req2["params"] == {"threadId":"thread-123"}, req2
assert req3["method"] == "turn/start", req3
assert req3["params"]["threadId"] == "thread-123", req3
assert req3["params"]["input"] == [{"type":"text","text":"yes","text_elements":[]}], req3
'"#.into(),
                ],
            },
        ));

        transport
            .submit(&SubmitTarget::ThreadId("thread-123".into()), "yes")
            .unwrap();
    }

    #[test]
    fn surfaces_transport_failed_when_stdio_submit_command_returns_rpc_error() {
        let transport = super::LocalCodexSubmitTransport::from_probe_result(Some(
            super::LocalSubmitProbe {
                command: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    r#"python3 -c 'import sys
sys.stdout.write("{\"id\":0,\"result\":{\"userAgent\":\"Codex Desktop\",\"codexHome\":\"/Users/cong/.codex\",\"platformFamily\":\"unix\",\"platformOs\":\"macos\"}}\n")
sys.stdout.write("{\"id\":1,\"error\":{\"code\":-32000,\"message\":\"missing thread\"}}\n")
sys.stdout.flush()
'"#.into(),
                ],
            },
        ));

        let error = transport
            .submit(&SubmitTarget::ThreadId("thread-123".into()), "yes")
            .unwrap_err()
            .to_string();

        assert!(error.contains("submit_transport_failed"));
        assert!(error.contains("missing thread"));
    }

    #[test]
    fn ignores_stderr_warnings_when_stdio_submit_command_succeeds() {
        let transport = super::LocalCodexSubmitTransport::from_probe_result(Some(
            super::LocalSubmitProbe {
                command: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    r#"python3 -c 'import sys
sys.stdout.write("{\"id\":0,\"result\":{\"userAgent\":\"Codex Desktop\",\"codexHome\":\"/Users/cong/.codex\",\"platformFamily\":\"unix\",\"platformOs\":\"macos\"}}\n")
sys.stdout.write("{\"id\":1,\"result\":{\"threadId\":\"thread-123\",\"turns\":[]}}\n")
sys.stdout.write("{\"id\":2,\"result\":{\"thread\":{\"id\":\"thread-123\",\"turns\":[]}}}\n")
sys.stdout.write("{\"id\":3,\"result\":{\"turnId\":\"turn-456\"}}\n")
sys.stdout.flush()
sys.stderr.write("warning: plugin sync failed\n")
sys.stderr.flush()
'"#.into(),
                ],
            },
        ));

        transport
            .submit(&SubmitTarget::ThreadId("thread-123".into()), "yes")
            .unwrap();
    }

    #[test]
    fn escapes_applescript_string_content() {
        assert_eq!(applescript_string("say \"hi\""), "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn converts_newlines_and_carriage_returns_for_applescript() {
        assert_eq!(
            applescript_string("say\nhi\r\nthere"),
            "\"say\" & return & \"hi\" & return & \"there\""
        );
    }

    #[test]
    fn maps_editor_app_names_for_applescript() {
        assert_eq!(applescript_app_name("VS Code"), "Visual Studio Code");
        assert_eq!(applescript_app_name("Cursor"), "Cursor");
    }
}
