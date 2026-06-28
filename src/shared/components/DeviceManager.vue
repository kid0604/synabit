<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Smartphone, Monitor, ShieldOff, Plus, Loader2, RefreshCw, Info } from 'lucide-vue-next';
import { useDevicePairing } from '../../composables/useDevicePairing';
import DevicePairing from './DevicePairing.vue';
import ConfirmModal from './ConfirmModal.vue';

const {
  devices, isLoading, error,
  loadDevices, removeDevice,
} = useDevicePairing();

// UI State
const showPairingModal = ref(false);
const showConfirmRemove = ref(false);
const removeTarget = ref<{ nodeIdHex: string; name: string } | null>(null);
const currentEpoch = ref<number>(0);
const isRevoking = ref(false);
const revocationSuccess = ref('');
let revocationTimer: number | null = null;

async function fetchEpoch() {
  try {
    currentEpoch.value = await invoke<number>('p2p_current_epoch');
  } catch {
    // Epoch display is best-effort; don't block the UI
  }
}

onMounted(() => {
  loadDevices();
  fetchEpoch();
});

const handleAddDevice = () => {
  showPairingModal.value = true;
};

const handlePaired = () => {
  showPairingModal.value = false;
  loadDevices();
};

const confirmRemove = (nodeIdHex: string, name: string) => {
  removeTarget.value = { nodeIdHex, name };
  showConfirmRemove.value = true;
};

const handleRevokeConfirmed = async () => {
  if (!removeTarget.value) return;
  isRevoking.value = true;
  try {
    const newEpoch = await invoke<number>('p2p_revoke_device', {
      nodeIdHex: removeTarget.value.nodeIdHex,
    });
    currentEpoch.value = newEpoch;
    await loadDevices();
    revocationSuccess.value = `"${removeTarget.value.name}" has been revoked. Security epoch advanced to #${newEpoch}.`;
    // Auto-dismiss success banner after 6 seconds
    if (revocationTimer) window.clearTimeout(revocationTimer);
    revocationTimer = window.setTimeout(() => {
      revocationSuccess.value = '';
    }, 6000);
  } catch (e: any) {
    // Fall back to plain remove if revoke command isn't available yet
    await removeDevice(removeTarget.value.nodeIdHex);
  } finally {
    isRevoking.value = false;
    showConfirmRemove.value = false;
    removeTarget.value = null;
  }
};

const handleRevokeCancelled = () => {
  showConfirmRemove.value = false;
  removeTarget.value = null;
};

const formatLastSeen = (timestamp: number): string => {
  if (!timestamp) return 'Never';
  const diff = Math.floor(Date.now() / 1000) - timestamp;
  if (diff < 60) return 'Just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;
  return new Date(timestamp * 1000).toLocaleDateString();
};

const formatPairedDate = (timestamp: number): string => {
  if (!timestamp) return '';
  return new Date(timestamp * 1000).toLocaleDateString(undefined, {
    month: 'short',
    day: 'numeric',
    year: 'numeric',
  });
};

const sortedDevices = computed(() => {
  return [...devices.value].sort((a, b) => {
    // Online first, then by last_seen descending
    if (a.is_online !== b.is_online) return a.is_online ? -1 : 1;
    return b.last_seen - a.last_seen;
  });
});
</script>

<template>
  <div class="space-y-4">
    <!-- Section Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Paired Devices</h4>
        <!-- Security Epoch Badge -->
        <div class="flex items-center gap-1 px-2 py-0.5 rounded-md bg-gray-100 dark:bg-[#2a2a2a] border border-[#e6e6e6] dark:border-[#3a3a3a]" :title="'Epoch increments when a device is revoked'">
          <span class="text-[10px] font-medium text-gray-500 dark:text-gray-400">Security Epoch: #{{ currentEpoch }}</span>
          <Info class="w-3 h-3 text-gray-400 dark:text-gray-500" />
        </div>
      </div>
      <div class="flex items-center gap-2">
        <button
          @click="loadDevices"
          :disabled="isLoading"
          class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-[#333] text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 transition-colors cursor-pointer disabled:opacity-50"
          title="Refresh device list"
        >
          <RefreshCw class="w-3.5 h-3.5" :class="isLoading ? 'animate-spin' : ''" />
        </button>
      </div>
    </div>

    <!-- Revocation Success Banner -->
    <div v-if="revocationSuccess" class="flex items-center gap-2 text-[12px] text-emerald-700 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-900/20 px-3 py-2 rounded-lg border border-emerald-200 dark:border-emerald-800">
      ✅ {{ revocationSuccess }}
    </div>

    <!-- Device List Card -->
    <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden">
      <!-- Loading State -->
      <div v-if="isLoading && devices.length === 0" class="flex items-center justify-center py-8 gap-3">
        <Loader2 class="w-5 h-5 text-gray-400 animate-spin" />
        <span class="text-[13px] text-gray-400">Loading devices...</span>
      </div>

      <!-- Empty State -->
      <div v-else-if="devices.length === 0" class="flex flex-col items-center justify-center py-8 px-4 text-center">
        <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-[#2a2a2a] flex items-center justify-center mb-3">
          <Smartphone class="w-6 h-6 text-gray-400" />
        </div>
        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] mb-1">No paired devices</p>
        <p class="text-[11px] text-gray-400 dark:text-gray-500 mb-4 max-w-[240px]">Pair another device to sync your data across multiple devices.</p>
        <button
          @click="handleAddDevice"
          class="px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white rounded-lg text-[12px] font-medium transition-all shadow-sm flex items-center gap-2 cursor-pointer"
        >
          <Plus class="w-3.5 h-3.5" />
          Add Device
        </button>
      </div>

      <!-- Device List -->
      <template v-else>
        <div
          v-for="(device, index) in sortedDevices"
          :key="device.node_id_hex"
          class="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-white/60 dark:hover:bg-[#252525]"
          :class="index > 0 ? 'border-t border-[#e6e6e6] dark:border-[#2c2c2c]' : ''"
        >
          <!-- Device Icon -->
          <div class="w-9 h-9 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e6e6e6] dark:border-[#3a3a3a] flex items-center justify-center shrink-0">
            <Monitor v-if="device.device_name?.toLowerCase().includes('desktop') || device.device_name?.toLowerCase().includes('mac') || device.device_name?.toLowerCase().includes('pc') || device.device_name?.toLowerCase().includes('linux')" class="w-4 h-4 text-gray-500" />
            <Smartphone v-else class="w-4 h-4 text-gray-500" />
          </div>

          <!-- Device Info -->
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ device.device_name || 'Unknown Device' }}</p>
              <!-- Online/Offline indicator -->
              <div class="flex items-center gap-1">
                <div :class="['w-1.5 h-1.5 rounded-full', device.is_online ? 'bg-emerald-500' : 'bg-gray-300 dark:bg-gray-600']"></div>
                <span :class="['text-[10px] font-medium', device.is_online ? 'text-emerald-600 dark:text-emerald-400' : 'text-gray-400 dark:text-gray-500']">
                  {{ device.is_online ? 'Online' : 'Offline' }}
                </span>
              </div>
            </div>
            <div class="flex items-center gap-3 mt-0.5">
              <span class="text-[11px] text-gray-400 dark:text-gray-500">Last seen: {{ formatLastSeen(device.last_seen) }}</span>
              <span class="text-[11px] text-gray-400 dark:text-gray-500">· Paired {{ formatPairedDate(device.paired_at) }}</span>
              <span class="text-[10px] font-medium text-gray-400 dark:text-gray-500 px-1.5 py-0.5 rounded bg-gray-100 dark:bg-[#2a2a2a]">Epoch #{{ currentEpoch }}</span>
            </div>
          </div>

          <!-- Revoke Button -->
          <button
            @click="confirmRemove(device.node_id_hex, device.device_name || 'Unknown Device')"
            class="flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-red-50 dark:hover:bg-red-900/20 text-gray-400 hover:text-red-500 dark:hover:text-red-400 transition-colors cursor-pointer shrink-0 group"
            title="Revoke device access"
          >
            <ShieldOff class="w-3.5 h-3.5" />
            <span class="text-[11px] font-medium hidden group-hover:inline">Revoke</span>
          </button>
        </div>

        <!-- Add Device Button (at bottom of list) -->
        <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] px-4 py-3">
          <button
            @click="handleAddDevice"
            class="w-full px-4 py-2 border-2 border-dashed border-[#e0e0e0] dark:border-[#3a3a3a] hover:border-emerald-400 dark:hover:border-emerald-600 rounded-lg text-[12px] font-medium text-gray-500 dark:text-gray-400 hover:text-emerald-600 dark:hover:text-emerald-400 transition-all flex items-center justify-center gap-2 cursor-pointer"
          >
            <Plus class="w-3.5 h-3.5" />
            Add Device
          </button>
        </div>
      </template>
    </div>

    <!-- Error Message -->
    <div v-if="error" class="text-[11px] text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
      ⚠️ {{ error }}
    </div>

    <!-- Pairing Modal -->
    <DevicePairing
      :show="showPairingModal"
      @close="showPairingModal = false"
      @paired="handlePaired"
    />

    <!-- Confirm Revoke Modal -->
    <ConfirmModal
      :show="showConfirmRemove"
      title="Revoke Device?"
      :message="`This will lock out '${removeTarget?.name}' and re-encrypt all data. The device will no longer be able to sync.`"
      confirm-text="Revoke Device"
      :is-destructive="true"
      @confirm="handleRevokeConfirmed"
      @cancel="handleRevokeCancelled"
    />
  </div>
</template>
