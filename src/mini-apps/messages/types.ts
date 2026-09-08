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
  /**
   * What this answer stood on. Absent on user messages, and on every assistant
   * message written before this existed — "nobody measured it" and "it was a
   * guess" are different claims.
   */
  footing?: Footing;
  tool_calls_log?: SynToolCallEvent[];
  images?: string[];  // base64 encoded
  notification?: any; // The raw chat notification
}

/**
 * The `node_type` a web citation carries. Mirrors `syn::web::WEB_SOURCE_TYPE`.
 *
 * Not a real node type — nothing in the vault has it. It exists so a source
 * chip can tell "open my note" from "open that page in your browser", which
 * are different acts behind the same-looking control.
 */
export const WEB_SOURCE = 'web';

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
  /**
   * The run that stopped for permission, now that it has an answer.
   *
   * `message` is empty then: nobody typed anything, and the question still on
   * the table is the last one they did type. The run's **id** rather than a
   * flag, because the run holds the call it was about to make — without it the
   * resumed run works the turn out again, and the permission just granted for
   * one page is spent on something else. See `syn_answer_consent`.
   */
  resume_run?: string;
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
  /** Stopped to ask permission. The work is unfinished, nothing went wrong,
   *  and the next move belongs to the user. */
  | 'awaiting_consent'
  /**
   * Stopped because the model was about to pick one of several, and which one
   * is the user's to say.
   *
   * Not the same as `awaiting_consent`, and the difference matters on screen:
   * consent means a permission was never granted, this means the work is going
   * fine and Syn refuses to guess one detail. See `syn::ambiguity`.
   */
  | 'awaiting_choice'
  /** Found as `working` by a process that was not driving it — the app was closed mid-run. */
  | 'interrupted';

export type RunTrigger = 'user';

/**
 * How heavy a turn was judged to be, before it ran. Mirrors `syn::tempo::Tempo`.
 *
 * `instant` means the question was recognised as a count, the count was run
 * before the model was asked, and the turn has no tools — one round trip rather
 * than two. Everything else is `working`.
 */
export type Tempo = 'instant' | 'working';

/**
 * What an answer turned out to be standing on. Mirrors `syn::footing::Footing`.
 *
 * Decided by arithmetic on the run's transcript, never by asking the model:
 *
 * * `grounded` — a tool that only looks came back, or this app counted it from
 *   the index. There is a source, and it can be looked at again. It does *not*
 *   claim the answer is right, only that it was not invented. A tool that
 *   *wrote* something does not count: saving a note is not reading one.
 * * `inferred` — retrieval put material in front of the model and nothing was
 *   looked up. The state where an answer sounds sourced and is not.
 * * `guessing` — nothing from the vault held it up at all.
 */
export type Footing = 'grounded' | 'inferred' | 'guessing';

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

/**
 * Every section the prompt can be made of.
 *
 * Kept in the order `SectionKind` declares them, which is the order they are
 * rendered in. This list had already fallen two behind — `memory` and `skills`
 * shipped without reaching it, so the panel that exists to say what Syn is told
 * had no name for two of the things it was told.
 *
 * `the_frontend_knows_every_section_the_prompt_can_have` in `syn/prompt.rs`
 * reads this file and fails when the two drift again.
 */
export type PromptSectionKind =
  | 'custom'
  | 'identity'
  | 'rules'
  | 'today'
  | 'focus'
  | 'counted'
  | 'thread'
  | 'tool_shape'
  | 'memory'
  | 'skills'
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
  /**
   * What the tool declarations cost — not part of the prompt text.
   *
   * They are the `tools` field of the request, so nothing on this screen used
   * to mention them, while they were the single largest thing a turn spends:
   * more than the whole fixed prompt. Mirrors `syn::prompt::ToolPayload`.
   */
  tools: ToolPayload;
}

export interface ToolPayload {
  count: number;
  chars: number;
  est_tokens: number;
  budget_chars: number;
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
/**
 * The kind of power a tool has, as `consent.rs` names it.
 *
 * The vault arms never ask. The rest do, and the card is keyed on which — so
 * the sentence a person reads comes from i18n rather than from a string
 * composed in Rust, which would be one language for a bilingual app.
 */
export type Capability =
  | 'VaultRead'
  | 'VaultWrite'
  | 'VaultStructural'
  | { NetRead: { domain: string } }
  // Not a `NetRead` with an empty host, which is what it used to be and what
  // made the card read "Syn wants to read from ." One capability for the whole
  // tool: the host is unknown when the question is put, and already read by the
  // time it is known.
  | 'Browse'
  | { NetWrite: { domain: string; tool: string } }
  | { Spend: { cents_estimate: number } }
  | 'Execute';

/**
 * One tool, as somebody deciding whether to trust Syn would read it.
 *
 * Mirrors `syn::registry::ToolCard`. `capability` is optional because a tool
 * the registry cannot classify is a bug the screen should show in amber rather
 * than hide behind a default — a Rust test makes it impossible today.
 */
export interface ToolCard {
  name: string;
  /** The description the model is given, verbatim — not a kinder paraphrase. */
  description: string;
  capability?: Capability | null;
  reversal?: Reversal | null;
}

/**
 * One thing a query found and the user could have meant.
 *
 * Mirrors `syn::ambiguity::Candidate`.
 */
export interface Candidate {
  id: string;
  title: string;
  node_type?: string;
}

/**
 * *Which one?* — asked by the engine, never by the model.
 *
 * A query returned several and the next destructive call named one of them.
 * Mirrors `syn::ambiguity::Choice`. `chose` is what the model was about to do,
 * shown as the pre-selected answer: it is usually right, and a question that
 * makes somebody redo the work from scratch is one they stop answering.
 */
export interface AmbiguousChoice {
  tool: string;
  candidates: Candidate[];
  chose: string;
  asked_at: string;
}

/** A question a run stopped on. */
export interface ConsentAsk {
  tool: string;
  capability: Capability;
  /** The capability in one English sentence — for the log, not for the card. */
  about: string;
  /** Whether "always" is on offer. False for money and for running code. */
  can_be_remembered: boolean;
  asked_at: string;
}

/** What the user said. */
export type ConsentAnswer = 'once' | 'always' | 'never';

/** One decision, written down. */
export interface Grant {
  scope: string;
  about: string;
  answer: ConsentAnswer;
  granted_at: string;
  /** Absent for a refusal, which does not expire. */
  expires_at?: string | null;
}

/** One line in the audit log. */
export interface AuditEntry {
  at: string;
  run_id: string;
  tool: string;
  about: string;
  outcome: 'allowed' | 'asked' | 'refused' | 'done' | 'failed';
  reversal?: string | null;
}

/**
 * A procedure written down for Syn to follow.
 *
 * Mirrors `Skill` in `src-tauri/src/syn/skill.rs`. A skill is an ordinary vault
 * node, so everything here is frontmatter a person can edit in any editor —
 * which is the point, because a skill changes what the assistant does.
 */
export interface Skill {
  /** Vault-relative path, which is also how every node tool addresses it. */
  id: string;
  title: string;
  /** The handle `load_skill` takes. */
  name: string;
  description: string;
  when_to_use: string;
  /** `prose` runs in the prompt, `recipe` in Rust, `code` in a sandbox. */
  tier: 'prose' | 'recipe' | 'code';
  /** What it expects to call. Shown to the user, not enforced — that is P4. */
  tools: string[];
  version: number;
  /** `user` or `syn`. Read differently, and shown differently. */
  author: string;
  /** Disabled skills are not named to the model at all. */
  enabled: boolean;
  /** The run that prompted this skill, when Syn wrote it. */
  source_run?: string | null;
  /**
   * When this skill was last answered a question both ways, as `YYYY-MM-DD`.
   *
   * A skill Syn wrote cannot be turned on until this is set. Somebody who has
   * not seen a procedure run is being asked to trust it on its own summary.
   */
  trial_at?: string | null;
  /**
   * A revision Syn is proposing, waiting on you. Not applied.
   *
   * The skill is enabled — that is why it ran and why it went wrong — so
   * rewriting the body would change behaviour the moment it was written.
   */
  pending_revision?: string | null;
  /** What went wrong that prompted it, in Syn's words. */
  revision_because?: string | null;
  /** The steps, in Markdown. */
  body: string;
}

/** One question, answered with a skill and without it. */
export interface SkillTrial {
  question: string;
  without: string;
  with: string;
}

/**
 * How often a skill has actually been opened.
 *
 * The number this feature answers for: a skill can be enabled, indexed, well
 * written and never once reached for, and nothing else would say so.
 */
export interface SkillUsage {
  name: string;
  /** Across the runs still on disk. Runs are pruned, so this is "recently". */
  runs: number;
  last_run: string;
  last_at: string;
}

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
