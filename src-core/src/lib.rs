pub mod discovery;
pub mod focus;
pub mod models;
pub mod notify;
pub mod session_store;
pub mod state;

pub use models::{SessionStatus, SessionsPayload as AppSnapshot};
pub use state::CoreState;

#[cfg(test)]
mod tests {
    use crate::{AppSnapshot, SessionStatus};

    #[test]
    fn builds_empty_snapshot_from_new_core_state() {
        let snapshot = AppSnapshot::empty();
        assert_eq!(snapshot.summary.total, 0);
        assert!(snapshot.sessions.is_empty());
        assert_eq!(SessionStatus::Running.as_str(), "running");
    }
}
