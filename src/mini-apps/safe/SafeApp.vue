<script setup lang="ts">
/**
 * Safe: the secret manager. Design in `docs/safe-2026-09-28.md`.
 *
 * Three screens, chosen by what Rust says rather than by anything remembered
 * here: no Safe yet → setup; a Safe that is locked → unlock; open → the Safe.
 * The status is asked for again every time the app is shown, because the Safe
 * can lock itself while another app is on screen, and a screen that still
 * looks open over a locked Safe is a screen full of errors.
 */
import { onActivated, onMounted, onUnmounted, ref, watch } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Loader2 } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import { LOCKED_EVENT, useSafeApi, type Status } from './api';
import { useEventBus } from '../../composables/useEventBus';
import SafeSetup from './SafeSetup.vue';
import SafeUnlock from './SafeUnlock.vue';
import SafeMain from './SafeMain.vue';
import { logger } from '../../utils/logger';

const props = defineProps<{ vaultPath: string }>();
const { t } = useI18n();

const api = useSafeApi(() => props.vaultPath);
type Screen = 'loading' | 'setup' | 'locked' | 'open' | 'no_vault';
const screen = ref<Screen>('loading');
const status = ref<Status | null>(null);
/** An item a link asked for, shown once the Safe is open. */
const wanted = ref<string | null>(null);
/**
 * Setup has created the Safe and is showing the Secret Key, or checking it
 * was written down. The Safe is open by then; the screen must not jump to it
 * — leaving for another tool and coming back would skip the words.
 */
const settingUp = ref(false);

function openItemById(id: string) {
  wanted.value = id;
  void refresh();
}
defineExpose({ openItemById });

async function refresh() {
  if (!props.vaultPath) {
    screen.value = 'no_vault';
    return;
  }
  if (settingUp.value) return;
  try {
    status.value = await api.status();
    screen.value = !status.value.exists ? 'setup' : status.value.unlocked ? 'open' : 'locked';
  } catch (e) {
    logger.error('[Safe] status failed', e);
    screen.value = 'locked';
  }
}

let unlisten: UnlistenFn | null = null;
onMounted(async () => {
  await refresh();
  unlisten = await listen(LOCKED_EVENT, () => {
    if (screen.value === 'open') screen.value = 'locked';
  });
});
onUnmounted(() => unlisten?.());
onActivated(refresh);
watch(
  () => props.vaultPath,
  () => {
    settingUp.value = false;
    void refresh();
  },
);

/**
 * A second device shows setup until sync brings the Safe; then it asks for
 * the password. Without this it stayed on setup, inviting a second Safe.
 */
const bus = useEventBus();
bus.on('vault:sync-completed', (payload) => {
  if (screen.value !== 'setup' || settingUp.value) return;
  const files = payload?.pulled_files;
  if (files && !files.some((f) => f.replace(/\\/g, '/').startsWith('Safe/'))) return;
  void refresh();
});

function setupDone() {
  settingUp.value = false;
  void refresh();
}
</script>

<template>
  <div class="h-full bg-base dark:bg-base-dark text-text dark:text-text-dark overflow-hidden">
    <div v-if="screen === 'loading'" class="h-full flex items-center justify-center text-text-tertiary dark:text-text-tertiary-dark">
      <Loader2 class="w-5 h-5 animate-spin" />
    </div>
    <div v-else-if="screen === 'no_vault'" class="h-full flex items-center justify-center px-6 text-center text-sm text-text-secondary dark:text-text-secondary-dark">
      {{ t('safe.no_vault') }}
    </div>
    <SafeSetup v-else-if="screen === 'setup'" :api="api" @started="settingUp = true" @done="setupDone" />
    <SafeUnlock
      v-else-if="screen === 'locked'"
      :api="api"
      :needs-secret-key="!!status && status.exists && !status.has_secret_key"
      @unlocked="refresh"
    />
    <SafeMain v-else-if="screen === 'open'" :api="api" :open-id="wanted" @opened="wanted = null" @locked="refresh" />
  </div>
</template>
