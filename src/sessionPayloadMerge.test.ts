import { describe, expect, it } from "vitest";

import { mergeNativeSessionsPayload } from "./sessionPayloadMerge";
import type { SessionViewModel, SessionsPayload } from "./types";

function makeSession(overrides: Partial<SessionViewModel>): SessionViewModel {
  return {
    session_id: "session-1",
    title: "Session",
    project_name: "project",
    status: "running",
    needs_attention: false,
    can_reply: false,
    subtitle: "Running",
    prompt_text: null,
    action_options: [],
    prompt_source: null,
    latest_user_prompt: null,
    status_history: [],
    conversation_history: [],
    ingestion_mode: "fallback",
    terminal_label: "Terminal",
    relative_last_activity: "just now",
    last_activity_unix_ms: 1_000,
    ...overrides
  };
}

function payload(sessions: SessionViewModel[]): SessionsPayload {
  return {
    sessions,
    summary: {
      total: sessions.length,
      running: sessions.filter((session) => session.status === "running").length,
      idle: sessions.filter((session) => session.status === "idle").length,
      waiting: sessions.filter((session) => session.status === "waiting_input").length,
      discovering: sessions.filter((session) => session.status === "discovering").length,
      failed: sessions.filter((session) => session.status === "failed").length,
      completed: sessions.filter((session) => session.status === "completed").length
    }
  };
}

describe("mergeNativeSessionsPayload", () => {
  it("keeps previously visible sessions when next payload is empty", () => {
    const previous = payload([
      makeSession({ session_id: "session-1", status: "running", last_activity_unix_ms: 1000 })
    ]);
    const next = payload([]);

    const merged = mergeNativeSessionsPayload(previous, next);
    expect(merged.sessions.map((session) => session.session_id)).toEqual(["session-1"]);
    expect(merged.summary.total).toBe(1);
  });

  it("updates existing sessions with newer payload data", () => {
    const previous = payload([
      makeSession({
        session_id: "session-1",
        status: "running",
        subtitle: "Running",
        last_activity_unix_ms: 1000
      })
    ]);
    const next = payload([
      makeSession({
        session_id: "session-1",
        status: "waiting_input",
        subtitle: "Need input",
        needs_attention: true,
        last_activity_unix_ms: 2000
      })
    ]);

    const merged = mergeNativeSessionsPayload(previous, next);
    expect(merged.sessions).toHaveLength(1);
    expect(merged.sessions[0].status).toBe("waiting_input");
    expect(merged.sessions[0].subtitle).toBe("Need input");
    expect(merged.summary.waiting).toBe(1);
  });

  it("preserves conversation history when incremental payload omits it", () => {
    const previous = payload([
      makeSession({
        session_id: "session-1",
        conversation_history: ["old-1", "old-2"],
        last_activity_unix_ms: 1000
      })
    ]);
    const next = payload([
      makeSession({
        session_id: "session-1",
        conversation_history: [],
        last_activity_unix_ms: 2000
      })
    ]);

    const merged = mergeNativeSessionsPayload(previous, next);
    expect(merged.sessions[0].conversation_history).toEqual(["old-1", "old-2"]);
    expect(merged.sessions[0].last_activity_unix_ms).toBe(2000);
  });
});
