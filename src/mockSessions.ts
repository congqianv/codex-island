import type { SessionsPayload } from "./types";

export const mockSessions: SessionsPayload = {
  sessions: [
    {
      session_id: "mock-1",
      title: "Fix Tauri monitor",
      project_name: "codex-island",
      status: "waiting_input",
      needs_attention: true,
      can_reply: false,
      subtitle: "Continue with file access approval?",
      prompt_text: "Continue with file access approval?",
      action_options: [],
      prompt_source: "thread",
      terminal_label: "iTerm2",
      relative_last_activity: "just now",
      last_activity_unix_ms: 2_000
    },
    {
      session_id: "mock-2",
      title: "Polish marketing site",
      project_name: "launch-site",
      status: "running",
      needs_attention: false,
      can_reply: false,
      subtitle: "Updating island interactions",
      prompt_text: null,
      action_options: [],
      prompt_source: null,
      terminal_label: "Terminal",
      relative_last_activity: "24s ago",
      last_activity_unix_ms: 1_000
    }
  ],
  summary: {
    total: 2,
    running: 1,
    waiting: 1,
    completed: 0
  }
};
