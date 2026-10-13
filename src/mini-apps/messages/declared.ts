/**
 * Declared connectors, as the settings screen reads them.
 *
 * The shapes mirror `syn::connector::{manifest, instance}` and
 * `commands::connector::DeclaredView` in Rust. Everything the approval card
 * shows comes from these — from the declaration, as Rust read it — and never
 * from anything a model wrote in a message. See
 * `docs/syn-connectors-2026-10-08.md` §4.2.
 */

export type Effect = 'read' | 'read_attested' | 'write' | 'irreversible';

export interface DeclaredParam {
  type: string;
  description: string;
  required: boolean;
  enum?: string[];
}

export interface DeclaredOperation {
  name: string;
  description: string;
  effect: Effect;
  /** Absent on a device connector's operation, which has a bridge instead. */
  call?: { method: string; path: string; query?: Record<string, string> };
  /** The device bridge it reads. See `connector::device`. */
  bridge?: string | null;
  filter?: Record<string, Array<string | number>>;
  params?: Record<string, DeclaredParam>;
  role?: string | null;
  language?: string | null;
}

export type DeclaredAuth =
  | { type: 'none' }
  | { type: 'bearer'; secret: string }
  | { type: 'header'; name: string; secret: string }
  | { type: 'query'; name: string; secret: string }
  | { type: 'basic'; user: string; secret: string };

/** Text in the person's language: one string, or one per language code. */
export type Words = string | Record<string, string>;

export function inLang(w: Words | undefined, lang: string): string {
  if (!w) return '';
  if (typeof w === 'string') return w;
  return w[lang] ?? w.en ?? Object.values(w)[0] ?? '';
}

/** Something only the person knows, asked on the connect card. */
export interface DeclaredInput {
  name: string;
  kind: 'account' | 'secret' | 'address';
  label: Words;
  example?: string | null;
  format?: string | null;
  guide?: { link?: string | null; steps?: Record<string, string[]> } | null;
}

/** What pressing "Connect" came to. See `syn_connect` in Rust. */
export interface Connected {
  ok: boolean;
  code?: string;
  field?: string;
  rows?: number;
  cert?: string;
  said: string;
}

export interface Manifest {
  id: string;
  name: string;
  version: number;
  provenance: 'user' | 'bundled' | 'syn';
  transport?: 'http' | 'device' | string;
  base_url: string;
  auth: DeclaredAuth;
  operations: DeclaredOperation[];
  inputs?: DeclaredInput[];
  /** Reads Rust keeps in the vault on a schedule. See `connector::mirror`. */
  mirrors?: DeclaredMirror[];
}

export interface DeclaredMirror {
  name: string;
  from: string;
  every_minutes: number;
  key: string;
  into: string;
  fields?: Record<string, string>;
  window?: { past_days: number; future_days: number } | null;
}

/** How a mirror last went on this device. */
export interface MirrorRan {
  at: number;
  ok: boolean;
  said?: string;
  rows: number;
  written: number;
  trashed: number;
  staged: number;
}

export type Zone = { kind: 'personal' } | { kind: 'work'; org: string };
export type Exposure = 'full' | 'aggregates';
export type Network = 'public' | 'internal';

export interface Choices {
  label: string;
  zone: Zone;
  exposure: Exposure;
  network: Network;
  pinned_cert: string | null;
  attested: string[];
  /** The declaration's mirrors this device runs. */
  mirrors: string[];
  /** For `folder.watch`: the folder chosen on this device. */
  folder?: string | null;
  /** The account and address inputs, as typed on this device. */
  inputs?: Record<string, string>;
}

export interface Instance extends Omit<Choices, 'attested' | 'mirrors'> {
  manifest_id: string;
  attested: string[];
  mirrors?: string[];
  approved: string;
  approved_at: string;
}

export interface Problem {
  code: string;
  at: string;
  said: string;
}

export type Standing = { state: 'approved' } | { state: 'new' } | { state: 'changed' } | { state: 'broken' };

export interface DeclaredView {
  node_id: string;
  title: string;
  manifest: Manifest | null;
  problems: Problem[];
  standing: Standing;
  instance: Instance | null;
  effects: Record<string, Effect>;
  secret_here: boolean;
  mirrors_ran?: Record<string, MirrorRan>;
}

/** The secret a declaration needs, by name, or `null`. */
export function secretName(m: Manifest): string | null {
  return m.auth.type === 'none' ? null : m.auth.secret;
}

/** The host a declaration reaches. */
/** The device bridges a declaration reads, when it is a device connector. */
export function bridgesOf(m: Manifest): string[] {
  return [...new Set(m.operations.map((o) => o.bridge).filter((b): b is string => !!b))];
}

/** The apps whose notifications an `android.notifications` connector keeps. */
export function notificationPackages(m: Manifest): string[] {
  return [
    ...new Set(
      m.operations
        .filter((o) => o.bridge === 'android.notifications')
        .flatMap((o) => (o.filter?.package ?? []).map(String)),
    ),
  ];
}

/** Whether any operation sends. */
export function sends(m: Manifest): boolean {
  return m.operations.some((o) => o.effect === 'write' || o.effect === 'irreversible');
}

/** Every input the card asks for, the auth's secret included when undeclared. */
export function inputsOf(m: Manifest): DeclaredInput[] {
  const declared = m.inputs ?? [];
  const secret = secretName(m);
  if (secret && !declared.some((i) => i.name === secret)) {
    return [...declared, { name: secret, kind: 'secret', label: secret.replace(/_/g, ' ') }];
  }
  return declared;
}

export function hostOf(m: Manifest): string {
  try {
    return new URL(m.base_url).host;
  } catch {
    return m.base_url;
  }
}

/**
 * Whether a host looks like this machine or a local network — the card's
 * default for "which network", which the person confirms.
 */
export function looksInternal(m: Manifest): boolean {
  let host = '';
  try {
    host = new URL(m.base_url).hostname.toLowerCase();
  } catch {
    return false;
  }
  if (host === 'localhost' || host.endsWith('.local') || host.endsWith('.internal') || host.endsWith('.localhost')) return true;
  if (/^127\./.test(host) || /^10\./.test(host) || /^192\.168\./.test(host)) return true;
  const m172 = /^172\.(\d+)\./.exec(host);
  if (m172 && Number(m172[1]) >= 16 && Number(m172[1]) <= 31) return true;
  return host === '[::1]' || host.startsWith('[fc') || host.startsWith('[fd');
}

/** What the card starts with: what was chosen before, or the safest defaults. */
export function choicesFrom(view: DeclaredView): Choices {
  if (view.instance) {
    const { label, zone, exposure, network, pinned_cert, attested, mirrors, folder, inputs } = view.instance;
    return { label, zone, exposure, network, pinned_cert, attested: [...attested], mirrors: [...(mirrors ?? [])], folder: folder ?? null, inputs: { ...(inputs ?? {}) } };
  }
  const internal = view.manifest ? looksInternal(view.manifest) : false;
  return {
    label: view.manifest?.name ?? '',
    zone: { kind: 'personal' },
    exposure: 'full',
    network: internal ? 'internal' : 'public',
    pinned_cert: null,
    attested: [],
    mirrors: [],
    folder: null,
  };
}


// ── How a figure was worked out ─────────────────────────────────────────────

export interface WorkedStep {
  kind: 'fetched' | 'crunched';
  /** For `fetched`: the connector or the vault; for `crunched`: the table it read. */
  from: string;
  /** What was asked: the call's arguments, or the pipeline. */
  asked: string;
  /** The table it made, when it made one. */
  handle: string | null;
  rows: number | null;
}

interface ToolCallLike {
  tool_name: string;
  tool_args: Record<string, unknown>;
  result_preview: string;
}

/** `{"handle":"r3","rows":3000…` → `{ handle: 'r3', rows: 3000 }`, read off a preview that may be cut. */
function tableIn(preview: string): { handle: string | null; rows: number | null } {
  const handle = /"handle"\s*:\s*"(r\d+)"/.exec(preview)?.[1] ?? null;
  const counted = /"rows"\s*:\s*(\d+)/.exec(preview)?.[1];
  const listed = counted === undefined ? null : Number(counted);
  return { handle, rows: listed };
}

/**
 * The steps behind the figures in an answer: every table a tool brought back,
 * and every `crunch` worked out over one, in order. Empty when the answer
 * computed nothing — the section is then not shown.
 */
export function howWorkedOut(log: ToolCallLike[] | undefined): WorkedStep[] {
  if (!log?.length) return [];
  const steps: WorkedStep[] = [];
  for (const tc of log) {
    const { handle, rows } = tableIn(tc.result_preview || '');
    if (tc.tool_name === 'crunch') {
      steps.push({
        kind: 'crunched',
        from: String(tc.tool_args.handle ?? ''),
        asked: String(tc.tool_args.pipeline ?? ''),
        handle,
        rows,
      });
    } else if (handle) {
      const rest = tc.tool_name.startsWith('connector__') ? tc.tool_name.slice('connector__'.length) : null;
      const cut = rest ? rest.indexOf('__') : -1;
      steps.push({
        kind: 'fetched',
        from: rest && cut > 0 ? `${rest.slice(0, cut)} · ${rest.slice(cut + 2)}` : tc.tool_name,
        asked: JSON.stringify(tc.tool_args),
        handle,
        rows,
      });
    }
  }
  return steps.some((s) => s.kind === 'crunched') ? steps : [];
}
