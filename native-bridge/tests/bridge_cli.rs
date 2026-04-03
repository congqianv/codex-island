use codex_island_native_bridge::run_cli;

#[test]
fn missing_command_reports_usage() {
    let args = vec!["native-bridge".into()];
    let error = run_cli(&args).unwrap_err();

    assert!(error.contains("missing command"));
    assert!(error.contains("get-sessions"));
}

#[test]
fn missing_focus_session_id_reports_usage() {
    let args = vec!["native-bridge".into(), "focus-session".into()];
    let error = run_cli(&args).unwrap_err();

    assert!(error.contains("focus-session requires a session id"));
    assert!(error.contains("native-bridge focus-session <session_id>"));
}

#[test]
fn missing_submit_session_reply_text_reports_usage() {
    let args = vec![
        "native-bridge".into(),
        "submit-session-reply".into(),
        "session-1".into(),
    ];
    let error = run_cli(&args).unwrap_err();

    assert!(error.contains("submit-session-reply requires reply text"));
    assert!(error.contains(
        "native-bridge submit-session-reply <session_id> <reply>"
    ));
}
