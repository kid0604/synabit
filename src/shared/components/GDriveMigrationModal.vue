<template>
  <div v-if="show" class="fixed inset-0 z-[9999] flex items-center justify-center">
    <div class="absolute inset-0 bg-black/60 backdrop-blur-sm"></div>
    <div class="relative bg-white dark:bg-[#1e1e1e] w-full max-w-md rounded-2xl shadow-2xl overflow-hidden border border-gray-200 dark:border-[#333] transform transition-all p-6 text-center">
      
      <!-- Icon -->
      <div class="w-16 h-16 bg-blue-100 dark:bg-blue-900/30 rounded-2xl mx-auto flex items-center justify-center mb-5 border border-blue-200 dark:border-blue-800/50">
        <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="text-blue-600 dark:text-blue-400">
          <path d="M14.4 16a2 2 0 0 1-1.6.8H5a2 2 0 0 1-1.6-3.2l4-5.6a2 2 0 0 1 1.6-.8h7.8a2 2 0 0 1 1.6.8l4 5.6a2 2 0 0 1-1.6 3.2z"/>
          <path d="M8.5 22h7.8a2 2 0 0 0 1.6-.8l4-5.6"/>
          <path d="M5 16l-4-5.6a2 2 0 0 1 1.6-3.2h7.8"/>
        </svg>
      </div>

      <!-- Content -->
      <h3 class="text-[18px] font-bold text-[#1c1c1e] dark:text-[#f4f4f5] mb-2 tracking-tight">
        Google Drive Sync Upgrade
      </h3>
      <p class="text-[13px] text-gray-500 dark:text-gray-400 mb-6 leading-relaxed">
        Synabit has upgraded its Sync Engine. Your Google Drive files are currently stored in a hidden system cache folder. To ensure your data is safe and easily accessible, please choose a visible folder on your computer to serve as your new Local Vault.
        <br><br>
        We will automatically move your existing files there.
      </p>

      <!-- Error State -->
      <div v-if="errorMsg" class="mb-5 p-3 bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-[12px] rounded-xl text-left flex gap-2 items-start border border-red-100 dark:border-red-900/30">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0 mt-0.5"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="8" x2="12" y2="12"></line><line x1="12" y1="16" x2="12.01" y2="16"></line></svg>
        <span>{{ errorMsg }}</span>
      </div>

      <!-- Action -->
      <button 
        @click="handleSelectFolder" 
        :disabled="loading"
        class="w-full flex items-center justify-center gap-2 bg-blue-600 hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed text-white text-[14px] font-medium py-3 rounded-xl transition-all shadow-md hover:shadow-lg"
      >
        <svg v-if="loading" class="animate-spin -ml-1 mr-2 h-4 w-4 text-white" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
          <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
          <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
        </svg>
        <svg v-else xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
        {{ loading ? 'Migrating files...' : 'Select Target Folder...' }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

const props = defineProps<{
  show: boolean
}>();

const emit = defineEmits<{
  (e: 'migrated', newPath: string): void
}>();

const loading = ref(false);
const errorMsg = ref('');

async function handleSelectFolder() {
  errorMsg.value = '';
  
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: 'Select an empty folder for your Local Vault'
    });
    
    if (selected && typeof selected === 'string') {
      loading.value = true;
      
      try {
        const newPath = await invoke<string>('migrate_gdrive_vault', {
          newTargetPath: selected
        });
        emit('migrated', newPath);
      } catch (err: any) {
        errorMsg.value = 'Migration failed: ' + (err?.toString() || 'Unknown error');
      } finally {
        loading.value = false;
      }
    }
  } catch (err: any) {
    errorMsg.value = 'Failed to open directory picker: ' + (err?.toString() || 'Unknown error');
  }
}
</script>
