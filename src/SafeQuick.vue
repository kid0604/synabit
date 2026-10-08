<script setup lang="ts">
/**
 * Safe's Quick Access: Cmd/Ctrl+Alt+\ from anywhere, type a few letters,
 * Enter, paste. The desktop's stand-in for a browser extension — section 9.6
 * of `docs/safe-2026-09-28.md`.
 *
 * Its own root, like the capture box, so it appears at once instead of
 * running the app's setup. It shows summaries only; copying goes through
 * `safe_copy_primary`, so no value ever reaches this window.
 *
 *   Enter          copy the password
 *   Shift+Enter    copy the username
 *   Alt+Enter      copy the one-time code
 *   Cmd/Ctrl+Enter open the website
 *   Escape         hide
 */
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { load } from '@tauri-apps/plugin-store';
import { openUrl } from '@tauri-apps/plugin-opener';
import { KeyRound, Lock, Search } from 'lucide-vue-next';
import { i18n, setAppLocale } from './i18n';
import { logger } from './utils/logger';
import { safeCode, useSafeApi, type ItemSummary } from './mini-apps/safe/api';
import { kindInfo } from './mini-apps/safe/kinds';
import { errorText } from './shared/errorText';
import { applyUiScale } from './utils/uiScale';

const t = (key: string, args?: Record<string, unknown>) => i18n.global.t(key, args ?? {});
/** A Safe refusal in words: its own sentence when it has one, never a bare key. */
function explain(e: unknown): string {
  const code = safeCode(e);
  return code && i18n.global.te(`safe.errors.${code}`) ? t(`safe.errors.${code}`) : t('safe.errors.failed', { msg: errorText(e) });
}
const win = getCurrentWindow();

const vaultPath = ref('');
const api = useSafeApi(() => vaultPath.value);

type State = 'loading' | 'no_safe' | 'locked' | 'open';
const state = ref<State>('loading');
const query = ref('');
const results = ref<ItemSummary[]>([]);
const selected = ref(0);
const password = ref('');
const message = ref('');
const busy = ref(false);
const searchInput = ref<HTMLInputElement | null>(null);
const passwordInput = ref<HTMLInputElement | null>(null);

const MAX = 8;

async function refresh() {
  message.value = '';
  if (!vaultPath.value) {
    state.value = 'no_safe';
    return;
  }
  try {
    const status = await api.status();
    state.value = !status.exists ? 'no_safe' : status.unlocked ? 'open' : 'locked';
  } catch (e) {
    logger.error('[Safe] quick access status failed', e);
    state.value = 'locked';
  }
  if (state.value === 'open') await search();
  await nextTick();
  (state.value === 'open' ? searchInput : passwordInput).value?.focus();
}

async function search() {
  try {
    results.value = (await api.list({ by: 'all' }, query.value)).slice(0, MAX);
    selected.value = 0;
  } catch (e) {
    if (safeCode(e) === 'locked') state.value = 'locked';
  }
}

let searchTimer: ReturnType<typeof setTimeout> | undefined;
watch(query, () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(search, 80);
});

async function unlock() {
  if (!password.value || busy.value) return;
  busy.value = true;
  message.value = '';
  try {
    await api.unlock(password.value);
    password.value = '';
    await refresh();
  } catch (e) {
    const code = safeCode(e);
    // What only the main window can do — type the Secret Key, choose between
    // two passwords — is sent there; anything else is said as it is.
    message.value = code === 'needs_secret_key' || code === 'password_changed_elsewhere' ? t('safe.quick.open_app_for_key') : explain(e);
  } finally {
    busy.value = false;
  }
}

async function hide() {
  query.value = '';
  message.value = '';
  // A password typed and not sent does not wait in a hidden window.
  password.value = '';
  await win.hide();
}

async function act(what: 'password' | 'username' | 'totp' | 'open') {
  const item = results.value[selected.value];
  if (!item) return;
  try {
    if (what === 'open') {
      const host = item.hosts[0];
      if (!host) {
        message.value = t('safe.quick.none_open');
        return;
      }
      await openUrl(`https://${host}`);
      await hide();
      return;
    }
    await invoke('safe_copy_primary', { vaultPath: vaultPath.value, id: item.id, what });
    await hide();
  } catch (e) {
    const code = safeCode(e);
    if (code === 'locked') {
      state.value = 'locked';
      return;
    }
    message.value = code === 'not_found' ? t(`safe.quick.none_${what}`) : explain(e);
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault();
    void hide();
    return;
  }
  if (state.value !== 'open') return;
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    selected.value = Math.min(selected.value + 1, results.value.length - 1);
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    selected.value = Math.max(selected.value - 1, 0);
  } else if (e.key === 'Enter') {
    // Enter that ends a word being composed — Telex, Pinyin — is the input
    // method's, not a request to copy and close.
    if (e.isComposing || e.keyCode === 229) return;
    e.preventDefault();
    void act(e.metaKey || e.ctrlKey ? 'open' : e.shiftKey ? 'username' : e.altKey ? 'totp' : 'password');
  }
}

const hint = computed(() => t('safe.quick.hint'));

let unlistenShown: UnlistenFn | null = null;
let stopFocus: (() => void) | null = null;
onMounted(async () => {
  try {
    const settings = await load('settings.json', { autoSave: false } as never);
    const language = await settings.get<'en' | 'vi'>('appLanguage');
    if (language) await setAppLocale(language);
    vaultPath.value = (await settings.get<string>('vaultPath')) || '';
    const theme = await settings.get<'light' | 'dark' | 'system'>('themeMode');
    const dark = theme === 'dark' || (theme !== 'light' && window.matchMedia('(prefers-color-scheme: dark)').matches);
    document.documentElement.classList.toggle('dark', dark);
    // The interface size chosen in Settings, as the main window has it.
    await applyUiScale((await settings.get<number>('uiScale')) ?? 1);
  } catch (e) {
    logger.error('[Safe] quick access could not read settings', e);
  }
  window.addEventListener('keydown', onKeydown);
  unlistenShown = await listen('safe-quick://shown', () => void refresh());
  // Clicking back into other work dismisses it, like the capture box.
  stopFocus = await win.onFocusChanged(({ payload: focused }) => {
    if (!focused) void hide();
  });
  await refresh();
});

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown);
  unlistenShown?.();
  stopFocus?.();
});
</script>

<template>
  <div class="h-screen w-screen flex flex-col bg-base dark:bg-base-dark text-text dark:text-text-dark border border-border dark:border-border-dark rounded-xl overflow-hidden">
    <template v-if="state === 'open'">
      <div class="flex items-center gap-3 px-4 h-14 border-b border-border dark:border-border-dark" data-tauri-drag-region>
        <Search class="w-4 h-4 text-text-tertiary dark:text-text-tertiary-dark flex-shrink-0" />
        <input
          ref="searchInput"
          v-model="query"
          :placeholder="t('safe.list.search')"
          :aria-label="t('safe.list.search')"
          spellcheck="false"
          autocomplete="off"
          class="flex-1 bg-transparent text-[15px] outline-none"
        />
      </div>
      <ul class="flex-1 overflow-y-auto p-2" role="listbox" :aria-label="t('safe.name')">
        <li
          v-for="(item, i) in results"
          :key="item.id"
          role="option"
          :aria-selected="i === selected"
          class="flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer"
          :class="i === selected ? 'bg-accent/10' : ''"
          @mouseenter="selected = i"
          @click="act('password')"
        >
          <component :is="kindInfo(item.kind).icon" class="w-4 h-4 text-text-secondary dark:text-text-secondary-dark flex-shrink-0" />
          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium truncate">{{ item.title }}</p>
            <p class="text-xs text-text-secondary dark:text-text-secondary-dark truncate">{{ item.subtitle }}</p>
          </div>
        </li>
        <li v-if="!results.length" class="px-3 py-8 text-center text-sm text-text-tertiary dark:text-text-tertiary-dark">
          {{ query ? t('safe.list.no_match', { q: query }) : t('safe.list.empty') }}
        </li>
      </ul>
      <p v-if="message" class="px-4 py-1.5 text-xs text-danger" role="alert">{{ message }}</p>
      <p class="px-4 py-2 border-t border-border dark:border-border-dark text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ hint }}</p>
    </template>

    <form v-else-if="state === 'locked'" class="flex-1 flex flex-col items-center justify-center gap-3 px-10" data-tauri-drag-region @submit.prevent="unlock">
      <Lock class="w-6 h-6 text-accent" />
      <p class="font-medium">{{ t('safe.unlock.title') }}</p>
      <input
        ref="passwordInput"
        v-model="password"
        type="password"
        :placeholder="t('safe.unlock.password')"
        :aria-label="t('safe.unlock.password')"
        autocomplete="off" spellcheck="false" autocapitalize="off"
        class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark outline-none focus:ring-2 focus:ring-accent"
      />
      <p v-if="message" class="text-xs text-danger text-center" role="alert">{{ message }}</p>
      <button type="submit" :disabled="!password || busy" class="btn-primary w-full">
        {{ busy ? t('safe.unlock.unlocking') : t('safe.unlock.submit') }}
      </button>
    </form>

    <div v-else-if="state === 'no_safe'" class="flex-1 flex flex-col items-center justify-center gap-2 px-10 text-center" data-tauri-drag-region>
      <KeyRound class="w-6 h-6 text-accent" />
      <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.quick.no_safe') }}</p>
    </div>
  </div>
</template>
