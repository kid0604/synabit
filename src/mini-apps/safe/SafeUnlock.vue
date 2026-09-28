<script setup lang="ts">
/**
 * The locked Safe: a master password, and the Secret Key when this device's
 * keychain does not have it.
 *
 * Nothing about the Safe shows here — not how many items it holds, not their
 * names. A locked Safe looks the same whether it is empty or full.
 */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Loader2, Lock } from 'lucide-vue-next';
import { safeCode, type SafeApi } from './api';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi; needsSecretKey: boolean }>();
const emit = defineEmits<{ (e: 'unlocked'): void }>();
const { t } = useI18n();
const explain = useSafeError();

const password = ref('');
const secretKey = ref('');
const askSecretKey = ref(props.needsSecretKey);
const busy = ref(false);
const error = ref('');

async function unlock() {
  if (!password.value || busy.value) return;
  busy.value = true;
  error.value = '';
  try {
    await props.api.unlock(password.value, askSecretKey.value ? secretKey.value : undefined);
    password.value = '';
    secretKey.value = '';
    emit('unlocked');
  } catch (e) {
    if (safeCode(e) === 'needs_secret_key') askSecretKey.value = true;
    error.value = explain(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="h-full flex items-center justify-center px-4" data-tauri-drag-region>
    <form class="w-full max-w-sm space-y-5" @submit.prevent="unlock">
      <div class="flex flex-col items-center gap-3 text-center">
        <div class="w-14 h-14 rounded-2xl bg-accent/10 flex items-center justify-center">
          <Lock class="w-6 h-6 text-accent" />
        </div>
        <h1 class="text-xl font-semibold">{{ t('safe.unlock.title') }}</h1>
        <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.unlock.body') }}</p>
      </div>

      <label class="block space-y-1.5">
        <span class="sr-only">{{ t('safe.unlock.password') }}</span>
        <input
          v-model="password"
          type="password"
          :placeholder="t('safe.unlock.password')"
          autocomplete="off" spellcheck="false" autocapitalize="off"
          class="w-full px-3 py-2.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark focus:outline-none focus:ring-2 focus:ring-accent"
          autofocus
        />
      </label>

      <label v-if="askSecretKey" class="block space-y-1.5">
        <span class="text-sm font-medium">{{ t('safe.unlock.secret_key') }}</span>
        <p class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.unlock.secret_key_body') }}</p>
        <textarea
          v-model="secretKey"
          rows="3"
          autocomplete="off" spellcheck="false" autocapitalize="off"
          class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark font-mono text-sm focus:outline-none focus:ring-2 focus:ring-accent"
        />
      </label>

      <p v-if="error" class="text-sm text-danger text-center" role="alert">{{ error }}</p>

      <button type="submit" :disabled="!password || busy" class="w-full py-2.5 rounded-xl bg-accent text-white font-medium hover:opacity-90 disabled:opacity-40 inline-flex items-center justify-center gap-2">
        <Loader2 v-if="busy" class="w-4 h-4 animate-spin" />
        {{ busy ? t('safe.unlock.unlocking') : t('safe.unlock.submit') }}
      </button>
    </form>
  </div>
</template>
