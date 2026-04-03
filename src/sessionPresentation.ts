import type { SessionStatus, SessionSummary, SessionViewModel } from "./types";

export type CollapsedVisualState =
  | "idle"
  | "working"
  | "needs-attention"
  | "completed";

export function formatSummaryLabel(summary: SessionSummary) {
  if (summary.waiting > 0) {
    return `${summary.waiting} needs attention`;
  }

  if (summary.running > 0) {
    return `${summary.running} Codex running`;
  }

  return "Codex Island is idle";
}

export function statusLabel(status: SessionStatus) {
  switch (status) {
    case "waiting_input":
      return "Needs input";
    case "running":
      return "Running";
    case "discovering":
      return "Discovering";
    case "completed":
      return "Completed";
    case "failed":
      return "Failed";
  }
}

export function latestProjectName(sessions: SessionViewModel[]) {
  return sessions[0]?.project_name ?? "Codex Island";
}

export function collapsedCountLabel(summary: SessionSummary) {
  return `x${summary.total}`;
}

export function collapsedStatusLabel(
  summary: SessionSummary,
  hasCompletedSticky = false
) {
  if (summary.waiting > 0) {
    return "Needs Attention";
  }

  if (summary.running > 0) {
    return "Working";
  }

  if (hasCompletedSticky) {
    return "Completed";
  }

  return "Idle";
}

export function collapsedVisualState(
  summary: SessionSummary,
  hasCompletedSticky = false
): CollapsedVisualState {
  if (summary.waiting > 0) {
    return "needs-attention";
  }

  if (summary.running > 0) {
    return "working";
  }

  if (hasCompletedSticky) {
    return "completed";
  }

  return "idle";
}

export function latestSessionByActivity(sessions: SessionViewModel[]) {
  return sessions.reduce<SessionViewModel | null>((latest, session) => {
    if (!latest || session.last_activity_unix_ms > latest.last_activity_unix_ms) {
      return session;
    }

    return latest;
  }, null);
}

export function detailPromptText(session: SessionViewModel) {
  return session.prompt_text ?? "No pending reminder";
}

export function shouldShowHandleButton(session: SessionViewModel) {
  return session.status === "waiting_input";
}

export function sortSessions(sessions: SessionViewModel[]) {
  const order: Record<SessionStatus, number> = {
    waiting_input: 4,
    running: 3,
    discovering: 2,
    failed: 1,
    completed: 0
  };

  return [...sessions].sort((left, right) => {
    const orderDelta = order[right.status] - order[left.status];
    if (orderDelta !== 0) {
      return orderDelta;
    }

    return left.title.localeCompare(right.title);
  });
}
