/**
 * Whether Syn's workings are on show on this device — tool names and
 * arguments, tokens, run tallies, the inspector, the tuning settings.
 *
 * Read through here rather than from the store directly, so a component
 * mounted without the app around it (a test, a preview) reads "off" instead of
 * failing: off is what everybody who has not asked sees.
 */
import { computed, type ComputedRef } from 'vue';
import { getActivePinia } from 'pinia';
import { useAppStore } from '../../stores/useAppStore';

export function useSynDeveloper(): ComputedRef<boolean> {
  const store = getActivePinia() ? useAppStore() : null;
  return computed(() => store?.synDeveloper === true);
}
