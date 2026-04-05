import type { SessionStatus, SessionsPayload, SessionViewModel } from "./types";

export function mergeNativeSessionsPayload(
  previous: SessionsPayload,
  next: SessionsPayload
): SessionsPayload {
  if (next.sessions.length === 0 && previous.sessions.length > 0) {
    return {
      sessions: previous.sessions,
      summary: buildSummaryFromSessions(previous.sessions)
    };
  }

  const previousById = new Map(previous.sessions.map((session) => [session.session_id, session]));
  const mergedSessions = next.sessions.map((session) => {
    const previousSession = previousById.get(session.session_id);
    if (!previousSession) {
      return session;
    }

    return {
      ...session,
      conversation_history:
        session.conversation_history && session.conversation_history.length > 0
          ? session.conversation_history
          : (previousSession.conversation_history ?? []),
      status_history:
        session.status_history && session.status_history.length > 0
          ? session.status_history
          : (previousSession.status_history ?? [])
    };
  });

  return {
    sessions: mergedSessions,
    summary: buildSummaryFromSessions(mergedSessions)
  };
}

export function buildSummaryFromSessions(sessions: SessionViewModel[]) {
  return sessions.reduce(
    (summary, session) => {
      summary.total += 1;
      if (session.status === "running") {
        summary.running += 1;
      } else if (session.status === "idle") {
        summary.idle += 1;
      } else if (session.status === "waiting_input") {
        summary.waiting += 1;
      } else if (session.status === "discovering") {
        summary.discovering += 1;
      } else if (session.status === "failed") {
        summary.failed += 1;
      } else if (session.status === "completed") {
        summary.completed += 1;
      }
      return summary;
    },
    {
      total: 0,
      running: 0,
      idle: 0,
      waiting: 0,
      discovering: 0,
      failed: 0,
      completed: 0
    }
  );
}

export function statusRank(status: SessionStatus): number {
  switch (status) {
    case "waiting_input":
      return 5;
    case "running":
      return 4;
    case "discovering":
      return 3;
    case "failed":
      return 2;
    case "idle":
      return 1;
    case "completed":
      return 0;
  }
}
