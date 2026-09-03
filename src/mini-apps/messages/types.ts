export interface OllamaStatus {
  connected: boolean;
  version: string | null;
  url: string;
  /**
   * Whether this provider can pull and delete models on the user's behalf.
   *
   * True for Ollama, which hosts the weights. False behind an
   * OpenAI-compatible API, where the catalogue is the server's business — the
   * pull field has to be hidden rather than offered and left to fail.
   *
   * Optional because a status object built locally as a placeholder, before
   * the backend has answered, has nothing to say about it.
   */
  supports_model_management?: boolean;
}

export interface ModelInfo {
  name: string;
  model: string;
  size: number;
  digest: string;
  modified_at: string;
  details?: {
    format?: string;
    family?: string;
    parameter_size?: string;
    quantization_level?: string;
  };
}

export interface SynToolCallEvent {
  conversation_id: string;
  tool_name: string;
  tool_args: Record<string, unknown>;
  result_preview: string;
  iteration: number;
}

export interface SynMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  model?: string;
  timestamp: string;
  tokens?: number;
  duration_ms?: number;
  sources?: SourceRef[];  // Source references from RAG
  tool_calls_log?: SynToolCallEvent[];
  images?: string[];  // base64 encoded
  notification?: any; // The raw chat notification
}

export interface SourceRef {
  id: string;
  title: string;
  node_type: string;
}

export interface SynConversation {
  id: string;
  title: string;
  model?: string;
  message_count: number;
  created_at: string;
  updated_at: string;
  pinned: boolean;
}

export interface SynConversationFull {
  meta: SynConversation;
  messages: SynMessage[];
}

export interface SynChatRequest {
  conversation_id: string;
  message: string;
  model?: string;
  temperature?: number;
  images?: string[];  // base64 encoded
}

export interface SynStreamToken {
  conversation_id: string;
  message_id: string;
  token: string;
  done: boolean;
}

export type { SynSettings } from './composables/useSynSettings';


// ─── Runs ────────────────────────────────────────────────────
//
// A run is one piece of work, from the sentence that asked for it to whatever
// came out. It is written into `{vault}/Syn/runs/` as it happens, which is what
// makes it readable after the app has been closed. See `src-tauri/src/syn/run.rs`.

/** Where a run got to. `working` means this app is driving it right now. */
export type RunState =
  | 'working'
  | 'done'
  | 'failed'
  | 'cancelled'
  | 'budget_exhausted'
  /** Found as `working` by a process that was not driving it — the app was closed mid-run. */
  | 'interrupted';

export type RunTrigger = 'user';

export type StepKind = 'assistant' | 'tool_call' | 'note';

/** What it would take to undo a step. */
export type Reversal =
  | { kind: 'nothing' }
  | { kind: 'automatic'; how: string }
  | { kind: 'manual'; how: string }
  | { kind: 'irreversible' };

export interface RunStep {
  index: number;
  kind: StepKind;
  iteration: number;
  tool?: string;
  args?: Record<string, unknown>;
  ok?: boolean;
  reversal?: Reversal;
  preview: string;
  tokens?: number;
  ms: number;
  at: string;
}

/** Ceilings for one run. `null` is no ceiling of that kind, not a ceiling of zero. */
export interface Budget {
  iterations: number | null;
  tool_calls: number | null;
  tokens: number | null;
  wall_ms: number | null;
}

export interface Spent {
  iterations: number;
  tool_calls: number;
  tokens: number;
  wall_ms: number;
}

export interface Run {
  id: string;
  conversation_id?: string | null;
  goal: string;
  trigger: RunTrigger;
  state: RunState;
  model?: string | null;
  provider?: string | null;
  budget: Budget;
  spent: Spent;
  steps: RunStep[];
  error?: string | null;
  created_at: string;
  updated_at: string;
}

/** A run as a list needs it: everything except the transcript. */
export interface RunSummary {
  id: string;
  conversation_id?: string | null;
  goal: string;
  trigger: RunTrigger;
  state: RunState;
  model?: string | null;
  step_count: number;
  tool_calls: number;
  created_at: string;
  updated_at: string;
}

// ─── What Syn is actually told ───────────────────────────────

export type PromptSectionKind =
  | 'custom'
  | 'identity'
  | 'personality'
  | 'rules'
  | 'today'
  | 'tool_shape'
  | 'vault_context';

export interface PromptSectionCost {
  kind: PromptSectionKind;
  label: string;
  chars: number;
  /** Characters divided by four. An estimate, and shown as one. */
  est_tokens: number;
  /** True when the section was left out to stay inside the budget. */
  dropped: boolean;
}

export interface PromptPreview {
  text: string;
  chars: number;
  est_tokens: number;
  budget_chars: number;
  sections: PromptSectionCost[];
}
