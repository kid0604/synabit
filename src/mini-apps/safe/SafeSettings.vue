<script setup lang="ts">
/**
 * Safe's settings, for this device: when it locks, when the clipboard is
 * cleared, the master password, and the Secret Key again.
 *
 * Showing the Secret Key and changing the password both ask for the master
 * password even though the Safe is open. Somebody standing at an unlocked
 * screen should not be able to walk away with the second factor, or lock the
 * owner out.
 */
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { save } from '@tauri-apps/plugin-dialog';
import { X } from 'lucide-vue-next';
import ModalDialog from '../calendar/components/ModalDialog.vue';
import type { SafeApi, Settings } from './api';
import PasswordStrength from './PasswordStrength.vue';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi }>();
const emit = defineEmits<{ (e: 'close'): void }>();
const { t } = useI18n();
const explain = useSafeError();

const settings = ref<Settings | null>(null);
const error = ref('');
const notice = ref('');

const AUTO_LOCK = [60, 300, 600, 1800, 3600, 14400, 28800];
const CLIPBOARD = [10, 30, 90, 0];

function duration(secs: number) {
  if (secs === 0) return t('safe.settings.never');
  if (secs < 60) return t('safe.settings.seconds', { n: secs });
  if (secs < 3600) return t('safe.settings.minutes', { n: secs / 60 });
  return t('safe.settings.hours', { n: secs / 3600 });
}

async function update() {
  if (!settings.value) return;
  try {
    settings.value = await props.api.setSettings(settings.value);
  } catch (e) {
    error.value = explain(e);
  }
}

const current = ref('');
const next = ref('');
const nextAgain = ref('');
const nextBits = ref(0);
async function changePassword() {
  error.value = '';
  notice.value = '';
  if (next.value !== nextAgain.value) {
    error.value = t('safe.setup.mismatch');
    return;
  }
  try {
    await props.api.changePassword(current.value, next.value);
    notice.value = t('safe.settings.changed');
  } catch (e) {
    error.value = explain(e);
  } finally {
    current.value = next.value = nextAgain.value = '';
  }
}

const keyPassword = ref('');
const words = ref<string[]>([]);
async function showSecretKey() {
  error.value = '';
  try {
    words.value = (await props.api.secretKey(keyPassword.value)).split(' ');
  } catch (e) {
    error.value = explain(e);
  } finally {
    keyPassword.value = '';
  }
}

async function saveKit() {
  const path = await save({ defaultPath: 'Synabit Safe Emergency Kit.html', filters: [{ name: 'HTML', extensions: ['html'] }] });
  if (!path) return;
  try {
    await props.api.saveEmergencyKit(path);
    notice.value = t('safe.kit.saved');
  } catch (e) {
    error.value = explain(e);
  }
}

onMounted(async () => {
  try {
    settings.value = await props.api.getSettings();
  } catch (e) {
    error.value = explain(e);
  }
});

function close() {
  words.value = [];
  emit('close');
}

const input = 'w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark focus:outline-none focus:ring-2 focus:ring-accent';
</script>

<template>
  <ModalDialog :show="true" labelled-by="safe-settings-title" card-class="max-w-[480px] max-h-[calc(100vh-64px)] text-text dark:text-text-dark" @close="close">
    <header class="px-5 pt-5 pb-3 flex items-center">
      <h2 id="safe-settings-title" class="text-lg font-semibold flex-1">{{ t('safe.settings.title') }}</h2>
      <button class="p-1.5 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark" :aria-label="t('safe.settings.close')" @click="close">
        <X class="w-4 h-4" />
      </button>
    </header>

    <div class="px-5 pb-6 space-y-6 overflow-y-auto">
      <section v-if="settings" class="space-y-3">
        <label class="flex items-center justify-between gap-3 text-sm">
          <span>{{ t('safe.settings.auto_lock') }}</span>
          <select v-model.number="settings.auto_lock_secs" class="px-2 py-1 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark" @change="update">
            <option v-for="s in AUTO_LOCK" :key="s" :value="s">{{ duration(s) }}</option>
          </select>
        </label>
        <label class="flex items-center justify-between gap-3 text-sm">
          <span>{{ t('safe.settings.clipboard') }}</span>
          <select v-model.number="settings.clipboard_clear_secs" class="px-2 py-1 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark" @change="update">
            <option v-for="s in CLIPBOARD" :key="s" :value="s">{{ duration(s) }}</option>
          </select>
        </label>
      </section>

      <form class="space-y-2.5" @submit.prevent="changePassword">
        <h3 class="text-sm font-semibold">{{ t('safe.settings.change_password') }}</h3>
        <input v-model="current" type="password" :placeholder="t('safe.settings.current')" :aria-label="t('safe.settings.current')" autocomplete="off" :class="input" />
        <input v-model="next" type="password" :placeholder="t('safe.settings.new')" :aria-label="t('safe.settings.new')" autocomplete="off" :class="input" />
        <PasswordStrength :api="api" :password="next" @bits="nextBits = $event" />
        <input v-model="nextAgain" type="password" :placeholder="t('safe.settings.confirm_new')" :aria-label="t('safe.settings.confirm_new')" autocomplete="off" :class="input" />
        <button type="submit" :disabled="!current || next.length < 10 || nextBits < 50 || next !== nextAgain" class="px-4 py-2 rounded-lg bg-accent text-white text-sm font-medium hover:opacity-90 disabled:opacity-40">
          {{ t('safe.settings.change_password') }}
        </button>
      </form>

      <section class="space-y-2.5">
        <h3 class="text-sm font-semibold">{{ t('safe.settings.secret_key') }}</h3>
        <template v-if="!words.length">
          <p class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.settings.secret_key_body') }}</p>
          <form class="flex gap-2" @submit.prevent="showSecretKey">
            <input v-model="keyPassword" type="password" :placeholder="t('safe.unlock.password')" :aria-label="t('safe.unlock.password')" autocomplete="off" :class="input" />
            <button type="submit" :disabled="!keyPassword" class="px-4 py-2 rounded-lg border border-border dark:border-border-dark text-sm disabled:opacity-40">{{ t('safe.settings.show') }}</button>
          </form>
        </template>
        <template v-else>
          <ol class="grid grid-cols-3 gap-2 select-text">
            <li v-for="(w, i) in words" :key="i" class="px-2.5 py-2 rounded-lg border border-border dark:border-border-dark bg-surface dark:bg-surface-dark font-mono text-sm">
              <span class="text-text-tertiary dark:text-text-tertiary-dark mr-1.5">{{ i + 1 }}</span>{{ w }}
            </li>
          </ol>
          <div class="flex gap-2">
            <button class="px-4 py-2 rounded-lg border border-border dark:border-border-dark text-sm" @click="saveKit">{{ t('safe.kit.save') }}</button>
            <button class="px-4 py-2 rounded-lg text-sm hover:bg-surface-hover dark:hover:bg-surface-hover-dark" @click="words = []">{{ t('safe.detail.hide') }}</button>
          </div>
        </template>
      </section>

      <p v-if="notice" class="text-sm text-success" role="status">{{ notice }}</p>
      <p v-if="error" class="text-sm text-danger" role="alert">{{ error }}</p>
    </div>
  </ModalDialog>
</template>
