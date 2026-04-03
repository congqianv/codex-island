import { mockSessions } from "./mockSessions";
import type { SessionsPayload } from "./types";

type SessionsListener = (payload: SessionsPayload) => void;

interface NativeHostBridge {
  getSessions?: () => Promise<SessionsPayload>;
  syncIslandWindow?: (payload: {
    expanded: boolean;
    expandedView: string;
    sessionCount: number;
  }) => Promise<void>;
  focusSession?: (sessionId: string) => Promise<void>;
  submitSessionReply?: (sessionId: string, reply: string) => Promise<void>;
  listenSessionsUpdated?: (listener: SessionsListener) => (() => void) | void;
}

declare global {
  interface Window {
    __CODEX_ISLAND_NATIVE__?: NativeHostBridge;
    __TAURI_INTERNALS__?: unknown;
  }
}

export type HostKind = "tauri" | "native" | "web";

export function detectHost(): HostKind {
  if ("__TAURI_INTERNALS__" in window) {
    return "tauri";
  }

  if ("__CODEX_ISLAND_NATIVE__" in window) {
    return "native";
  }

  return "web";
}

export async function getSessionsFromHost(host: HostKind): Promise<SessionsPayload> {
  if (host === "tauri") {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<SessionsPayload>("get_sessions");
  }

  if (host === "native") {
    const bridge = window.__CODEX_ISLAND_NATIVE__;
    if (!bridge?.getSessions) {
      throw new Error("Native host bridge is missing getSessions");
    }
    return bridge.getSessions();
  }

  return mockSessions;
}

export async function syncIslandWindowOnHost(
  host: HostKind,
  payload: { expanded: boolean; expandedView: string; sessionCount: number }
) {
  if (host === "tauri") {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("sync_island_window", payload);
    return;
  }

  if (host === "native") {
    const bridge = window.__CODEX_ISLAND_NATIVE__;
    if (!bridge?.syncIslandWindow) {
      throw new Error("Native host bridge is missing syncIslandWindow");
    }
    await bridge.syncIslandWindow(payload);
  }
}

export async function focusSessionOnHost(host: HostKind, sessionId: string) {
  if (host === "tauri") {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("focus_session", { sessionId });
    return;
  }

  if (host === "native") {
    const bridge = window.__CODEX_ISLAND_NATIVE__;
    if (!bridge?.focusSession) {
      throw new Error("Native host bridge is missing focusSession");
    }
    await bridge.focusSession(sessionId);
  }
}

export async function submitSessionReplyOnHost(
  host: HostKind,
  sessionId: string,
  reply: string
) {
  if (host === "tauri") {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("reply_session", { sessionId, reply });
    return;
  }

  if (host === "native") {
    const bridge = window.__CODEX_ISLAND_NATIVE__;
    if (!bridge?.submitSessionReply) {
      throw new Error("Native host bridge is missing submitSessionReply");
    }
    await bridge.submitSessionReply(sessionId, reply);
  }
}

export async function subscribeSessions(
  host: HostKind,
  listener: SessionsListener
): Promise<() => void> {
  if (host === "tauri") {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<SessionsPayload>("sessions:updated", (event) => {
      listener(event.payload);
    });
  }

  if (host === "native") {
    const bridge = window.__CODEX_ISLAND_NATIVE__;
    if (!bridge?.listenSessionsUpdated) {
      throw new Error("Native host bridge is missing listenSessionsUpdated");
    }
    return bridge.listenSessionsUpdated(listener) ?? (() => undefined);
  }

  return () => undefined;
}
