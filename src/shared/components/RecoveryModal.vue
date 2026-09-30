<template>
  <!--
    Shown when the phone's secure storage could not be read at startup.

    It used to offer "Reset secure storage", which called a `reset_secure_store`
    command that never existed in the backend, from a dialog that could not be
    closed: the one button that looked like a way out was an error, and the
    other quit. There is deliberately still no reset. Wiping the store would
    delete this device's sync key along with everything else in it, which a
    read that failed once is no reason to do — and `SecureStore.java` already
    recreates a store that is actually corrupt. So it offers what works: try
    again, carry on, or close.
  -->
  <AppDialog
    :show="isOpen"
    labelledby="recovery-title"
    unstyled
    @close="close"
  >
    <div class="bg-white dark:bg-gray-800 rounded-2xl shadow-2xl p-6 w-full border border-red-200 dark:border-red-900/30">
      <div class="flex items-center gap-3 mb-4 text-red-600 dark:text-red-400">
        <AlertTriangle class="w-8 h-8" />
        <h2 id="recovery-title" class="text-xl font-bold">{{ $t('shell.recovery.title') }}</h2>
      </div>

      <p class="text-gray-600 dark:text-gray-300 mb-4 text-sm leading-relaxed">
        {{ $t('shell.recovery.body') }}
      </p>

      <p class="text-gray-600 dark:text-gray-300 mb-6 text-sm leading-relaxed font-semibold">
        {{ $t('shell.recovery.body_safe') }}
      </p>

      <div class="flex flex-col gap-3">
        <button type="button" class="btn-primary w-full justify-center" @click="tryAgain">
          {{ $t('shell.recovery.try_again') }}
        </button>
        <button type="button" class="btn-secondary w-full justify-center" @click="close">
          {{ $t('shell.recovery.continue') }}
        </button>
        <button type="button" class="btn-secondary w-full justify-center" @click="exitApp">
          {{ $t('shell.recovery.exit') }}
        </button>
      </div>
    </div>
  </AppDialog>
</template>

<script setup lang="ts">
import { exit } from '@tauri-apps/plugin-process';
import { AlertTriangle } from 'lucide-vue-next';
import AppDialog from './AppDialog.vue';

defineProps<{ isOpen: boolean }>();
const emit = defineEmits<{ (e: 'update:isOpen', value: boolean): void }>();

/** Read the store again from the start: the usual cause is a keystore that
 *  was busy or not yet unlocked, which a second look gets past. */
function tryAgain() {
  window.location.reload();
}

/** Carry on without it. Whatever needs the store says so when it is used. */
function close() {
  emit('update:isOpen', false);
}

async function exitApp() {
  await exit(0);
}
</script>
