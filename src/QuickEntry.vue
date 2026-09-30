<script setup lang="ts">
/**
 * The window the global hotkey opens.
 *
 * The point of a capture inbox is the distance between having a thought and
 * having it written down. Raising the whole app closed that distance a
 * little and opened another one: Synabit covered whatever the user was doing,
 * and they had to find their way back. This is a box that appears over their
 * work, takes a sentence, and disappears.
 *
 * # Why it does not write the cap itself
 *
 * It calls `queue_capture` — the same queue the Android share sheet uses —
 * and the main window turns that into a cap. So this window needs no vault,
 * no node service and no store of its own: it works while the vault is
 * locked, while it is still loading, and while no vault has been chosen.
 *
 * That is also what keeps it fast. A window that had to open a vault before
 * accepting a sentence would not be worth opening.
 *
 * # Asking Syn from here
 *
 * Tab turns the box from "capture" into "ask Syn", and Enter then hands the
 * question to the main window's ask bar instead of the queue. See `quickAsk.ts`
 * for why that is a mode the person picks rather than a guess made from the
 * text, and `ask_syn_from_quick_entry` for why the answer is shown in the main
 * window rather than in a chat grown inside this one.
 */
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { load } from '@tauri-apps/plugin-store';
import { logger } from './utils/logger';
import { i18n } from './i18n';
import synAvatar from './assets/syn-avatar.jpg';
import { useSynEnabled } from './shared/syn/useSynEnabled';
import { nextMode, quickEntryAction, type QuickEntryMode } from './shared/syn/quickAsk';
import { applyUiScale } from './utils/uiScale';

const text = ref('');
const inputRef = ref<HTMLTextAreaElement | null>(null);
const isSaving = ref(false);

const win = getCurrentWindow();
const t = (key: string) => i18n.global.t(key);

/**
 * Capture by default, always. The hotkey promises a capture box; asking is the
 * second thing it does, chosen each time with Tab.
 */
const mode = ref<QuickEntryMode>('capture');

/**
 * Whether "ask" is offered at all.
 *
 * Read from the same settings file the main window uses, because this window
 * has no vault open and must not start opening one. No vault chosen, or Syn
 * switched off for it, and the box is exactly the capture box it always was —
 * nothing Syn-shaped in it. Re-read each time the box appears, because the
 * switch lives in another window and can be flipped while this one sleeps.
 */
const vaultPath = ref('');
const { enabled: synEnabled, refresh: refreshSyn } = useSynEnabled(() => vaultPath.value);
/** Simple mode hides Syn in the main window, so it is not offered here either. */
const simpleMode = ref(false);
const askAvailable = computed(() => !!vaultPath.value && synEnabled.value && !simpleMode.value);

/**
 * The settings that can change in the main window while this one sleeps:
 * simple mode and the interface size. Read on open and every time the box
 * comes back, from the store the main window writes.
 */
let settingsStore: Awaited<ReturnType<typeof load>> | null = null;
const readLiveSettings = async () => {
  if (!settingsStore) return;
  try {
    simpleMode.value = (await settingsStore.get<boolean>('simpleMode')) === true;
    await applyUiScale((await settingsStore.get<number>('uiScale')) ?? 1);
  } catch (e) {
    logger.warn('Quick entry could not re-read settings', e);
  }
};

const focusInput = async () => {
  await nextTick();
  inputRef.value?.focus();
};

/**
 * Hide, keeping whatever was typed.
 *
 * Discarding it would be the wrong trade: someone who switched away
 * mid-sentence wants that sentence back, and the alternative to keeping it is
 * silently throwing away the exact thing this window exists to catch.
 */
const dismiss = async () => {
  await win.hide();
};

const save = async () => {
  const action = quickEntryAction(mode.value, text.value, askAvailable.value);
  if (!action || isSaving.value) return;

  isSaving.value = true;
  try {
    if (action.kind === 'ask') {
      // Rust brings the main window forward, which takes focus from this one
      // and so hides it; the explicit hide below is for the platforms where
      // focus does not move on its own.
      await invoke('ask_syn_from_quick_entry', { text: action.text });
    } else {
      await invoke('queue_capture', { text: action.text, source: 'quick-entry' });
    }
    text.value = '';
    // Back to capture once a question has gone: the next press of the hotkey
    // should be the box it always was.
    mode.value = 'capture';
    await win.hide();
  } catch (e) {
    logger.error(
      action.kind === 'ask'
        ? 'Quick entry could not hand the question to Syn'
        : 'Quick entry could not queue the capture',
      e,
    );
  } finally {
    isSaving.value = false;
  }
};

const toggleMode = () => {
  mode.value = nextMode(mode.value, askAvailable.value);
  void focusInput();
};

const onKeydown = (event: KeyboardEvent) => {
  // The input method's key, not ours. With Telex or VNI, Enter commits the
  // word being composed — saving on it saved half a word, and Escape there
  // cancels the composition rather than the window. `keyCode` 229 is what
  // WebKit reports for those keys before `isComposing` is set.
  if (event.isComposing || event.keyCode === 229) return;
  if (event.key === 'Escape') {
    event.preventDefault();
    void dismiss();
    return;
  }
  // Tab has nothing to move focus to in a one-field window, which is what
  // makes it free to mean "capture or ask". Without Syn it keeps doing
  // whatever it did before.
  if (event.key === 'Tab' && askAvailable.value) {
    event.preventDefault();
    toggleMode();
    return;
  }
  // Enter saves; Shift+Enter is a new line. A capture is usually one line,
  // so the common case should not need a modifier.
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault();
    void save();
  }
};

let stopFocusListener: (() => void) | null = null;

onMounted(async () => {
  // The settings file is shared between windows, so this one can match the
  // app's language and theme without running the app's whole setup.
  try {
    const settings = await load('settings.json', { autoSave: false } as never);
    settingsStore = settings;
    const language = await settings.get<'en' | 'vi'>('appLanguage');
    if (language) i18n.global.locale.value = language;

    vaultPath.value = (await settings.get<string>('vaultPath')) || '';
    await readLiveSettings();
    void refreshSyn();

    const theme = await settings.get<'light' | 'dark' | 'system'>('themeMode');
    const dark =
      theme === 'dark' ||
      (theme !== 'light' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    document.documentElement.classList.toggle('dark', dark);
  } catch (e) {
    logger.error('Quick entry could not read settings', e);
  }

  window.addEventListener('keydown', onKeydown);

  stopFocusListener = await win.onFocusChanged(({ payload: focused }) => {
    if (focused) {
      void focusInput();
      void refreshSyn();
      void readLiveSettings();
    } else {
      // Clicking back into their work dismisses this, the way every other
      // quick-entry panel behaves. The draft survives; see `dismiss`.
      void dismiss();
    }
  });

  void focusInput();
});

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown);
  stopFocusListener?.();
});
</script>

<template>
  <div
    class="h-screen w-screen flex flex-col bg-white dark:bg-surface-dark border border-border dark:border-border-dark overflow-hidden"
  >
    <textarea
      ref="inputRef"
      v-model="text"
      :placeholder="mode === 'ask' ? t('quickcap.placeholder_quick_ask') : t('quickcap.placeholder_quick_entry')"
      class="flex-1 w-full resize-none bg-transparent px-5 pt-4 pb-2 text-[15px] leading-relaxed outline-none text-text dark:text-text-dark placeholder:text-gray-500 dark:placeholder:text-gray-400"
      spellcheck="false"
    ></textarea>

    <div
      class="shrink-0 flex items-center justify-between px-5 pb-3 text-xs text-gray-500 dark:text-gray-400 select-none"
    >
      <span>{{
        !askAvailable
          ? t('quickcap.quick_entry_hint')
          : mode === 'ask'
            ? t('quickcap.quick_ask_hint')
            : t('quickcap.quick_entry_hint_with_ask')
      }}</span>
      <span v-if="isSaving">{{ t('quickcap.save') }}…</span>
      <!--
        Which one Enter is about to do, and a way to change it with the mouse.
        Two buttons rather than a switch, so the word for each is always on
        screen: somebody glancing down has to be able to tell a capture box
        from a question box without knowing which side of a toggle means what.
      -->
      <div
        v-else-if="askAvailable"
        role="group"
        :aria-label="t('quickcap.quick_entry_mode')"
        class="flex items-center gap-0.5 rounded-full bg-gray-100 dark:bg-white/5 p-0.5"
      >
        <button
          type="button"
          :aria-pressed="mode === 'capture'"
          :class="[
            'px-2 py-0.5 rounded-full transition-colors cursor-pointer',
            mode === 'capture'
              ? 'bg-white dark:bg-white/15 text-gray-700 dark:text-gray-200 shadow-sm'
              : 'hover:text-gray-600 dark:hover:text-gray-300',
          ]"
          @mousedown.prevent
          @click="mode !== 'capture' && toggleMode()"
        >
          {{ t('quickcap.quick_entry_mode_capture') }}
        </button>
        <button
          type="button"
          :aria-pressed="mode === 'ask'"
          :class="[
            'inline-flex items-center gap-1 pl-0.5 pr-2 py-0.5 rounded-full transition-colors cursor-pointer',
            mode === 'ask'
              ? 'bg-white dark:bg-white/15 text-gray-700 dark:text-gray-200 shadow-sm'
              : 'hover:text-gray-600 dark:hover:text-gray-300',
          ]"
          @mousedown.prevent
          @click="mode !== 'ask' && toggleMode()"
        >
          <img :src="synAvatar" alt="" class="w-4 h-4 rounded-full object-cover" />
          {{ t('quickcap.quick_entry_mode_ask') }}
        </button>
      </div>
    </div>
  </div>
</template>
