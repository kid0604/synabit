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
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { open as openFile, save } from '@tauri-apps/plugin-dialog';
import { X } from 'lucide-vue-next';
import ModalDialog from '../calendar/components/ModalDialog.vue';
import type { SafeApi, Settings, SshStatus } from './api';
import PasswordStrength from './PasswordStrength.vue';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi }>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'changed'): void }>();
const { t } = useI18n();
const explain = useSafeError();

const settings = ref<Settings | null>(null);
const error = ref('');
const notice = ref('');

const AUTO_LOCK = [60, 300, 600, 1800, 3600, 14400, 28800];
const CLIPBOARD = [10, 30, 90, 0];

function duration(secs: number) {
  if (secs === 0) return t('safe.settings.never');
  if (secs < 60 || secs % 60 !== 0) return t('safe.settings.seconds', { n: secs });
  if (secs < 3600) return t('safe.settings.minutes', { n: secs / 60 });
  return t('safe.settings.hours', { n: secs / 3600 });
}

async function update() {
  if (!settings.value) return;
  try {
    settings.value = await props.api.setSettings(settings.value);
    ssh.value = await props.api.sshStatus().catch(() => null);
  } catch (e) {
    error.value = explain(e);
  }
}

/** The SSH agent: whether it runs, where, and which keys it offers. */
const ssh = ref<SshStatus | null>(null);
const exportLine = computed(() => (ssh.value?.socket ? `export SSH_AUTH_SOCK="${ssh.value.socket}"` : ''));
async function copyExport() {
  await navigator.clipboard.writeText(exportLine.value).catch(() => undefined);
  notice.value = t('safe.ssh.copied');
}

const current = ref('');
const next = ref('');
const nextAgain = ref('');
const nextScore = ref(0);
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
  ssh.value = await props.api.sshStatus().catch(() => null);
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

// ─── moving in and out ───────────────────────────────────

const importPath = ref('');
const importPassword = ref('');
const importing = ref(false);
const imported = ref<{ imported: number; warnings: string[]; source_was_plaintext: boolean } | null>(null);
const needsExportPassword = computed(() => importPath.value.toLowerCase().endsWith('.safe-export'));

async function chooseImport() {
  error.value = '';
  imported.value = null;
  const picked = await openFile({
    multiple: false,
    filters: [{ name: t('safe.exchange.exports'), extensions: ['1pux', 'json', 'xml', 'csv', 'safe-export'] }],
  });
  if (typeof picked !== 'string') return;
  importPath.value = picked;
  if (!needsExportPassword.value) await runImport();
}

async function runImport() {
  importing.value = true;
  error.value = '';
  try {
    imported.value = await props.api.importFile(importPath.value, importPassword.value || undefined);
    importPath.value = '';
    emit('changed');
  } catch (e) {
    error.value = explain(e);
  } finally {
    importing.value = false;
    importPassword.value = '';
  }
}

const exportPassword = ref('');
const exportPasswordAgain = ref('');
async function exportSealed() {
  error.value = '';
  notice.value = '';
  const path = await save({ defaultPath: 'Synabit Safe.safe-export', filters: [{ name: 'Safe export', extensions: ['safe-export'] }] });
  if (!path) return;
  try {
    const n = await props.api.exportSealed(path, exportPassword.value);
    notice.value = t('safe.exchange.exported', { n });
  } catch (e) {
    error.value = explain(e);
  } finally {
    exportPassword.value = exportPasswordAgain.value = '';
  }
}

const plainPassword = ref('');
const plainPhrase = ref('');
const phrase = computed(() => t('safe.exchange.plain_phrase'));
async function exportPlain() {
  error.value = '';
  notice.value = '';
  const path = await save({ defaultPath: 'Synabit Safe.csv', filters: [{ name: 'CSV', extensions: ['csv'] }] });
  if (!path) return;
  try {
    const n = await props.api.exportPlain(path, plainPassword.value);
    notice.value = t('safe.exchange.exported_plain', { n });
  } catch (e) {
    error.value = explain(e);
  } finally {
    plainPassword.value = plainPhrase.value = '';
  }
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

      <label v-if="settings" class="flex items-start gap-3 text-sm">
        <input v-model="settings.breach_check" type="checkbox" class="mt-1" @change="update" />
        <span>
          <span class="block">{{ t('safe.health.setting') }}</span>
          <span class="block text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.health.setting_body') }}</span>
        </span>
      </label>

      <section v-if="settings && ssh?.supported" class="space-y-2.5">
        <h3 class="text-sm font-semibold">{{ t('safe.ssh.title') }}</h3>
        <label class="flex items-start gap-3 text-sm">
          <input v-model="settings.ssh_agent" type="checkbox" class="mt-1" @change="update" />
          <span>
            <span class="block">{{ t('safe.ssh.enable') }}</span>
            <span class="block text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.ssh.enable_body') }}</span>
          </span>
        </label>
        <template v-if="settings.ssh_agent">
          <label class="flex items-center gap-3 text-sm">
            <input v-model="settings.ssh_confirm" type="checkbox" @change="update" />
            {{ t('safe.ssh.confirm') }}
          </label>
          <div v-if="exportLine" class="space-y-1">
            <p class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.ssh.use') }}</p>
            <div class="flex gap-2 items-center">
              <code class="flex-1 min-w-0 px-2 py-1.5 rounded bg-surface dark:bg-surface-dark text-xs break-all">{{ exportLine }}</code>
              <button class="px-2 py-1 rounded border border-border dark:border-border-dark text-xs" @click="copyExport">{{ t('safe.detail.copy') }}</button>
            </div>
          </div>
          <ul class="space-y-1.5">
            <li v-for="k in ssh.keys" :key="k.title + (k.fingerprint ?? '')" class="text-xs">
              <span class="font-medium">{{ k.title }}</span>
              <span v-if="k.fingerprint" class="block font-mono text-text-tertiary dark:text-text-tertiary-dark break-all">{{ k.fingerprint }}</span>
              <span v-else class="block text-warning">{{ k.problem }}</span>
            </li>
            <li v-if="!ssh.keys.length" class="text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.ssh.no_keys') }}</li>
          </ul>
        </template>
      </section>

      <form class="space-y-2.5" @submit.prevent="changePassword">
        <h3 class="text-sm font-semibold">{{ t('safe.settings.change_password') }}</h3>
        <input v-model="current" type="password" :placeholder="t('safe.settings.current')" :aria-label="t('safe.settings.current')" autocomplete="off" :class="input" />
        <input v-model="next" type="password" :placeholder="t('safe.settings.new')" :aria-label="t('safe.settings.new')" autocomplete="off" :class="input" />
        <PasswordStrength :api="api" :password="next" @score="nextScore = $event" />
        <input v-model="nextAgain" type="password" :placeholder="t('safe.settings.confirm_new')" :aria-label="t('safe.settings.confirm_new')" autocomplete="off" :class="input" />
        <button type="submit" :disabled="!current || next.length < 10 || nextScore < 3 || next !== nextAgain" class="px-4 py-2 rounded-lg bg-accent text-white text-sm font-medium hover:opacity-90 disabled:opacity-40">
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

      <section class="space-y-2.5">
        <h3 class="text-sm font-semibold">{{ t('safe.exchange.import') }}</h3>
        <p class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.exchange.import_body') }}</p>
        <form v-if="needsExportPassword" class="flex gap-2" @submit.prevent="runImport">
          <input v-model="importPassword" type="password" :placeholder="t('safe.exchange.export_password')" :aria-label="t('safe.exchange.export_password')" autocomplete="off" :class="input" />
          <button type="submit" :disabled="!importPassword || importing" class="px-4 py-2 rounded-lg bg-accent text-white text-sm disabled:opacity-40">{{ t('safe.exchange.import_go') }}</button>
        </form>
        <button v-else :disabled="importing" class="px-4 py-2 rounded-lg border border-border dark:border-border-dark text-sm disabled:opacity-40" @click="chooseImport">
          {{ importing ? t('safe.exchange.importing') : t('safe.exchange.choose') }}
        </button>
        <div v-if="imported" class="p-3 rounded-lg bg-surface dark:bg-surface-dark text-sm space-y-1.5" role="status">
          <p class="font-medium">{{ t('safe.exchange.imported', { n: imported.imported }) }}</p>
          <p v-if="imported.source_was_plaintext" class="text-warning text-xs">{{ t('safe.exchange.delete_source') }}</p>
          <details v-if="imported.warnings.length" class="text-xs text-text-secondary dark:text-text-secondary-dark">
            <summary>{{ t('safe.exchange.warnings', { n: imported.warnings.length }) }}</summary>
            <ul class="mt-1 list-disc pl-4 space-y-0.5"><li v-for="(w, i) in imported.warnings" :key="i">{{ w }}</li></ul>
          </details>
        </div>
      </section>

      <form class="space-y-2.5" @submit.prevent="exportSealed">
        <h3 class="text-sm font-semibold">{{ t('safe.exchange.export') }}</h3>
        <p class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.exchange.export_body') }}</p>
        <input v-model="exportPassword" type="password" :placeholder="t('safe.exchange.export_password')" :aria-label="t('safe.exchange.export_password')" autocomplete="off" :class="input" />
        <input v-model="exportPasswordAgain" type="password" :placeholder="t('safe.settings.confirm_new')" :aria-label="t('safe.settings.confirm_new')" autocomplete="off" :class="input" />
        <button type="submit" :disabled="exportPassword.length < 10 || exportPassword !== exportPasswordAgain" class="px-4 py-2 rounded-lg border border-border dark:border-border-dark text-sm disabled:opacity-40">{{ t('safe.exchange.export_go') }}</button>
      </form>

      <details class="space-y-2.5">
        <summary class="text-sm font-semibold cursor-pointer">{{ t('safe.exchange.plain') }}</summary>
        <form class="space-y-2.5 pt-2" @submit.prevent="exportPlain">
          <p class="text-xs text-danger">{{ t('safe.exchange.plain_body') }}</p>
          <input v-model="plainPassword" type="password" :placeholder="t('safe.unlock.password')" :aria-label="t('safe.unlock.password')" autocomplete="off" :class="input" />
          <input v-model="plainPhrase" :placeholder="t('safe.exchange.plain_type', { phrase })" :aria-label="t('safe.exchange.plain_type', { phrase })" autocomplete="off" spellcheck="false" :class="input" />
          <button type="submit" :disabled="!plainPassword || plainPhrase.trim() !== phrase" class="px-4 py-2 rounded-lg text-sm text-danger border border-danger/40 hover:bg-danger/10 disabled:opacity-40">{{ t('safe.exchange.plain_go') }}</button>
        </form>
      </details>

      <p v-if="notice" class="text-sm text-success" role="status">{{ notice }}</p>
      <p v-if="error" class="text-sm text-danger" role="alert">{{ error }}</p>
    </div>
  </ModalDialog>
</template>
