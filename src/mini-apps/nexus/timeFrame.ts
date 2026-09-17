/**
 * Reading the graph at a point in the past.
 *
 * The frame comes from `timeline_frame` (`src-tauri/src/timeline/frame.rs`):
 * the day each node entered the user's life, the day people died, and when
 * relationships with dates ran. This turns it into a lens the graph can ask on
 * every frame of a scrub, which is dozens of times a second, so everything
 * here is a lookup.
 */

export interface TimedLink {
  source: string;
  target: string;
  since: string;
  until: string | null;
  /** Events both ends were in. Zero when the relationship was declared, not met. */
  met: number;
}

export interface MonthCount {
  month: string;
  count: number;
  /** The month's events added up by size rather than counted one apiece. */
  weight: number;
}

/** A period the person sealed. See `src-tauri/src/timeline/seal.rs`. */
export interface SealedPeriod {
  id: string;
  from: string;
  to: string;
  from_text: string;
  to_text: string;
}

export interface TimeFrame {
  first_seen: Record<string, string>;
  died_on: Record<string, string>;
  links: TimedLink[];
  density: MonthCount[];
  earliest: string | null;
  sealed?: SealedPeriod[];
}

export interface TimeLens {
  arrival: Map<string, string>;
  died: Map<string, string>;
  timed: Map<string, TimedLink[]>;
}

/** One key for a pair, whichever end is named first. */
export const pairKey = (a: string, b: string) => (a < b ? `${a}\u0000${b}` : `${b}\u0000${a}`);

export function lensFor(frame: TimeFrame, links: Array<{ source: string; target: string }>): TimeLens {
  const arrival = new Map(Object.entries(frame.first_seen));

  // Tags and unresolved links are invented by the graph and carry no dates of
  // their own. One appears with the first thing that uses it.
  const derived = new Map<string, string>();
  for (const { source, target } of links) {
    const ends: Array<[string, string]> = [[source, target], [target, source]];
    for (const [self, other] of ends) {
      if (arrival.has(self)) continue;
      const day = arrival.get(other);
      if (!day) continue;
      const known = derived.get(self);
      if (!known || day < known) derived.set(self, day);
    }
  }
  for (const [id, day] of derived) arrival.set(id, day);

  const timed = new Map<string, TimedLink[]>();
  for (const link of frame.links) {
    const key = pairKey(link.source, link.target);
    const spans = timed.get(key);
    if (spans) spans.push(link);
    else timed.set(key, [link]);
  }

  return { arrival, died: new Map(Object.entries(frame.died_on)), timed };
}

/**
 * Whether a node is in the picture on `day`.
 *
 * A node the frame has no date for is shown. No date is not evidence that it
 * was absent, and hiding it would make the past look emptier than the vault
 * says it was.
 */
export function nodeVisible(lens: TimeLens, id: string, day: string): boolean {
  const arrived = lens.arrival.get(id);
  return !arrived || arrived <= day;
}

/**
 * Whether a link between two nodes that are both shown holds on `day`.
 *
 * Most links have no dates and hold whenever both ends are there. A
 * relationship with dates holds only while it lasted, and if it was recorded
 * more than once, while any of them lasted.
 */
export function linkVisible(lens: TimeLens, source: string, target: string, day: string): boolean {
  const spans = lens.timed.get(pairKey(source, target));
  if (!spans) return true;
  return spans.some((span) => span.since <= day && (!span.until || day <= span.until));
}

export function isDeceased(lens: TimeLens, id: string, day: string): boolean {
  const died = lens.died.get(id);
  return !!died && died <= day;
}
