<script setup lang="ts">
/**
 * Family-safe answers, where simple mode can reach them.
 *
 * Simple mode hides the Syn tab with the rest of Syn — and with it the one
 * switch a parent setting the app up for somebody else most needs. Syn can
 * still be reached from outside the app (Telegram, a routine), so the switch
 * has to stay somewhere. This is that switch alone, in General, and only in
 * simple mode.
 *
 * It is the same setting as the one on the Syn tab and changes it the same
 * way: through `setFamilySafe`, which the backend guards. Switching it *off*
 * asks for the app-lock PIN by the same rule (`needsPinToSave`) and hands the
 * PIN to the backend, which checks it again. `syn_save_settings` no longer
 * touches the flag, so saving the Syn settings would change nothing here.
 */
import { onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import LockScreen from './LockScreen.vue';
import { needsPinToSave, getFamilySafe, setFamilySafe } from '../../mini-apps/messages/familySafe';
import { useAppLockStore as useLock, pinErrorKey } from '../../stores/useAppLockStore';
import { showAppNotice } from '../../composables/useAppNotice';
import { SETTINGS_SAVED } from '../syn/useSynEnabled';

const props = defineProps<{ vaultPath: string }>();

const { t } = useI18n();
const appLock = useLock();
const isLoading = ref(true);

/** What is in force — what was loaded or last saved. */
const onDisk = ref(false);
const saving = ref(false);
const askingPin = ref(false);

const load = async () => {
  isLoading.value = true;
  try {
    onDisk.value = await getFamilySafe(props.vaultPath);
  } finally {
    isLoading.value = false;
  }
};

onMounted(load);

const apply = async (next: boolean, pin?: string) => {
  saving.value = true;
  try {
    await setFamilySafe(next, { pin, vaultPath: props.vaultPath });
    window.dispatchEvent(new CustomEvent(SETTINGS_SAVED));
  } catch (e) {
    showAppNotice(t(pinErrorKey(e) ?? 'syn.family_safe_failed'), 'error');
  } finally {
    // Read back rather than assumed: a refused change leaves the switch
    // showing what is actually in force.
    await load();
    saving.value = false;
  }
};

const toggle = () => {
  if (saving.value || isLoading.value) return;
  const next = !onDisk.value;
  if (needsPinToSave(onDisk.value, next, appLock.isEnabled)) {
    askingPin.value = true;
    return;
  }
  void apply(next);
};

/** The PIN the lock screen checked, handed on so the backend checks it too. */
const pinGiven = (pin?: string) => {
  askingPin.value = false;
  void apply(false, pin);
};
</script>

<template>
  <section>
    <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ t('settings.general.syn_heading') }}</h4>
    <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
      <div class="flex items-center justify-between">
        <div class="pr-4">
          <p id="simple-family-safe-label" class="text-[13px] font-medium text-text dark:text-text-dark">{{ t('syn.family_safe') }}</p>
          <p id="simple-family-safe-desc" class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ t('syn.family_safe_hint') }}</p>
          <p v-if="appLock.isEnabled && onDisk" class="text-xs text-gray-500 dark:text-gray-400 mt-1">{{ t('syn.family_safe_pin') }}</p>
        </div>
        <button
          type="button"
          role="switch"
          :aria-checked="onDisk"
          :disabled="saving || isLoading"
          aria-labelledby="simple-family-safe-label"
          aria-describedby="simple-family-safe-desc"
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full focus:outline-none focus-visible:ring-2 focus-visible:ring-accent transition-colors duration-200 ease-in-out disabled:opacity-50"
          :class="onDisk ? 'bg-accent' : 'bg-gray-300 dark:bg-gray-600'"
          @click="toggle"
        >
          <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="onDisk ? 'translate-x-2' : '-translate-x-2'"></span>
        </button>
      </div>
    </div>

    <LockScreen
      v-if="askingPin"
      :title="t('syn.family_safe_pin_title')"
      @unlocked="pinGiven"
      @cancelled="askingPin = false"
    />
  </section>
</template>
