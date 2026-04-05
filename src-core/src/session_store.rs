use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeZone, Utc};

use crate::models::{
    status_label_for_history, CodexSession, DiscoveryObservation, SessionEvent, SessionIngestionMode,
    SessionStatus,
};

const HOOK_EVENT_STALE_AFTER_MS: i64 = 60_000;

#[derive(Default)]
pub struct SessionStore {
    sessions: HashMap<String, CodexSession>,
}

impl SessionStore {
    pub fn ingest(&mut self, observations: Vec<DiscoveryObservation>) -> bool {
        self.ingest_observations(observations)
    }

    pub fn ingest_event(&mut self, event: SessionEvent) -> bool {
        let session_id = event.session_key();
        let happened_at = unix_ms_to_utc(event.happened_at_unix_ms);
        let needs_attention = matches!(event.status, SessionStatus::WaitingInput);
        let history_entry = event
            .activity_label
            .as_deref()
            .or(event.prompt_text.as_deref())
            .unwrap_or(status_label_for_history(&event.status))
            .to_string();
        let user_prompt_for_history = event.user_prompt.clone();
        let assistant_prompt_for_history = event.prompt_text.clone();

        match self.sessions.get_mut(&session_id) {
            Some(session) => {
                let previous = session.clone();
                if let Some(pid) = event.pid {
                    session.pid = pid;
                }
                session.cwd = event.cwd.or_else(|| session.cwd.clone());
                session.tty = event.tty.or_else(|| session.tty.clone());
                session.terminal_app = event.terminal_app.or_else(|| session.terminal_app.clone());
                session.title = event.title.unwrap_or_else(|| session.title.clone());
                session.project_name = event.project_name.or_else(|| session.project_name.clone());
                session.source = event.source;
                session.status = event.status;
                session.ingestion_mode = SessionIngestionMode::Hooks;
                session.needs_attention = needs_attention;
                session.last_activity_at = happened_at;
                session.activity_label = event.activity_label;
                session.last_snapshot = event.prompt_text;
                if let Some(user_prompt) = event.user_prompt {
                    append_conversation_history(session, "You", &user_prompt);
                    session.latest_user_prompt = Some(user_prompt);
                }
                if let Some(assistant_prompt) = assistant_prompt_for_history.as_deref() {
                    append_conversation_history(session, "Assistant", assistant_prompt);
                }
                session.prompt_actions = event.prompt_actions;
                session.prompt_source = event.prompt_source;
                session.submit_target = event.submit_target.or_else(|| session.submit_target.clone());
                session.last_event_at = Some(happened_at);
                session.transcript_path = event.transcript_path.or_else(|| session.transcript_path.clone());
                append_status_history(session, &history_entry);

                if session.needs_attention && !previous.needs_attention {
                    session.notification_sent_at = None;
                }

                *session != previous
            }
            None => {
                self.sessions.insert(
                    session_id.clone(),
                    CodexSession {
                        session_id,
                        source: event.source,
                        pid: event.pid.unwrap_or_default(),
                        parent_pid: None,
                        tty: event.tty,
                        cwd: event.cwd,
                        terminal_app: event.terminal_app,
                        title: event.title.unwrap_or_else(|| "Codex".into()),
                        project_name: event.project_name,
                        status: event.status,
                        ingestion_mode: SessionIngestionMode::Hooks,
                        needs_attention,
                        last_activity_at: happened_at,
                        activity_label: event.activity_label,
                        last_snapshot: event.prompt_text,
                        latest_user_prompt: event.user_prompt,
                        prompt_actions: event.prompt_actions,
                        prompt_source: event.prompt_source,
                        submit_target: event.submit_target,
                        notification_sent_at: None,
                        last_event_at: Some(happened_at),
                        last_observation_at: None,
                        transcript_path: event.transcript_path,
                        status_history: vec![history_entry],
                        conversation_history: initial_conversation_history(
                            user_prompt_for_history.as_deref(),
                            assistant_prompt_for_history.as_deref(),
                        ),
                    },
                );
                true
            }
        }
    }

    pub fn ingest_observations(&mut self, observations: Vec<DiscoveryObservation>) -> bool {
        let mut changed = false;
        let mut seen = HashSet::new();
        for observation in observations {
            let session_id = observation.session_id();
            seen.insert(session_id.clone());
            let next_status = if observation.interaction_hint.is_some() {
                SessionStatus::WaitingInput
            } else if observation
                .activity_label
                .as_deref()
                .map(is_idle_activity_label)
                .unwrap_or(false)
            {
                SessionStatus::Idle
            } else {
                SessionStatus::Running
            };
            let next_seen_at = unix_ms_to_utc(observation.seen_at_unix_ms);

            match self.sessions.get_mut(&session_id) {
                Some(session) => {
                    let previous_status = session.status.clone();
                    let previous_ingestion_mode = session.ingestion_mode.clone();
                    let previous_attention = session.needs_attention;
                    let previous_prompt_actions = session.prompt_actions.clone();
                    let previous_submit_target = session.submit_target.clone();
                    let should_apply_fallback = !has_recent_event(session, observation.seen_at_unix_ms);

                    session.pid = observation.pid;
                    session.parent_pid = observation.parent_pid;
                    session.tty = observation.tty.clone();
                    session.cwd = observation.cwd.clone();
                    session.terminal_app = observation.terminal_app.clone();
                    session.title = observation.title.clone();
                    session.source = observation.source.clone();
                    session.last_activity_at = next_seen_at;
                    session.last_observation_at = Some(next_seen_at);

                    if should_apply_fallback {
                        session.status = next_status.clone();
                        session.ingestion_mode = SessionIngestionMode::Fallback;
                        session.needs_attention = matches!(next_status, SessionStatus::WaitingInput);
                        session.activity_label = observation.activity_label.clone();
                        session.last_snapshot = observation.interaction_hint.clone();
                        session.prompt_actions = observation.prompt_actions.clone();
                        session.prompt_source = observation.prompt_source.clone();
                        session.submit_target = observation.submit_target.clone();
                        append_status_history(
                            session,
                            observation
                                .activity_label
                                .as_deref()
                                .or(observation.interaction_hint.as_deref())
                                .unwrap_or(status_label_for_history(&next_status)),
                        );
                    } else if session.submit_target.is_none() {
                        session.submit_target = observation.submit_target.clone();
                    }

                    if session.needs_attention && !previous_attention {
                        session.notification_sent_at = None;
                    }

                    changed |= previous_status != session.status
                        || previous_ingestion_mode != session.ingestion_mode
                        || previous_attention != session.needs_attention
                        || previous_prompt_actions != session.prompt_actions
                        || previous_submit_target != session.submit_target;
                }
                None => {
                    self.sessions.insert(
                        session_id,
                        CodexSession::from_observation(observation, next_status),
                    );
                    changed = true;
                }
            }
        }

        changed
    }

    pub fn sessions(&self) -> Vec<CodexSession> {
        let mut sessions = self.sessions.values().cloned().collect::<Vec<_>>();
        sessions.sort_by(|left, right| {
            right
                .last_activity_at
                .cmp(&left.last_activity_at)
                .then_with(|| right.status.sort_priority().cmp(&left.status.sort_priority()))
                .then_with(|| left.title.cmp(&right.title))
        });
        sessions
    }

    pub fn sessions_mut(&mut self) -> impl Iterator<Item = &mut CodexSession> {
        self.sessions.values_mut()
    }
}

fn unix_ms_to_utc(value: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(value)
        .single()
        .unwrap_or_else(Utc::now)
}

fn has_recent_event(session: &CodexSession, now_unix_ms: i64) -> bool {
    session
        .last_event_at
        .map(|instant| now_unix_ms - instant.timestamp_millis() <= HOOK_EVENT_STALE_AFTER_MS)
        .unwrap_or(false)
}

fn is_idle_activity_label(activity_label: &str) -> bool {
    activity_label.to_lowercase().contains("idle")
}

fn append_status_history(session: &mut CodexSession, entry: &str) {
    if session
        .status_history
        .last()
        .map(|last| last == entry)
        .unwrap_or(false)
    {
        return;
    }

    session.status_history.push(entry.to_string());
    if session.status_history.len() > 10 {
        let overflow = session.status_history.len() - 10;
        session.status_history.drain(0..overflow);
    }
}

fn initial_conversation_history(user_prompt: Option<&str>, assistant_prompt: Option<&str>) -> Vec<String> {
    let mut history = Vec::new();
    if let Some(user_prompt) = user_prompt {
        history.push(format!("You: {user_prompt}"));
    }
    if let Some(assistant_prompt) = assistant_prompt {
        history.push(format!("Assistant: {assistant_prompt}"));
    }
    history
}

fn append_conversation_history(session: &mut CodexSession, role: &str, text: &str) {
    let entry = format!("{role}: {text}");
    if session
        .conversation_history
        .last()
        .map(|last| last == &entry)
        .unwrap_or(false)
    {
        return;
    }

    session.conversation_history.push(entry);
    if session.conversation_history.len() > 10 {
        let overflow = session.conversation_history.len() - 10;
        session.conversation_history.drain(0..overflow);
    }
}
