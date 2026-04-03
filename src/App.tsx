import { useDeferredValue, useEffect, useRef, useState } from "react";

import {
  focusSessionOnHost,
  syncIslandWindowOnHost
} from "./hostBridge";
import {
  collapsedStatusLabel,
  collapsedVisualState,
  detailPromptText,
  latestSessionByActivity,
  shouldShowHandleButton,
  sortSessions,
  statusLabel
} from "./sessionPresentation";
import { useSessions } from "./useSessions";

export function App() {
  const { payload, host } = useSessions();
  const [hoverExpanded, setHoverExpanded] = useState(false);
  const [pinnedExpanded, setPinnedExpanded] = useState(false);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [completedSticky, setCompletedSticky] = useState(false);
  const deferredSessions = useDeferredValue(payload.sessions);
  const sessions = sortSessions(deferredSessions);
  const latestSession = latestSessionByActivity(payload.sessions);
  const previousActiveCountRef = useRef(payload.summary.waiting + payload.summary.running);
  const hasSessions = sessions.length > 0;
  const selectedSession = sessions.find((session) => session.session_id === selectedSessionId) ?? null;
  const expanded = hoverExpanded || pinnedExpanded;
  const collapsedStatus = collapsedStatusLabel(payload.summary, completedSticky);
  const collapsedTone = collapsedVisualState(payload.summary, completedSticky);
  const expandedView = !hasSessions ? "empty" : selectedSession ? "detail" : "list";
  const showHandleButton = selectedSession ? shouldShowHandleButton(selectedSession) : false;

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
        setHoverExpanded(Boolean(detail));
      };

      window.addEventListener("codex-island:hover", onHoverChanged as EventListener);
      return () => {
        window.removeEventListener("codex-island:hover", onHoverChanged as EventListener);
      };
    }
  }, [host]);

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
    const activeCount = payload.summary.waiting + payload.summary.running;
    const previousActiveCount = previousActiveCountRef.current;

    if (activeCount > 0) {
      setCompletedSticky(false);
    } else if (previousActiveCount > 0 && !expanded) {
      setCompletedSticky(true);
    }

    previousActiveCountRef.current = activeCount;
  }, [expanded, payload.summary.running, payload.summary.waiting]);

  useEffect(() => {
    if (selectedSessionId && !selectedSession) {
      setSelectedSessionId(null);
    }
  }, [selectedSession, selectedSessionId]);

  return (
    <main className={`shell ${expanded ? "shell--expanded" : ""}`}>
      <section
        className={`island island--${collapsedTone}`}
      >
        {!expanded ? (
          <button
            className="island__capsule"
            type="button"
            onClick={() => setPinnedExpanded((current) => !current)}
          >
            <span className={`notch-dot notch-dot--${collapsedTone}`} />
            <strong className="capsule__status">{collapsedStatus}</strong>
          </button>
        ) : null}

        <div
          className={`panel ${expanded ? "panel--visible" : ""} ${!hasSessions ? "panel--empty" : ""
            }`}
        >
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
                  <span className={`status-pill status-pill--${selectedSession.status}`}>
                    {statusLabel(selectedSession.status)}
                  </span>
                </div>

                <div className="detail-panel__body">
                  <p className="detail-panel__prompt">
                    {detailPromptText(selectedSession)}
                  </p>
                </div>

                {showHandleButton ? (
                  <div>
                    <button
                      className="detail-panel__action"
                      type="button"
                      onClick={() => {
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
                  <button
                    className={`session-row ${session.needs_attention ? "session-row--attention" : ""
                      }`}
                    key={session.session_id}
                    type="button"
                    onClick={() => setSelectedSessionId(session.session_id)}
                  >
                    <strong className="session-row__project">{session.project_name}</strong>
                    <div className="session-row__meta">
                      <span className="session-row__app">{session.terminal_label}</span>
                      <span className="session-row__time">{session.relative_last_activity}</span>
                    </div>
                  </button>
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
