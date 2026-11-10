/**
 * The main window's page, watched while it runs: `ui.responsive` and
 * `ui.page-alive`. See `invariants`.
 *
 * A late timer says the page's thread was busy for as long as it was late —
 * reported with where the page was and what was last pressed, because the
 * first such line (2026-10-15, "blocked for 11462 ms") said neither, and the
 * cause had to be worked out from whatever else the log happened to hold.
 *
 * A block that never ends cannot be reported from here: the timer never fires
 * again. So the page also beats to the Rust side, which notices the silence
 * (`invariants::Heartbeat`).
 */
import { invoke } from '@tauri-apps/api/core';
import { reportViolation } from './index';

/** Longer than this, and the page was not responsive. */
const BLOCKED_MS = 500;
const TICK_MS = 100;
/**
 * Later than this is the machine asleep, not the page busy: a timer is not run
 * while the lid is shut. A block this long is seen by the heartbeat anyway.
 */
const ASLEEP_MS = 60_000;
/** As `invariants::BEAT_EVERY`. */
const BEAT_MS = 2000;
/** One line per route this often at most; the ones between are counted in it. */
const REPORT_EVERY_MS = 10_000;

/**
 * Whether a tick this late was the page's thread held, and not the timer slowed.
 *
 * A window that is behind another, or on another Space, keeps
 * `visibilityState` at `visible`, yet WebKit runs its timers once a second:
 * every tick 900 ms late, for as long as the window stays there. From
 * 2026-10-07 to 2026-11-10 that was 208 000 lines, 99.7% of every
 * `ui.responsive` there was and three quarters of the whole log. They pushed
 * five weeks of history out of the 60 MB the log is kept to, and hid the one
 * block that was real (3.4 s in the note editor). Only a window that had focus
 * from one tick to the next has timers run on time, so only that window's late
 * tick is a block. A block in a window put away is no worse for that: the
 * heartbeat (`ui.page-alive`) sees it from Rust.
 */
export function isBlocked(lateMs: number, focusedThroughout: boolean, visible: boolean): boolean {
  return lateMs > BLOCKED_MS && lateMs < ASLEEP_MS && visible && focusedThroughout;
}

/**
 * At most one report per route every `everyMs`. Returns `null` when this one
 * is to be held back, otherwise how many were held back since the last.
 */
export function reportLimiter(everyMs: number): (route: string, now: number) => number | null {
  const last = new Map<string, { at: number; held: number }>();
  return (route, now) => {
    const seen = last.get(route);
    if (seen && now - seen.at < everyMs) {
      seen.held += 1;
      return null;
    }
    last.set(route, { at: now, held: 0 });
    return seen?.held ?? 0;
  };
}

interface Pressed {
  kind: 'pointer' | 'key';
  /** The element, by tag, role and first class — never its text. */
  target: string;
  /** A named key (Enter, Escape…), or `char` for anything typed. */
  key?: string;
  at: number;
}

let last: Pressed | null = null;

/** An element as something to find in the source, not as what it says. */
export function signature(el: EventTarget | null): string {
  if (!(el instanceof Element)) return '?';
  const role = el.getAttribute('role');
  const cls = el.classList[0];
  return `${el.tagName.toLowerCase()}${role ? `[role=${role}]` : ''}${cls ? `.${cls}` : ''}`;
}

/** What was last pressed, for a report: how long ago, and on what. */
function lastPressed(now: number): Record<string, string | number> {
  if (!last) return {};
  const out: Record<string, string | number> = {
    last_input: last.kind,
    last_target: last.target,
    since_input_ms: Math.round(now - last.at),
  };
  if (last.key) out.last_key = last.key;
  return out;
}

export function watchPage(route: () => string): void {
  const remember = (kind: Pressed['kind']) => (e: Event) => {
    const key = e instanceof KeyboardEvent ? (e.key.length === 1 ? 'char' : e.key) : undefined;
    last = { kind, target: signature(e.target), key, at: performance.now() };
  };
  window.addEventListener('pointerdown', remember('pointer'), { capture: true, passive: true });
  window.addEventListener('keydown', remember('key'), { capture: true, passive: true });

  let prev = performance.now();
  let wasFocused = document.hasFocus();
  const limit = reportLimiter(REPORT_EVERY_MS);
  setInterval(() => {
    const now = performance.now();
    const late = now - prev - TICK_MS;
    prev = now;
    const focused = document.hasFocus();
    const blocked = isBlocked(late, wasFocused && focused, document.visibilityState === 'visible');
    wasFocused = focused;
    if (!blocked) return;
    const repeats = limit(route(), now);
    if (repeats === null) return;
    reportViolation({
      invariant: 'ui.responsive',
      subject: route(),
      detail: { blocked_ms: Math.round(late), ...(repeats ? { repeats } : {}), ...lastPressed(now) },
    });
  }, TICK_MS);

  const beat = () => {
    invoke('page_alive', { route: route(), visible: document.visibilityState === 'visible' }).catch(() => {
      // A beat that cannot be sent says nothing the silence will not.
    });
  };
  setInterval(beat, BEAT_MS);
  // Hidden is said at once, so a window put away is not taken for a stalled one.
  document.addEventListener('visibilitychange', beat);
  beat();
}
