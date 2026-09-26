/**
 * How often what Syn has actually fires, as numbers on a screen.
 *
 * The arithmetic is all in Rust (`syn::stats`), over the runs already in the
 * vault. What lives here is only what the screen needs to draw it: the order
 * rows go in, and the two small sums every row does — a share, and how wide
 * its bar is. Pulled out of the component so they can be tested without
 * mounting it, which needs the i18n plugin and a Tauri mock that have nothing
 * to do with whether 3 of 12 is 25%.
 *
 * Nothing here is sent anywhere. `syn_stats` reads files on this device and
 * returns to the screen that asked; there is no other caller.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';
import type { CeilingKind, RoundBuckets, RunState, SynStats } from '../types';

/** Buckets in the order a person reads them: fewest rounds first. */
export const ROUND_BUCKETS: (keyof RoundBuckets)[] = [
  'none', 'one', 'two', 'three_to_five', 'six_to_ten', 'over_ten',
];

/**
 * Endings, the ordinary one first and the ones worth a look after.
 *
 * Every state is listed so a zero shows as a zero. A state missing from the
 * table would read as a state that cannot happen.
 */
export const ENDINGS: RunState[] = [
  'done', 'budget_exhausted', 'failed', 'cancelled',
  'awaiting_consent', 'awaiting_choice', 'interrupted', 'working',
];

/** `unknown` last: it is the count that says this screen needs fixing. */
export const CEILINGS: CeilingKind[] = ['iterations', 'tool_calls', 'tokens', 'wall_ms', 'unknown'];

/**
 * `part` of `whole` as a whole percentage, or `null` when there is no whole.
 *
 * `null` rather than 0: "none of zero runs" is not "0% of runs", and a screen
 * that shows 0% on a vault with no measured runs is reporting something it
 * does not know.
 */
export const share = (part: number, whole: number): number | null =>
  whole > 0 ? Math.round((part / whole) * 100) : null;

/**
 * How wide a bar is, as a percentage of the row's width.
 *
 * Against the largest value in its own table rather than against the total,
 * so the longest bar fills the row and the rest compare to it. Anything above
 * zero gets at least a sliver: a real count drawn as no bar at all reads as
 * zero, which is the one thing a count of one is not.
 */
export const barWidth = (value: number, max: number): number => {
  if (max <= 0 || value <= 0) return 0;
  return Math.max(2, Math.round((value / max) * 100));
};

export function useSynStats(vaultPath: () => string) {
  const stats = ref<SynStats | null>(null);
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  /** Re-read every time the tab is shown. The numbers move with every run. */
  const load = async () => {
    isLoading.value = true;
    error.value = null;
    try {
      stats.value = await invoke<SynStats>('syn_stats', { vaultPath: vaultPath() });
    } catch (e) {
      logger.error('[Syn] Failed to count runs', e);
      error.value = (e as { message?: string })?.message ?? String(e);
      stats.value = null;
    } finally {
      isLoading.value = false;
    }
  };

  return { stats, isLoading, error, load };
}
