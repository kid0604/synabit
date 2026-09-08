/**
 * How the Messages sidebar arranges the work that is open.
 *
 * # Why grouped by state rather than listed by date
 *
 * The threads list answers one question a plain list cannot: *whose move is
 * it*. Things already shows threads — they are ordinary nodes — and shows them
 * the way it shows everything, as rows in a table sorted by a column. That is
 * the right screen for "what is in my vault" and the wrong one for "what am I
 * in the middle of".
 *
 * So the grouping is the feature. Split out from the component because the
 * order is the part worth being sure about, and a rule that can only be checked
 * by mounting a screen is a rule nobody checks.
 *
 * # Why this lives under Messages
 *
 * Because everything else Syn keeps does: the conversations, the run
 * transcripts, what it remembers, what it knows how to do, what it has been
 * allowed. A thread was briefly a mini-app of its own, which split one family
 * of features across two sidebar entries and spent a permanent slot on a
 * feature one day old.
 */
import type { Thread, ThreadState } from '../../shared/syn/useThreads';

/**
 * The groups, in the order they are shown.
 *
 * `mine` first because a thread waiting on Syn is one where somebody asked for
 * something and has not got it — the closest this app has to an unread. `yours`
 * next, because that is the actual to-do. `world` and `resting` are below the
 * fold in spirit: nobody's move, nothing to answer.
 *
 * `closed` is not here. Finished work belongs in the vault, in Nexus and in
 * Things, and not in the list of what is open — a screen that shows everything
 * that ever happened stops being a screen about now.
 */
export const GROUPS: ThreadState[] = ['mine', 'yours', 'world', 'resting'];

export interface Group {
  state: ThreadState;
  threads: Thread[];
}

/**
 * Most recently moved first, within each group.
 *
 * `last_moved` is the node's own `updated_at`, so it means what it says without
 * anything having to maintain it — see `syn/thread.rs`.
 */
export function byLastMoved(a: Thread, b: Thread): number {
  return b.last_moved.localeCompare(a.last_moved);
}

/**
 * The open threads, grouped and ordered.
 *
 * Empty groups are dropped rather than rendered as a heading over nothing: a
 * screen of four headings and one thread reads as three things missing.
 */
export function grouped(threads: Thread[]): Group[] {
  return GROUPS.map((state) => ({
    state,
    threads: threads.filter((t) => t.state === state).sort(byLastMoved),
  })).filter((g) => g.threads.length > 0);
}

/** Finished work, newest first. Kept for a section that stays folded. */
export function closed(threads: Thread[]): Thread[] {
  return threads.filter((t) => t.state === 'closed').sort(byLastMoved);
}

/**
 * The one line under a thread's name in the list.
 *
 * What it is waiting for, when somebody wrote that down, because that is the
 * sentence that makes a list of names into a list of work. Falling back to the
 * first line of the body rather than to nothing — a thread whose body starts
 * with the headings it was created with has nothing to say yet, and says so by
 * being blank rather than by showing `## What this is`.
 */
export function subtitle(thread: Thread): string {
  if (thread.waiting_for?.trim()) return thread.waiting_for.trim();

  const firstReal = thread.body
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line.length > 0 && !line.startsWith('#'));

  return firstReal ?? '';
}
