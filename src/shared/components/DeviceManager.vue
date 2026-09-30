<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Smartphone, Monitor, ShieldOff, Plus, Loader2, RefreshCw, Info } from 'lucide-vue-next';
import { useDevicePairing } from '../../composables/useDevicePairing';
import DevicePairing from './DevicePairing.vue';
import ConfirmModal from './ConfirmModal.vue';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();

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
    revocationSuccess.value = t('settings.devices.revoked', { name: removeTarget.value.name });
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
  if (!timestamp) return t('settings.devices.never');
  const diff = Math.floor(Date.now() / 1000) - timestamp;
  if (diff < 60) return t('settings.devices.just_now');
  if (diff < 3600) return t('settings.devices.minutes_ago', { n: Math.floor(diff / 60) });
  if (diff < 86400) return t('settings.devices.hours_ago', { n: Math.floor(diff / 3600) });
  if (diff < 604800) return t('settings.devices.days_ago', { n: Math.floor(diff / 86400) });
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
    // Sort by last_seen descending
    return b.last_seen - a.last_seen;
  });
});
</script>

<template>
  <div class="space-y-4">
    <!-- Section Header -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-3">
        <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider">{{ $t('settings.devices.paired') }}</h4>
        <!--
          Counts disconnections, and nothing more. It was labelled "Security
          Epoch", which reads as a key generation the vault has advanced
          through — it is not one. The counter is local and deliberately does
          not feed key derivation, because an epoch key would have to travel to
          the other devices over the only channel available: the vault key a
          disconnected device still holds.
        -->
        <div class="flex items-center gap-1 px-2 py-0.5 rounded-md bg-gray-100 dark:bg-surface-hover-dark border border-border dark:border-border-subtle-dark" :title="$t('settings.devices.disconnections_hint')">
          <span class="text-xs font-medium text-gray-500 dark:text-gray-400">{{ $t('settings.devices.disconnections', { n: currentEpoch }) }}</span>
          <Info class="w-3 h-3 text-gray-500 dark:text-gray-400" />
        </div>
      </div>
      <div class="flex items-center gap-2">
        <button
          @click="loadDevices"
          :disabled="isLoading"
          class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-[#333] text-gray-500 dark:text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 transition-colors cursor-pointer disabled:opacity-50"
          :title="$t('settings.devices.refresh')"
          :aria-label="$t('settings.devices.refresh')"
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
    <div class="bg-[#f8f8f8] dark:bg-surface-dark rounded-xl border border-border dark:border-border-dark overflow-hidden">
      <!-- Loading State -->
      <div v-if="isLoading && devices.length === 0" class="flex items-center justify-center py-8 gap-3">
        <Loader2 class="w-5 h-5 text-gray-500 dark:text-gray-400 animate-spin" />
        <span class="text-[13px] text-gray-500 dark:text-gray-400">{{ $t('settings.devices.loading') }}</span>
      </div>

      <!-- Empty State -->
      <div v-else-if="devices.length === 0" class="flex flex-col items-center justify-center py-8 px-4 text-center">
        <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-surface-hover-dark flex items-center justify-center mb-3">
          <Smartphone class="w-6 h-6 text-gray-500 dark:text-gray-400" />
        </div>
        <p class="text-[13px] font-medium text-text dark:text-text-dark mb-1">{{ $t('settings.devices.empty') }}</p>
        <p class="text-xs text-gray-500 dark:text-gray-400 mb-4 max-w-[240px]">{{ $t('settings.devices.empty_hint') }}</p>
        <button
          @click="handleAddDevice"
          class="btn-primary"
        >
          <Plus class="w-3.5 h-3.5" />
          {{ $t('settings.devices.add') }}
        </button>
      </div>

      <!-- Device List -->
      <template v-else>
        <div
          v-for="(device, index) in sortedDevices"
          :key="device.node_id_hex"
          class="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-white/60 dark:hover:bg-[#252525]"
          :class="index > 0 ? 'border-t border-border dark:border-border-dark' : ''"
        >
          <!-- Device Icon -->
          <div class="w-9 h-9 rounded-lg bg-white dark:bg-surface-hover-dark border border-border dark:border-border-subtle-dark flex items-center justify-center shrink-0">
            <Monitor v-if="device.device_name?.toLowerCase().includes('desktop') || device.device_name?.toLowerCase().includes('mac') || device.device_name?.toLowerCase().includes('pc') || device.device_name?.toLowerCase().includes('linux')" class="w-4 h-4 text-gray-500 dark:text-gray-400" />
            <Smartphone v-else class="w-4 h-4 text-gray-500 dark:text-gray-400" />
          </div>

          <!-- Device Info -->
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <p class="text-[13px] font-medium text-text dark:text-text-dark truncate">{{ device.device_name || $t('settings.devices.unknown') }}</p>
            </div>
            <div class="flex items-center gap-3 mt-0.5">
              <span class="text-xs text-gray-500 dark:text-gray-400">{{ $t('settings.devices.last_seen', { when: formatLastSeen(device.last_seen) }) }}</span>
              <span class="text-xs text-gray-500 dark:text-gray-400">· {{ $t('settings.devices.paired_on', { date: formatPairedDate(device.paired_at) }) }}</span>
            </div>
          </div>

          <!-- Disconnect Button -->
          <button
            @click="confirmRemove(device.node_id_hex, device.device_name || t('settings.devices.unknown'))"
            class="flex items-center gap-1 px-2 py-1 rounded-lg hover:bg-red-50 dark:hover:bg-red-900/20 text-gray-500 dark:text-gray-400 hover:text-red-500 dark:hover:text-red-400 transition-colors cursor-pointer shrink-0 group"
            :title="$t('settings.devices.disconnect_hint')"
            :aria-label="$t('settings.devices.disconnect_hint')"
          >
            <ShieldOff class="w-3.5 h-3.5" />
            <span class="text-xs font-medium hidden group-hover:inline">{{ $t('settings.general.disconnect_p2p') }}</span>
          </button>
        </div>

        <!-- Add Device Button (at bottom of list) -->
        <div class="border-t border-border dark:border-border-dark px-4 py-3">
          <button
            @click="handleAddDevice"
            class="w-full px-4 py-2 border-2 border-dashed border-border-subtle dark:border-border-subtle-dark hover:border-emerald-400 dark:hover:border-emerald-600 rounded-lg text-[12px] font-medium text-gray-500 dark:text-gray-400 hover:text-emerald-600 dark:hover:text-emerald-400 transition-all flex items-center justify-center gap-2 cursor-pointer"
          >
            <Plus class="w-3.5 h-3.5" />
            {{ $t('settings.devices.add') }}
          </button>
        </div>
      </template>
    </div>

    <!-- Error Message -->
    <div v-if="error" class="text-xs text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
      ⚠️ {{ error }}
    </div>

    <!-- Pairing Modal -->
    <DevicePairing
      :show="showPairingModal"
      @close="showPairingModal = false"
      @paired="handlePaired"
    />

    <!-- Confirm Disconnect Modal -->
    <!--
      The wording here used to promise a lock-out and a re-encryption, and the
      code does neither. Every device holds the same vault key, derived from the
      one recovery phrase, so there is no key to rotate away from a device and
      nothing this button can make unreadable.

      It does not point at changing the recovery phrase either, tempting as that
      is to write: `setup_e2ee` refuses outright once a key exists, and no other
      command replaces one. Sending someone who has just lost a laptop after a
      feature that is not there would be worse than the overpromise it replaced.

      So it says what happens and where that stops, and nothing else.
    -->
    <ConfirmModal
      :show="showConfirmRemove"
      :title="$t('settings.devices.confirm_title')"
      :message="$t('settings.devices.confirm_message', { name: removeTarget?.name ?? '' })"
      :confirm-text="$t('settings.general.disconnect_p2p')"
      :is-destructive="true"
      @confirm="handleRevokeConfirmed"
      @cancel="handleRevokeCancelled"
    />
  </div>
</template>
