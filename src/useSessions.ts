import { startTransition, useEffect, useState } from "react";

import { mockSessions } from "./mockSessions";
import { detectHost, getSessionsFromHost, subscribeSessions } from "./hostBridge";
import type { SessionsPayload } from "./types";

const emptySessions: SessionsPayload = {
  sessions: [],
  summary: {
    total: 0,
    running: 0,
    waiting: 0,
    completed: 0
  }
};

export function useSessions() {
  const host = detectHost();
  const [payload, setPayload] = useState<SessionsPayload>(() =>
    host === "web" ? mockSessions : emptySessions
  );

  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | undefined = () => undefined;

    void getSessionsFromHost(host).then((nextPayload) => {
      if (mounted) {
        startTransition(() => setPayload(nextPayload));
      }
    });

    void subscribeSessions(host, (nextPayload) => {
      if (!mounted) {
        return;
      }

      startTransition(() => setPayload(nextPayload));
    }).then((dispose) => {
      unlisten = dispose;
    });

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, [host]);

  return {
    payload,
    host,
    isTauri: host === "tauri",
    isNativeHost: host === "native"
  };
}
