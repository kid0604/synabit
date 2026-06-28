<template>
  <div class="space-y-6">
    <!-- Sync Policies -->
    <div class="space-y-3">
      <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-2">{{ $t('settings.mobile.policies', 'Cellular Policies') }}</h4>
      <div class="flex items-center justify-between p-3 rounded-xl bg-gray-50 dark:bg-black/20 border border-gray-100 dark:border-white/5">
        <div>
          <h5 class="text-sm font-medium text-gray-900 dark:text-gray-100">{{ $t('settings.mobile.cellular_sync', 'Sync on Cellular') }}</h5>
          <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.mobile.cellular_sync_desc', 'Allow background syncing over mobile data') }}</p>
        </div>
        <select 
          v-model="policy"
          @change="updatePolicy"
          class="bg-white dark:bg-[#222] border border-gray-200 dark:border-white/10 rounded-lg text-sm px-3 py-1.5 focus:outline-none focus:ring-2 focus:ring-blue-500/50"
        >
          <option value="all">All Data</option>
          <option value="text_only">Text Only</option>
          <option value="off">Off</option>
        </select>
      </div>
    </div>

    <!-- Data Usage Stats -->
    <div class="space-y-3">
      <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-2">{{ $t('settings.mobile.data_usage', 'Data Usage (Today)') }}</h4>
      <div class="grid grid-cols-2 gap-3">
        <!-- Cellular -->
        <div class="p-4 rounded-xl bg-gray-50 dark:bg-black/20 border border-gray-100 dark:border-white/5">
          <div class="flex items-center gap-2 mb-3">
            <div class="w-2 h-2 rounded-full bg-blue-500"></div>
            <span class="text-sm font-medium text-gray-700 dark:text-gray-300">Cellular</span>
          </div>
          <div class="space-y-1">
            <div class="flex justify-between text-xs">
              <span class="text-gray-500">Sent</span>
              <span class="font-medium text-gray-900 dark:text-gray-100">{{ formatBytes(metrics.cellular_bytes_tx) }}</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-gray-500">Received</span>
              <span class="font-medium text-gray-900 dark:text-gray-100">{{ formatBytes(metrics.cellular_bytes_rx) }}</span>
            </div>
          </div>
        </div>
        
        <!-- Wi-Fi -->
        <div class="p-4 rounded-xl bg-gray-50 dark:bg-black/20 border border-gray-100 dark:border-white/5">
          <div class="flex items-center gap-2 mb-3">
            <div class="w-2 h-2 rounded-full bg-green-500"></div>
            <span class="text-sm font-medium text-gray-700 dark:text-gray-300">Wi-Fi</span>
          </div>
          <div class="space-y-1">
            <div class="flex justify-between text-xs">
              <span class="text-gray-500">Sent</span>
              <span class="font-medium text-gray-900 dark:text-gray-100">{{ formatBytes(metrics.wifi_bytes_tx) }}</span>
            </div>
            <div class="flex justify-between text-xs">
              <span class="text-gray-500">Received</span>
              <span class="font-medium text-gray-900 dark:text-gray-100">{{ formatBytes(metrics.wifi_bytes_rx) }}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../stores/useAppStore';

const appStore = useAppStore();
const policy = ref(appStore.p2pCellularPolicy);

const metrics = ref({
  cellular_bytes_tx: 0,
  cellular_bytes_rx: 0,
  wifi_bytes_tx: 0,
  wifi_bytes_rx: 0,
});

function formatBytes(bytes: number, decimals = 2) {
  if (!+bytes) return '0 B';
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(dm))} ${sizes[i]}`;
}

async function updatePolicy() {
  appStore.p2pCellularPolicy = policy.value;
  const store = appStore.getStoreInstance();
  if (store) {
    await store.set('p2pCellularPolicy', policy.value);
    await store.save();
  }
}

onMounted(async () => {
  try {
    // Format YYYY-MM-DD
    const date = new Date();
    const yyyy = date.getFullYear();
    const mm = String(date.getMonth() + 1).padStart(2, '0');
    const dd = String(date.getDate()).padStart(2, '0');
    const today = `${yyyy}-${mm}-${dd}`;
    
    const data = await invoke<any>('p2p_sync_metrics', { date: today });
    metrics.value = data;
  } catch (e) {
    console.error('Failed to load sync metrics:', e);
  }
});
</script>
