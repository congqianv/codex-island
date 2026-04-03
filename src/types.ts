export type SessionStatus =
  | "discovering"
  | "running"
  | "waiting_input"
  | "completed"
  | "failed";

export interface SessionActionOption {
  id: string;
  label: string;
  reply: string;
}

export type PromptSource = "thread" | "terminal";

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
  terminal_label: string;
  relative_last_activity: string;
  last_activity_unix_ms: number;
}

export interface SessionSummary {
  total: number;
  running: number;
  waiting: number;
  completed: number;
}

export interface SessionsPayload {
  sessions: SessionViewModel[];
  summary: SessionSummary;
}
