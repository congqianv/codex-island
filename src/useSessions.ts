import { startTransition, useEffect, useState } from "react";

import { mockSessions } from "./mockSessions";
import { detectHost, getSessionsFromHost, subscribeSessions } from "./hostBridge";
import { mergeNativeSessionsPayload } from "./sessionPayloadMerge";
import type { SessionsPayload } from "./types";

const emptySessions: SessionsPayload = {
  sessions: [],
  summary: {
    total: 0,
    running: 0,
    idle: 0,
    waiting: 0,
    discovering: 0,
    failed: 0,
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
        startTransition(() =>
          setPayload((current) => mergeNativeSessionsPayload(current, nextPayload))
        );
      }
    });

    void subscribeSessions(host, (nextPayload) => {
      if (!mounted) {
        return;
      }

      startTransition(() =>
        setPayload((current) => mergeNativeSessionsPayload(current, nextPayload))
      );
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
