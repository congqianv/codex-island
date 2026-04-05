import { useDeferredValue, useEffect, useRef, useState } from "react";

import {
  focusSessionOnHost,
  openSessionProjectOnHost,
  syncIslandWindowOnHost
} from "./hostBridge";
import {
  autoOpenDetailSession,
  collapsedStatusLabel,
  type CollapsedVisualState,
  collapsedVisualState,
  detailConversationHistory,
  latestSessionByActivity,
  shouldCollapsePinnedPanel,
  shouldShowHandleButton,
  sortSessions,
  statusLabel
} from "./sessionPresentation";
import { useSessions } from "./useSessions";
import type { SessionStatus, SessionSummary, SessionViewModel } from "./types";
import stateDoneIcon from "./icons/state-done.svg";
import stateIdleIcon from "./icons/state-idle.svg";
import stateRunningIcon from "./icons/state-running.svg";
import stateThinkingIcon from "./icons/state-thinking.svg";
import stateWaitingIcon from "./icons/state-waiting.svg";

function SessionChatIcon() {
  return (
    <svg
      className="session-row__icon"
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M4.5 7.4a3 3 0 0 1 3-3h9a3 3 0 0 1 3 3v5.8a3 3 0 0 1-3 3H12l-3.5 2.8v-2.8H7.5a3 3 0 0 1-3-3z" />
      <circle cx="9.3" cy="10.4" r="0.9" className="session-row__icon-dot" />
      <circle cx="12" cy="10.4" r="0.9" className="session-row__icon-dot" />
      <circle cx="14.7" cy="10.4" r="0.9" className="session-row__icon-dot" />
    </svg>
  );
}

function SessionCodeIcon() {
  return (
    <svg
      className="session-row__icon"
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M8.4 7.5 5 12l3.4 4.5" />
      <path d="M15.6 7.5 19 12l-3.4 4.5" />
      <path d="m13.4 6.3-2.8 11.4" />
    </svg>
  );
}

function waitingSessionSignature(session: SessionViewModel): string | null {
  if (session.status !== "waiting_input") {
    return null;
  }

  const actionSignature = session.action_options
    .map((action) => `${action.id}:${action.reply}`)
    .join("|");
  return `${session.prompt_source ?? ""}::${session.prompt_text ?? ""}::${actionSignature}`;
}

function displayStatusForSession(
  session: SessionViewModel,
  isHandledWaitingSession: (session: SessionViewModel) => boolean
): SessionStatus {
  if (isHandledWaitingSession(session)) {
    return "idle";
  }

  return session.status;
}

function buildSummary(
  sessions: SessionViewModel[],
  isHandledWaitingSession: (session: SessionViewModel) => boolean
): SessionSummary {
  return sessions.reduce<SessionSummary>(
    (acc, session) => {
      const status = displayStatusForSession(session, isHandledWaitingSession);
      acc.total += 1;
      if (status === "waiting_input") {
        acc.waiting += 1;
      } else if (status === "running") {
        acc.running += 1;
      } else if (status === "discovering") {
        acc.discovering += 1;
      } else if (status === "failed") {
        acc.failed += 1;
      } else if (status === "completed") {
        acc.completed += 1;
      } else if (status === "idle") {
        acc.idle += 1;
      }

      return acc;
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

function collapsedIconForState(state: CollapsedVisualState): string {
  switch (state) {
    case "needs-attention":
      return stateWaitingIcon;
    case "working":
      return stateRunningIcon;
    case "completed":
      return stateDoneIcon;
    case "failed":
      return stateWaitingIcon;
    case "idle":
      return stateIdleIcon;
  }
}

function listDotToneForStatus(status: SessionStatus): CollapsedVisualState {
  switch (status) {
    case "waiting_input":
      return "needs-attention";
    case "running":
    case "discovering":
      return "working";
    case "completed":
      return "completed";
    case "failed":
      return "failed";
    case "idle":
      return "idle";
  }
}

export function App() {
  const { payload, host } = useSessions();
  const [hoverExpanded, setHoverExpanded] = useState(false);
  const [pinnedExpanded, setPinnedExpanded] = useState(false);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [completedSticky, setCompletedSticky] = useState(false);
  const [handledWaitingSessions, setHandledWaitingSessions] = useState<Record<string, string>>({});
  const deferredSessions = useDeferredValue(payload.sessions);
  const isHandledWaitingSession = (session: SessionViewModel) => {
    const signature = waitingSessionSignature(session);
    return signature !== null && handledWaitingSessions[session.session_id] === signature;
  };
  const rawSessions = sortSessions(deferredSessions);
  const sessions = rawSessions.map((session) => {
    if (!isHandledWaitingSession(session)) {
      return session;
    }

    return {
      ...session,
      status: "idle" as const,
      needs_attention: false,
      subtitle: "Idle",
      prompt_text: null
    };
  });
  const summary = buildSummary(sessions, isHandledWaitingSession);
  const latestSession = latestSessionByActivity(payload.sessions);
  const previousActiveCountRef = useRef(summary.waiting + summary.running);
  const previousAutoExpandTriggerRef = useRef(false);
  const suppressAutoExpandUntilResetRef = useRef(false);
  const suppressHoverUntilLeaveRef = useRef(false);
  const previousStatusBySessionIdRef = useRef<Record<string, SessionStatus | undefined>>({});
  const hasSessions = sessions.length > 0;
  const selectedSession = sessions.find((session) => session.session_id === selectedSessionId) ?? null;
  const selectedSessionHandled = selectedSession ? isHandledWaitingSession(selectedSession) : false;
  const selectedSessionDisplayStatus = selectedSession
    ? displayStatusForSession(selectedSession, isHandledWaitingSession)
    : null;
  const selectedSessionConversationHistory = selectedSession
    ? detailConversationHistory(selectedSession, 5)
    : [];
  const expanded = hoverExpanded || pinnedExpanded;
  const collapsedStatus = collapsedStatusLabel(summary, completedSticky);
  const collapsedTone = collapsedVisualState(summary, completedSticky);
  const collapsedIcon = collapsedIconForState(collapsedTone);
  const expandedView = !hasSessions ? "empty" : selectedSession ? "detail" : "list";
  const showHandleButton = selectedSession ? shouldShowHandleButton(selectedSession) && !selectedSessionHandled : false;

  useEffect(() => {
    if (host === "web") {
      return;
    }

    void syncIslandWindowOnHost(host, {
      expanded,
      expandedView,
      sessionCount: sessions.length
    });
  }, [expanded, expandedView, host, sessions.length]);

  useEffect(() => {
    if (host === "tauri") {
      let mounted = true;
      let unlisten: (() => void) | undefined;

      import("@tauri-apps/api/event").then(({ listen }) =>
        listen<boolean>("island:hover", (event) => {
          if (!mounted) {
            return;
          }

          if (suppressHoverUntilLeaveRef.current) {
            if (!event.payload) {
              suppressHoverUntilLeaveRef.current = false;
              setHoverExpanded(false);
            }
            return;
          }

          setHoverExpanded(event.payload);
        })
      ).then((dispose) => {
        unlisten = dispose;
      });

      return () => {
        mounted = false;
        unlisten?.();
      };
    }

    if (host === "native") {
      const onHoverChanged = (event: Event) => {
        const detail = (event as CustomEvent<boolean>).detail;
        if (suppressHoverUntilLeaveRef.current) {
          if (!detail) {
            suppressHoverUntilLeaveRef.current = false;
            setHoverExpanded(false);
          }
          return;
        }
        setHoverExpanded(Boolean(detail));
      };

      window.addEventListener("codex-island:hover", onHoverChanged as EventListener);
      return () => {
        window.removeEventListener("codex-island:hover", onHoverChanged as EventListener);
      };
    }
  }, [host]);

  useEffect(() => {
    if (shouldCollapsePinnedPanel(hoverExpanded, pinnedExpanded, selectedSessionId)) {
      suppressAutoExpandUntilResetRef.current = true;
      setPinnedExpanded(false);
    }
  }, [hoverExpanded, pinnedExpanded, selectedSessionId]);

  useEffect(() => {
    if (!expanded) {
      setSelectedSessionId(null);
    }
  }, [expanded]);

  useEffect(() => {
    if (expanded) {
      setCompletedSticky(false);
    }
  }, [expanded]);

  useEffect(() => {
    const activeCount = summary.waiting + summary.running;
    const previousActiveCount = previousActiveCountRef.current;

    if (activeCount > 0) {
      setCompletedSticky(false);
    } else if (previousActiveCount > 0 && !expanded) {
      setCompletedSticky(true);
    }

    previousActiveCountRef.current = activeCount;
  }, [expanded, summary.running, summary.waiting]);

  useEffect(() => {
    const shouldAutoExpand = summary.waiting > 0 || completedSticky;
    if (!shouldAutoExpand) {
      suppressAutoExpandUntilResetRef.current = false;
    }
    if (
      shouldAutoExpand &&
      !previousAutoExpandTriggerRef.current &&
      !suppressAutoExpandUntilResetRef.current
    ) {
      setPinnedExpanded(true);
    }
    previousAutoExpandTriggerRef.current = shouldAutoExpand;
  }, [completedSticky, summary.waiting]);

  useEffect(() => {
    setHandledWaitingSessions((current) => {
      const next: Record<string, string> = {};

      for (const session of payload.sessions) {
        const handledSignature = current[session.session_id];
        const currentSignature = waitingSessionSignature(session);
        if (
          handledSignature &&
          currentSignature &&
          handledSignature === currentSignature
        ) {
          next[session.session_id] = handledSignature;
        }
      }

      const currentKeys = Object.keys(current);
      const nextKeys = Object.keys(next);
      const unchanged =
        currentKeys.length === nextKeys.length &&
        nextKeys.every((key) => current[key] === next[key]);

      return unchanged ? current : next;
    });
  }, [payload.sessions]);

  useEffect(() => {
    if (selectedSessionId && !selectedSession) {
      setSelectedSessionId(null);
    }
  }, [selectedSession, selectedSessionId]);

  useEffect(() => {
    const previousStatusBySessionId = previousStatusBySessionIdRef.current;
    const targetSession = autoOpenDetailSession(sessions, previousStatusBySessionId);

    const nextStatusBySessionId: Record<string, SessionStatus | undefined> = {};
    for (const session of sessions) {
      nextStatusBySessionId[session.session_id] = session.status;
    }
    previousStatusBySessionIdRef.current = nextStatusBySessionId;

    if (!targetSession) {
      return;
    }

    setPinnedExpanded(true);
    setSelectedSessionId(targetSession.session_id);
  }, [sessions]);

  const collapsePanel = () => {
    suppressAutoExpandUntilResetRef.current = true;
    suppressHoverUntilLeaveRef.current = true;
    setPinnedExpanded(false);
    setHoverExpanded(false);
    setSelectedSessionId(null);
  };

  return (
    <main className={`shell ${expanded ? "shell--expanded" : ""}`}>
      <section
        className={`island island--${collapsedTone}`}
      >
        {!expanded ? (
          <button
            className="island__capsule"
            type="button"
            onClick={() => setPinnedExpanded(true)}
          >
            <img
              className="capsule__state-icon"
              src={collapsedIcon}
              alt={collapsedStatus}
            />
            <strong className="capsule__status">{collapsedStatus}</strong>
          </button>
        ) : null}

        <div
          className={`panel ${expanded ? "panel--visible" : ""} ${!hasSessions ? "panel--empty" : ""
            } panel--${expandedView}`}
        >
          <button
            className="panel__collapse"
            type="button"
            aria-label="Collapse island panel"
            onClick={collapsePanel}
          >
            ✕
          </button>
          {hasSessions ? (
            selectedSession ? (
              <div className="detail-panel">
                <div className="detail-panel__header">
                  <button
                    className="detail-panel__back"
                    type="button"
                    onClick={() => setSelectedSessionId(null)}
                  >
                    Back
                  </button>
                  <span className={`status-pill status-pill--${selectedSessionDisplayStatus ?? "idle"}`}>
                    {statusLabel(selectedSessionDisplayStatus ?? "idle")}
                  </span>
                </div>

                <div className="detail-panel__body">
                  {selectedSessionConversationHistory.length > 0 ? (
                    <ul className="detail-panel__history">
                      {selectedSessionConversationHistory.map((entry, index) => (
                        <li className="detail-panel__history-item" key={`${entry}-${index}`}>
                          <span className="detail-panel__history-text">{entry}</span>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="detail-panel__prompt">No conversation yet</p>
                  )}
                </div>

                {showHandleButton ? (
                  <div>
                    <button
                      className="detail-panel__action"
                      type="button"
                      onClick={() => {
                        const signature = waitingSessionSignature(selectedSession);
                        if (!signature) {
                          return;
                        }
                        setHandledWaitingSessions((current) => ({
                          ...current,
                          [selectedSession.session_id]: signature
                        }));
                        void focusSessionOnHost(host, selectedSession.session_id);
                      }}
                    >
                      处理
                    </button>
                  </div>
                ) : null}
              </div>
            ) : (
              <div className="session-list">
                {sessions.map((session) => (
                  <div
                    className={`session-row ${session.needs_attention ? "session-row--attention" : ""
                      }`}
                    key={session.session_id}
                  >
                    <span className={`notch-dot notch-dot--${listDotToneForStatus(session.status)} session-row__status-dot`} />
                    <div className="session-row__main">
                      <strong className="session-row__project">{session.project_name}</strong>
                      <p className="session-row__process">{session.terminal_label}</p>
                    </div>
                    <div className="session-row__meta">
                      <button
                        className="session-row__icon-action"
                        type="button"
                        aria-label="Open session details"
                        onClick={() => setSelectedSessionId(session.session_id)}
                      >
                        <SessionChatIcon />
                      </button>
                      <button
                        className="session-row__icon-action"
                        type="button"
                        aria-label="Open project folder"
                        onClick={() => {
                          void openSessionProjectOnHost(host, session.session_id);
                        }}
                      >
                        <SessionCodeIcon />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )
          ) : (
            <div className="empty-state">
              <strong>No sessions</strong>
              <p>Run codex in terminal</p>
            </div>
          )}
        </div>
      </section>
    </main>
  );
}
