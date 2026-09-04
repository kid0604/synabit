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

// ─── Memory ──────────────────────────────────────────────────
//
// What Syn remembers between conversations. Stored as ordinary nodes under
// `Memory/`, which is why editing one goes through `useNodeService` rather
// than a command of its own — see `src-tauri/src/syn/memory.rs`.

export interface Memory {
  /** Vault-relative path, which is also how every node tool addresses it. */
  id: string;
  title: string;
  body: string;
  /** `fact`, `preference`, `instruction`, `relationship`, `project`, or one the user invented. */
  kind: string;
  subject?: string | null;
  /** 0 to 1. A sort order and a reason to ask again, not a probability. */
  confidence: number;
  source_run?: string | null;
  source_nodes: string[];
  first_seen: string;
  last_confirmed: string;
  review_after?: string | null;
  /**
   * Every memory rides in every prompt. Pinning decides only who survives if
   * there is ever more than the budget holds.
   */
  pinned: boolean;
  supersedes?: string | null;
}

/** What the pinned memories cost against what they are allowed. */
export interface MemoryBudget {
  /** Everything remembered. All of it is sent, up to the budget. */
  total: number;
  /** How many of those are pinned, which now decides only who survives a cut. */
  pinned: number;
  chars: number;
  budget_chars: number;
  /** Memories that do not fit, and so are not reaching the model. */
  dropped: number;
}

/**
 * Something Syn worked out and would like to remember, waiting to be allowed.
 *
 * Not a memory and not a node: it lives in `Syn/proposals.json` until it is
 * accepted, so declining one leaves nothing behind in the vault.
 */
export interface Proposal {
  id: string;
  body: string;
  kind: string;
  subject?: string | null;
  confidence: number;
  /** The evidence, which is what makes the tray reviewable rather than a coin toss. */
  because: string;
  source_run: string;
  conversation_id?: string | null;
  /** The exact text of the memory this replaces, when it replaces one. */
  supersedes?: string | null;
  /** Whether this came out of the user correcting Syn — the strongest evidence there is. */
  from_correction: boolean;
  proposed_at: string;
}
