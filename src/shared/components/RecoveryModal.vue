<template>
  <div v-if="isOpen" class="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 backdrop-blur-sm">
    <div class="bg-white dark:bg-gray-800 rounded-2xl shadow-2xl p-6 w-[90%] max-w-md border border-red-200 dark:border-red-900/30">
      <div class="flex items-center gap-3 mb-4 text-red-600 dark:text-red-400">
        <AlertTriangle class="w-8 h-8" />
        <h2 class="text-xl font-bold">Secure Storage Error</h2>
      </div>
      
      <p class="text-gray-600 dark:text-gray-300 mb-4 text-sm leading-relaxed">
        The Android Keystore on your device is corrupted, invalidated, or unavailable. This typically happens if you changed your device's lock screen settings, removed biometrics, or restored the app from a backup.
      </p>
      
      <p class="text-gray-600 dark:text-gray-300 mb-6 text-sm leading-relaxed font-semibold">
        Your data is still safe, but you must reset the secure storage and re-authenticate to continue using the app.
      </p>
      
      <div class="flex flex-col gap-3">
        <button 
          @click="resetStorage" 
          class="w-full px-4 py-3 bg-red-600 hover:bg-red-700 text-white rounded-xl font-medium transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
          :disabled="isResetting"
        >
          <RefreshCw v-if="isResetting" class="w-5 h-5 animate-spin" />
          {{ isResetting ? 'Resetting...' : 'Reset Secure Storage' }}
        </button>
        <button 
          @click="exitApp" 
          class="w-full px-4 py-3 bg-gray-100 hover:bg-gray-200 dark:bg-gray-700 dark:hover:bg-gray-600 text-gray-700 dark:text-gray-200 rounded-xl font-medium transition-colors"
        >
          Exit App
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { exit } from '@tauri-apps/plugin-process';
import { AlertTriangle, RefreshCw } from 'lucide-vue-next';

const props = defineProps<{ isOpen: boolean }>();
const emit = defineEmits<{ (e: 'update:isOpen', value: boolean): void }>();

const isResetting = ref(false);

async function resetStorage() {
  try {
    isResetting.value = true;
    await invoke('reset_secure_store');
    // Reload the app completely to re-initialize
    window.location.reload();
  } catch (e) {
    console.error('Failed to reset secure store', e);
    alert('Failed to reset secure store: ' + e);
  } finally {
    isResetting.value = false;
  }
}

async function exitApp() {
  await exit(1);
}
</script>
