import { computed, type ComputedRef } from 'vue';
import { useRemembered } from './useRemembered';

/**
 * A sidebar section somebody can fold away, and finds folded next time.
 *
 * Remembered on this device and not in the vault, for the reasons
 * `useRemembered` gives: how a sidebar is laid out is not an edit, and a phone
 * and a desktop can want different answers. Stored as words rather than a
 * boolean so it reads the same as the conversation list's fold, which came
 * first (`ChatSidebar`).
 */
export interface Fold {
  folded: ComputedRef<boolean>;
  toggle: () => void;
  /** Open it — when somebody has just asked to see what it holds. */
  open: () => void;
}

const STATES = ['open', 'folded'] as const;

export function useFold(key: string, initially: (typeof STATES)[number] = 'open'): Fold {
  const shown = useRemembered(key, STATES, initially);
  const folded = computed(() => shown.value === 'folded');
  return {
    folded,
    toggle: () => {
      shown.value = folded.value ? 'open' : 'folded';
    },
    open: () => {
      shown.value = 'open';
    },
  };
}
