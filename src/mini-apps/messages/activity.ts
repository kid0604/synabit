/**
 * Syn's work, grouped the way a person asks about it.
 *
 * The runs were already on disk and already listed — in the inspector, a
 * developer's console, newest first, every state mixed together. The question
 * a person actually has is not "what are the runs" but three narrower ones:
 * *is anything still going*, *is anything waiting for me*, and *what finished
 * while I was not looking*. This is those three, in that order, because the
 * second is the only one that needs them to do something.
 */
import type { RunSummary } from './types';

export interface Activity {
  /** Still going. */
  working: RunSummary[];
  /** Stopped to ask the user something — permission, or which one. */
  waiting: RunSummary[];
  /** Ended, recently enough to still be news. */
  finished: RunSummary[];
}

/** How long a finished run stays in the list: a day. Older ones are history,
 *  and the inspector keeps all of it. */
export const RECENT_MS = 24 * 60 * 60 * 1000;

const WAITING = new Set(['awaiting_consent', 'awaiting_choice']);

export function groupRuns(all: RunSummary[], now: number = Date.now()): Activity {
  // A helper's run is part of its parent's work, not a piece of its own: the
  // parent is what the person asked for, and it is the one listed.
  const runs = all.filter(r => !r.parent_run_id);
  const newest = (a: RunSummary, b: RunSummary) => b.updated_at.localeCompare(a.updated_at);
  const working = runs.filter(r => r.state === 'working').sort(newest);
  const waiting = runs.filter(r => WAITING.has(r.state)).sort(newest);
  const finished = runs
    .filter(r => r.state !== 'working' && !WAITING.has(r.state))
    .filter(r => {
      const at = Date.parse(r.updated_at);
      return Number.isFinite(at) && now - at <= RECENT_MS;
    })
    .sort(newest);
  return { working, waiting, finished };
}

/** The badge: what needs the person. Running work is shown, not counted. */
export const needsYou = (activity: Activity): number => activity.waiting.length;
