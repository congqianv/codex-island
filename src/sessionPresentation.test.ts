import { describe, expect, it } from "vitest";

import {
  autoOpenDetailSession,
  collapsedCountLabel,
  collapsedStatusLabel,
  collapsedVisualState,
  shouldCollapsePinnedPanel,
  detailConversationHistory,
  detailPromptText,
  formatSummaryLabel,
  latestProjectName,
  latestSessionByActivity,
  sortSessions,
  statusLabel,
  shouldShowHandleButton
} from "./sessionPresentation";
import type { SessionViewModel } from "./types";

function makeSummary(overrides: Partial<{
  total: number;
  running: number;
  waiting: number;
  completed: number;
  discovering: number;
  failed: number;
  idle: number;
}> = {}) {
  return {
    total: 0,
    running: 0,
    waiting: 0,
    completed: 0,
    discovering: 0,
    failed: 0,
    idle: 0,
    ...overrides
  };
}

describe("formatSummaryLabel", () => {
  it("prioritizes attention state", () => {
    expect(formatSummaryLabel(makeSummary({ total: 3, running: 2, waiting: 1 }))).toBe(
      "1 needs attention"
    );
  });

  it("falls back to running state", () => {
    expect(formatSummaryLabel(makeSummary({ total: 2, running: 2 }))).toBe(
      "2 Codex running"
    );
  });

  it("handles idle state", () => {
    expect(formatSummaryLabel(makeSummary())).toBe("Codex Island is idle");
  });
});

describe("latestProjectName", () => {
  it("uses the first session as the collapsed project label source", () => {
    const sessions = [
      {
        session_id: "1",
        title: "Latest",
        project_name: "playground",
        status: "running",
        needs_attention: false,
        can_reply: false,
        subtitle: "Working",
        prompt_text: null,
        action_options: [],
        prompt_source: null,
        ingestion_mode: "fallback",
        terminal_label: "VS Code",
        relative_last_activity: "1m ago",
        last_activity_unix_ms: 2_000
      },
      {
        session_id: "2",
        title: "Older",
        project_name: "codex-island",
        status: "waiting_input",
        needs_attention: true,
        can_reply: true,
        subtitle: "Need your input",
        prompt_text: "How should I proceed?",
        action_options: [],
        prompt_source: "thread",
        ingestion_mode: "hooks",
        terminal_label: "Terminal",
        relative_last_activity: "2m ago",
        last_activity_unix_ms: 1_000
      }
    ] satisfies SessionViewModel[];

    expect(latestProjectName(sessions)).toBe("playground");
  });

  it("falls back to idle label when there are no sessions", () => {
    expect(latestProjectName([])).toBe("Codex Island");
  });
});

describe("collapsedCountLabel", () => {
  it("renders total session count in compact xN format", () => {
    expect(collapsedCountLabel(makeSummary({ total: 3, running: 2, waiting: 1 }))).toBe(
      "x3"
    );
  });
});

describe("collapsedStatusLabel", () => {
  it("renders idle when there are no sessions", () => {
    expect(collapsedStatusLabel(makeSummary())).toBe("Idle");
  });

  it("renders working when there are running sessions without attention", () => {
    expect(collapsedStatusLabel(makeSummary({ total: 3, running: 1, completed: 1 }))).toBe(
      "Working"
    );
  });

  it("renders working when there are discovering sessions", () => {
    expect(collapsedStatusLabel(makeSummary({ total: 1, discovering: 1 }))).toBe(
      "Working"
    );
  });

  it("renders needs attention when there are waiting sessions", () => {
    expect(collapsedStatusLabel(makeSummary({ total: 2, running: 1, waiting: 1 }))).toBe(
      "Needs Attention"
    );
  });

  it("renders failed when failed sessions exist without active work", () => {
    expect(collapsedStatusLabel(makeSummary({ total: 1, failed: 1 }))).toBe("Failed");
  });

  it("renders completed when sticky completion is present", () => {
    expect(collapsedStatusLabel(makeSummary(), true)).toBe("Completed");
  });

  it("renders completed when completed sessions exist", () => {
    expect(collapsedStatusLabel(makeSummary({ total: 1, completed: 1 }))).toBe(
      "Completed"
    );
  });
});

describe("collapsedVisualState", () => {
  it("returns idle when there are no active or completed tasks", () => {
    expect(collapsedVisualState(makeSummary())).toBe("idle");
  });

  it("returns working when there are running sessions without attention", () => {
    expect(collapsedVisualState(makeSummary({ total: 1, running: 1 }))).toBe("working");
  });

  it("returns working when there are discovering sessions", () => {
    expect(collapsedVisualState(makeSummary({ total: 1, discovering: 1 }))).toBe(
      "working"
    );
  });

  it("returns needs-attention when there are waiting sessions", () => {
    expect(collapsedVisualState(makeSummary({ total: 1, waiting: 1 }))).toBe(
      "needs-attention"
    );
  });

  it("returns failed when failed sessions exist without active work", () => {
    expect(collapsedVisualState(makeSummary({ total: 1, failed: 1 }))).toBe("failed");
  });

  it("returns completed when sticky completion is present", () => {
    expect(collapsedVisualState(makeSummary(), true)).toBe("completed");
  });

  it("returns completed when completed sessions exist", () => {
    expect(collapsedVisualState(makeSummary({ total: 1, completed: 1 }))).toBe(
      "completed"
    );
  });
});

describe("statusLabel", () => {
  it("renders idle label for idle status", () => {
    expect(statusLabel("idle")).toBe("Idle");
  });
});

describe("latestSessionByActivity", () => {
  it("picks the most recently active session", () => {
    const latest = latestSessionByActivity([
      {
        session_id: "older",
        title: "Older",
        project_name: "alpha",
        status: "waiting_input",
        needs_attention: true,
        can_reply: false,
        subtitle: "Needs input",
        prompt_text: "Continue?",
        action_options: [],
        prompt_source: "thread",
        ingestion_mode: "hooks",
        terminal_label: "Terminal",
        relative_last_activity: "1m ago",
        last_activity_unix_ms: 1_000
      },
      {
        session_id: "newer",
        title: "Newer",
        project_name: "beta",
        status: "running",
        needs_attention: false,
        can_reply: false,
        subtitle: "Working",
        prompt_text: null,
        action_options: [],
        prompt_source: null,
        ingestion_mode: "fallback",
        terminal_label: "Codex app",
        relative_last_activity: "just now",
        last_activity_unix_ms: 2_000
      }
    ]);

    expect(latest?.session_id).toBe("newer");
  });
});

describe("sortSessions", () => {
  it("orders sessions by last activity before status", () => {
    const sessions = sortSessions([
      {
        session_id: "older-waiting",
        title: "A",
        project_name: "alpha",
        status: "waiting_input",
        needs_attention: true,
        can_reply: false,
        subtitle: "Needs input",
        prompt_text: "Continue?",
        action_options: [],
        prompt_source: "thread",
        ingestion_mode: "hooks",
        terminal_label: "Terminal",
        relative_last_activity: "1m ago",
        last_activity_unix_ms: 1_000
      },
      {
        session_id: "newer-running",
        title: "B",
        project_name: "beta",
        status: "running",
        needs_attention: false,
        can_reply: false,
        subtitle: "Running",
        prompt_text: null,
        action_options: [],
        prompt_source: null,
        ingestion_mode: "fallback",
        terminal_label: "Terminal",
        relative_last_activity: "just now",
        last_activity_unix_ms: 2_000
      }
    ]);

    expect(sessions.map((session) => session.session_id)).toEqual([
      "newer-running",
      "older-waiting"
    ]);
  });
});

describe("detailPromptText", () => {
  it("shows the pending reminder text when input is needed", () => {
    const session = {
      session_id: "1",
      title: "Ignored title",
      project_name: "ignored-project",
      status: "waiting_input",
      needs_attention: true,
      can_reply: false,
      subtitle: "Continue with workspace-write approval?",
      prompt_text: "Allow file access?",
      action_options: [],
      prompt_source: "thread",
      ingestion_mode: "hooks",
      terminal_label: "Terminal",
      relative_last_activity: "1m ago",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(detailPromptText(session)).toBe("Allow file access?");
  });

  it("shows an idle reminder message when there is no pending prompt", () => {
    const session = {
      session_id: "1",
      title: "Ignored title",
      project_name: "ignored-project",
      status: "running",
      needs_attention: false,
      can_reply: false,
      subtitle: "Desktop app active",
      prompt_text: null,
      action_options: [],
      prompt_source: null,
      ingestion_mode: "fallback",
      terminal_label: "Codex app",
      relative_last_activity: "1m ago",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(detailPromptText(session)).toBe("No pending reminder");
  });
});

describe("detailConversationHistory", () => {
  it("keeps only the latest five entries in reverse chronological order", () => {
    const session = {
      session_id: "1",
      title: "History",
      project_name: "codex-island",
      status: "running",
      needs_attention: false,
      can_reply: false,
      subtitle: "Working",
      prompt_text: null,
      action_options: [],
      prompt_source: null,
      conversation_history: ["1", "2", "3", "4", "5", "6", "7"],
      ingestion_mode: "fallback",
      terminal_label: "Codex app",
      relative_last_activity: "just now",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(detailConversationHistory(session)).toEqual(["7", "6", "5", "4", "3"]);
  });

  it("falls back to status history when conversation history is empty", () => {
    const session = {
      session_id: "1",
      title: "History",
      project_name: "codex-island",
      status: "running",
      needs_attention: false,
      can_reply: false,
      subtitle: "Working",
      prompt_text: null,
      action_options: [],
      prompt_source: null,
      status_history: ["State 1", "State 2", "State 3"],
      conversation_history: [],
      ingestion_mode: "fallback",
      terminal_label: "Codex app",
      relative_last_activity: "just now",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(detailConversationHistory(session)).toEqual(["State 3", "State 2", "State 1"]);
  });

  it("normalizes multiline entries and fills remaining slots from status history", () => {
    const session = {
      session_id: "1",
      title: "History",
      project_name: "codex-island",
      status: "running",
      needs_attention: false,
      can_reply: false,
      subtitle: "Working",
      prompt_text: null,
      action_options: [],
      prompt_source: null,
      status_history: ["S1", "S2", "S3"],
      conversation_history: ["", "Assistant:\n\nline one\n\nline two"],
      ingestion_mode: "fallback",
      terminal_label: "Codex app",
      relative_last_activity: "just now",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(detailConversationHistory(session)).toEqual([
      "Assistant: line one line two",
      "S3",
      "S2",
      "S1"
    ]);
  });
});

describe("shouldShowHandleButton", () => {
  it("shows the handle button when the session needs input", () => {
    const session = {
      session_id: "1",
      title: "Question",
      project_name: "codex-island",
      status: "waiting_input",
      needs_attention: true,
      can_reply: false,
      subtitle: "Need your input",
      prompt_text: "How should I proceed?",
      action_options: [],
      prompt_source: "thread",
      ingestion_mode: "hooks",
      terminal_label: "Codex app",
      relative_last_activity: "just now",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(shouldShowHandleButton(session)).toBe(true);
  });

  it("hides the handle button for non-reminder sessions", () => {
    const session = {
      session_id: "1",
      title: "Working",
      project_name: "codex-island",
      status: "running",
      needs_attention: false,
      can_reply: false,
      subtitle: "Working",
      prompt_text: null,
      action_options: [],
      prompt_source: null,
      ingestion_mode: "fallback",
      terminal_label: "Codex app",
      relative_last_activity: "just now",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(shouldShowHandleButton(session)).toBe(false);
  });
});

describe("autoOpenDetailSession", () => {
  it("returns the latest session that newly becomes waiting_input or completed", () => {
    const sessions = [
      {
        session_id: "a",
        title: "A",
        project_name: "p",
        status: "completed",
        needs_attention: false,
        can_reply: false,
        subtitle: "Completed",
        prompt_text: null,
        action_options: [],
        prompt_source: null,
        ingestion_mode: "fallback",
        terminal_label: "Terminal",
        relative_last_activity: "1m ago",
        last_activity_unix_ms: 1_000
      },
      {
        session_id: "b",
        title: "B",
        project_name: "p",
        status: "waiting_input",
        needs_attention: true,
        can_reply: true,
        subtitle: "Need input",
        prompt_text: "Continue?",
        action_options: [],
        prompt_source: "thread",
        ingestion_mode: "hooks",
        terminal_label: "Terminal",
        relative_last_activity: "just now",
        last_activity_unix_ms: 2_000
      }
    ] satisfies SessionViewModel[];

    const next = autoOpenDetailSession(sessions, { a: "running", b: "running" });
    expect(next?.session_id).toBe("b");
  });

  it("returns null when target status is unchanged", () => {
    const sessions = [
      {
        session_id: "a",
        title: "A",
        project_name: "p",
        status: "waiting_input",
        needs_attention: true,
        can_reply: true,
        subtitle: "Need input",
        prompt_text: "Continue?",
        action_options: [],
        prompt_source: "thread",
        ingestion_mode: "hooks",
        terminal_label: "Terminal",
        relative_last_activity: "just now",
        last_activity_unix_ms: 2_000
      }
    ] satisfies SessionViewModel[];

    const next = autoOpenDetailSession(sessions, { a: "waiting_input" });
    expect(next).toBeNull();
  });
});

describe("shouldCollapsePinnedPanel", () => {
  it("keeps panel open when a detail session is selected", () => {
    expect(shouldCollapsePinnedPanel(false, true, "session-1")).toBe(false);
  });

  it("collapses panel when not hovered, pinned, and no detail selected", () => {
    expect(shouldCollapsePinnedPanel(false, true, null)).toBe(true);
  });
});
