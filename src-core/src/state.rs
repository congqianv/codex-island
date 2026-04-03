use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

use crate::models::{
    CodexSession, SessionStatus, SessionSummary, SessionViewModel, SessionsPayload,
};
use crate::session_store::SessionStore;

pub struct CoreState {
    pub store: Arc<Mutex<SessionStore>>,
    pub window_expanded: Arc<AtomicBool>,
}

impl Default for CoreState {
    fn default() -> Self {
        Self {
            store: Arc::new(Mutex::new(SessionStore::default())),
            window_expanded: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CoreState {
    pub fn snapshot(&self) -> SessionsPayload {
        let store = self.store.lock().expect("session store lock poisoned");
        build_payload(store.sessions())
    }

    pub fn focusable_session(&self, session_id: &str) -> Option<CodexSession> {
        let store = self.store.lock().expect("session store lock poisoned");
        store
            .sessions()
            .into_iter()
            .find(|session| session.session_id == session_id)
    }

    pub fn set_window_expanded(&self, expanded: bool) {
        self.window_expanded.store(expanded, Ordering::Relaxed);
    }

    pub fn is_window_expanded(&self) -> bool {
        self.window_expanded.load(Ordering::Relaxed)
    }
}

fn build_payload(sessions: Vec<CodexSession>) -> SessionsPayload {
    let summary = SessionSummary {
        total: sessions.len(),
        running: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::Running))
            .count(),
        waiting: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::WaitingInput))
            .count(),
        completed: sessions
            .iter()
            .filter(|session| matches!(session.status, SessionStatus::Completed))
            .count(),
    };

    SessionsPayload {
        sessions: sessions
            .iter()
            .take(5)
            .map(SessionViewModel::from)
            .collect(),
        summary,
    }
}
