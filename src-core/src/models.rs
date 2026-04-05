use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionSource {
    Cli,
    Desktop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalApp {
    Terminal,
    ITerm,
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Discovering,
    Running,
    Idle,
    WaitingInput,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionIngestionMode {
    Hooks,
    Fallback,
}

impl SessionIngestionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hooks => "hooks",
            Self::Fallback => "fallback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptSource {
    Thread,
    Terminal,
}

impl SessionStatus {
    pub fn sort_priority(&self) -> u8 {
        match self {
            Self::WaitingInput => 5,
            Self::Running => 4,
            Self::Discovering => 3,
            Self::Failed => 2,
            Self::Idle => 1,
            Self::Completed => 0,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discovering => "discovering",
            Self::Running => "running",
            Self::Idle => "idle",
            Self::WaitingInput => "waiting_input",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveryObservation {
    pub pid: i32,
    pub parent_pid: Option<i32>,
    pub tty: Option<String>,
    pub cwd: Option<String>,
    pub terminal_app: Option<TerminalApp>,
    pub title: String,
    pub project_name: Option<String>,
    pub source: SessionSource,
    pub activity_label: Option<String>,
    pub interaction_hint: Option<String>,
    pub prompt_actions: Vec<PromptAction>,
    pub prompt_source: Option<PromptSource>,
    pub submit_target: Option<SubmitTarget>,
    pub seen_at_unix_ms: i64,
}

impl DiscoveryObservation {
    pub fn session_id(&self) -> String {
        match self.source {
            SessionSource::Cli => match &self.tty {
                Some(tty) => format!("cli:{tty}"),
                None => format!("cli:{}", self.pid),
            },
            SessionSource::Desktop => {
                if let Some(SubmitTarget::ThreadId(thread_id)) = &self.submit_target {
                    return format!("thread:{thread_id}");
                }
                format!("desktop:{}", self.pid)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEvent {
    pub session_id: String,
    pub thread_id: Option<String>,
    pub source: SessionSource,
    pub pid: Option<i32>,
    pub cwd: Option<String>,
    pub tty: Option<String>,
    pub terminal_app: Option<TerminalApp>,
    pub title: Option<String>,
    pub project_name: Option<String>,
    pub activity_label: Option<String>,
    pub prompt_text: Option<String>,
    pub user_prompt: Option<String>,
    pub prompt_actions: Vec<PromptAction>,
    pub prompt_source: Option<PromptSource>,
    pub submit_target: Option<SubmitTarget>,
    pub status: SessionStatus,
    pub transcript_path: Option<String>,
    pub happened_at_unix_ms: i64,
}

impl SessionEvent {
    pub fn session_key(&self) -> String {
        if matches!(self.source, SessionSource::Cli) {
            if let Some(tty) = self.tty.as_deref() {
                return format!("cli:{tty}");
            }
        }

        match &self.thread_id {
            Some(thread_id) => format!("thread:{thread_id}"),
            None => format!("hook:{}", self.session_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodexSession {
    pub session_id: String,
    pub source: SessionSource,
    pub pid: i32,
    pub parent_pid: Option<i32>,
    pub tty: Option<String>,
    pub cwd: Option<String>,
    pub terminal_app: Option<TerminalApp>,
    pub title: String,
    pub project_name: Option<String>,
    pub status: SessionStatus,
    pub ingestion_mode: SessionIngestionMode,
    pub needs_attention: bool,
    pub last_activity_at: DateTime<Utc>,
    pub activity_label: Option<String>,
    pub last_snapshot: Option<String>,
    pub prompt_actions: Vec<PromptAction>,
    pub prompt_source: Option<PromptSource>,
    pub submit_target: Option<SubmitTarget>,
    pub notification_sent_at: Option<DateTime<Utc>>,
    pub last_event_at: Option<DateTime<Utc>>,
    pub last_observation_at: Option<DateTime<Utc>>,
    pub transcript_path: Option<String>,
    pub latest_user_prompt: Option<String>,
    pub status_history: Vec<String>,
    pub conversation_history: Vec<String>,
}

impl CodexSession {
    pub fn from_observation(observation: DiscoveryObservation, status: SessionStatus) -> Self {
        let needs_attention = matches!(status, SessionStatus::WaitingInput);
        let status_entry = observation
            .activity_label
            .clone()
            .unwrap_or_else(|| status_label_for_history(&status).to_string());

        Self {
            session_id: observation.session_id(),
            source: observation.source,
            pid: observation.pid,
            parent_pid: observation.parent_pid,
            tty: observation.tty,
            cwd: observation.cwd,
            terminal_app: observation.terminal_app,
            title: observation.title,
            project_name: observation.project_name,
            status,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention,
            last_activity_at: Utc
                .timestamp_millis_opt(observation.seen_at_unix_ms)
                .single()
                .unwrap_or_else(Utc::now),
            activity_label: observation.activity_label,
            last_snapshot: observation.interaction_hint,
            prompt_actions: observation.prompt_actions,
            prompt_source: observation.prompt_source,
            submit_target: observation.submit_target,
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Utc.timestamp_millis_opt(observation.seen_at_unix_ms).single(),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec![status_entry],
            conversation_history: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptAction {
    pub id: String,
    pub label: String,
    pub reply: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionViewModel {
    pub session_id: String,
    pub title: String,
    pub project_name: String,
    pub status: SessionStatus,
    pub needs_attention: bool,
    pub can_reply: bool,
    pub subtitle: String,
    pub prompt_text: Option<String>,
    pub action_options: Vec<PromptAction>,
    pub prompt_source: Option<PromptSource>,
    pub latest_user_prompt: Option<String>,
    pub status_history: Vec<String>,
    pub conversation_history: Vec<String>,
    pub ingestion_mode: SessionIngestionMode,
    pub terminal_label: String,
    pub relative_last_activity: String,
    pub last_activity_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum SubmitTarget {
    ThreadId(String),
    DesktopCommandApproval {
        conversation_id: String,
        request_id: String,
    },
}

impl From<&CodexSession> for SessionViewModel {
    fn from(session: &CodexSession) -> Self {
        let subtitle = session
            .activity_label
            .clone()
            .or_else(|| session.last_snapshot.clone())
            .unwrap_or_else(|| match session.status {
                SessionStatus::WaitingInput => "Waiting for your input".into(),
                SessionStatus::Running => "Working".into(),
                SessionStatus::Idle => "Idle".into(),
                SessionStatus::Discovering => "Connecting".into(),
                SessionStatus::Completed => "Completed".into(),
                SessionStatus::Failed => "Failed".into(),
            });
        let project_name = session
            .project_name
            .clone()
            .unwrap_or_else(|| match session.source {
                SessionSource::Cli => "Unknown project".into(),
                SessionSource::Desktop => "Codex app".into(),
            });

        let terminal_label = match &session.terminal_app {
            Some(TerminalApp::Terminal) => "Terminal".into(),
            Some(TerminalApp::ITerm) => "iTerm2".into(),
            Some(TerminalApp::Unsupported(name)) => name.clone(),
            None => match session.source {
                SessionSource::Desktop => "Codex app".into(),
                SessionSource::Cli => session
                    .tty
                    .as_deref()
                    .map(|tty| format!("TTY {}", tty.trim_start_matches("/dev/")))
                    .unwrap_or_else(|| "Terminal session".into()),
            },
        };

        Self {
            session_id: session.session_id.clone(),
            title: session.title.clone(),
            project_name,
            status: session.status.clone(),
            needs_attention: session.needs_attention,
            can_reply: matches!(session.status, SessionStatus::WaitingInput)
                && session.submit_target.is_some(),
            subtitle,
            prompt_text: session.last_snapshot.clone(),
            action_options: session.prompt_actions.clone(),
            prompt_source: session.prompt_source.clone(),
            latest_user_prompt: session.latest_user_prompt.clone(),
            status_history: session.status_history.clone(),
            conversation_history: session.conversation_history.clone(),
            ingestion_mode: session.ingestion_mode.clone(),
            terminal_label,
            relative_last_activity: relative_time_label(session.last_activity_at),
            last_activity_unix_ms: session.last_activity_at.timestamp_millis(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub total: usize,
    pub running: usize,
    pub idle: usize,
    pub waiting: usize,
    pub discovering: usize,
    pub failed: usize,
    pub completed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionsPayload {
    pub sessions: Vec<SessionViewModel>,
    pub summary: SessionSummary,
}

impl SessionsPayload {
    pub fn empty() -> Self {
        Self {
            sessions: Vec::new(),
            summary: SessionSummary {
                total: 0,
                running: 0,
                idle: 0,
                waiting: 0,
                discovering: 0,
                failed: 0,
                completed: 0,
            },
        }
    }
}

fn relative_time_label(instant: DateTime<Utc>) -> String {
    let delta = (Utc::now() - instant).num_seconds().max(0);

    if delta < 5 {
        "just now".into()
    } else if delta < 60 {
        format!("{delta}s ago")
    } else {
        format!("{}m ago", delta / 60)
    }
}

pub fn status_label_for_history(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::WaitingInput => "Waiting input",
        SessionStatus::Running => "Running",
        SessionStatus::Idle => "Idle",
        SessionStatus::Discovering => "Discovering",
        SessionStatus::Completed => "Completed",
        SessionStatus::Failed => "Failed",
    }
}

pub fn new_mock_sessions() -> Vec<SessionViewModel> {
    let now = Utc::now();
    [
        CodexSession {
            session_id: Uuid::new_v4().to_string(),
            source: SessionSource::Cli,
            pid: 4312,
            parent_pid: Some(4310),
            tty: Some("/dev/ttys008".into()),
            cwd: Some("/Users/cong/Desktop/AI相关/codex-island".into()),
            terminal_app: Some(TerminalApp::ITerm),
            title: "Refactor session store".into(),
            project_name: Some("codex-island".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: now,
            activity_label: Some("Continue with workspace-write approval?".into()),
            last_snapshot: Some("Continue with workspace-write approval?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Thread),
            submit_target: None,
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(now),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        },
        CodexSession {
            session_id: Uuid::new_v4().to_string(),
            source: SessionSource::Cli,
            pid: 4470,
            parent_pid: Some(4465),
            tty: Some("/dev/ttys010".into()),
            cwd: Some("/Users/cong/Desktop/AI相关/launch-site".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Render landing page".into(),
            project_name: Some("launch-site".into()),
            status: SessionStatus::Running,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: false,
            last_activity_at: now - chrono::Duration::seconds(24),
            activity_label: Some("Updating island interactions".into()),
            last_snapshot: Some("Updating Tauri frontend".into()),
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: None,
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(now - chrono::Duration::seconds(24)),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Running".into()],
            conversation_history: vec![],
        },
    ]
    .iter()
    .map(SessionViewModel::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::session_store::SessionStore;

    use super::{
        CodexSession, DiscoveryObservation, PromptAction, PromptSource, SessionEvent,
        SessionIngestionMode, SessionSource, SessionStatus, SessionViewModel, SubmitTarget,
        TerminalApp,
    };

    #[test]
    fn creates_running_session_for_new_observation() {
        let mut store = SessionStore::default();

        let changed = store.ingest(vec![DiscoveryObservation {
            pid: 101,
            parent_pid: Some(100),
            tty: Some("/dev/ttys001".into()),
            cwd: Some("/tmp/alpha".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Agent A".into(),
            project_name: Some("alpha".into()),
            source: SessionSource::Cli,
            activity_label: Some("Planning changes".into()),
            interaction_hint: None,
            prompt_actions: vec![],
            prompt_source: None,
            seen_at_unix_ms: 1_000,
            submit_target: None,
        }]);

        let sessions = store.sessions();
        assert!(changed);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].status, SessionStatus::Running);
        assert!(!sessions[0].needs_attention);
        assert_eq!(sessions[0].session_id, "cli:/dev/ttys001");
    }

    #[test]
    fn promotes_session_to_waiting_input_and_only_marks_changed_once() {
        let mut store = SessionStore::default();
        let base = DiscoveryObservation {
            pid: 101,
            parent_pid: Some(100),
            tty: Some("/dev/ttys001".into()),
            cwd: Some("/tmp/alpha".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Agent A".into(),
            project_name: Some("alpha".into()),
            source: SessionSource::Cli,
            activity_label: Some("Planning changes".into()),
            interaction_hint: None,
            prompt_actions: vec![],
            prompt_source: None,
            seen_at_unix_ms: 1_000,
            submit_target: None,
        };

        store.ingest(vec![base.clone()]);

        let first_wait = store.ingest(vec![DiscoveryObservation {
            interaction_hint: Some("confirm".into()),
            seen_at_unix_ms: 2_000,
            submit_target: None,
            ..base.clone()
        }]);
        let second_wait = store.ingest(vec![DiscoveryObservation {
            interaction_hint: Some("confirm".into()),
            seen_at_unix_ms: 2_500,
            submit_target: None,
            ..base
        }]);

        let sessions = store.sessions();
        assert!(first_wait);
        assert!(!second_wait);
        assert_eq!(sessions[0].status, SessionStatus::WaitingInput);
        assert!(sessions[0].needs_attention);
    }

    #[test]
    fn keeps_missing_sessions_without_auto_cleanup() {
        let mut store = SessionStore::default();

        store.ingest(vec![DiscoveryObservation {
            pid: 101,
            parent_pid: Some(100),
            tty: Some("/dev/ttys001".into()),
            cwd: Some("/tmp/alpha".into()),
            terminal_app: Some(TerminalApp::ITerm),
            title: "Agent A".into(),
            project_name: Some("alpha".into()),
            source: SessionSource::Cli,
            activity_label: Some("Planning changes".into()),
            interaction_hint: None,
            prompt_actions: vec![],
            prompt_source: None,
            seen_at_unix_ms: 1_000,
            submit_target: None,
        }]);

        let changed = store.ingest(vec![]);
        let sessions = store.sessions();

        assert!(!changed);
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn only_terminal_and_iterm_sessions_are_replyable() {
        let cli_without_target = CodexSession {
            session_id: "cli:/dev/ttys001".into(),
            source: SessionSource::Cli,
            pid: 101,
            parent_pid: Some(100),
            tty: Some("/dev/ttys001".into()),
            cwd: Some("/tmp/alpha".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "CLI".into(),
            project_name: Some("alpha".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: Utc::now(),
            activity_label: Some("Approval required".into()),
            last_snapshot: Some("Continue?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Thread),
            submit_target: None,
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };
        let cli_with_target = CodexSession {
            session_id: "cli:/dev/ttys002".into(),
            source: SessionSource::Cli,
            pid: 102,
            parent_pid: Some(100),
            tty: Some("/dev/ttys002".into()),
            cwd: Some("/tmp/beta".into()),
            terminal_app: Some(TerminalApp::Unsupported("VS Code".into())),
            title: "CLI".into(),
            project_name: Some("beta".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: Utc::now(),
            activity_label: Some("Approval required".into()),
            last_snapshot: Some("Continue?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Thread),
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };
        let desktop_with_target = CodexSession {
            session_id: "desktop:1".into(),
            source: SessionSource::Desktop,
            pid: 202,
            parent_pid: None,
            tty: None,
            cwd: Some("/tmp/alpha".into()),
            terminal_app: None,
            title: "Desktop".into(),
            project_name: Some("alpha".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: Utc::now(),
            activity_label: Some("Approval required".into()),
            last_snapshot: Some("Continue?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Thread),
            submit_target: Some(SubmitTarget::ThreadId("desktop-thread".into())),
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };
        let vscode = CodexSession {
            session_id: "cli:/dev/ttys002".into(),
            source: SessionSource::Cli,
            pid: 103,
            parent_pid: Some(100),
            tty: Some("/dev/ttys002".into()),
            cwd: Some("/tmp/beta".into()),
            terminal_app: Some(TerminalApp::Unsupported("VS Code".into())),
            title: "VS Code".into(),
            project_name: Some("beta".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: Utc::now(),
            activity_label: Some("Approval required".into()),
            last_snapshot: Some("Continue?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Thread),
            submit_target: None,
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };

        assert!(!SessionViewModel::from(&cli_without_target).can_reply);
        assert!(SessionViewModel::from(&cli_with_target).can_reply);
        assert!(SessionViewModel::from(&desktop_with_target).can_reply);
        assert!(!SessionViewModel::from(&vscode).can_reply);
    }

    #[test]
    fn sorts_waiting_sessions_ahead_of_running() {
        let mut store = SessionStore::default();

        store.ingest(vec![
            DiscoveryObservation {
                pid: 201,
                parent_pid: Some(100),
                tty: Some("/dev/ttys001".into()),
                cwd: Some("/tmp/alpha".into()),
                terminal_app: Some(TerminalApp::Terminal),
                title: "Running".into(),
                project_name: Some("alpha".into()),
                source: SessionSource::Cli,
                activity_label: Some("Applying patch".into()),
                interaction_hint: None,
                prompt_actions: vec![],
                prompt_source: None,
                seen_at_unix_ms: 1_000,
                submit_target: None,
            },
            DiscoveryObservation {
                pid: 301,
                parent_pid: Some(100),
                tty: Some("/dev/ttys002".into()),
                cwd: Some("/tmp/beta".into()),
                terminal_app: Some(TerminalApp::Terminal),
                title: "Waiting".into(),
                project_name: Some("beta".into()),
                source: SessionSource::Cli,
                activity_label: Some("Continue? [Y/n]".into()),
                interaction_hint: Some("continue?".into()),
                prompt_actions: vec![],
                prompt_source: Some(PromptSource::Terminal),
                seen_at_unix_ms: 1_100,
                submit_target: None,
            },
        ]);

        let sessions = store.sessions();
        assert_eq!(sessions[0].title, "Waiting");
        assert_eq!(sessions[1].title, "Running");
    }

    #[test]
    fn sorts_most_recent_sessions_first_even_when_status_is_lower_priority() {
        let mut store = SessionStore::default();

        store.ingest(vec![
            DiscoveryObservation {
                pid: 201,
                parent_pid: Some(100),
                tty: Some("/dev/ttys001".into()),
                cwd: Some("/tmp/alpha".into()),
                terminal_app: Some(TerminalApp::Terminal),
                title: "Older Waiting".into(),
                project_name: Some("alpha".into()),
                source: SessionSource::Cli,
                activity_label: Some("Need confirmation".into()),
                interaction_hint: Some("continue?".into()),
                prompt_actions: vec![],
                prompt_source: Some(PromptSource::Terminal),
                seen_at_unix_ms: 1_000,
                submit_target: None,
            },
            DiscoveryObservation {
                pid: 301,
                parent_pid: Some(100),
                tty: Some("/dev/ttys002".into()),
                cwd: Some("/tmp/beta".into()),
                terminal_app: Some(TerminalApp::Terminal),
                title: "Newer Running".into(),
                project_name: Some("beta".into()),
                source: SessionSource::Cli,
                activity_label: Some("Applying patch".into()),
                interaction_hint: None,
                prompt_actions: vec![],
                prompt_source: None,
                seen_at_unix_ms: 2_000,
                submit_target: None,
            },
        ]);

        let sessions = store.sessions();
        assert_eq!(sessions[0].title, "Newer Running");
        assert_eq!(sessions[1].title, "Older Waiting");
    }

    #[test]
    fn does_not_remove_hook_only_session_within_short_gap() {
        let mut store = SessionStore::default();
        let now = Utc::now().timestamp_millis();

        store.ingest_event(SessionEvent {
            session_id: "thread-keep".into(),
            thread_id: Some("thread-keep".into()),
            source: SessionSource::Cli,
            pid: Some(404),
            cwd: Some("/tmp/alpha".into()),
            tty: Some("/dev/ttys009".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: Some("Hook only".into()),
            project_name: Some("alpha".into()),
            activity_label: Some("Running".into()),
            prompt_text: None,
            user_prompt: None,
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: Some(SubmitTarget::ThreadId("thread-keep".into())),
            status: SessionStatus::Running,
            transcript_path: None,
            happened_at_unix_ms: now - 30_000,
        });

        let changed = store.ingest(vec![]);

        assert!(!changed);
        assert_eq!(store.sessions().len(), 1);
    }

    #[test]
    fn keeps_missing_session_when_process_is_still_alive() {
        let mut store = SessionStore::default();

        store.ingest(vec![DiscoveryObservation {
            pid: std::process::id() as i32,
            parent_pid: None,
            tty: Some("/dev/ttys999".into()),
            cwd: Some("/tmp/alive".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Alive session".into(),
            project_name: Some("alive".into()),
            source: SessionSource::Cli,
            activity_label: Some("Working".into()),
            interaction_hint: None,
            prompt_actions: vec![],
            prompt_source: None,
            seen_at_unix_ms: 1_000,
            submit_target: None,
        }]);

        let changed = store.ingest(vec![]);

        assert!(!changed);
        assert_eq!(store.sessions().len(), 1);
    }

    #[test]
    fn merges_cli_processes_sharing_same_tty_into_one_session() {
        let mut store = SessionStore::default();

        let first = DiscoveryObservation {
            pid: 84269,
            parent_pid: Some(84260),
            tty: Some("/dev/ttys019".into()),
            cwd: Some("/Users/cong/Desktop/AI相关/codex-island".into()),
            terminal_app: Some(TerminalApp::ITerm),
            title: "node codex wrapper".into(),
            project_name: Some("codex-island".into()),
            source: SessionSource::Cli,
            activity_label: Some("Starting".into()),
            interaction_hint: None,
            prompt_actions: vec![],
            prompt_source: None,
            seen_at_unix_ms: 1_000,
            submit_target: None,
        };
        let second = DiscoveryObservation {
            pid: 84275,
            parent_pid: Some(84269),
            tty: Some("/dev/ttys019".into()),
            cwd: Some("/Users/cong/Desktop/AI相关/codex-island".into()),
            terminal_app: Some(TerminalApp::ITerm),
            title: "native codex".into(),
            project_name: Some("codex-island".into()),
            source: SessionSource::Cli,
            activity_label: Some("Applying patch".into()),
            interaction_hint: None,
            prompt_actions: vec![],
            prompt_source: None,
            seen_at_unix_ms: 1_100,
            submit_target: None,
        };

        store.ingest(vec![first, second]);

        let sessions = store.sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "cli:/dev/ttys019");
        assert_eq!(sessions[0].pid, 84275);
        assert_eq!(sessions[0].title, "native codex");
    }

    #[test]
    fn creates_session_with_submit_target_from_observation() {
        let mut store = SessionStore::default();

        store.ingest(vec![DiscoveryObservation {
            pid: 202,
            parent_pid: Some(201),
            tty: Some("/dev/ttys002".into()),
            cwd: Some("/tmp/beta".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Desktop".into(),
            project_name: Some("beta".into()),
            source: SessionSource::Desktop,
            activity_label: Some("Approval required".into()),
            interaction_hint: Some("continue?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Thread),
            seen_at_unix_ms: 1_000,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
        }]);

        let session = store.sessions().into_iter().next().unwrap();
        assert_eq!(
            session.submit_target,
            Some(SubmitTarget::ThreadId("thread-123".into()))
        );
        assert!(SessionViewModel::from(&session).can_reply);
    }

    #[test]
    fn exposes_prompt_actions_in_view_model() {
        let session = CodexSession {
            session_id: "desktop:1".into(),
            source: SessionSource::Desktop,
            pid: 202,
            parent_pid: None,
            tty: None,
            cwd: Some("/tmp/alpha".into()),
            terminal_app: None,
            title: "Desktop".into(),
            project_name: Some("alpha".into()),
            status: SessionStatus::WaitingInput,
            ingestion_mode: SessionIngestionMode::Fallback,
            needs_attention: true,
            last_activity_at: Utc::now(),
            activity_label: Some("Approval required".into()),
            last_snapshot: Some("是否运行此命令？".into()),
            prompt_actions: vec![
                PromptAction {
                    id: "1".into(),
                    label: "是".into(),
                    reply: "1".into(),
                },
                PromptAction {
                    id: "2".into(),
                    label: "否".into(),
                    reply: "2".into(),
                },
            ],
            prompt_source: Some(PromptSource::Terminal),
            submit_target: Some(SubmitTarget::ThreadId("desktop-thread".into())),
            notification_sent_at: None,
            last_event_at: None,
            last_observation_at: Some(Utc::now()),
            transcript_path: None,
            latest_user_prompt: None,
            status_history: vec!["Waiting input".into()],
            conversation_history: vec![],
        };

        let view = SessionViewModel::from(&session);
        assert_eq!(view.action_options.len(), 2);
        assert_eq!(view.action_options[0].reply, "1");
    }

    #[test]
    fn refresh_updates_submit_target_in_existing_session() {
        let mut store = SessionStore::default();

        let base = DiscoveryObservation {
            pid: 202,
            parent_pid: Some(201),
            tty: Some("/dev/ttys002".into()),
            cwd: Some("/tmp/beta".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Desktop".into(),
            project_name: Some("beta".into()),
            source: SessionSource::Desktop,
            activity_label: Some("Approval required".into()),
            interaction_hint: Some("continue?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Thread),
            seen_at_unix_ms: 1_000,
            submit_target: None,
        };

        store.ingest(vec![base.clone()]);
        store.ingest(vec![DiscoveryObservation {
            seen_at_unix_ms: 2_000,
            submit_target: Some(SubmitTarget::ThreadId("thread-456".into())),
            ..base
        }]);

        let session = store.sessions().into_iter().next().unwrap();
        assert_eq!(
            session.submit_target,
            Some(SubmitTarget::ThreadId("thread-456".into()))
        );
        assert!(SessionViewModel::from(&session).can_reply);
    }

    #[test]
    fn hook_event_creates_running_session_with_hooks_ingestion_mode() {
        let mut store = SessionStore::default();

        let changed = store.ingest_event(SessionEvent {
            session_id: "thread-123".into(),
            thread_id: Some("thread-123".into()),
            source: SessionSource::Cli,
            pid: Some(404),
            cwd: Some("/tmp/alpha".into()),
            tty: Some("/dev/ttys009".into()),
            terminal_app: Some(TerminalApp::ITerm),
            title: Some("alpha".into()),
            project_name: Some("alpha".into()),
            activity_label: Some("Running Bash".into()),
            prompt_text: None,
            user_prompt: None,
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
            status: SessionStatus::Running,
            transcript_path: Some("/tmp/rollout.jsonl".into()),
            happened_at_unix_ms: 2_000,
        });

        let session = store.sessions().into_iter().next().expect("session");
        assert!(changed);
        assert_eq!(session.session_id, "cli:/dev/ttys009");
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(session.ingestion_mode, SessionIngestionMode::Hooks);
    }

    #[test]
    fn hook_event_promotes_existing_session_to_waiting_input() {
        let mut store = SessionStore::default();

        store.ingest(vec![DiscoveryObservation {
            pid: 101,
            parent_pid: Some(100),
            tty: Some("/dev/ttys001".into()),
            cwd: Some("/tmp/alpha".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Agent A".into(),
            project_name: Some("alpha".into()),
            source: SessionSource::Cli,
            activity_label: Some("Planning changes".into()),
            interaction_hint: None,
            prompt_actions: vec![],
            prompt_source: None,
            seen_at_unix_ms: 1_000,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
        }]);

        let changed = store.ingest_event(SessionEvent {
            session_id: "thread-123".into(),
            thread_id: Some("thread-123".into()),
            source: SessionSource::Cli,
            pid: Some(101),
            cwd: Some("/tmp/alpha".into()),
            tty: Some("/dev/ttys001".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: Some("Agent A".into()),
            project_name: Some("alpha".into()),
            activity_label: Some("Approval required".into()),
            prompt_text: Some("Continue?".into()),
            user_prompt: None,
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Thread),
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
            status: SessionStatus::WaitingInput,
            transcript_path: Some("/tmp/rollout.jsonl".into()),
            happened_at_unix_ms: 2_000,
        });

        let session = store.sessions().into_iter().next().expect("session");
        assert!(changed);
        assert_eq!(session.status, SessionStatus::WaitingInput);
        assert!(session.needs_attention);
        assert_eq!(session.ingestion_mode, SessionIngestionMode::Hooks);
    }

    #[test]
    fn recent_hook_event_wins_over_polling_status() {
        let mut store = SessionStore::default();

        store.ingest_event(SessionEvent {
            session_id: "thread-123".into(),
            thread_id: Some("thread-123".into()),
            source: SessionSource::Cli,
            pid: Some(101),
            cwd: Some("/tmp/alpha".into()),
            tty: Some("/dev/ttys001".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: Some("Agent A".into()),
            project_name: Some("alpha".into()),
            activity_label: Some("Running tool".into()),
            prompt_text: None,
            user_prompt: None,
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
            status: SessionStatus::Running,
            transcript_path: Some("/tmp/rollout.jsonl".into()),
            happened_at_unix_ms: 10_000,
        });

        store.ingest(vec![DiscoveryObservation {
            pid: 101,
            parent_pid: Some(100),
            tty: Some("/dev/ttys001".into()),
            cwd: Some("/tmp/alpha".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Agent A".into(),
            project_name: Some("alpha".into()),
            source: SessionSource::Cli,
            activity_label: Some("Continue?".into()),
            interaction_hint: Some("Continue?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Terminal),
            seen_at_unix_ms: 10_500,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
        }]);

        let session = store.sessions().into_iter().next().expect("session");
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(session.ingestion_mode, SessionIngestionMode::Hooks);
    }

    #[test]
    fn stale_hook_event_falls_back_to_polling_status() {
        let mut store = SessionStore::default();

        store.ingest_event(SessionEvent {
            session_id: "thread-123".into(),
            thread_id: Some("thread-123".into()),
            source: SessionSource::Cli,
            pid: Some(101),
            cwd: Some("/tmp/alpha".into()),
            tty: Some("/dev/ttys001".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: Some("Agent A".into()),
            project_name: Some("alpha".into()),
            activity_label: Some("Running tool".into()),
            prompt_text: None,
            user_prompt: None,
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
            status: SessionStatus::Running,
            transcript_path: Some("/tmp/rollout.jsonl".into()),
            happened_at_unix_ms: 1_000,
        });

        store.ingest(vec![DiscoveryObservation {
            pid: 101,
            parent_pid: Some(100),
            tty: Some("/dev/ttys001".into()),
            cwd: Some("/tmp/alpha".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: "Agent A".into(),
            project_name: Some("alpha".into()),
            source: SessionSource::Cli,
            activity_label: Some("Continue?".into()),
            interaction_hint: Some("Continue?".into()),
            prompt_actions: vec![],
            prompt_source: Some(PromptSource::Terminal),
            seen_at_unix_ms: 80_000,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
        }]);

        let session = store.sessions().into_iter().next().expect("session");
        assert_eq!(session.status, SessionStatus::WaitingInput);
        assert_eq!(session.ingestion_mode, SessionIngestionMode::Fallback);
    }

    #[test]
    fn keeps_last_ten_unique_status_history_entries() {
        let mut store = SessionStore::default();

        store.ingest_event(SessionEvent {
            session_id: "thread-123".into(),
            thread_id: Some("thread-123".into()),
            source: SessionSource::Cli,
            pid: Some(101),
            cwd: Some("/tmp/alpha".into()),
            tty: Some("/dev/ttys001".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: Some("Agent A".into()),
            project_name: Some("alpha".into()),
            activity_label: Some("State 1".into()),
            prompt_text: None,
            user_prompt: None,
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
            status: SessionStatus::Running,
            transcript_path: Some("/tmp/rollout.jsonl".into()),
            happened_at_unix_ms: 1_000,
        });
        store.ingest_event(SessionEvent {
            session_id: "thread-123".into(),
            thread_id: Some("thread-123".into()),
            source: SessionSource::Cli,
            pid: Some(101),
            cwd: Some("/tmp/alpha".into()),
            tty: Some("/dev/ttys001".into()),
            terminal_app: Some(TerminalApp::Terminal),
            title: Some("Agent A".into()),
            project_name: Some("alpha".into()),
            activity_label: Some("State 1".into()),
            prompt_text: None,
            user_prompt: None,
            prompt_actions: vec![],
            prompt_source: None,
            submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
            status: SessionStatus::Running,
            transcript_path: Some("/tmp/rollout.jsonl".into()),
            happened_at_unix_ms: 1_100,
        });

        for index in 2..=12 {
            store.ingest_event(SessionEvent {
                session_id: "thread-123".into(),
                thread_id: Some("thread-123".into()),
                source: SessionSource::Cli,
                pid: Some(101),
                cwd: Some("/tmp/alpha".into()),
                tty: Some("/dev/ttys001".into()),
                terminal_app: Some(TerminalApp::Terminal),
                title: Some("Agent A".into()),
                project_name: Some("alpha".into()),
                activity_label: Some(format!("State {index}")),
                prompt_text: None,
                user_prompt: None,
                prompt_actions: vec![],
                prompt_source: None,
                submit_target: Some(SubmitTarget::ThreadId("thread-123".into())),
                status: SessionStatus::Running,
                transcript_path: Some("/tmp/rollout.jsonl".into()),
                happened_at_unix_ms: 1_000 + (index as i64),
            });
        }

        let session = store.sessions().into_iter().next().expect("session");
        assert_eq!(session.status_history.len(), 10);
        assert_eq!(session.status_history.first().map(String::as_str), Some("State 3"));
        assert_eq!(session.status_history.last().map(String::as_str), Some("State 12"));
    }
}
