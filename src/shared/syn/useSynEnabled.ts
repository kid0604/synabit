import { onMounted, onUnmounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../utils/logger';

/**
 * Whether Syn is switched on, for the screens that have to stop offering it.
 *
 * # Why a composable and not a prop
 *
 * Two places need this and they are nowhere near each other: the Syn app, which
 * has to show why the composer is gone, and `App.vue`, which owns the ask bar
 * and has to stop Cmd+J opening it over a note. Threading a boolean from a
 * settings panel inside one mini-app up to the shell and back down is a longer
 * path than reading the file twice.
 *
 * # Why the screen reading this is not the enforcement
 *
 * It is a courtesy. `syn_send_message` and `syn_skill_trial` refuse on their
 * own — see `commands::syn::SWITCHED_OFF` — because a switch enforced only in
 * the UI is a switch that fails the moment anything else calls the command: a
 * stale window, the ask bar, or whatever is added next.
 *
 * So this exists to avoid showing somebody a composer that will refuse them,
 * not to be the thing that refuses.
 *
 * # Failing on
 *
 * A vault whose settings cannot be read reports Syn as on, which is what a
 * fresh vault gets. The opposite default would turn Syn off for anyone with an
 * unreadable settings file and give them no clue why — and the backend would
 * still be answering, so the app would disagree with itself.
 */
export function useSynEnabled(vaultPath: () => string) {
  const enabled = ref(true);

  const refresh = async () => {
    const path = vaultPath();
    if (!path) return;
    try {
      const settings = await invoke<{ enabled?: boolean }>('syn_get_settings', {
        vaultPath: path,
      });
      // `?? true` rather than `=== true`: a backend that has not yet learned
      // the field is one where Syn is on, not one where it is off.
      enabled.value = settings?.enabled ?? true;
    } catch (e) {
      logger.warn('[Syn] Could not read whether Syn is on; assuming it is', e);
      enabled.value = true;
    }
  };

  const onSaved = () => {
    void refresh();
  };

  onMounted(() => {
    void refresh();
    window.addEventListener(SETTINGS_SAVED, onSaved);
  });

  onUnmounted(() => {
    window.removeEventListener(SETTINGS_SAVED, onSaved);
  });

  return { enabled, refresh };
}

/**
 * Fired when Syn's settings are written.
 *
 * A window event because the two listeners are in different component trees:
 * the switch is inside the Syn app's settings panel, and one of the things it
 * turns off — the ask bar — belongs to the shell above every mini-app. Nothing
 * else connects those two, and inventing a store for one boolean would be the
 * larger change.
 */
export const SETTINGS_SAVED = 'syn-settings-saved';
