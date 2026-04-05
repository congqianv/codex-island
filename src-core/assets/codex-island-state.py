#!/usr/bin/env python3
import json
import os
import socket
import subprocess
import sys
from datetime import datetime, timezone

SOCKET_PATH = "/tmp/codex-island.sock"
CACHE_PATH = os.path.expanduser("~/.codex/hooks/codex-island-events.jsonl")


def now_ms():
    return int(datetime.now(timezone.utc).timestamp() * 1000)


def append_cache(record):
    try:
        os.makedirs(os.path.dirname(CACHE_PATH), exist_ok=True)
        with open(CACHE_PATH, "a", encoding="utf-8") as handle:
            handle.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError:
        pass


def detect_tty():
    parent_pid = os.getppid()
    try:
        result = subprocess.run(
            ["ps", "-p", str(parent_pid), "-o", "tty="],
            capture_output=True,
            text=True,
            timeout=1,
        )
        tty = result.stdout.strip()
        if tty and tty not in {"??", "-"}:
            return tty if tty.startswith("/dev/") else f"/dev/{tty}"
    except Exception:
        pass

    for candidate in (sys.stdin, sys.stdout, sys.stderr):
        try:
            return os.ttyname(candidate.fileno())
        except OSError:
            continue
    return None


def normalize(payload):
    event = payload.get("hook_event_name")
    state = {
        "provider": "codex",
        "session_id": payload.get("session_id"),
        "cwd": payload.get("cwd"),
        "transcript_path": payload.get("transcript_path"),
        "event": event,
        "timestamp": now_ms(),
        "pid": os.getppid(),
        "tty": detect_tty(),
        "terminal_name": os.environ.get("TERM_PROGRAM") or os.environ.get("TERM"),
    }

    if event == "SessionStart":
        state["status"] = "processing"
    elif event == "Stop":
        state["status"] = "completed"
        if payload.get("last_assistant_message"):
            state["prompt"] = payload.get("last_assistant_message")
    elif event == "UserPromptSubmit":
        state["status"] = "processing"
        if payload.get("prompt"):
            state["user_prompt"] = payload.get("prompt")
    elif event == "PreToolUse":
        state["status"] = "running_tool"
        state["tool"] = payload.get("tool_name")
    elif event == "PostToolUse":
        state["status"] = "processing"
        state["tool"] = payload.get("tool_name")
    else:
        state["status"] = "notification"

    return state


def send_socket(record):
    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(1)
        sock.connect(SOCKET_PATH)
        sock.sendall(json.dumps(record).encode("utf-8"))
        sock.close()
    except OSError:
        pass


def main():
    try:
        payload = json.load(sys.stdin)
    except json.JSONDecodeError:
        return 1

    record = normalize(payload)
    append_cache(record)
    send_socket(record)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
