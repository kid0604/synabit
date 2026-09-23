import { ref, watch, type Ref } from 'vue';

/**
 * A choice about how something is shown, kept on this device.
 *
 * The reasoning `viewportMemory.ts` sets out for a board's camera applies to
 * every preference of this shape: it is not an edit, so writing it into the
 * vault would stamp a file as changed and push a display setting to every
 * other device; and it is not shared, because a phone and a desktop can
 * reasonably want different answers. So it lives here, per device, and the
 * vault never hears about it.
 *
 * The value is checked against the list of ones the app understands on the way
 * back in. Storage outlives the version that wrote it and anybody can edit it,
 * so a stored setting is input from outside — the same reason `asFieldKind`
 * validates rather than casts.
 */

/** Absent in some environments, full in others, and blocked in a third. */
function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function recallChoice<T extends string>(
  key: string,
  allowed: readonly T[],
  fallback: T,
): T {
  try {
    const stored = storage()?.getItem(key);
    return allowed.includes(stored as T) ? (stored as T) : fallback;
  } catch {
    return fallback;
  }
}

export function rememberChoice(key: string, value: string): void {
  try {
    storage()?.setItem(key, value);
  } catch {
    // A full or blocked store costs the user their preference next launch,
    // which is not worth an error in front of whatever they were doing.
  }
}

/**
 * A ref that starts where it was left and writes itself back when it changes.
 *
 * `allowed` is the whole set the caller understands; anything else on disk is
 * treated as absent.
 */
export function useRemembered<T extends string>(
  key: string,
  allowed: readonly T[],
  fallback: T,
): Ref<T> {
  const choice = ref(recallChoice(key, allowed, fallback)) as Ref<T>;
  watch(choice, value => rememberChoice(key, value));
  return choice;
}

/** What a remembered number is allowed to be. */
export interface NumberRange {
  min: number;
  max: number;
  fallback: number;
}

/**
 * A remembered number, clamped to what the app can draw.
 *
 * The same rule as `recallChoice`, for the shape a size takes: storage
 * outlives the version that wrote it, so a width saved when the minimum was
 * 220 must not reopen a pane narrower than today's minimum, and a hand-edited
 * `"-1"` or `"Infinity"` must not reach a stylesheet. Anything unreadable is
 * treated as absent rather than repaired into something arbitrary.
 */
export function recallNumber(key: string, range: NumberRange): number {
  try {
    const stored = storage()?.getItem(key);
    // Nothing stored is the fallback, not zero. `Number(null)` and
    // `Number('')` are both `0`, which is finite — so testing the number
    // alone would open every pane at its minimum on a device that had never
    // been dragged, and call it a remembered choice.
    if (stored === null || stored === undefined || stored.trim() === '') return range.fallback;
    const value = Number(stored);
    if (!Number.isFinite(value)) return range.fallback;
    return Math.min(Math.max(value, range.min), range.max);
  } catch {
    return range.fallback;
  }
}

/** A number ref that starts where it was left and writes itself back. */
export function useRememberedNumber(key: string, range: NumberRange): Ref<number> {
  const value = ref(recallNumber(key, range));
  watch(value, next => rememberChoice(key, String(next)));
  return value;
}
