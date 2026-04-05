import type { SessionStatus, SessionSummary, SessionViewModel } from "./types";

export type CollapsedVisualState =
  | "idle"
  | "working"
  | "needs-attention"
  | "failed"
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
    case "idle":
      return "Idle";
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

  if (summary.discovering > 0) {
    return "Working";
  }

  if (summary.failed > 0) {
    return "Failed";
  }

  if (summary.completed > 0) {
    return "Completed";
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

  if (summary.discovering > 0) {
    return "working";
  }

  if (summary.failed > 0) {
    return "failed";
  }

  if (summary.completed > 0) {
    return "completed";
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

export function detailConversationHistory(session: SessionViewModel, maxItems = 5) {
  const entries: string[] = [];
  const seen = new Set<string>();

  const pushNewest = (values: string[]) => {
    for (let index = values.length - 1; index >= 0 && entries.length < maxItems; index -= 1) {
      const normalized = values[index].replace(/\s+/g, " ").trim();
      if (!normalized || seen.has(normalized)) {
        continue;
      }
      entries.push(normalized);
      seen.add(normalized);
    }
  };

  pushNewest(session.conversation_history ?? []);
  pushNewest(session.status_history ?? []);

  const promptFallback: string[] = [];
  if (session.latest_user_prompt) {
    promptFallback.push(`You: ${session.latest_user_prompt}`);
  }
  if (session.prompt_text) {
    promptFallback.push(`Assistant: ${session.prompt_text}`);
  }
  pushNewest(promptFallback);

  return entries;
}

export function shouldShowHandleButton(session: SessionViewModel) {
  return session.status === "waiting_input";
}

export function autoOpenDetailSession(
  sessions: SessionViewModel[],
  previousStatusBySessionId: Record<string, SessionStatus | undefined>
) {
  const candidates = sessions.filter((session) => {
    const previousStatus = previousStatusBySessionId[session.session_id];
    const isTargetStatus =
      session.status === "waiting_input" || session.status === "completed";
    return isTargetStatus && previousStatus !== session.status;
  });

  if (candidates.length === 0) {
    return null;
  }

  return candidates.reduce((latest, session) => {
    if (session.last_activity_unix_ms > latest.last_activity_unix_ms) {
      return session;
    }
    return latest;
  });
}

export function shouldCollapsePinnedPanel(
  hoverExpanded: boolean,
  pinnedExpanded: boolean,
  selectedSessionId: string | null
) {
  return !hoverExpanded && pinnedExpanded && selectedSessionId === null;
}

export function sortSessions(sessions: SessionViewModel[]) {
  const order: Record<SessionStatus, number> = {
    waiting_input: 4,
    running: 3,
    discovering: 2,
    failed: 1,
    idle: 0,
    completed: 0
  };

  return [...sessions].sort((left, right) => {
    const activityDelta = right.last_activity_unix_ms - left.last_activity_unix_ms;
    if (activityDelta !== 0) {
      return activityDelta;
    }

    const orderDelta = order[right.status] - order[left.status];
    if (orderDelta !== 0) {
      return orderDelta;
    }

    return left.title.localeCompare(right.title);
  });
}
