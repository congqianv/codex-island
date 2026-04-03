use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeZone, Utc};

use crate::models::{CodexSession, DiscoveryObservation, SessionStatus};

#[derive(Default)]
pub struct SessionStore {
    sessions: HashMap<String, CodexSession>,
}

impl SessionStore {
    pub fn ingest(&mut self, observations: Vec<DiscoveryObservation>) -> bool {
        let mut changed = false;
        let mut seen = HashSet::new();
        for observation in observations {
            let session_id = observation.session_id();
            seen.insert(session_id.clone());
            let next_status = if observation.interaction_hint.is_some() {
                SessionStatus::WaitingInput
            } else {
                SessionStatus::Running
            };
            let next_seen_at = unix_ms_to_utc(observation.seen_at_unix_ms);

            match self.sessions.get_mut(&session_id) {
                Some(session) => {
                    let previous_status = session.status.clone();
                    let previous_attention = session.needs_attention;
                    let previous_prompt_actions = session.prompt_actions.clone();
                    let previous_submit_target = session.submit_target.clone();

                    session.pid = observation.pid;
                    session.parent_pid = observation.parent_pid;
                    session.tty = observation.tty.clone();
                    session.cwd = observation.cwd.clone();
                    session.terminal_app = observation.terminal_app.clone();
                    session.title = observation.title.clone();
                    session.source = observation.source.clone();
                    session.status = next_status.clone();
                    session.needs_attention = matches!(next_status, SessionStatus::WaitingInput);
                    session.last_activity_at = next_seen_at;
                    session.last_snapshot = observation.interaction_hint.clone();
                    session.prompt_actions = observation.prompt_actions.clone();
                    session.submit_target = observation.submit_target.clone();

                    if session.needs_attention && !previous_attention {
                        session.notification_sent_at = None;
                    }

                    changed |= previous_status != session.status
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

        let missing_ids = self
            .sessions
            .keys()
            .filter(|session_id| !seen.contains(*session_id))
            .cloned()
            .collect::<Vec<_>>();

        for session_id in missing_ids {
            if self.sessions.remove(&session_id).is_some() {
                changed = true;
            }
        }

        changed
    }

    pub fn sessions(&self) -> Vec<CodexSession> {
        let mut sessions = self.sessions.values().cloned().collect::<Vec<_>>();
        sessions.sort_by(|left, right| {
            right
                .status
                .sort_priority()
                .cmp(&left.status.sort_priority())
                .then_with(|| right.last_activity_at.cmp(&left.last_activity_at))
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
