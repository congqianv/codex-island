import { describe, expect, it } from "vitest";

import {
  collapsedCountLabel,
  collapsedStatusLabel,
  collapsedVisualState,
  detailPromptText,
  formatSummaryLabel,
  latestProjectName,
  latestSessionByActivity,
  shouldShowHandleButton
} from "./sessionPresentation";
import type { SessionViewModel } from "./types";

describe("formatSummaryLabel", () => {
  it("prioritizes attention state", () => {
    expect(
      formatSummaryLabel({
        total: 3,
        running: 2,
        waiting: 1,
        completed: 0
      })
    ).toBe("1 needs attention");
  });

  it("falls back to running state", () => {
    expect(
      formatSummaryLabel({
        total: 2,
        running: 2,
        waiting: 0,
        completed: 0
      })
    ).toBe("2 Codex running");
  });

  it("handles idle state", () => {
    expect(
      formatSummaryLabel({
        total: 0,
        running: 0,
        waiting: 0,
        completed: 0
      })
    ).toBe("Codex Island is idle");
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
    expect(
      collapsedCountLabel({
        total: 3,
        running: 2,
        waiting: 1,
        completed: 0
      })
    ).toBe("x3");
  });
});

describe("collapsedStatusLabel", () => {
  it("renders idle when there are no sessions", () => {
    expect(
      collapsedStatusLabel({
        total: 0,
        running: 0,
        waiting: 0,
        completed: 0
      })
    ).toBe("Idle");
  });

  it("renders working when there are active sessions without attention", () => {
    expect(
      collapsedStatusLabel({
        total: 3,
        running: 1,
        waiting: 0,
        completed: 1
      })
    ).toBe("Working");
  });

  it("renders needs attention when there are waiting sessions", () => {
    expect(
      collapsedStatusLabel({
        total: 2,
        running: 1,
        waiting: 1,
        completed: 0
      })
    ).toBe("Needs Attention");
  });

  it("renders completed when sticky completion is present", () => {
    expect(
      collapsedStatusLabel(
        {
          total: 0,
          running: 0,
          waiting: 0,
          completed: 0
        },
        true
      )
    ).toBe("Completed");
  });
});

describe("collapsedVisualState", () => {
  it("returns idle when there are no active or completed tasks", () => {
    expect(
      collapsedVisualState({
        total: 0,
        running: 0,
        waiting: 0,
        completed: 0
      })
    ).toBe("idle");
  });

  it("returns working when there are active sessions without attention", () => {
    expect(
      collapsedVisualState({
        total: 1,
        running: 1,
        waiting: 0,
        completed: 0
      })
    ).toBe("working");
  });

  it("returns needs-attention when there are waiting sessions", () => {
    expect(
      collapsedVisualState({
        total: 1,
        running: 0,
        waiting: 1,
        completed: 0
      })
    ).toBe("needs-attention");
  });

  it("returns completed when sticky completion is present", () => {
    expect(
      collapsedVisualState(
        {
          total: 0,
          running: 0,
          waiting: 0,
          completed: 0
        },
        true
      )
    ).toBe("completed");
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
        terminal_label: "Codex app",
        relative_last_activity: "just now",
        last_activity_unix_ms: 2_000
      }
    ]);

    expect(latest?.session_id).toBe("newer");
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
      terminal_label: "Codex app",
      relative_last_activity: "1m ago",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(detailPromptText(session)).toBe("No pending reminder");
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
      terminal_label: "Codex app",
      relative_last_activity: "just now",
      last_activity_unix_ms: 1_000
    } satisfies SessionViewModel;

    expect(shouldShowHandleButton(session)).toBe(false);
  });
});
