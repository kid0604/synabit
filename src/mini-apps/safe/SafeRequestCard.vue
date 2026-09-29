<script setup lang="ts">
/**
 * Syn asked for a secret (`safe_request`). This is where the user types it —
 * and the only place: the value goes from here to Rust, and Syn is told only
 * the name it will have. Section 8.8 of `docs/safe-2026-09-28.md`.
 *
 * Everything Syn suggested — title, name, connectors — is only a suggestion.
 * The card is part of the app, not something Syn drew, and every field on it
 * is the user's to change; nothing is shared with Syn beyond what they leave
 * ticked. If Syn was talked into asking by a page it read, this is a form the
 * user sees and can close.
 */
import { onMounted, onUnmounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { KeyRound } from 'lucide-vue-next';
import ModalDialog from '../calendar/components/ModalDialog.vue';
import { safeCode, useSafeApi } from './api';
import { useSafeError } from './useSafeError';

const props = defineProps<{ vaultPath: string }>();
const { t } = useI18n();
const explain = useSafeError();
const api = useSafeApi(() => props.vaultPath);

interface Request {
  id: string;
  title: string;
  handle: string;
  why: string;
  destinations: { key: string; label: string; suggested: boolean }[];
}

const request = ref<Request | null>(null);
const title = ref('');
const handle = ref('');
const value = ref('');
const chosen = ref<string[]>([]);
const locked = ref(false);
const password = ref('');
const busy = ref(false);
const error = ref('');

async function show(r: Request) {
  request.value = r;
  title.value = r.title;
  handle.value = r.handle;
  value.value = '';
  // Nothing ticked for the user: Syn's suggestion may be a stranger's words
  // it read. Suggested ones are marked; the user ticks what they mean.
  chosen.value = [];
  error.value = '';
  try {
    const status = await api.status();
    locked.value = !status.unlocked;
  } catch {
    locked.value = true;
  }
}

function close() {
  request.value = null;
  value.value = '';
  password.value = '';
}

async function unlock() {
  busy.value = true;
  error.value = '';
  try {
    await api.unlock(password.value);
    password.value = '';
    locked.value = false;
  } catch (e) {
    error.value = explain(e);
  } finally {
    busy.value = false;
  }
}

async function submit() {
  if (!request.value || !value.value) return;
  busy.value = true;
  error.value = '';
  try {
    await api.requestSubmit(request.value.id, title.value, handle.value, value.value, chosen.value);
    close();
  } catch (e) {
    if (safeCode(e) === 'locked') locked.value = true;
    error.value = explain(e);
  } finally {
    busy.value = false;
  }
}

let unlisten: UnlistenFn | null = null;
onMounted(async () => {
  unlisten = await listen<Request>('safe://request', (e) => void show(e.payload));
});
onUnmounted(() => unlisten?.());

const input =
  'w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark focus:outline-none focus:ring-2 focus:ring-accent';
</script>

<template>
  <ModalDialog
    v-if="request"
    :show="true"
    labelled-by="safe-request-title"
    card-class="max-w-[460px] max-h-[calc(100vh-64px)] text-text dark:text-text-dark"
    @close="close"
  >
    <div class="p-5 space-y-4 overflow-y-auto">
      <div class="flex items-start gap-3">
        <div class="w-9 h-9 rounded-xl bg-accent/10 flex items-center justify-center flex-shrink-0">
          <KeyRound class="w-4 h-4 text-accent" />
        </div>
        <div class="space-y-1">
          <h2 id="safe-request-title" class="font-semibold">{{ t('safe.request.title') }}</h2>
          <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.request.body', { why: request.why }) }}</p>
        </div>
      </div>

      <form v-if="locked" class="space-y-2" @submit.prevent="unlock">
        <p class="text-sm">{{ t('safe.request.locked') }}</p>
        <input v-model="password" type="password" :placeholder="t('safe.unlock.password')" :aria-label="t('safe.unlock.password')" autocomplete="off" :class="input" />
        <button type="submit" :disabled="!password || busy" class="w-full py-2 rounded-lg bg-accent text-white text-sm font-medium disabled:opacity-40">{{ t('safe.unlock.submit') }}</button>
      </form>

      <form v-else class="space-y-3" @submit.prevent="submit">
        <label class="block space-y-1">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.editor.title') }}</span>
          <input v-model="title" :class="input" />
        </label>
        <label class="block space-y-1">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.request.value') }}</span>
          <input v-model="value" type="password" autocomplete="off" spellcheck="false" autocapitalize="off" :class="input + ' font-mono'" autofocus />
        </label>
        <label class="block space-y-1">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.ai.handle') }}</span>
          <input v-model="handle" spellcheck="false" autocomplete="off" :class="input + ' font-mono text-sm'" />
        </label>
        <div v-if="request.destinations.length" class="space-y-1.5">
          <span class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.ai.destinations') }}</span>
          <label v-for="d in request.destinations" :key="d.key" class="flex items-center gap-2 text-sm">
            <input v-model="chosen" type="checkbox" :value="d.key" /> {{ d.label }}
            <span v-if="d.suggested" class="text-xs text-text-tertiary dark:text-text-tertiary-dark">· {{ t('safe.request.suggested') }}</span>
          </label>
          <p class="text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.request.none_ticked') }}</p>
        </div>
        <p v-if="error" class="text-sm text-danger" role="alert">{{ error }}</p>
        <div class="flex justify-end gap-2 pt-1">
          <button type="button" class="px-4 py-2 rounded-lg text-sm hover:bg-surface-hover dark:hover:bg-surface-hover-dark" @click="close">{{ t('safe.request.not_now') }}</button>
          <button type="submit" :disabled="!value || !title.trim() || busy" class="px-4 py-2 rounded-lg bg-accent text-white text-sm font-medium disabled:opacity-40">{{ t('safe.request.save') }}</button>
        </div>
      </form>
    </div>
  </ModalDialog>
</template>
