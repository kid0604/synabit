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
  setInterval(() => {
    const now = performance.now();
    const late = now - prev - TICK_MS;
    prev = now;
    if (late > BLOCKED_MS && late < ASLEEP_MS && document.visibilityState === 'visible') {
      reportViolation({
        invariant: 'ui.responsive',
        subject: route(),
        detail: { blocked_ms: Math.round(late), ...lastPressed(now) },
      });
    }
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
