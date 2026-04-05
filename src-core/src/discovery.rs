use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{HashMap, HashSet},
    fs::{metadata, File},
    io::{BufRead, BufReader},
};

use chrono::{Datelike, Utc};
use serde_json::Value;
use sysinfo::{ProcessesToUpdate, System};

use crate::models::{
    DiscoveryObservation, PromptAction, PromptSource, SessionSource, SubmitTarget, TerminalApp,
};

const INTERACTION_HINTS: [&str; 6] = [
    "press enter",
    "continue?",
    "approve",
    "confirm",
    "allow",
    "y/n",
];
const STALE_ROLLOUT_MAX_AGE_SECS: u64 = 300;
const ACTIVE_TASK_MAX_AGE_SECS: i64 = 30;

pub trait SessionMonitor: Send + Sync {
    fn poll(&self) -> Vec<DiscoveryObservation>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessKind {
    Cli,
    Desktop,
}

#[derive(Debug, Clone)]
struct DesktopState {
    title: String,
    project_name: Option<String>,
    activity_label: Option<String>,
    interaction_hint: Option<String>,
    prompt_actions: Vec<PromptAction>,
    prompt_source: Option<PromptSource>,
    submit_target: Option<SubmitTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingInput {
    prompt: String,
    actions: Vec<PromptAction>,
    source: PromptSource,
    submit_target: Option<SubmitTarget>,
}

#[derive(Debug, Clone)]
struct ProcessRow {
    pid: i32,
    parent_pid: Option<i32>,
    tty: Option<String>,
    executable: String,
    command: String,
}

#[derive(Default)]
pub struct CliSessionMonitor;

impl SessionMonitor for CliSessionMonitor {
    fn poll(&self) -> Vec<DiscoveryObservation> {
        let mut system = System::new_all();
        system.refresh_processes(ProcessesToUpdate::All, true);
        let process_rows = process_rows_from_ps();
        let process_index = process_rows
            .iter()
            .cloned()
            .map(|row| (row.pid, row))
            .collect::<HashMap<_, _>>();

        process_rows
            .into_iter()
            .filter_map(|process| {
                let kind = classify_process(&process.executable, &process.command)?;

                let tty = match kind {
                    ProcessKind::Cli => process.tty.clone(),
                    ProcessKind::Desktop => None,
                };
                let cwd = match kind {
                    ProcessKind::Cli => cwd_for_pid(process.pid),
                    ProcessKind::Desktop => None,
                };
                let terminal_app = match kind {
                    ProcessKind::Cli => terminal_app_for_process(&process, &process_index)
                        .or_else(|| tty.as_deref().and_then(terminal_app_for_tty)),
                    ProcessKind::Desktop => None,
                };
                let snapshot = match kind {
                    ProcessKind::Cli => tty
                        .as_ref()
                        .and_then(|value| terminal_snapshot(value, terminal_app.as_ref())),
                    ProcessKind::Desktop => None,
                };
                let project_name = cwd.as_deref().and_then(cwd_basename);

                if matches!(kind, ProcessKind::Cli) && tty.is_none() {
                    return None;
                }

                let desktop_state = match kind {
                    ProcessKind::Desktop => current_desktop_state(process.pid),
                    ProcessKind::Cli => None,
                };
                let cli_state = match kind {
                    ProcessKind::Cli => {
                        cli_state(
                            snapshot.as_deref(),
                            cwd.as_deref(),
                            process_start_unix_secs(process.pid, &system),
                        )
                    }
                    ProcessKind::Desktop => None,
                };

                if matches!(kind, ProcessKind::Desktop) && desktop_state.is_none() {
                    return None;
                }

                if matches!(kind, ProcessKind::Cli) && cli_state.is_none() {
                    return None;
                }

                Some(DiscoveryObservation {
                    pid: process.pid,
                    parent_pid: process.parent_pid,
                    tty,
                    cwd: cwd.clone(),
                    terminal_app,
                    title: process_title(
                        &process.executable,
                        desktop_state.as_ref(),
                        project_name.as_deref(),
                        &process.command,
                        kind,
                    ),
                    project_name: desktop_state
                        .as_ref()
                        .and_then(|state| state.project_name.clone())
                        .or(project_name),
                    source: match kind {
                        ProcessKind::Cli => SessionSource::Cli,
                        ProcessKind::Desktop => SessionSource::Desktop,
                    },
                    activity_label: match kind {
                        ProcessKind::Cli => cli_state
                            .as_ref()
                            .and_then(|state| state.activity_label.clone()),
                        ProcessKind::Desktop => desktop_state
                            .as_ref()
                            .and_then(|state| state.activity_label.clone())
                            .or_else(|| Some("Desktop app active".into())),
                    },
                    interaction_hint: match kind {
                        ProcessKind::Cli => cli_state
                            .as_ref()
                            .and_then(|state| state.interaction_hint.clone()),
                        ProcessKind::Desktop => desktop_state
                            .as_ref()
                            .and_then(|state| state.interaction_hint.clone()),
                    },
                    prompt_actions: match kind {
                        ProcessKind::Cli => cli_state
                            .as_ref()
                            .map(|state| state.prompt_actions.clone())
                            .unwrap_or_default(),
                        ProcessKind::Desktop => desktop_state
                            .as_ref()
                            .map(|state| state.prompt_actions.clone())
                            .unwrap_or_default(),
                    },
                    prompt_source: match kind {
                        ProcessKind::Cli => cli_state
                            .as_ref()
                            .and_then(|state| state.prompt_source.clone()),
                        ProcessKind::Desktop => desktop_state
                            .as_ref()
                            .and_then(|state| state.prompt_source.clone()),
                    },
                    submit_target: match kind {
                        ProcessKind::Cli => cli_state.and_then(|state| state.submit_target),
                        ProcessKind::Desktop => desktop_state
                            .as_ref()
                            .and_then(|state| state.submit_target.clone()),
                    },
                    seen_at_unix_ms: now_unix_ms(),
                })
            })
            .collect()
    }
}

fn process_rows_from_ps() -> Vec<ProcessRow> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,tty=,command="])
        .output()
        .ok();

    let Some(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_process_row)
        .collect()
}

fn parse_process_row(line: &str) -> Option<ProcessRow> {
    let mut parts = line.split_whitespace();
    let pid = parts.next()?.parse().ok()?;
    let parent_pid = parts.next()?.parse().ok();
    let tty = match parts.next()? {
        "??" => None,
        value => Some(format!("/dev/{value}")),
    };
    let command = parts.collect::<Vec<_>>().join(" ");
    let executable = command
        .split_whitespace()
        .next()
        .map(|part| {
            Path::new(part)
                .file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_else(|| part.to_lowercase())
        })
        .unwrap_or_default();

    Some(ProcessRow {
        pid,
        parent_pid,
        tty,
        executable,
        command: command.to_lowercase(),
    })
}

fn process_start_unix_secs(pid: i32, system: &System) -> i64 {
    system
        .process(sysinfo::Pid::from_u32(pid as u32))
        .map(|process| process.start_time() as i64)
        .unwrap_or_default()
}

fn terminal_app_for_process(
    process: &ProcessRow,
    process_index: &HashMap<i32, ProcessRow>,
) -> Option<TerminalApp> {
    let mut current_pid = process.parent_pid;

    while let Some(pid) = current_pid {
        let parent = process_index.get(&pid)?;
        let command = parent.command.as_str();
        let executable = parent.executable.as_str();

        if executable.contains("iterm")
            || command.contains("/iterm.app/")
            || command.contains("itermserver")
        {
            return Some(TerminalApp::ITerm);
        }

        if executable.contains("terminal")
            || command.contains("/terminal.app/")
        {
            return Some(TerminalApp::Terminal);
        }

        if executable == "code"
            || executable.contains("code helper")
            || command.contains("/visual studio code.app/")
        {
            return Some(TerminalApp::Unsupported("VS Code".into()));
        }

        if executable.contains("cursor") || command.contains("/cursor.app/") {
            return Some(TerminalApp::Unsupported("Cursor".into()));
        }

        current_pid = parent.parent_pid;
    }

    None
}

fn classify_process(executable: &str, command: &str) -> Option<ProcessKind> {
    if command.contains("/codex.app/contents/macos/codex")
        && !command.contains("helper")
        && !command.contains("crashpad")
    {
        return Some(ProcessKind::Desktop);
    }

    let cli_name = executable == "codex";
    let cli_command = command.starts_with("codex ")
        || command.contains("/bin/codex ")
        || command.ends_with("/bin/codex")
        || command.contains("/codex --")
        || command.contains("@openai/codex")
        || command.contains("/node_modules/.bin/codex")
        || command.contains("/codex/dist/");

    if cli_name || cli_command {
        return Some(ProcessKind::Cli);
    }

    None
}

fn process_title(
    name: &str,
    desktop_state: Option<&DesktopState>,
    project_name: Option<&str>,
    command: &str,
    kind: ProcessKind,
) -> String {
    if matches!(kind, ProcessKind::Desktop) {
        return desktop_state
            .map(|state| state.title.clone())
            .unwrap_or_else(|| "Codex desktop".into());
    }

    if let Some(project_name) = project_name {
        return project_name.to_string();
    }

    let cleaned = command.trim();
    if cleaned.is_empty() {
        name.to_string()
    } else {
        cleaned
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn cwd_for_pid(pid: i32) -> Option<String> {
    let output = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-Fn"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix('n'))
        .map(str::to_string)
}

fn current_desktop_state(pid: i32) -> Option<DesktopState> {
    let thread = current_thread_row()?;
    let rollout_attention = unresolved_attention_from_rollout(&thread.rollout_path);
    let approval_request_id = desktop_pending_command_approval_request_id(&thread.thread_id, pid);

    desktop_state_for_thread(thread, rollout_attention, approval_request_id)
}

fn desktop_state_for_thread(
    thread: ThreadRow,
    rollout_attention: Option<PendingInput>,
    approval_request_id: Option<String>,
) -> Option<DesktopState> {
    let is_recent = is_recent_unix_secs(thread.updated_at, ACTIVE_TASK_MAX_AGE_SECS);
    let attention = resolve_desktop_attention(
        &thread.thread_id,
        rollout_attention,
        approval_request_id,
    );
    let interaction_hint = attention.as_ref().map(|pending| pending.prompt.clone());
    let prompt_actions = attention
        .as_ref()
        .map(|pending| pending.actions.clone())
        .unwrap_or_default();
    let prompt_source = attention.as_ref().map(|pending| pending.source.clone());

    let submit_target = attention
        .as_ref()
        .and_then(|pending| pending.submit_target.clone())
        .or_else(|| Some(SubmitTarget::ThreadId(thread.thread_id.clone())));

    Some(DesktopState {
        title: thread.title,
        project_name: cwd_basename(&thread.cwd),
        activity_label: Some(if attention.is_some() {
            "Approval required".into()
        } else if is_recent {
            "Working".into()
        } else {
            "Idle".into()
        }),
        interaction_hint,
        prompt_actions,
        prompt_source,
        submit_target,
    })
}

fn resolve_desktop_attention(
    conversation_id: &str,
    rollout_attention: Option<PendingInput>,
    approval_request_id: Option<String>,
) -> Option<PendingInput> {
    match rollout_attention {
        Some(mut pending) if is_escalation_pending(&pending) => {
            if let Some(request_id) = approval_request_id {
                pending.submit_target = Some(SubmitTarget::DesktopCommandApproval {
                    conversation_id: conversation_id.to_string(),
                    request_id,
                });
            }
            Some(pending)
        }
        Some(pending) => Some(pending),
        None => approval_request_id.map(|request_id| PendingInput {
            prompt: "Approval required".into(),
            actions: escalation_actions(),
            source: PromptSource::Thread,
            submit_target: Some(SubmitTarget::DesktopCommandApproval {
                conversation_id: conversation_id.to_string(),
                request_id,
            }),
        }),
    }
}

fn is_escalation_pending(pending: &PendingInput) -> bool {
    pending.source == PromptSource::Thread && pending.actions == escalation_actions()
}

struct ThreadRow {
    thread_id: String,
    rollout_path: String,
    cwd: String,
    title: String,
    created_at: i64,
    updated_at: i64,
}

fn desktop_thread_query() -> &'static str {
    "SELECT id, rollout_path, cwd, title, created_at, updated_at FROM threads WHERE archived = 0 AND source = 'vscode' ORDER BY updated_at DESC LIMIT 1;"
}

fn cli_thread_query_for_cwd(cwd: &str) -> String {
    let escaped_cwd = cwd.replace('\'', "''");
    format!(
        "SELECT id, rollout_path, cwd, title, created_at, updated_at FROM threads WHERE archived = 0 AND cwd = '{escaped_cwd}' AND source = 'cli' ORDER BY updated_at DESC LIMIT 12;"
    )
}

fn current_thread_row() -> Option<ThreadRow> {
    let output = Command::new("sqlite3")
        .args([
            "-tabs",
            &format!("{}/.codex/state_5.sqlite", std::env::var("HOME").ok()?),
            desktop_thread_query(),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let row = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut parts = row.split('\t');
    Some(ThreadRow {
        thread_id: parts.next()?.to_string(),
        rollout_path: parts.next()?.to_string(),
        cwd: parts.next()?.to_string(),
        title: parts.next()?.to_string(),
        created_at: parts.next()?.parse().ok()?,
        updated_at: parts.next()?.parse().ok()?,
    })
}

fn thread_rows_for_cwd(cwd: &str) -> Vec<ThreadRow> {
    let Some(home) = std::env::var("HOME").ok() else {
        return Vec::new();
    };
    let output = Command::new("sqlite3")
        .args([
            "-tabs",
            &format!("{home}/.codex/state_5.sqlite"),
            &cli_thread_query_for_cwd(cwd),
        ])
        .output()
        .ok();

    let Some(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|row| {
            let mut parts = row.split('\t');
            Some(ThreadRow {
                thread_id: parts.next()?.to_string(),
                rollout_path: parts.next()?.to_string(),
                cwd: parts.next()?.to_string(),
                title: parts.next()?.to_string(),
                created_at: parts.next()?.parse().ok()?,
                updated_at: parts.next()?.parse().ok()?,
            })
        })
        .collect()
}

fn cwd_basename(cwd: &str) -> Option<String> {
    Path::new(cwd)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

fn unresolved_attention_from_rollout(path: &str) -> Option<PendingInput> {
    if !rollout_file_is_recent(path, SystemTime::now()) {
        return None;
    }

    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut recent_lines = std::collections::VecDeque::with_capacity(240);

    for line in reader.lines().map_while(Result::ok) {
        if recent_lines.len() == 240 {
            recent_lines.pop_front();
        }
        recent_lines.push_back(line);
    }

    unresolved_attention(recent_lines.iter().map(String::as_str))
}

fn codex_log_directory_for_today() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let now = Utc::now();
    Some(format!(
        "{home}/Library/Logs/com.openai.codex/{:04}/{:02}/{:02}",
        now.year(),
        now.month(),
        now.day()
    ))
}

fn desktop_pending_command_approval_request_id(conversation_id: &str, pid: i32) -> Option<String> {
    let log_dir = codex_log_directory_for_today()?;
    let entries = std::fs::read_dir(log_dir).ok()?;
    let mut pending = Vec::new();
    let mut resolved = HashSet::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(path_str) = path.to_str() else {
            continue;
        };
        if !is_desktop_log_for_pid(path_str, pid) {
            continue;
        }

        let Ok(file) = File::open(&path) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Some(request_id) =
                desktop_log_pending_approval_request_id(&line, conversation_id)
            {
                pending.push(request_id);
            }
            if let Some(request_id) = desktop_log_resolved_approval_request_id(&line) {
                resolved.insert(request_id);
            }
        }
    }

    pending
        .into_iter()
        .rev()
        .find(|request_id| !resolved.contains(&**request_id))
}

fn is_desktop_log_for_pid(path: &str, pid: i32) -> bool {
    path.contains("codex-desktop-")
        && path.ends_with(".log")
        && path.contains(&format!("-{pid}-"))
}

fn desktop_log_pending_approval_request_id(
    line: &str,
    conversation_id: &str,
) -> Option<String> {
    if !line.contains("[desktop-notifications] show approval")
        || !line.contains(&format!("conversationId={conversation_id}"))
        || !line.contains("kind=commandExecution")
    {
        return None;
    }

    Some(line.split("requestId=").nth(1)?.trim().to_string())
}

fn desktop_log_resolved_approval_request_id(line: &str) -> Option<String> {
    if !line.contains("Sending server response")
        || !line.contains("method=item/commandExecution/requestApproval")
    {
        return None;
    }

    let request_id = line.split("id=").nth(1)?.split_whitespace().next()?;
    Some(request_id.to_string())
}

fn rollout_file_is_recent(path: &str, now: SystemTime) -> bool {
    let Ok(modified_at) = metadata(path).and_then(|entry| entry.modified()) else {
        return false;
    };

    now.duration_since(modified_at)
        .map(|age| age.as_secs() <= STALE_ROLLOUT_MAX_AGE_SECS)
        .unwrap_or(true)
}

struct CliState {
    activity_label: Option<String>,
    interaction_hint: Option<String>,
    prompt_actions: Vec<PromptAction>,
    prompt_source: Option<PromptSource>,
    submit_target: Option<SubmitTarget>,
}

fn cli_state(snapshot: Option<&str>, cwd: Option<&str>, process_start_unix_secs: i64) -> Option<CliState> {
    let thread = cwd
        .map(thread_rows_for_cwd)
        .and_then(|rows| select_cli_thread_for_process(rows, process_start_unix_secs));
    let thread_hint = thread
        .as_ref()
        .and_then(|row| unresolved_attention_from_rollout(&row.rollout_path));
    let thread_is_recent = thread
        .as_ref()
        .map(|row| row.updated_at >= (now_unix_ms() / 1000) - 30)
        .unwrap_or(false);
    let (activity_label, interaction_hint, prompt_actions, prompt_source) =
        cli_state_from_sources(snapshot, thread_hint, thread.is_some(), thread_is_recent);

    if activity_label.is_none() && interaction_hint.is_none() {
        return None;
    }

    Some(CliState {
        activity_label,
        interaction_hint,
        prompt_actions,
        prompt_source,
        submit_target: thread.map(|row| SubmitTarget::ThreadId(row.thread_id)),
    })
}

fn select_cli_thread_for_process(rows: Vec<ThreadRow>, process_start_unix_secs: i64) -> Option<ThreadRow> {
    rows.into_iter()
        .min_by_key(|row| (row.created_at - process_start_unix_secs).abs())
}

fn cli_state_from_sources(
    snapshot: Option<&str>,
    thread_hint: Option<PendingInput>,
    thread_exists: bool,
    thread_is_recent: bool,
) -> (
    Option<String>,
    Option<String>,
    Vec<PromptAction>,
    Option<PromptSource>,
) {
    let snapshot_activity = snapshot
        .and_then(infer_activity_label)
        .filter(|label| looks_like_active_codex_status(label));
    let snapshot_prompt = snapshot.and_then(infer_terminal_prompt);
    let effective_prompt = if snapshot.is_some() {
        snapshot_prompt
            .clone()
            .or_else(|| thread_is_recent.then_some(()).and(thread_hint.clone()))
    } else {
        thread_hint.clone()
    };
    let interaction_hint = effective_prompt.as_ref().map(|pending| pending.prompt.clone());
    let prompt_actions = effective_prompt
        .as_ref()
        .map(|pending| pending.actions.clone())
        .unwrap_or_default();
    let prompt_source = effective_prompt.as_ref().map(|pending| pending.source.clone());
    let activity_label = snapshot_activity
        .or_else(|| {
            interaction_hint
                .as_ref()
                .map(|_| "Approval required".to_string())
        })
        .or_else(|| thread_is_recent.then(|| "Working".to_string()))
        .or_else(|| thread_exists.then(|| "Idle".to_string()))
        .or_else(|| Some("Idle".to_string()))
        .filter(|label| looks_like_active_codex_status(label));

    (activity_label, interaction_hint, prompt_actions, prompt_source)
}

fn looks_like_active_codex_status(label: &str) -> bool {
    let lower = label.to_lowercase();

    [
        "thinking",
        "working",
        "planning",
        "running",
        "calling tool",
        "applying",
        "updating",
        "reading",
        "writing",
        "editing",
        "searching",
        "executing",
        "compacting",
        "approval required",
        "idle",
    ]
    .iter()
    .any(|candidate| lower.contains(candidate))
}

fn unresolved_attention<'a, I>(lines: I) -> Option<PendingInput>
where
    I: IntoIterator<Item = &'a str>,
{
    let collected = lines.into_iter().collect::<Vec<_>>();

    pending_escalation_prompt(collected.iter().copied())
        .or_else(|| pending_assistant_question(collected.iter().copied()))
}

fn pending_escalation_prompt<'a, I>(lines: I) -> Option<PendingInput>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut resolved_call_ids = HashSet::new();
    let mut pending = Vec::new();

    for line in lines {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(payload) = value.get("payload") else {
            continue;
        };
        let Some(payload_type) = payload.get("type").and_then(Value::as_str) else {
            continue;
        };

        if payload_type == "function_call_output" {
            if let Some(call_id) = payload.get("call_id").and_then(Value::as_str) {
                resolved_call_ids.insert(call_id.to_string());
            }
            continue;
        }

        if payload_type != "function_call" {
            continue;
        }

        let Some(call_id) = payload.get("call_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(arguments) = payload.get("arguments").and_then(Value::as_str) else {
            continue;
        };
        let Ok(arguments_json) = serde_json::from_str::<Value>(arguments) else {
            continue;
        };
        if arguments_json
            .get("sandbox_permissions")
            .and_then(Value::as_str)
            != Some("require_escalated")
        {
            continue;
        }

        let prompt = arguments_json
            .get("justification")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "Approval required".into());
        pending.push((
            call_id.to_string(),
            PendingInput {
                prompt,
                actions: escalation_actions(),
                source: PromptSource::Thread,
                submit_target: None,
            },
        ));
    }

    pending
        .into_iter()
        .rev()
        .find(|(call_id, _)| !resolved_call_ids.contains(call_id))
        .map(|(_, prompt)| prompt)
}

fn pending_assistant_question<'a, I>(lines: I) -> Option<PendingInput>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut latest_user_message_at = String::new();
    let mut latest_assistant_question: Option<(String, PendingInput)> = None;

    for line in lines {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(timestamp) = value
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let Some(payload) = value.get("payload") else {
            continue;
        };

        if value.get("type").and_then(Value::as_str) == Some("event_msg")
            && payload.get("type").and_then(Value::as_str) == Some("user_message")
        {
            latest_user_message_at = timestamp;
            continue;
        }

        if value.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        if payload.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        if payload.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }

        let text = payload
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");

        if let Some(question) = extract_attention_question(&text) {
            latest_assistant_question = Some((timestamp, question));
        }
    }

    latest_assistant_question.and_then(|(question_at, question)| {
        if latest_user_message_at.is_empty() || question_at > latest_user_message_at {
            Some(question)
        } else {
            None
        }
    })
}

fn extract_attention_question(text: &str) -> Option<PendingInput> {
    let lines = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let question_index = lines.iter().rposition(|line| {
        line.ends_with('?')
            || line.ends_with('？')
            || line.contains("Do you want to")
            || line.contains("现在要")
            || line.contains("要继续")
    })?;

    Some(PendingInput {
        prompt: lines[question_index].to_string(),
        actions: numbered_actions(&lines[question_index + 1..]),
        source: PromptSource::Thread,
        submit_target: None,
    })
}

fn escalation_actions() -> Vec<PromptAction> {
    vec![
        PromptAction {
            id: "1".into(),
            label: "是".into(),
            reply: "1".into(),
        },
        PromptAction {
            id: "2".into(),
            label: "是，并对以后类似命令开放".into(),
            reply: "2".into(),
        },
        PromptAction {
            id: "3".into(),
            label: "否".into(),
            reply: "3".into(),
        },
    ]
}

fn numbered_actions(lines: &[&str]) -> Vec<PromptAction> {
    lines.iter()
        .filter_map(|line| {
            let (number, label) = line.split_once('.')?;
            let id = number.trim();
            if id.is_empty() || !id.chars().all(|character| character.is_ascii_digit()) {
                return None;
            }

            let label = label.trim();
            (!label.is_empty()).then(|| PromptAction {
                id: id.to_string(),
                label: label.to_string(),
                reply: id.to_string(),
            })
        })
        .collect()
}

fn tty_for_pid(pid: i32, system: &System) -> Option<String> {
    let mut current = Some(sysinfo::Pid::from_u32(pid as u32));

    while let Some(current_pid) = current {
        if let Some(tty) = tty_for_single_pid(current_pid.as_u32() as i32) {
            return Some(tty);
        }

        current = system.process(current_pid)?.parent();
    }

    None
}

fn tty_for_single_pid(pid: i32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()?;

    let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if tty.is_empty() || tty == "??" {
        None
    } else {
        Some(format!("/dev/{tty}"))
    }
}

fn terminal_app_for_pid(pid: i32, system: &System) -> Option<TerminalApp> {
    let mut current = system.process(sysinfo::Pid::from_u32(pid as u32))?.parent();

    while let Some(parent_pid) = current {
        let process = system.process(parent_pid)?;
        let name = process.name().to_string_lossy();
        let lower_name = name.to_lowercase();
        let lower_command = process
            .cmd()
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();

        if name.contains("Terminal")
            || lower_command.contains("/terminal.app/")
        {
            return Some(TerminalApp::Terminal);
        }
        if name.contains("iTerm")
            || lower_name.contains("itermserver")
            || lower_command.contains("/iterm.app/")
            || lower_command.contains("itermserver")
        {
            return Some(TerminalApp::ITerm);
        }
        if let Some(editor_label) = editor_terminal_label(&lower_name) {
            return Some(TerminalApp::Unsupported(editor_label.to_string()));
        }
        if is_supported_terminal_alias(&lower_name) {
            return Some(TerminalApp::Unsupported(name.to_string()));
        }

        current = process.parent();
    }

    None
}

fn editor_terminal_label(name: &str) -> Option<&'static str> {
    if name == "code"
        || name.contains("code helper")
        || name.contains("visual studio code")
        || name.contains("vscodium")
    {
        return Some("VS Code");
    }

    if name.contains("cursor") {
        return Some("Cursor");
    }

    None
}

fn is_supported_terminal_alias(name: &str) -> bool {
    [
        "warp",
        "ghostty",
        "wezterm",
        "alacritty",
        "kitty",
        "tabby",
        "hyper",
        "rio",
    ]
    .iter()
    .any(|candidate| name.contains(candidate))
}

fn terminal_snapshot(tty: &str, terminal_app: Option<&TerminalApp>) -> Option<String> {
    let short_tty = tty.trim_start_matches("/dev/");
    let full_tty = format!("/dev/{short_tty}");
    let script = match terminal_app {
        Some(TerminalApp::Terminal) => format!(
            r#"
tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      try
        if tty of t is "{short_tty}" or tty of t is "{full_tty}" then
          return contents of t
        end if
      end try
    end repeat
  end repeat
end tell
"#
        ),
        Some(TerminalApp::ITerm) => format!(
            r#"
tell application "iTerm2"
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        try
          if tty of s is "{short_tty}" or tty of s is "{full_tty}" then
            return contents of s
          end if
        end try
      end repeat
    end repeat
  end repeat
end tell
"#
        ),
        _ => return None,
    };

    run_osascript(&script)
}

fn terminal_app_for_tty(tty: &str) -> Option<TerminalApp> {
    let short_tty = tty.trim_start_matches("/dev/");
    let full_tty = format!("/dev/{short_tty}");

    let iterm_script = format!(
        r#"
tell application "iTerm2"
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        try
          if tty of s is "{short_tty}" or tty of s is "{full_tty}" then
            return "iterm"
          end if
        end try
      end repeat
    end repeat
  end repeat
end tell
"#
    );

    if run_osascript(&iterm_script).as_deref() == Some("iterm") {
        return Some(TerminalApp::ITerm);
    }

    let terminal_script = format!(
        r#"
tell application "Terminal"
  repeat with w in windows
    repeat with t in tabs of w
      try
        if tty of t is "{short_tty}" or tty of t is "{full_tty}" then
          return "terminal"
        end if
      end try
    end repeat
  end repeat
end tell
"#
    );

    if run_osascript(&terminal_script).as_deref() == Some("terminal") {
        return Some(TerminalApp::Terminal);
    }

    None
}

fn run_osascript(script: &str) -> Option<String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn infer_terminal_prompt(snapshot: &str) -> Option<PendingInput> {
    let recent_lines = snapshot
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let start = recent_lines.len().saturating_sub(20);
    let recent_lines = &recent_lines[start..];

    let enter_index = recent_lines.iter().rposition(|line| {
        let lower = line.to_lowercase();
        lower.contains("press enter to continue")
    });

    if let Some(enter_index) = enter_index {
        let actions = numbered_actions(&recent_lines[..enter_index]);
        if !actions.is_empty() {
            return Some(PendingInput {
                prompt: recent_lines[enter_index].to_string(),
                actions,
                source: PromptSource::Terminal,
                submit_target: None,
            });
        }
    }

    infer_interaction_hint(snapshot)
        .map(|prompt| PendingInput {
            prompt,
            actions: Vec::new(),
            source: PromptSource::Terminal,
            submit_target: None,
        })
        .or_else(|| {
            extract_attention_question(&recent_lines.join("\n")).map(|pending| PendingInput {
                prompt: pending.prompt,
                actions: Vec::new(),
                source: PromptSource::Terminal,
                submit_target: None,
            })
        })
}

fn infer_interaction_hint(snapshot: &str) -> Option<String> {
    let recent_lines = snapshot
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let start = recent_lines.len().saturating_sub(8);
    let recent_lines = &recent_lines[start..];

    let prompt_index = recent_lines.iter().rposition(|line| {
        let lower = line.to_lowercase();
        INTERACTION_HINTS.iter().any(|hint| lower.contains(hint))
    })?;

    if recent_lines[prompt_index + 1..]
        .iter()
        .any(|line| !looks_like_shell_prompt(line))
    {
        return None;
    }

    Some(recent_lines[prompt_index].to_string())
}

fn looks_like_shell_prompt(line: &str) -> bool {
    line.ends_with('$') || line.ends_with('%') || line.ends_with('#')
}

fn infer_activity_label(snapshot: &str) -> Option<String> {
    snapshot
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && line.len() <= 160
                && !line.ends_with('$')
                && !line.ends_with('%')
                && !line.ends_with('#')
        })
        .map(str::to_string)
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn is_recent_unix_secs(value: i64, max_age_secs: i64) -> bool {
    value >= (now_unix_ms() / 1000) - max_age_secs
}

#[cfg(test)]
mod tests {
    use super::{
        classify_process, cli_state_from_sources, cli_thread_query_for_cwd, desktop_state_for_thread,
        desktop_thread_query, editor_terminal_label, extract_attention_question, infer_activity_label,
        infer_interaction_hint, infer_terminal_prompt, is_desktop_log_for_pid,
        is_supported_terminal_alias, looks_like_active_codex_status, now_unix_ms, pending_assistant_question,
        pending_escalation_prompt, resolve_desktop_attention, tty_for_single_pid,
        unresolved_attention, PendingInput, ProcessKind, PromptSource, ThreadRow,
    };
    use crate::models::SubmitTarget;

    #[test]
    fn extracts_last_prompt_line_when_confirmation_is_present() {
        let snapshot = "Working...\nContinue? [Y/n]\n";
        assert_eq!(
            infer_interaction_hint(snapshot).as_deref(),
            Some("Continue? [Y/n]")
        );
    }

    #[test]
    fn extracts_numbered_terminal_actions_before_press_enter_prompt() {
        let snapshot = "1. Update now\n2. Skip\n3. Skip until next version\nPress enter to continue\n";
        let pending = infer_terminal_prompt(snapshot).expect("terminal prompt");
        assert_eq!(pending.prompt, "Press enter to continue");
        assert_eq!(pending.actions.len(), 3);
        assert_eq!(pending.actions[1].label, "Skip");
        assert_eq!(pending.source, PromptSource::Terminal);
    }

    #[test]
    fn extracts_terminal_question_before_idle_prompt_banner() {
        let snapshot = "\
• 你好，有什么需要我处理的？\n\
\n\
› Use /skills to list available skills\n\
\n\
  gpt-5.4 medium fast · 98% left · ~/Desktop/AI相关/playground\n";
        let pending = infer_terminal_prompt(snapshot).expect("terminal question");
        assert_eq!(pending.prompt, "• 你好，有什么需要我处理的？");
        assert_eq!(pending.source, PromptSource::Terminal);
    }

    #[test]
    fn ignores_non_interactive_output() {
        let snapshot = "Generating patch\nApplying changes\n";
        assert!(infer_interaction_hint(snapshot).is_none());
    }

    #[test]
    fn ignores_stale_confirmation_prompt_when_recent_output_is_progress() {
        let snapshot =
            "Working...\nContinue? [Y/n]\nCompressing background information\nUpdating plan\n";
        assert!(infer_interaction_hint(snapshot).is_none());
    }

    #[test]
    fn ignores_idle_cli_banner_lines_as_active_status() {
        assert!(!looks_like_active_codex_status("OpenAI Codex (v0.117.0)"));
        assert!(!looks_like_active_codex_status("Tip: New Try the Codex App"));
        assert!(!looks_like_active_codex_status("Explain this codebase"));
    }

    #[test]
    fn extracts_last_activity_line() {
        let snapshot = "Thinking...\nApplying patch to src/app.tsx\n";
        assert_eq!(
            infer_activity_label(snapshot).as_deref(),
            Some("Applying patch to src/app.tsx")
        );
    }

    #[test]
    fn preserves_snapshot_activity_when_no_attention_is_present() {
        let (activity_label, interaction_hint, prompt_actions, prompt_source) =
            cli_state_from_sources(Some("Working...\n"), None, false, false);
        assert_eq!(activity_label.as_deref(), Some("Working..."));
        assert!(interaction_hint.is_none());
        assert!(prompt_actions.is_empty());
        assert!(prompt_source.is_none());
    }

    #[test]
    fn falls_back_to_thread_attention_when_terminal_snapshot_is_unavailable() {
        let (activity_label, interaction_hint, prompt_actions, prompt_source) = cli_state_from_sources(
            None,
            Some(PendingInput {
                prompt: "Do you want to allow me to inspect open files?".to_string(),
                actions: vec![],
                source: PromptSource::Thread,
                submit_target: None,
            }),
            true,
            false,
        );
        assert_eq!(activity_label.as_deref(), Some("Approval required"));
        assert_eq!(
            interaction_hint.as_deref(),
            Some("Do you want to allow me to inspect open files?")
        );
        assert!(prompt_actions.is_empty());
        assert_eq!(prompt_source, Some(PromptSource::Thread));
    }

    #[test]
    fn falls_back_to_recent_thread_attention_when_terminal_snapshot_has_no_prompt() {
        let (activity_label, interaction_hint, prompt_actions, prompt_source) = cli_state_from_sources(
            Some("✨ Update available!\n% "),
            Some(PendingInput {
                prompt: "Do you want to allow me to inspect open files?".to_string(),
                actions: vec![super::PromptAction {
                    id: "1".into(),
                    label: "Yes".into(),
                    reply: "1".into(),
                }],
                source: PromptSource::Thread,
                submit_target: None,
            }),
            true,
            true,
        );
        assert_eq!(activity_label.as_deref(), Some("Approval required"));
        assert_eq!(
            interaction_hint.as_deref(),
            Some("Do you want to allow me to inspect open files?")
        );
        assert_eq!(prompt_actions.len(), 1);
        assert_eq!(prompt_source, Some(PromptSource::Thread));
    }

    #[test]
    fn ignores_stale_thread_attention_when_terminal_snapshot_has_no_prompt() {
        let (activity_label, interaction_hint, prompt_actions, prompt_source) = cli_state_from_sources(
            Some("✨ Update available!\n% "),
            Some(PendingInput {
                prompt: "Do you want to allow me to inspect open files?".to_string(),
                actions: vec![super::PromptAction {
                    id: "1".into(),
                    label: "Yes".into(),
                    reply: "1".into(),
                }],
                source: PromptSource::Thread,
                submit_target: None,
            }),
            true,
            false,
        );
        assert_eq!(activity_label.as_deref(), Some("Idle"));
        assert!(interaction_hint.is_none());
        assert!(prompt_actions.is_empty());
        assert!(prompt_source.is_none());
    }

    #[test]
    fn falls_back_to_working_when_recent_thread_is_active_without_snapshot_text() {
        let (activity_label, interaction_hint, prompt_actions, prompt_source) =
            cli_state_from_sources(None, None, true, true);
        assert_eq!(activity_label.as_deref(), Some("Working"));
        assert!(interaction_hint.is_none());
        assert!(prompt_actions.is_empty());
        assert!(prompt_source.is_none());
    }

    #[test]
    fn keeps_idle_thread_visible_when_terminal_snapshot_is_unavailable() {
        let (activity_label, interaction_hint, prompt_actions, prompt_source) =
            cli_state_from_sources(None, None, true, false);
        assert_eq!(activity_label.as_deref(), Some("Idle"));
        assert!(interaction_hint.is_none());
        assert!(prompt_actions.is_empty());
        assert!(prompt_source.is_none());
    }

    #[test]
    fn keeps_live_cli_process_visible_without_snapshot_or_recent_thread() {
        let (activity_label, interaction_hint, prompt_actions, prompt_source) =
            cli_state_from_sources(None, None, false, false);
        assert_eq!(activity_label.as_deref(), Some("Idle"));
        assert!(interaction_hint.is_none());
        assert!(prompt_actions.is_empty());
        assert!(prompt_source.is_none());
    }

    #[test]
    fn accepts_real_cli_processes() {
        assert!(matches!(
            classify_process("codex", "/usr/local/bin/codex --model gpt-5"),
            Some(ProcessKind::Cli)
        ));
    }

    #[test]
    fn accepts_node_wrapped_cli_processes() {
        assert!(matches!(
            classify_process(
                "node",
                "node /Users/cong/.npm/_npx/123/node_modules/@openai/codex/dist/cli.js"
            ),
            Some(ProcessKind::Cli)
        ));
    }

    #[test]
    fn rejects_helper_and_crashpad_processes() {
        assert!(classify_process(
            "codex helper",
            "/applications/codex.app/contents/frameworks/codex helper.app/contents/macos/codex helper --type=utility"
        )
        .is_none());
        assert!(classify_process(
            "chrome_crashpad_handler",
            "/applications/codex.app/contents/frameworks/electron framework.framework/helpers/chrome_crashpad_handler --monitor-self"
        )
        .is_none());
    }

    #[test]
    fn detects_desktop_app_only_for_live_app_binary() {
        assert!(matches!(
            classify_process("codex", "/applications/codex.app/contents/macos/codex"),
            Some(ProcessKind::Desktop)
        ));
    }

    #[test]
    fn recognizes_other_terminal_hosts() {
        assert!(is_supported_terminal_alias("ghostty"));
        assert!(is_supported_terminal_alias("wezterm"));
        assert!(!is_supported_terminal_alias("finder"));
    }

    #[test]
    fn recognizes_editor_integrated_terminals() {
        assert_eq!(
            editor_terminal_label("code helper (plugin)"),
            Some("VS Code")
        );
        assert_eq!(editor_terminal_label("visual studio code"), Some("VS Code"));
        assert_eq!(editor_terminal_label("cursor helper"), Some("Cursor"));
    }

    #[test]
    fn rejects_unknown_tty_value_for_single_pid_lookup() {
        assert!(tty_for_single_pid(-1).is_none());
    }

    #[test]
    fn detects_unresolved_escalation_prompt() {
        let lines = [
            r#"{"payload":{"type":"function_call","call_id":"call_1","arguments":"{\"cmd\":\"lsof -p 1\",\"sandbox_permissions\":\"require_escalated\",\"justification\":\"Do you want to allow me to inspect open files?\"}"}}"#,
        ];

        assert_eq!(
            pending_escalation_prompt(lines).map(|pending| pending.prompt),
            Some("Do you want to allow me to inspect open files?".to_string())
        );
    }

    #[test]
    fn ignores_resolved_escalation_prompt() {
        let lines = [
            r#"{"payload":{"type":"function_call","call_id":"call_1","arguments":"{\"cmd\":\"lsof -p 1\",\"sandbox_permissions\":\"require_escalated\",\"justification\":\"Do you want to allow me to inspect open files?\"}"}}"#,
            r#"{"payload":{"type":"function_call_output","call_id":"call_1","output":"ok"}}"#,
        ];

        assert!(pending_escalation_prompt(lines).is_none());
    }

    #[test]
    fn extracts_assistant_question_line() {
        assert_eq!(
            extract_attention_question("第一行\n现在要继续扫描 Codex desktop 的内部状态源吗？")
                .map(|pending| pending.prompt),
            Some("现在要继续扫描 Codex desktop 的内部状态源吗？".to_string())
        );
    }

    #[test]
    fn treats_generic_greeting_questions_as_attention() {
        assert_eq!(
            extract_attention_question("你好，有什么需要我处理的？")
                .map(|pending| pending.prompt),
            Some("你好，有什么需要我处理的？".to_string())
        );
        assert_eq!(
            extract_attention_question("How can I help?").map(|pending| pending.prompt),
            Some("How can I help?".to_string())
        );
    }

    #[test]
    fn detects_unanswered_assistant_question() {
        let lines = [
            r#"{"timestamp":"2026-04-02T05:00:00Z","type":"event_msg","payload":{"type":"user_message","message":"hello"}}"#,
            r#"{"timestamp":"2026-04-02T05:00:10Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"现在要继续扫描 Codex desktop 的内部状态源吗？"}]}}"#,
        ];

        assert_eq!(
            pending_assistant_question(lines).map(|pending| pending.prompt),
            Some("现在要继续扫描 Codex desktop 的内部状态源吗？".to_string())
        );
    }

    #[test]
    fn ignores_question_after_user_replied() {
        let lines = [
            r#"{"timestamp":"2026-04-02T05:00:00Z","type":"event_msg","payload":{"type":"user_message","message":"hello"}}"#,
            r#"{"timestamp":"2026-04-02T05:00:10Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"现在要继续扫描 Codex desktop 的内部状态源吗？"}]}}"#,
            r#"{"timestamp":"2026-04-02T05:00:20Z","type":"event_msg","payload":{"type":"user_message","message":"继续"}}"#,
        ];

        assert!(pending_assistant_question(lines).is_none());
    }

    #[test]
    fn builds_quick_actions_for_escalation_prompts() {
        let lines = [
            r#"{"payload":{"type":"function_call","call_id":"call_1","arguments":"{\"cmd\":\"ps -p 1\",\"sandbox_permissions\":\"require_escalated\",\"justification\":\"是否运行此命令？\"}"}}"#,
        ];

        let pending = pending_escalation_prompt(lines).expect("pending prompt");
        assert_eq!(pending.prompt, "是否运行此命令？");
        assert_eq!(pending.actions.len(), 3);
        assert_eq!(pending.actions[0].reply, "1");
        assert_eq!(pending.actions[1].reply, "2");
        assert_eq!(pending.actions[2].reply, "3");
    }

    #[test]
    fn desktop_threads_without_attention_stay_visible_while_recent() {
        let thread = ThreadRow {
            thread_id: "thread-1".into(),
            rollout_path: "/tmp/demo.rollout".into(),
            cwd: "/tmp/demo".into(),
            title: "Codex".into(),
            created_at: now_unix_ms() / 1000,
            updated_at: now_unix_ms() / 1000,
        };

        let state = desktop_state_for_thread(
            thread,
            None,
            None,
        )
        .expect("desktop state");

        assert_eq!(state.activity_label.as_deref(), Some("Working"));
        assert!(state.interaction_hint.is_none());
        assert!(state.prompt_actions.is_empty());
    }

    #[test]
    fn desktop_threads_without_attention_fall_back_to_idle_when_stale() {
        let thread = ThreadRow {
            thread_id: "thread-1".into(),
            rollout_path: "/tmp/demo.rollout".into(),
            cwd: "/tmp/demo".into(),
            title: "Codex".into(),
            created_at: 1,
            updated_at: 1,
        };

        let state = desktop_state_for_thread(thread, None, None).expect("desktop state");

        assert_eq!(state.activity_label.as_deref(), Some("Idle"));
        assert!(state.interaction_hint.is_none());
        assert!(state.prompt_actions.is_empty());
    }

    #[test]
    fn desktop_escalation_prompt_is_kept_for_reminder_even_without_live_request_id() {
        let attention = resolve_desktop_attention(
            "thread-1",
            Some(PendingInput {
                prompt: "Do you want to allow this command?".into(),
                actions: super::escalation_actions(),
                source: PromptSource::Thread,
                submit_target: None,
            }),
            None,
        )
        .expect("attention");

        assert_eq!(attention.prompt, "Do you want to allow this command?");
        assert!(attention.submit_target.is_none());
    }

    #[test]
    fn desktop_unresolved_escalation_prompt_uses_live_request_target() {
        let attention = resolve_desktop_attention(
            "thread-1",
            Some(PendingInput {
                prompt: "Do you want to allow this command?".into(),
                actions: super::escalation_actions(),
                source: PromptSource::Thread,
                submit_target: None,
            }),
            Some("39".into()),
        )
        .expect("attention");

        assert_eq!(
            attention.submit_target,
            Some(SubmitTarget::DesktopCommandApproval {
                conversation_id: "thread-1".into(),
                request_id: "39".into(),
            })
        );
    }

    #[test]
    fn desktop_non_escalation_question_survives_without_live_request_id() {
        let attention = resolve_desktop_attention(
            "thread-1",
            Some(PendingInput {
                prompt: "现在要继续扫描 Codex desktop 的内部状态源吗？".into(),
                actions: vec![],
                source: PromptSource::Thread,
                submit_target: None,
            }),
            None,
        )
        .expect("attention");

        assert_eq!(attention.prompt, "现在要继续扫描 Codex desktop 的内部状态源吗？");
        assert!(attention.submit_target.is_none());
    }

    #[test]
    fn desktop_thread_query_ignores_subagent_and_cli_rows() {
        assert!(desktop_thread_query().contains("source = 'vscode'"));
    }

    #[test]
    fn desktop_thread_query_selects_thread_id() {
        assert!(desktop_thread_query().contains("SELECT id, rollout_path, cwd, title, created_at, updated_at"));
    }

    #[test]
    fn desktop_log_filter_only_matches_current_pid() {
        assert!(is_desktop_log_for_pid(
            "/Users/cong/Library/Logs/com.openai.codex/2026/04/03/codex-desktop-abc-4100-t0-i1-072330-0.log",
            4100
        ));
        assert!(!is_desktop_log_for_pid(
            "/Users/cong/Library/Logs/com.openai.codex/2026/04/03/codex-desktop-abc-85842-t0-i1-052802-0.log",
            4100
        ));
    }

    #[test]
    fn cli_thread_query_for_cwd_ignores_non_cli_rows() {
        let query = cli_thread_query_for_cwd("/tmp/demo");
        assert!(query.contains("source = 'cli'"));
        assert!(query.contains("cwd = '/tmp/demo'"));
    }

    #[test]
    fn cli_thread_query_for_cwd_selects_thread_id() {
        let query = cli_thread_query_for_cwd("/tmp/demo");
        assert!(query.contains("SELECT id, rollout_path, cwd, title, created_at, updated_at"));
    }

    #[test]
    fn prefers_escalation_prompt_over_plain_question() {
        let lines = [
            r#"{"timestamp":"2026-04-02T05:00:00Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"现在要继续扫描 Codex desktop 的内部状态源吗？"}]}}"#,
            r#"{"timestamp":"2026-04-02T05:00:02Z","type":"response_item","payload":{"type":"function_call","call_id":"call_1","arguments":"{\"cmd\":\"lsof -p 1\",\"sandbox_permissions\":\"require_escalated\",\"justification\":\"Do you want to allow me to inspect open files?\"}"}}"#,
        ];

        assert_eq!(
            unresolved_attention(lines).map(|pending| pending.prompt),
            Some("Do you want to allow me to inspect open files?".to_string())
        );
    }
}
