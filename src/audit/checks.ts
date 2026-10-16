/**
 * The invariants, checked over what is on disk: a vault's notes, Syn's run
 * files, the app's log, and another device's copy of the vault. See
 * `contracts/invariants.json` and `audit.spec.ts`, which runs them.
 *
 * Each check is a function of what it reads, so it can be tested on a few
 * lines and trusted on a few thousand.
 */
import type { Editor } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';
import { lostOnReopen, savedMarkdown, type Losses } from '../mini-apps/note/editor/roundTrip';
import { claimsAction, parseViolation, type Violation } from '../shared/invariants';

// ── note.roundtrip ─────────────────────────────────────────────

// A backslash-escaped character belongs to the address: `\]` and `\)` do not
// end it the way a bare one does.
const ADDRESS = /(?:https?|synabit):\/\/(?:\\.|[^\s<>()"'`\]\\])+/g;
const TRAILING = /[.,;:!?*_]+$/;

/**
 * An address as a reader follows it: percent-escapes and markdown's backslash
 * escapes undone. The editor writes `synabit://person/People/x.md` back as
 * `People%2Fx.md` and a file holds `\_layouts` for `_layouts`; both are the
 * same address.
 */
export function asFollowed(address: string): string {
  const unescaped = address.replace(/\\([!-/:-@[-`{-~])/g, '$1');
  try {
    return decodeURIComponent(unescaped);
  } catch {
    return unescaped;
  }
}

/** Every address written in a note's markdown, as a reader would follow it. */
export function addressesIn(markdown: string): string[] {
  return (markdown.match(ADDRESS) ?? []).map((a) => asFollowed(a.replace(/&amp;/g, '&')).replace(TRAILING, ''));
}

/** Every string a document holds — text, link addresses, node attributes. */
function everythingIn(doc: PMNode): string {
  const parts: string[] = [];
  const attrs = (a: Record<string, unknown>) => {
    for (const v of Object.values(a)) if (typeof v === 'string') parts.push(asFollowed(v));
  };
  doc.descendants((n) => {
    if (n.isText && n.text) parts.push(n.text, asFollowed(n.text));
    attrs(n.attrs);
    for (const m of n.marks) attrs(m.attrs);
    return true;
  });
  return parts.join('\n');
}

export interface NoteFinding {
  /** Addresses in the file that opening it does not keep. */
  lostOnOpen: string[];
  /** What saving it and opening it again would lose. */
  lostOnSave: Losses;
  /** Opened and saved twice, the markdown still changes: every open rewrites it. */
  unstable: boolean;
}

/**
 * One note through the editor as the app opens it: is what the file holds
 * there once opened, does saving lose anything, and does saving settle.
 * `open` puts markdown into `editor` the way the app does, and `save` takes it
 * out the way saving writes the file.
 */
export function auditNote(
  editor: Editor,
  open: (markdown: string) => void,
  body: string,
  save: () => string = () => savedMarkdown(editor),
): NoteFinding {
  open(body);
  const held = everythingIn(editor.state.doc);
  const lostOnOpen = [...new Set(addressesIn(body))].filter((a) => !held.includes(a));

  const first = save();
  const lostOnSave = lostOnReopen(editor, editor.state.doc, first);
  open(first);
  const second = save();
  return { lostOnOpen, lostOnSave, unstable: first !== second };
}

export const noteFails = (f: NoteFinding) => f.lostOnOpen.length > 0 || f.lostOnSave.words > 0 || f.lostOnSave.links > 0;

/** A note's file without its front matter, which the editor never sees. */
export function bodyOf(file: string): string {
  return file.replace(/^---\n[\s\S]*?\n---\n?/, '');
}

// ── syn.claims-match-tools ─────────────────────────────────────

interface RunFile {
  id?: string;
  created_at?: string;
  surface?: string;
  model?: string;
  steps?: { kind?: string; preview?: string; ok?: boolean | null }[];
}

export interface ClaimFinding {
  run: string;
  at: string;
  surface: string;
  model: string;
}

/** A run whose last answer says it did something, with no tool call that went through. */
export function claimWithoutTools(run: RunFile): ClaimFinding | null {
  const steps = run.steps ?? [];
  const answer = [...steps].reverse().find((s) => s.kind === 'assistant')?.preview ?? '';
  const wentThrough = steps.some((s) => s.kind === 'tool_call' && s.ok !== false);
  if (wentThrough || !claimsAction(answer)) return null;
  return { run: run.id ?? '?', at: run.created_at ?? '', surface: run.surface ?? '?', model: run.model ?? '?' };
}

// ── The log ────────────────────────────────────────────────────

const LINE = /^\[(\d{4}-\d{2}-\d{2})\]\[(\d{2}:\d{2}:\d{2})\]\[([^\]]*)\]\[(\w+)\] ?(.*)$/;

export interface LogFindings {
  /** Every `[invariant]` line, as written while the app ran. */
  violations: (Violation & { at: string })[];
  /** Errors grouped by what they say once ids and numbers are taken out. */
  errors: { shape: string; count: number; first: string; last: string; uncaught: boolean }[];
}

/** An error's message with what varies between occurrences taken out. */
export function shapeOf(message: string): string {
  return message
    .replace(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, '<id>')
    .replace(/\b\d+(\.\d+)?\b/g, '#')
    .replace(/\s+/g, ' ')
    .slice(0, 160);
}

/** Read log lines written on or after `since` (YYYY-MM-DD). */
export function readLog(lines: string[], since: string): LogFindings {
  const violations: LogFindings['violations'] = [];
  const groups = new Map<string, LogFindings['errors'][number]>();
  for (const line of lines) {
    const m = LINE.exec(line);
    if (!m || m[1] < since) continue;
    const at = `${m[1]} ${m[2]}`;
    const v = parseViolation(m[5]);
    if (v) {
      violations.push({ ...v, at });
      continue;
    }
    if (m[4] !== 'ERROR') continue;
    const shape = shapeOf(m[5]);
    const g = groups.get(shape);
    if (g) {
      // The log is several files, read in no particular order.
      g.count++;
      if (at < g.first) g.first = at;
      if (at > g.last) g.last = at;
    } else {
      groups.set(shape, { shape, count: 1, first: at, last: at, uncaught: m[5].includes('Uncaught (') });
    }
  }
  const errors = [...groups.values()].sort((a, b) => Number(b.uncaught) - Number(a.uncaught) || b.count - a.count);
  return { violations, errors };
}

// ── sync.converges ─────────────────────────────────────────────

/** A vault as paths and content hashes, to compare with another device's. */
export type Manifest = Record<string, string>;

export interface Divergence {
  /** On both, with different content. */
  differ: string[];
  onlyHere: string[];
  onlyThere: string[];
}

export function compareManifests(here: Manifest, there: Manifest): Divergence {
  const differ: string[] = [];
  const onlyHere: string[] = [];
  for (const [path, hash] of Object.entries(here)) {
    if (!(path in there)) onlyHere.push(path);
    else if (there[path] !== hash) differ.push(path);
  }
  const onlyThere = Object.keys(there).filter((p) => !(p in here));
  return { differ: differ.sort(), onlyHere: onlyHere.sort(), onlyThere: onlyThere.sort() };
}
