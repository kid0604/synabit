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
 */
import { ref, onMounted, onUnmounted, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { load } from '@tauri-apps/plugin-store';
import { logger } from './utils/logger';
import { i18n } from './i18n';

const text = ref('');
const inputRef = ref<HTMLTextAreaElement | null>(null);
const isSaving = ref(false);

const win = getCurrentWindow();
const t = (key: string) => i18n.global.t(key);

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
  const body = text.value.trim();
  if (!body || isSaving.value) return;

  isSaving.value = true;
  try {
    await invoke('queue_capture', { text: body, source: 'quick-entry' });
    text.value = '';
    await win.hide();
  } catch (e) {
    logger.error('Quick entry could not queue the capture', e);
  } finally {
    isSaving.value = false;
  }
};

const onKeydown = (event: KeyboardEvent) => {
  if (event.key === 'Escape') {
    event.preventDefault();
    void dismiss();
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
    const language = await settings.get<'en' | 'vi'>('appLanguage');
    if (language) i18n.global.locale.value = language;

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
    class="h-screen w-screen flex flex-col bg-white dark:bg-[#1e1e1e] border border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden"
  >
    <textarea
      ref="inputRef"
      v-model="text"
      :placeholder="t('quickcap.placeholder_quick_entry')"
      class="flex-1 w-full resize-none bg-transparent px-5 pt-4 pb-2 text-[15px] leading-relaxed outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400"
      spellcheck="false"
    ></textarea>

    <div
      class="shrink-0 flex items-center justify-between px-5 pb-3 text-[11px] text-gray-400 dark:text-gray-500 select-none"
    >
      <span>{{ t('quickcap.quick_entry_hint') }}</span>
      <span v-if="isSaving">{{ t('quickcap.save') }}…</span>
    </div>
  </div>
</template>
