import { ref, computed, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logger } from '../utils/logger';

// ─── Types ──────────────────────────────────────────────────

interface PairingInfo {
  code: string;
  node_id_hex: string;
  expires_at: number;
}

interface PairedDevice {
  device_name: string;
  node_id_hex: string;
  paired_at: number;
  last_seen: number;
  is_online: boolean;
}

// ─── Composable ─────────────────────────────────────────────

/**
 * Composable for P2P device pairing.
 * Manages pairing flow (initiate/accept/cancel) and paired device list.
 *
 * Uses Tauri commands:
 *   p2p_pair_initiate, p2p_pair_accept, p2p_pair_cancel,
 *   p2p_list_devices, p2p_remove_device
 */
export function useDevicePairing() {

  // --- State ---
  const isPairing = ref(false);
  const pairingCode = ref('');
  const pairingExpiry = ref(0);
  const devices = ref<PairedDevice[]>([]);
  const isLoading = ref(false);
  const error = ref('');
  const pairingSuccess = ref(false);

  // Countdown
  const countdown = ref(0);
  let countdownTimer: number | null = null;

  function startCountdown() {
    stopCountdown();
    if (!pairingExpiry.value) return;

    const updateCountdown = () => {
      const now = Math.floor(Date.now() / 1000);
      const remaining = pairingExpiry.value - now;
      if (remaining <= 0) {
        countdown.value = 0;
        stopCountdown();
        isPairing.value = false;
        pairingCode.value = '';
        error.value = 'Pairing code expired';
      } else {
        countdown.value = remaining;
      }
    };

    updateCountdown();
    countdownTimer = window.setInterval(updateCountdown, 1000);
  }

  function stopCountdown() {
    if (countdownTimer !== null) {
      window.clearInterval(countdownTimer);
      countdownTimer = null;
    }
    countdown.value = 0;
  }

  // --- Load paired devices ---
  async function loadDevices() {
    isLoading.value = true;
    try {
      devices.value = await invoke<PairedDevice[]>('p2p_list_devices');
    } catch (e) {
      logger.error('Failed to load paired devices:', e);
    } finally {
      isLoading.value = false;
    }
  }

  // --- Initiate pairing (generate code) ---
  async function initiatePairing() {
    try {
      isPairing.value = true;
      pairingSuccess.value = false;
      error.value = '';
      const info: PairingInfo = await invoke<PairingInfo>('p2p_pair_initiate');
      pairingCode.value = info.code;
      pairingExpiry.value = info.expires_at;
      startCountdown();
    } catch (e: any) {
      error.value = e?.toString() || 'Failed to initiate pairing';
      isPairing.value = false;
      logger.error('Pairing initiation failed:', e);
    }
  }

  // --- Accept pairing (enter code from other device) ---
  async function acceptPairing(code: string) {
    if (!code.trim()) {
      error.value = 'Please enter a pairing code';
      return;
    }
    try {
      isPairing.value = true;
      pairingSuccess.value = false;
      error.value = '';
      await invoke('p2p_pair_accept', { code: code.trim().toUpperCase() });
      pairingSuccess.value = true;
      isPairing.value = false;
      pairingCode.value = '';
      stopCountdown();
      await loadDevices();
    } catch (e: any) {
      error.value = e?.toString() || 'Failed to pair device';
      isPairing.value = false;
      logger.error('Pairing acceptance failed:', e);
    }
  }

  // --- Cancel pairing ---
  async function cancelPairing() {
    try {
      await invoke('p2p_pair_cancel');
    } catch (e) {
      logger.error('Cancel pairing failed:', e);
    }
    isPairing.value = false;
    pairingCode.value = '';
    pairingSuccess.value = false;
    error.value = '';
    stopCountdown();
  }

  // --- Remove a paired device ---
  async function removeDevice(nodeIdHex: string) {
    try {
      await invoke('p2p_remove_device', { nodeIdHex });
      await loadDevices();
    } catch (e: any) {
      error.value = e?.toString() || 'Failed to remove device';
      logger.error('Remove device failed:', e);
    }
  }

  // --- Format countdown for display ---
  const countdownFormatted = computed(() => {
    const m = Math.floor(countdown.value / 60);
    const s = countdown.value % 60;
    return `${m}:${s.toString().padStart(2, '0')}`;
  });

  // --- Listen for remote pairing acceptance ---
  let unlistenPairingAccepted: UnlistenFn | null = null;
  
  // Set up listener for when a remote device accepts our pairing code
  listen<{ device_name: string; node_id_hex: string }>('pairing-accepted', (event) => {
    logger.info('Pairing accepted by remote device:', event.payload.device_name);
    pairingSuccess.value = true;
    isPairing.value = false;
    pairingCode.value = '';
    stopCountdown();
    loadDevices();
  }).then((unlisten) => {
    unlistenPairingAccepted = unlisten;
  });

  // --- Cleanup on unmount ---
  onUnmounted(() => {
    stopCountdown();
    if (unlistenPairingAccepted) {
      unlistenPairingAccepted();
    }
  });

  return {
    // State
    isPairing,
    pairingCode,
    pairingExpiry,
    devices,
    isLoading,
    error,
    pairingSuccess,
    countdown,
    countdownFormatted,
    // Actions
    loadDevices,
    initiatePairing,
    acceptPairing,
    cancelPairing,
    removeDevice,
  };
}
