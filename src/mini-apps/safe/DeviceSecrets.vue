<script setup lang="ts">
/**
 * "This device": the secrets Synabit keeps in this machine's keychain beside
 * the Safe — provider keys, connector tokens, the Telegram bot, the sync key,
 * the PIN. Names only. They live outside the Safe because they must work
 * while it is locked; this is where they can at least be seen, and the ones
 * no other screen manages can be forgotten.
 */
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Bot, Cable, KeyRound, Lock, RefreshCw } from 'lucide-vue-next';
import ConfirmModal from '../../shared/components/ConfirmModal.vue';
import type { DeviceSecret, SafeApi } from './api';

const props = defineProps<{ api: SafeApi }>();
const emit = defineEmits<{ (e: 'error', err: unknown): void }>();
const { t } = useI18n();

const secrets = ref<DeviceSecret[] | null>(null);
const forgetting = ref<DeviceSecret | null>(null);

const ICONS = { sync_key: RefreshCw, app_lock_pin: Lock, provider: KeyRound, telegram: Bot, connector: Cable } as const;

async function load() {
  try {
    secrets.value = await props.api.deviceSecrets();
  } catch (e) {
    emit('error', e);
  }
}

async function forget() {
  const s = forgetting.value;
  forgetting.value = null;
  if (!s) return;
  try {
    await props.api.forgetDeviceSecret(s.slot);
    await load();
  } catch (e) {
    emit('error', e);
  }
}

onMounted(load);
</script>

<template>
  <div class="h-full overflow-y-auto px-6 py-6 space-y-4">
    <div class="space-y-1">
      <h2 class="text-xl font-semibold">{{ t('safe.device.title') }}</h2>
      <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.device.body') }}</p>
    </div>
    <ul v-if="secrets?.length" class="rounded-xl border border-border dark:border-border-dark divide-y divide-border dark:divide-border-dark">
      <li v-for="s in secrets" :key="s.kind + s.slot" class="px-4 py-3 flex items-center gap-3">
        <component :is="ICONS[s.kind]" class="w-4 h-4 text-text-secondary dark:text-text-secondary-dark flex-shrink-0" />
        <div class="flex-1 min-w-0">
          <!-- The sync key and the PIN have no name of their own; their kind is the name. -->
          <p class="text-sm font-medium truncate">{{ s.slot ? s.name : t(`safe.device.kind.${s.kind}`) }}</p>
          <p v-if="s.slot" class="text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ t(`safe.device.kind.${s.kind}`) }}</p>
        </div>
        <button v-if="s.forgettable" class="px-3 py-1.5 rounded-lg text-sm text-danger hover:bg-danger/10" @click="forgetting = s">
          {{ t('safe.device.forget') }}
        </button>
        <span v-else class="text-xs text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.device.elsewhere') }}</span>
      </li>
    </ul>
    <p v-else-if="secrets" class="text-sm text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.device.none') }}</p>

    <ConfirmModal
      :show="!!forgetting"
      :title="t('safe.device.forget_title', { name: forgetting?.name ?? '' })"
      :message="t('safe.device.forget_body')"
      :confirm-text="t('safe.device.forget')"
      :cancel-text="t('safe.editor.cancel')"
      is-destructive
      @confirm="forget"
      @cancel="forgetting = null"
    />
  </div>
</template>
