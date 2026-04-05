export type SessionStatus =
  | "discovering"
  | "running"
  | "idle"
  | "waiting_input"
  | "completed"
  | "failed";

export interface SessionActionOption {
  id: string;
  label: string;
  reply: string;
}

export type PromptSource = "thread" | "terminal";
export type SessionIngestionMode = "hooks" | "fallback";

export interface SessionViewModel {
  session_id: string;
  title: string;
  project_name: string;
  status: SessionStatus;
  needs_attention: boolean;
  can_reply: boolean;
  subtitle: string;
  prompt_text: string | null;
  action_options: SessionActionOption[];
  prompt_source: PromptSource | null;
  latest_user_prompt?: string | null;
  status_history?: string[];
  conversation_history?: string[];
  ingestion_mode: SessionIngestionMode;
  terminal_label: string;
  relative_last_activity: string;
  last_activity_unix_ms: number;
}

export interface SessionSummary {
  total: number;
  running: number;
  idle: number;
  waiting: number;
  discovering: number;
  failed: number;
  completed: number;
}

export interface SessionsPayload {
  sessions: SessionViewModel[];
  summary: SessionSummary;
}
