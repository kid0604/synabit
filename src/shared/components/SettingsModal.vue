<script setup lang="ts">
import { Settings, FileText, CheckSquare, Globe, X, FolderOpen, Cloud, CloudOff, RefreshCw, MessageSquare, Zap, Calendar, Palette, Users, Wallet, Lock, Shield, Trash2, Server, Unplug, Monitor, HardDrive, Check } from 'lucide-vue-next';
import { useSettings } from '../../composables/useSettings';
import { ref, onMounted, watch, defineAsyncComponent } from 'vue';

const LockScreenVerify = defineAsyncComponent(() => import('./LockScreen.vue'));
const DeviceManager = defineAsyncComponent(() => import('./DeviceManager.vue'));

const SyncMobileSettings = defineAsyncComponent(() => import('./SyncMobileSettings.vue'));
const ConfirmModal = defineAsyncComponent(() => import('./ConfirmModal.vue'));
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { type } from '@tauri-apps/plugin-os';
import { logger } from '../../utils/logger';
import { useAppLockStore } from '../../stores/useAppLockStore';
import { useAppUpdate } from '../../composables/useAppUpdate';

const LicenseModal = defineAsyncComponent(() => import('./LicenseModal.vue'));
const showLicenseModal = ref(false);

const {
  showSettingsModal, settingsTab,
  themeMode, appLanguage, defaultApp,
  taskArchiveDays,
  enableDailyNotes, dailyNoteFormat, dailyNoteTag, isValidDailyFormat,
  nestedNumberListStyle, hiddenSidebarApps
} = useSettings();

const {
  updateAvailable, updateVersion, updateNotes,
  isChecking: updateChecking,
  isDownloading: updateDownloading,
  downloadProgress: updateProgress,
  lastCheckResult,
  checkForUpdates, downloadAndInstall,
} = useAppUpdate();

const availableApps = [
  { id: 'nexus', name: 'Nexus', icon: Globe },
  { id: 'chat', name: 'Chat', icon: MessageSquare },
  { id: 'quickcap', name: 'QuickCap', icon: Zap },
  { id: 'note', name: 'Notes', icon: FileText },
  { id: 'task', name: 'Tasks', icon: CheckSquare },
  { id: 'calendar', name: 'Calendar', icon: Calendar },
  { id: 'file', name: 'Files', icon: FolderOpen },
  { id: 'whiteboard', name: 'Whiteboard', icon: Palette },
  { id: 'people', name: 'People', icon: Users },
  { id: 'finance', name: 'Finance', icon: Wallet },
];

const toggleAppVisibility = (appId: string) => {
  if (defaultApp.value === appId) return;
  if (hiddenSidebarApps.value.includes(appId)) {
    hiddenSidebarApps.value = hiddenSidebarApps.value.filter(id => id !== appId);
  } else {
    hiddenSidebarApps.value.push(appId);
  }
};

const appVersion = ref('');
const isDesktop = ref(true);

onMounted(async () => {
  try {
    appVersion.value = await getVersion();
    const osType = type();
    isDesktop.value = osType === 'macos' || osType === 'windows' || osType === 'linux';
    
    // Check E2EE status
    await checkE2eeStatus();
  } catch(e) {
    logger.error("Failed to get version/os or E2EE status", e);
  }
});

// Re-check E2EE status whenever the settings modal is opened
watch(showSettingsModal, (visible) => {
  if (visible) checkE2eeStatus();
});

const openLogFolder = async () => {
  try {
    await invoke('open_app_log_folder');
  } catch (e) {
    logger.error("Failed to open log folder", e);
  }
};

const props = defineProps<{
  vaultPath: string;
  vaultType: 'local' | 'gdrive';
  gdriveConnected: boolean;
  gdriveSyncing: boolean;
  gdriveSyncError: string;
  lastSyncTime: string;
  gdriveAutoSyncEnabled: boolean;
  gdriveAutoSyncInterval: number;
  // P2P Sync
  p2pConnected: boolean;
  p2pSyncing: boolean;
  p2pSyncError: string;
  p2pConnecting: boolean;
  p2pLastSyncTime: string;
  p2pAutoSyncEnabled: boolean;
  p2pAutoSyncInterval: number;
  p2pServerAddr: string;
  p2pServerIdHex: string;
}>();

const emit = defineEmits<{
  (e: 'clear-vault'): void;
  (e: 'sync-gdrive'): void;
  (e: 'disconnect-gdrive'): void;
  (e: 'connect-gdrive'): void;
  (e: 'update:gdriveAutoSyncEnabled', val: boolean): void;
  (e: 'update:gdriveAutoSyncInterval', val: number): void;
  (e: 'show-setup-pin', mode: 'setup' | 'change'): void;
  // P2P Sync
  (e: 'p2p-connect', serverAddr: string, serverIdHex: string): void;
  (e: 'p2p-disconnect'): void;
  (e: 'p2p-sync'): void;
  (e: 'update:p2pAutoSyncEnabled', val: boolean): void;
  (e: 'update:p2pAutoSyncInterval', val: number): void;
}>();

// P2P Sync form state
const p2pFormAddr = ref('');
const p2pFormId = ref('');
const p2pServerMode = ref<'none' | 'official' | 'custom'>('none');

const activeSettingsProvider = ref<'gdrive' | 'p2p' | 'none'>(
  props.p2pConnected ? 'p2p' : props.gdriveConnected ? 'gdrive' : 'none'
);

watch(() => [props.p2pConnected, props.gdriveConnected], ([p2p, gdrive]) => {
  if (p2p) activeSettingsProvider.value = 'p2p';
  else if (gdrive) activeSettingsProvider.value = 'gdrive';
  else activeSettingsProvider.value = 'none';
}, { immediate: true });

const showConfirmDisconnectGDrive = ref(false);
const showConfirmDisconnectP2P = ref(false);
const showConfirmDisconnectAll = ref(false);

const handleConnectGDrive = () => {
  if (props.p2pConnected) emit('p2p-disconnect');
  emit('connect-gdrive');
};

const handleConnectP2P = (addr: string, id: string) => {
  if (props.gdriveConnected) emit('disconnect-gdrive');
  emit('p2p-connect', addr, id);
};

const handleDisconnectAll = () => {
  if (props.gdriveConnected) emit('disconnect-gdrive');
  if (props.p2pConnected) emit('p2p-disconnect');
  activeSettingsProvider.value = 'none';
};

// Official Synabit Sync Server config
// TODO: Update these when the official server is deployed
const OFFICIAL_SERVER = {
  addr: 'sync.synabit.net:4433',
  id: 'a68ab6ca34233b3cb2b5613bfd9eebd6377c0895b2fb9ddcb338543aab593be1',
  available: true,
};

// ─── App Lock ─────────────────────────────────────────────────
const appLockStore = useAppLockStore();
const removingLock = ref(false);
const autoLockOptions = [
  { value: 60, label: '1 minute' },
  { value: 300, label: '5 minutes' },
  { value: 900, label: '15 minutes' },
  { value: 1800, label: '30 minutes' },
  { value: 0, label: 'Never' },
];

const handleRemoveLock = async () => {
  removingLock.value = true;
  try {
    await invoke('remove_app_lock');
    await appLockStore.refreshConfig();
  } catch (e) {
    logger.error('Failed to remove app lock:', e);
  } finally {
    removingLock.value = false;
  }
};

// ─── PIN Verification for destructive actions ─────────────
const showPinVerify = ref(false);
const pinVerifyTitle = ref('');
const pendingAction = ref<(() => void) | null>(null);

const requirePin = (title: string, action: () => void) => {
  pinVerifyTitle.value = title;
  pendingAction.value = action;
  showPinVerify.value = true;
};

const onPinVerified = () => {
  showPinVerify.value = false;
  if (pendingAction.value) {
    pendingAction.value();
    pendingAction.value = null;
  }
};

const handleToggleTier1 = () => {
  if (appLockStore.appLockActive) {
    // Turning OFF → require PIN
    requirePin('Enter PIN to disable app lock', () => appLockStore.setAppLockActive(false));
  } else {
    // Turning ON → free
    appLockStore.setAppLockActive(true);
  }
};

const handleToggleProtectedApp = (appId: string, appName: string) => {
  if (appLockStore.isAppProtected(appId)) {
    // Removing protection → require PIN
    requirePin(`Enter PIN to unprotect ${appName}`, () => appLockStore.toggleProtectedApp(appId));
  } else {
    // Adding protection → free
    appLockStore.toggleProtectedApp(appId);
  }
};

// ─── E2EE Security State ─────────────────────────────────
interface E2eeStatus {
  key_available: boolean;
  needs_setup: boolean;
}
interface SetupResult {
  recovery_phrase: string;
}

const e2eeStatus = ref<E2eeStatus>({ key_available: false, needs_setup: true });
const restorePhrase = ref('');
const showRestoreForm = ref(false);
const e2eeError = ref('');
const e2eeSuccess = ref('');
const e2eeLoading = ref(false);

const checkE2eeStatus = async () => {
  try {
    e2eeStatus.value = await invoke<E2eeStatus>('check_e2ee_status');
  } catch (e) {
    logger.error("Failed to check E2EE status", e);
  }
};

const setupE2ee = async () => {
  e2eeError.value = '';
  e2eeLoading.value = true;
  try {
    await invoke<SetupResult>('setup_e2ee');
    e2eeStatus.value.key_available = true;
    e2eeStatus.value.needs_setup = false;
    e2eeSuccess.value = 'Encryption is active.';
  } catch (err) {
    e2eeError.value = String(err);
  } finally {
    e2eeLoading.value = false;
  }
};

const restoreFromPhrase = async () => {
  e2eeError.value = '';
  if (!restorePhrase.value.trim()) {
    e2eeError.value = 'Please enter your Recovery Phrase';
    return;
  }
  e2eeLoading.value = true;
  try {
    await invoke('restore_e2ee_from_phrase', { phrase: restorePhrase.value.trim().toLowerCase() });
    e2eeStatus.value.key_available = true;
    e2eeStatus.value.needs_setup = false;
    showRestoreForm.value = false;
    restorePhrase.value = '';
    e2eeSuccess.value = 'Restored successfully! You can sync now.';
  } catch (err) {
    e2eeError.value = String(err);
  } finally {
    e2eeLoading.value = false;
  }
};
</script>

<template>
  <Teleport to="body">
    <Transition name="settings-modal">
      <div v-if="showSettingsModal" class="fixed inset-0 z-[200] flex items-center justify-center">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/40 dark:bg-black/60 backdrop-blur-sm" @mousedown="showSettingsModal = false"></div>
        
        <!-- Modal Container -->
        <div class="relative w-[95vw] md:w-[720px] md:max-w-[90vw] h-[90vh] md:h-[520px] md:max-h-[85vh] bg-[#fdfdfc] dark:bg-[#242424] rounded-2xl shadow-2xl border border-[#e0e0e0] dark:border-[#333] flex flex-col md:flex-row overflow-hidden" @mousedown.stop>
          
          <!-- Top/Left Tab Navigation -->
          <nav class="w-full md:w-[200px] shrink-0 bg-[#f5f5f5] dark:bg-[#1a1a1a] border-b md:border-b-0 md:border-r border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col py-2 md:py-5 px-2 md:px-3 z-10">
            <h2 class="hidden md:block text-[13px] font-bold text-[#1c1c1e] dark:text-[#f4f4f5] mb-5 px-2">{{ $t('settings.title') }}</h2>
            
            <div class="flex flex-row md:flex-col gap-1 md:gap-0 md:space-y-0.5 overflow-x-auto no-scrollbar">
              <button @click="settingsTab = 'general'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'general' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Settings class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.general') }}</span>
              </button>
              <button @click="settingsTab = 'notes'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'notes' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <FileText class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.notes') }}</span>
              </button>
              <button @click="settingsTab = 'tasks'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'tasks' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <CheckSquare class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.tasks') }}</span>
              </button>
              <button @click="settingsTab = 'security'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'security' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Lock class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.security') }}</span>
              </button>
              <button @click="settingsTab = 'license'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'license' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Shield class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">License</span>
              </button>
              <button @click="settingsTab = 'devices'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'devices' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Monitor class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.devices', 'Devices') }}</span>
              </button>
              <button @click="settingsTab = 'about'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'about' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Globe class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.about') }}</span>
              </button>
            </div>
          </nav>
          
          <!-- Content Area -->
          <div class="flex-1 flex flex-col overflow-hidden min-h-0 relative">
            <!-- Header -->
            <div class="h-12 shrink-0 flex items-center justify-between px-4 md:px-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c] sticky top-0 bg-[#fdfdfc]/90 dark:bg-[#242424]/90 backdrop-blur-sm z-10">
              <h3 class="text-[15px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] capitalize">{{ $t(`settings.tabs.${settingsTab}`) }}</h3>
              <button @click="showSettingsModal = false" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-[#333] text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 transition-colors">
                <X class="w-4 h-4" />
              </button>
            </div>
            
            <!-- Scrollable Content -->
            <div class="flex-1 overflow-y-auto p-6">
              
              <!-- === GENERAL TAB === -->
              <div v-if="settingsTab === 'general'" class="space-y-6">
                <!-- Vault Management -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">{{ $t('settings.general.vault') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center gap-2 mb-2">
                      <p class="text-[11px] font-medium text-gray-400 dark:text-gray-500">{{ $t('settings.general.storage_type') }}</p>
                      <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 text-[10px] font-semibold">
                        <FolderOpen class="w-3 h-3" /> Local
                      </span>
                    </div>
                    <p class="font-mono text-[12px] break-all text-[#1c1c1e] dark:text-[#f4f4f5] bg-white dark:bg-[#2a2a2a] px-3 py-2 rounded-lg border border-gray-200 dark:border-transparent">{{ vaultPath }}</p>
                    <button @click="emit('clear-vault')" class="mt-3 px-4 py-2 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-lg text-[12px] font-medium transition-all shadow-sm flex items-center gap-2">
                      <FolderOpen class="w-3.5 h-3.5" /> {{ $t('settings.general.switch_vault') }}
                    </button>
                  </div>
                </section>

                <!-- Sync Provider -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Sync Provider</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-2 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] space-y-2">
                    
                    <!-- LOCAL ONLY -->
                    <div class="rounded-lg overflow-hidden border border-transparent transition-colors" :class="activeSettingsProvider === 'none' ? 'border-gray-300 dark:border-gray-600 bg-white dark:bg-[#2a2a2a] shadow-sm' : ''">
                      <button @click="activeSettingsProvider = 'none'" class="w-full px-3 py-2.5 flex items-center gap-3 text-left hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                        <div class="w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors" :class="activeSettingsProvider === 'none' ? 'border-gray-500' : 'border-gray-300 dark:border-gray-600'">
                          <div v-if="activeSettingsProvider === 'none'" class="w-2 h-2 rounded-full bg-gray-500"></div>
                        </div>
                        <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 bg-gray-100 dark:bg-gray-800">
                          <HardDrive class="w-4 h-4 text-gray-500" />
                        </div>
                        <div class="flex-1 min-w-0">
                          <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Local Storage Only</p>
                        </div>
                        <span v-if="!gdriveConnected && !p2pConnected" class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400">
                          <Check class="w-3 h-3" /> Active
                        </span>
                      </button>
                      <div v-if="activeSettingsProvider === 'none'" class="px-10 pb-4 pt-1 ml-4 border-t border-[#f0f0f0] dark:border-[#333] mt-1">
                        <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-3 mt-3">Your data will only be saved locally on this device. Synabit will not sync with any cloud providers.</p>
                        <button v-if="gdriveConnected || p2pConnected" @click="showConfirmDisconnectAll = true" class="px-3 py-1.5 bg-gray-800 hover:bg-gray-900 text-white dark:bg-gray-200 dark:text-black rounded-lg text-[12px] font-medium transition-all shadow-sm">
                          Disconnect active providers
                        </button>
                      </div>
                    </div>

                    <!-- GOOGLE DRIVE -->
                    <div class="rounded-lg overflow-hidden border border-transparent transition-colors" :class="activeSettingsProvider === 'gdrive' ? 'border-blue-400 dark:border-blue-600 bg-white dark:bg-[#2a2a2a] shadow-sm' : ''">
                      <button @click="activeSettingsProvider = 'gdrive'" class="w-full px-3 py-2.5 flex items-center gap-3 text-left hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                        <div class="w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors" :class="activeSettingsProvider === 'gdrive' ? 'border-blue-500' : 'border-gray-300 dark:border-gray-600'">
                          <div v-if="activeSettingsProvider === 'gdrive'" class="w-2 h-2 rounded-full bg-blue-500"></div>
                        </div>
                        <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 bg-blue-100 dark:bg-blue-900/20">
                          <Cloud class="w-4 h-4 text-blue-500" />
                        </div>
                        <div class="flex-1 min-w-0">
                          <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Google Drive</p>
                        </div>
                        <span v-if="gdriveConnected" class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-blue-100 dark:bg-blue-900/40 text-blue-600 dark:text-blue-400">
                          <Check class="w-3 h-3" /> Connected
                        </span>
                      </button>
                      
                      <!-- GDrive Details -->
                      <div v-if="activeSettingsProvider === 'gdrive'" class="px-3 md:px-10 pb-4 pt-1 ml-0 md:ml-4 border-t border-[#f0f0f0] dark:border-[#333] mt-1 space-y-4">
                        <div class="flex items-center justify-between mt-3">
                          <div class="flex items-center gap-2">
                            <div :class="['w-2 h-2 rounded-full', gdriveConnected ? 'bg-green-500' : 'bg-red-500']"></div>
                            <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ gdriveConnected ? $t('settings.general.connected') : $t('settings.general.disconnected') }}</p>
                          </div>
                          <button v-if="gdriveConnected" @click="emit('sync-gdrive')" :disabled="gdriveSyncing" class="px-3 py-1.5 rounded-lg text-[12px] font-medium transition-all flex items-center gap-1.5 bg-blue-500 hover:bg-blue-600 text-white disabled:opacity-60">
                            <RefreshCw class="w-3.5 h-3.5" :class="gdriveSyncing ? 'animate-spin' : ''" />
                            {{ gdriveSyncing ? $t('settings.general.syncing') : $t('settings.general.sync_now') }}
                          </button>
                          <button v-else @click="handleConnectGDrive" class="px-3 py-1.5 rounded-lg text-[12px] font-medium transition-all flex items-center gap-1.5 bg-blue-500 hover:bg-blue-600 text-white">
                            <Cloud class="w-3.5 h-3.5" />
                            {{ $t('settings.general.connect') }}
                          </button>
                        </div>
                        
                        <template v-if="gdriveConnected">
                          <div v-if="lastSyncTime" class="flex items-center gap-2 text-[11px] text-gray-400">
                            <span>{{ $t('settings.general.last_synced') }}: {{ lastSyncTime }}</span>
                          </div>
                          <div v-if="gdriveSyncError" class="text-[11px] text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
                            ⚠️ {{ gdriveSyncError }}
                          </div>
                          <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                            <div class="flex items-center justify-between mb-3">
                              <p class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.periodic_auto_sync') }}</p>
                              <label class="relative inline-flex items-center cursor-pointer">
                                <input type="checkbox" :checked="gdriveAutoSyncEnabled" @change="emit('update:gdriveAutoSyncEnabled', ($event.target as HTMLInputElement).checked)" class="sr-only peer">
                                <div class="w-9 h-5 bg-gray-200 peer-focus:outline-none rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all dark:border-gray-600 peer-checked:bg-blue-500"></div>
                              </label>
                            </div>
                            <div v-if="gdriveAutoSyncEnabled" class="flex items-center justify-between">
                              <p class="text-[11px] text-gray-500 dark:text-gray-400">{{ $t('settings.general.sync_interval') }}</p>
                              <input type="number" :value="gdriveAutoSyncInterval" @input="emit('update:gdriveAutoSyncInterval', Number(($event.target as HTMLInputElement).value))" min="1" max="60" class="w-16 px-2 py-1 bg-white dark:bg-[#2a2a2a] border border-[#e6e6e6] dark:border-[#3a3a3a] rounded text-[12px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:border-blue-500" />
                            </div>
                          </div>
                          <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                            <button @click="showConfirmDisconnectGDrive = true" class="px-4 py-2 rounded-lg text-[12px] font-medium border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 transition-all flex items-center gap-2">
                              <CloudOff class="w-3.5 h-3.5" /> {{ $t('settings.general.disconnect_gdrive') }}
                            </button>
                          </div>
                        </template>
                      </div>
                    </div>

                    <!-- SYNABIT SERVER (P2P) -->
                    <div class="rounded-lg overflow-hidden border border-transparent transition-colors" :class="activeSettingsProvider === 'p2p' ? 'border-emerald-400 dark:border-emerald-600 bg-white dark:bg-[#2a2a2a] shadow-sm' : ''">
                      <button @click="activeSettingsProvider = 'p2p'" class="w-full px-3 py-2.5 flex items-center gap-3 text-left hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                        <div class="w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors" :class="activeSettingsProvider === 'p2p' ? 'border-emerald-500' : 'border-gray-300 dark:border-gray-600'">
                          <div v-if="activeSettingsProvider === 'p2p'" class="w-2 h-2 rounded-full bg-emerald-500"></div>
                        </div>
                        <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 bg-emerald-100 dark:bg-emerald-900/20">
                          <Server class="w-4 h-4 text-emerald-500" />
                        </div>
                        <div class="flex-1 min-w-0">
                          <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.p2p_sync', 'Relay Server (P2P)') }}</p>
                        </div>
                        <span v-if="p2pConnected" class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-100 dark:bg-emerald-900/40 text-emerald-600 dark:text-emerald-400">
                          <Check class="w-3 h-3" /> Connected
                        </span>
                      </button>

                      <!-- P2P Details Accordion -->
                      <div v-if="activeSettingsProvider === 'p2p'" class="px-3 md:px-10 pb-4 pt-1 ml-0 md:ml-4 border-t border-[#f0f0f0] dark:border-[#333] mt-1 space-y-4">
                        <!-- Connected state -->
                        <template v-if="p2pConnected">
                          <div class="flex items-center justify-between mt-3">
                            <div class="flex items-center gap-2">
                              <div class="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></div>
                              <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.connected', 'Connected') }}</p>
                            </div>
                            <button @click="emit('p2p-sync')" :disabled="p2pSyncing" class="px-3 py-1.5 rounded-lg text-[12px] font-medium transition-all flex items-center gap-1.5 bg-emerald-500 hover:bg-emerald-600 text-white disabled:opacity-60">
                              <RefreshCw class="w-3.5 h-3.5" :class="p2pSyncing ? 'animate-spin' : ''" />
                              {{ p2pSyncing ? $t('settings.general.syncing', 'Syncing...') : $t('settings.general.sync_now', 'Sync Now') }}
                            </button>
                          </div>
                          <div class="flex items-center gap-2 text-[11px] text-gray-400">
                            <Server class="w-3 h-3" />
                            <span class="font-mono">{{ p2pServerAddr }}</span>
                            <span v-if="p2pServerAddr === OFFICIAL_SERVER.addr" class="px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider rounded bg-emerald-100 dark:bg-emerald-900/30 text-emerald-600 dark:text-emerald-400">Official</span>
                            <span v-else class="px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider rounded bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400">Self-hosted</span>
                          </div>
                          <div v-if="p2pLastSyncTime" class="flex items-center gap-2 text-[11px] text-gray-400">
                            <span>{{ $t('settings.general.last_synced', 'Last synced') }}: {{ p2pLastSyncTime }}</span>
                          </div>
                          <div v-if="p2pSyncError" class="text-[11px] text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
                            ⚠️ {{ p2pSyncError }}
                          </div>
                          <!-- Auto-sync -->
                          <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                            <div class="flex items-center justify-between mb-3">
                              <p class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.periodic_auto_sync', 'Periodic auto-sync') }}</p>
                              <label class="relative inline-flex items-center cursor-pointer">
                                <input type="checkbox" :checked="p2pAutoSyncEnabled" @change="emit('update:p2pAutoSyncEnabled', ($event.target as HTMLInputElement).checked)" class="sr-only peer">
                                <div class="w-9 h-5 bg-gray-200 peer-focus:outline-none rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all dark:border-gray-600 peer-checked:bg-emerald-500"></div>
                              </label>
                            </div>
                            <div v-if="p2pAutoSyncEnabled" class="flex items-center justify-between">
                              <p class="text-[11px] text-gray-500 dark:text-gray-400">{{ $t('settings.general.sync_interval', 'Interval (minutes)') }}</p>
                              <input type="number" :value="p2pAutoSyncInterval" @input="emit('update:p2pAutoSyncInterval', Number(($event.target as HTMLInputElement).value))" min="1" max="60" class="w-16 px-2 py-1 bg-white dark:bg-[#2a2a2a] border border-[#e6e6e6] dark:border-[#3a3a3a] rounded text-[12px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:border-emerald-500" />
                            </div>
                          </div>
                          <!-- Disconnect -->
                          <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                            <button @click="showConfirmDisconnectP2P = true" class="px-4 py-2 rounded-lg text-[12px] font-medium border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 transition-all flex items-center gap-2">
                              <Unplug class="w-3.5 h-3.5" /> {{ $t('settings.general.disconnect_p2p', 'Disconnect') }}
                            </button>
                          </div>

                          <!-- Mobile Settings -->
                          <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                            <SyncMobileSettings />
                          </div>
                        </template>

                        <!-- Disconnected state -->
                        <template v-else>
                          <!-- Option cards -->
                          <div v-if="p2pServerMode === 'none'" class="space-y-2 mt-3">
                            <!-- Synabit Cloud (Official) -->
                            <button @click="OFFICIAL_SERVER.available ? handleConnectP2P(OFFICIAL_SERVER.addr, OFFICIAL_SERVER.id) : undefined" :disabled="!OFFICIAL_SERVER.available || p2pConnecting" class="w-full p-3 rounded-xl border-2 text-left transition-all flex items-start gap-3 group" :class="OFFICIAL_SERVER.available ? 'border-[#e6e6e6] dark:border-[#2c2c2c] hover:border-emerald-400 dark:hover:border-emerald-600 cursor-pointer' : 'border-[#e6e6e6] dark:border-[#2c2c2c] opacity-60 cursor-not-allowed'">
                              <div class="w-9 h-9 rounded-lg bg-emerald-50 dark:bg-emerald-900/20 flex items-center justify-center shrink-0 mt-0.5">
                                <Cloud class="w-4.5 h-4.5 text-emerald-500" />
                              </div>
                              <div class="flex-1 min-w-0">
                                <div class="flex items-center gap-2">
                                  <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Official Synabit Relay</p>
                                  <span v-if="!OFFICIAL_SERVER.available" class="px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider rounded bg-amber-100 dark:bg-amber-900/30 text-amber-600 dark:text-amber-400">{{ $t('settings.general.coming_soon', 'Coming soon') }}</span>
                                  <span v-else class="px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider rounded bg-emerald-100 dark:bg-emerald-900/30 text-emerald-600 dark:text-emerald-400">{{ $t('settings.general.official', 'Official') }}</span>
                                </div>
                                <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.general.official_desc', 'One-click setup • No configuration needed') }}</p>
                              </div>
                            </button>

                            <!-- Self-hosted Server -->
                            <button @click="p2pServerMode = 'custom'" class="w-full p-3 rounded-xl border-2 border-[#e6e6e6] dark:border-[#2c2c2c] hover:border-gray-400 dark:hover:border-gray-500 text-left transition-all flex items-start gap-3 cursor-pointer group">
                              <div class="w-9 h-9 rounded-lg bg-gray-100 dark:bg-gray-800 flex items-center justify-center shrink-0 mt-0.5">
                                <Server class="w-4.5 h-4.5 text-gray-500" />
                              </div>
                              <div class="flex-1 min-w-0">
                                <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.self_hosted', 'Self-hosted Server') }}</p>
                                <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.general.self_hosted_desc', 'Connect to your own Synabit sync server') }}</p>
                              </div>
                            </button>
                          </div>

                          <!-- Self-hosted form -->
                          <div v-else-if="p2pServerMode === 'custom'" class="space-y-3 mt-3">
                            <div class="space-y-1">
                              <label class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.server_address', 'Server Address') }}</label>
                              <input v-model="p2pFormAddr" type="text" :placeholder="$t('settings.general.server_address_hint', 'e.g. 1.2.3.4:4433')" class="w-full px-3 py-2 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-emerald-500 font-mono" />
                            </div>
                            <div class="space-y-1">
                              <label class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.server_id', 'Server ID') }}</label>
                              <input v-model="p2pFormId" type="text" :placeholder="$t('settings.general.server_id_hint', '64-character hex string')" class="w-full px-3 py-2 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-emerald-500 font-mono" />
                            </div>
                            <div class="flex gap-2">
                              <button @click="handleConnectP2P(p2pFormAddr, p2pFormId)" :disabled="p2pConnecting || !p2pFormAddr || !p2pFormId" class="flex-1 px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white rounded-lg text-[13px] font-medium transition-all shadow-sm flex items-center justify-center gap-2 disabled:opacity-60">
                                <RefreshCw v-if="p2pConnecting" class="w-3.5 h-3.5 animate-spin" />
                                <Server v-else class="w-3.5 h-3.5" />
                                {{ p2pConnecting ? $t('settings.general.connecting', 'Connecting...') : $t('settings.general.connect', 'Connect') }}
                              </button>
                              <button @click="p2pServerMode = 'none'" class="px-4 py-2 border border-[#e0e0e0] dark:border-[#3a3a3a] text-[#52525b] dark:text-[#a1a1aa] rounded-lg text-[13px] font-medium transition-all">
                                {{ $t('settings.security.cancel', 'Cancel') }}
                              </button>
                            </div>
                            <div v-if="p2pSyncError" class="text-[11px] text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
                              ⚠️ {{ p2pSyncError }}
                            </div>
                          </div>
                        </template>
                      </div>
                    </div>

                  </div>
                </section>

                <!-- Behavior -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">{{ $t('settings.general.behavior') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.startup_app') }}</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.general.startup_app_desc') }}</p>
                      </div>
                      <select v-model="defaultApp" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                        <option value="nexus">Nexus</option>
                        <option value="quickcap">QuickCap</option>
                        <option value="note">Notes</option>
                        <option value="task">Tasks</option>
                        <option value="calendar">Calendar</option>
                        <option value="file">Files</option>
                      </select>
                    </div>
                  </div>
                </section>

                <!-- Sidebar Navigation -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">{{ $t('settings.general.sidebar') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] mb-1">{{ $t('settings.general.visible_apps') }}</p>
                    <p class="text-[11px] text-gray-400 dark:text-gray-500 mb-4">{{ $t('settings.general.visible_apps_desc') }}</p>
                    
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                      <label v-for="app in availableApps" :key="app.id" 
                        class="flex items-center justify-between p-2 rounded-lg border transition-colors cursor-pointer"
                        :class="defaultApp === app.id ? 'bg-gray-100 dark:bg-[#252525] border-transparent opacity-60 cursor-not-allowed' : 'bg-white dark:bg-[#2a2a2a] border-[#e6e6e6] dark:border-[#3a3a3a] hover:border-gray-300 dark:hover:border-gray-500'"
                      >
                        <span class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2">
                          <component :is="app.icon" class="w-4 h-4 text-gray-500" />
                          {{ app.name }}
                          <span v-if="defaultApp === app.id" class="text-[9px] px-1.5 py-0.5 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded uppercase font-bold ml-1 tracking-wide">{{ $t('settings.general.default') }}</span>
                        </span>
                        
                        <div class="relative inline-flex h-4 w-7 shrink-0 items-center justify-center rounded-full transition-colors duration-200 ease-in-out" :class="!hiddenSidebarApps.includes(app.id) ? 'bg-green-500' : 'bg-gray-300 dark:bg-gray-600'">
                          <input type="checkbox" :checked="!hiddenSidebarApps.includes(app.id)" :disabled="defaultApp === app.id" @change="toggleAppVisibility(app.id)" class="sr-only">
                          <span class="pointer-events-none inline-block h-3 w-3 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="!hiddenSidebarApps.includes(app.id) ? 'translate-x-1.5' : '-translate-x-1.5'"/>
                        </div>
                      </label>
                    </div>
                  </div>
                </section>

                <!-- Theme & Language -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">{{ $t('settings.general.appearance') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] space-y-4">
                    <div>
                      <p class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] mb-3">{{ $t('settings.general.theme') }}</p>
                      <div class="flex gap-2">
                        <button v-for="mode in (['light', 'dark', 'system'] as const)" :key="mode"
                          @click="themeMode = mode"
                          :class="['px-4 py-2 rounded-lg text-[12px] font-medium transition-all border capitalize', themeMode === mode ? 'bg-black text-white dark:bg-white dark:text-black border-transparent shadow-sm' : 'bg-white dark:bg-[#2a2a2a] border-[#e0e0e0] dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 hover:border-gray-400 dark:hover:border-gray-500']">
                          {{ $t(`settings.general.themes.${mode}`) }}
                        </button>
                      </div>
                    </div>
                    
                    <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                      <div class="flex items-center justify-between">
                        <div>
                          <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.general.language') }}</p>
                          <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.general.language_desc') }}</p>
                        </div>
                        <select v-model="appLanguage" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                          <option value="en">English</option>
                          <option value="vi">Tiếng Việt</option>
                        </select>
                      </div>
                    </div>
                  </div>
                </section>
              </div>
              
              <!-- === NOTES TAB === -->
              <div v-else-if="settingsTab === 'notes'" class="space-y-6">
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">{{ $t('settings.notes.features') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col gap-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.notes.enable_daily_notes') }}</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.notes.enable_daily_notes_desc') }}</p>
                      </div>
                      <button @click="enableDailyNotes = !enableDailyNotes" class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full focus:outline-none transition-colors duration-200 ease-in-out" :class="enableDailyNotes ? 'bg-purple-600' : 'bg-gray-300 dark:bg-gray-600'">
                        <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="enableDailyNotes ? 'translate-x-2' : '-translate-x-2'"/>
                      </button>
                    </div>
                    <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4 flex items-center justify-between" :class="!enableDailyNotes ? 'opacity-50 pointer-events-none' : ''">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.notes.date_format') }}</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.notes.date_format_desc') }}</p>
                      </div>
                      <div class="flex flex-col items-end gap-1">
                        <input type="text" v-model="dailyNoteFormat" class="w-28 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 transition-colors" :class="isValidDailyFormat ? 'border-[#e0e0e0] dark:border-[#3a3a3a] focus:ring-black dark:focus:ring-white' : 'border-red-400 focus:ring-red-500'" />
                        <span v-if="!isValidDailyFormat" class="text-[10px] text-red-500 font-medium">{{ $t('settings.notes.date_format_req') }}</span>
                      </div>
                    </div>
                    <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4 flex items-center justify-between" :class="!enableDailyNotes ? 'opacity-50 pointer-events-none' : ''">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.notes.default_tag') }}</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.notes.default_tag_desc') }}</p>
                      </div>
                      <input type="text" v-model="dailyNoteTag" placeholder="daily" class="w-28 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors" />
                    </div>
                  </div>
                </section>
                
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3 mt-6">{{ $t('settings.notes.editor') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col gap-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.notes.list_style') }}</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.notes.list_style_desc') }}</p>
                      </div>
                      <select v-model="nestedNumberListStyle" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                        <option value="decimal">{{ $t('settings.notes.list_style_decimal', '1. 2. 3.') }}</option>
                        <option value="alpha">{{ $t('settings.notes.list_style_alpha', 'A. B. C.') }}</option>
                        <option value="nested">{{ $t('settings.notes.list_style_nested', '1. 1.1. 1.1.1.') }}</option>
                      </select>
                    </div>
                    
                    <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4 flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.notes.code_tab_size', 'Code Block Tab Size') }}</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.notes.code_tab_size_desc', 'Number of spaces for indentation') }}</p>
                      </div>
                      <select v-model="codeBlockTabSize" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                        <option :value="2">2 spaces</option>
                        <option :value="4">4 spaces</option>
                      </select>
                    </div>
                  </div>
                </section>
              </div>
              
              <!-- === TASKS TAB === -->
              <div v-else-if="settingsTab === 'tasks'" class="space-y-6">
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">{{ $t('settings.tasks.auto_archive') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center justify-between mb-2">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.tasks.archive_completed') }}</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.tasks.archive_desc_1') }} <code class="px-1 py-0.5 bg-gray-200 dark:bg-[#333] rounded text-[10px]">Tasks/archived</code> {{ $t('settings.tasks.archive_desc_2') }}</p>
                      </div>
                    </div>
                    <div class="flex items-center gap-3 mt-3">
                      <label class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('settings.tasks.after') }}</label>
                      <input type="number" v-model.number="taskArchiveDays" min="1" max="365" class="w-20 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white" />
                      <span class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('settings.tasks.days') }}</span>
                    </div>
                  </div>
                </section>
              </div>
              <!-- === SECURITY TAB === -->
              <div v-else-if="settingsTab === 'security'" class="space-y-6">
                <!-- Local Security (App Lock) -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">{{ $t('settings.security.app_lock') }}</h4>
                  
                  <!-- Setup or Managed States -->
                  <div v-if="!appLockStore.isEnabled" class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center gap-3 mb-3">
                      <div class="w-10 h-10 rounded-xl bg-purple-100 dark:bg-purple-900/30 flex items-center justify-center">
                        <Shield class="w-5 h-5 text-purple-600 dark:text-purple-400" />
                      </div>
                      <div>
                        <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.security.protect_app') }}</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500">{{ $t('settings.security.protect_app_desc') }}</p>
                      </div>
                    </div>
                    <button @click="emit('show-setup-pin', 'setup')" class="w-full px-4 py-2.5 bg-purple-600 hover:bg-purple-700 text-white rounded-lg text-[13px] font-medium transition-all shadow-sm flex items-center justify-center gap-2">
                      <Lock class="w-4 h-4" /> {{ $t('settings.security.setup_pin') }}
                    </button>
                  </div>

                  <template v-else>
                    <!-- Part 1: PIN Settings -->
                    <div class="mb-4 bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                      <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">{{ $t('settings.security.pin_settings') }}</p>
                      
                      <!-- Idle Timeout -->
                      <div class="flex items-center justify-between mb-4">
                        <div>
                          <p class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.security.idle_timeout') }}</p>
                          <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">{{ $t('settings.security.idle_timeout_desc') }}</p>
                        </div>
                        <select :value="appLockStore.autoLockTimeoutSecs" @change="appLockStore.setAutoLockTimeout(Number(($event.target as HTMLSelectElement).value))" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                          <option v-for="opt in autoLockOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
                        </select>
                      </div>

                      <hr class="border-[#e6e6e6] dark:border-[#2c2c2c] mb-4" />
                      
                      <!-- PIN Actions -->
                      <div class="flex gap-2">
                        <button @click="emit('show-setup-pin', 'change')" class="flex-1 px-4 py-2 border border-[#e0e0e0] dark:border-[#3a3a3a] text-[#52525b] dark:text-[#a1a1aa] hover:bg-gray-100 dark:hover:bg-[#333] rounded-lg text-[12px] font-medium transition-all flex items-center justify-center gap-2">
                          <Lock class="w-3.5 h-3.5" /> {{ $t('settings.security.change_pin') }}
                        </button>
                        <button @click="requirePin('Enter PIN to remove lock', handleRemoveLock)" :disabled="removingLock" class="px-4 py-2 border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg text-[12px] font-medium transition-all flex items-center justify-center gap-2 disabled:opacity-60">
                          <Trash2 class="w-3.5 h-3.5" /> {{ $t('settings.security.remove_pin') }}
                        </button>
                      </div>
                    </div>

                    <!-- Part 2: Lock Entire App -->
                    <div class="mb-4 bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                      <div class="flex items-center justify-between">
                        <div>
                          <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.security.lock_entire_app') }}</p>
                          <p class="text-[11px] text-gray-500 dark:text-gray-400 mt-1">{{ $t('settings.security.lock_entire_app_desc') }}</p>
                        </div>
                        <div class="relative inline-flex h-5 w-9 shrink-0 items-center justify-center rounded-full transition-colors duration-200 ease-in-out cursor-pointer" :class="appLockStore.appLockActive ? 'bg-purple-500' : 'bg-gray-300 dark:bg-gray-600'" @click="handleToggleTier1">
                          <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="appLockStore.appLockActive ? 'translate-x-2' : '-translate-x-2'"/>
                        </div>
                      </div>
                    </div>

                    <!-- Part 3: Protected Mini Apps -->
                    <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                      <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-2">{{ $t('settings.security.protected_mini_apps') }}</p>
                      <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-4 leading-relaxed">{{ $t('settings.security.protected_mini_apps_desc') }}</p>
                      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                        <label v-for="app in availableApps" :key="app.id"
                          class="flex items-center justify-between p-2 rounded-lg border transition-colors cursor-pointer"
                          :class="appLockStore.isAppProtected(app.id) ? 'bg-purple-50 dark:bg-purple-900/20 border-purple-200 dark:border-purple-800' : 'bg-white dark:bg-[#2a2a2a] border-[#e6e6e6] dark:border-[#3a3a3a] hover:border-gray-300 dark:hover:border-gray-500'"
                        >
                          <span class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2">
                            <component :is="app.icon" class="w-4 h-4 text-gray-500" />
                            {{ app.name }}
                          </span>
                          <div class="relative inline-flex h-4 w-7 shrink-0 items-center justify-center rounded-full transition-colors duration-200 ease-in-out cursor-pointer" :class="appLockStore.isAppProtected(app.id) ? 'bg-purple-500' : 'bg-gray-300 dark:bg-gray-600'" @click="handleToggleProtectedApp(app.id, app.name)">
                            <span class="pointer-events-none inline-block h-3 w-3 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="appLockStore.isAppProtected(app.id) ? 'translate-x-1.5' : '-translate-x-1.5'"/>
                          </div>
                        </label>
                      </div>
                    </div>
                  </template>
                </section>

                <!-- Cloud Security (E2EE) -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">{{ $t('settings.security.e2ee') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center gap-2 mb-4">
                      <div class="w-2.5 h-2.5 rounded-full" :class="e2eeStatus.key_available ? 'bg-green-500' : 'bg-amber-500'"></div>
                      <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ e2eeStatus.key_available ? $t('settings.security.e2ee_active') : $t('settings.security.e2ee_setup') }}</p>
                    </div>

                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-6 leading-relaxed">{{ $t('settings.security.e2ee_desc') }}</p>

                    <!-- First time setup -->
                    <div v-if="e2eeStatus.needs_setup && !showRestoreForm" class="space-y-4">
                      <button @click="setupE2ee" :disabled="e2eeLoading" class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-[13px] font-medium transition-all shadow-sm w-full flex items-center justify-center gap-2 disabled:opacity-60">
                        <Lock class="w-4 h-4" /> {{ e2eeLoading ? $t('settings.security.setting_up') : $t('settings.security.setup_encryption') }}
                      </button>
                      <button @click="showRestoreForm = true" class="px-4 py-2 border border-[#e0e0e0] dark:border-[#3a3a3a] text-[#52525b] dark:text-[#a1a1aa] hover:bg-gray-100 dark:hover:bg-[#333] rounded-lg text-[13px] font-medium transition-all w-full">
                        {{ $t('settings.security.existing_vault') }}
                      </button>
                    </div>

                    <!-- Restore from phrase -->
                    <div v-else-if="showRestoreForm && !e2eeStatus.key_available" class="space-y-4">
                      <div class="space-y-1">
                        <label class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('settings.security.recovery_phrase') }}</label>
                        <textarea v-model="restorePhrase" rows="3" placeholder="word1 word2 word3 ... word12" class="w-full px-3 py-2 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-blue-500 resize-none font-mono"></textarea>
                      </div>
                      <div class="flex gap-2">
                        <button @click="restoreFromPhrase" :disabled="e2eeLoading" class="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-[13px] font-medium transition-all shadow-sm flex items-center justify-center gap-2 disabled:opacity-60">
                          {{ e2eeLoading ? $t('settings.security.restoring') : $t('settings.security.restore') }}
                        </button>
                        <button @click="showRestoreForm = false; restorePhrase = ''" class="px-4 py-2 border border-[#e0e0e0] dark:border-[#3a3a3a] text-[#52525b] dark:text-[#a1a1aa] rounded-lg text-[13px] font-medium transition-all">
                          {{ $t('settings.security.cancel') }}
                        </button>
                      </div>
                    </div>

                    <!-- Key available: just show status -->
                    <div v-else-if="e2eeStatus.key_available" class="">
                      <p class="text-[12px] text-green-600 dark:text-green-400 font-medium">{{ $t('settings.security.key_stored_securely') }}</p>
                    </div>

                    <!-- Messages -->
                    <p v-if="e2eeError" class="mt-4 text-[12px] text-red-500 font-medium p-2 bg-red-50 dark:bg-red-900/20 rounded">{{ e2eeError }}</p>
                    <p v-if="e2eeSuccess" class="mt-4 text-[12px] text-green-600 dark:text-green-400 font-medium p-2 bg-green-50 dark:bg-green-900/20 rounded">{{ e2eeSuccess }}</p>
                  </div>
                </section>
              </div>

              <!-- === DEVICES TAB === -->
              <div v-else-if="settingsTab === 'devices'" class="space-y-6">
                <DeviceManager />
              </div>
              
              <!-- === LICENSE TAB === -->
              <div v-if="settingsTab === 'license'" class="space-y-6">
                 <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">License & Subscription</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-6 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] text-center">
                    <Shield class="w-12 h-12 text-primary mx-auto mb-4" />
                    <h5 class="text-lg font-medium text-text dark:text-text-dark mb-2">Synabit License Manager</h5>
                    <p class="text-sm text-text-muted dark:text-text-muted-dark mb-6">View your active subscription plan, manage your license key, or activate a new device.</p>
                    
                    <button @click="showLicenseModal = true" class="px-6 py-2.5 bg-primary hover:bg-primary-dark text-white rounded-xl font-medium transition-colors shadow-sm">
                      Manage License
                    </button>
                  </div>
                </section>
              </div>

              <!-- === ABOUT TAB === -->
              <div v-else-if="settingsTab === 'about'" class="space-y-6">
                <section>
                  <div class="text-center pt-8">
                    <div class="w-16 h-16 bg-gradient-to-br from-gray-100 to-gray-200 dark:from-[#2a2a2a] dark:to-[#333] rounded-2xl flex items-center justify-center mx-auto mb-4 shadow-inner">
                      <Globe class="w-8 h-8 text-gray-400" />
                    </div>
                    <h3 class="text-[18px] font-bold text-[#1c1c1e] dark:text-[#f4f4f5]">Synabit</h3>
                    <p class="text-[12px] text-gray-400 dark:text-gray-500 mt-1">{{ $t('settings.about.version') }} {{ appVersion || '...' }}</p>
                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-4 max-w-xs mx-auto leading-relaxed">{{ $t('settings.about.desc') }}</p>
                    
                    <div v-if="isDesktop" class="mt-8 flex flex-col items-center gap-3">
                      <!-- Check for Updates -->
                      <button @click="checkForUpdates(false, true)" 
                              :disabled="updateChecking || updateDownloading"
                              class="px-4 py-2 rounded-lg text-[12px] font-medium border border-indigo-200 dark:border-indigo-800 text-indigo-600 dark:text-indigo-400 hover:bg-indigo-50 dark:hover:bg-indigo-900/30 transition-all flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed">
                        <RefreshCw v-if="updateChecking" class="w-4 h-4 animate-spin" />
                        <template v-if="updateChecking">{{ $t('update.checking') }}</template>
                        <template v-else-if="updateDownloading">{{ $t('update.downloading') }} {{ updateProgress }}%</template>
                        <template v-else-if="updateAvailable">
                          {{ $t('update.available', { version: updateVersion }) }}
                        </template>
                        <template v-else>{{ $t('update.checkNow') }}</template>
                      </button>

                      <!-- Check Result Feedback -->
                      <Transition name="fade">
                        <p v-if="lastCheckResult === 'up-to-date'" class="text-[12px] text-emerald-600 dark:text-emerald-400 flex items-center gap-1.5">
                          <Check class="w-3.5 h-3.5" /> {{ $t('update.upToDate') }}
                        </p>
                        <p v-else-if="lastCheckResult === 'error'" class="text-[12px] text-red-500 dark:text-red-400">
                          {{ $t('update.failed') }}
                        </p>
                      </Transition>

                      <!-- Install button (khi có update) -->
                      <button v-if="updateAvailable && !updateDownloading" 
                              @click="downloadAndInstall"
                              class="px-4 py-2 rounded-lg text-[12px] font-semibold bg-indigo-600 text-white hover:bg-indigo-700 transition-all">
                        {{ $t('update.installNow') }}
                      </button>

                      <!-- Release Notes (khi có update) -->
                      <p v-if="updateAvailable && updateNotes" 
                         class="text-[11px] text-gray-500 dark:text-gray-400 max-w-xs text-center leading-relaxed">
                        {{ updateNotes.split('\n').slice(0, 3).join('\n') }}
                      </p>

                      <!-- Open Logs -->
                      <button @click="openLogFolder" class="px-4 py-2 rounded-lg text-[12px] font-medium border border-[#e0e0e0] dark:border-[#3a3a3a] text-[#52525b] dark:text-[#a1a1aa] hover:bg-gray-100 dark:hover:bg-[#333] transition-all flex items-center gap-2">
                        <FolderOpen class="w-4 h-4" /> {{ $t('settings.about.open_logs') }}
                      </button>
                    </div>
                  </div>
                </section>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>

  <!-- PIN Verification Overlay -->
  <Teleport to="body">
    <LockScreenVerify
      v-if="showPinVerify"
      :title="pinVerifyTitle"
      @unlocked="onPinVerified"
      @cancelled="showPinVerify = false; pendingAction = null"
    />

    <!-- Confirm Disconnect Modals -->
    <ConfirmModal
      :show="showConfirmDisconnectGDrive"
      title="Disconnect Google Drive?"
      message="Your data will remain safely stored on this device, but changes will no longer sync to your Google Drive account."
      confirm-text="Disconnect"
      @confirm="showConfirmDisconnectGDrive = false; emit('disconnect-gdrive')"
      @cancel="showConfirmDisconnectGDrive = false"
    />
    <ConfirmModal
      :show="showConfirmDisconnectP2P"
      title="Disconnect P2P Sync?"
      message="Other devices will no longer sync with this device until reconnected."
      confirm-text="Disconnect"
      @confirm="showConfirmDisconnectP2P = false; emit('p2p-disconnect')"
      @cancel="showConfirmDisconnectP2P = false"
    />
    <ConfirmModal v-if="showConfirmDisconnectAll" :title="'Disconnect Providers'" :message="'Are you sure you want to disconnect all cloud sync providers? Your data will only remain local.'" confirmText="Disconnect All" @confirm="() => { handleDisconnectAll(); showConfirmDisconnectAll = false; }" @cancel="showConfirmDisconnectAll = false" />

    <LicenseModal :is-open="showLicenseModal" @close="showLicenseModal = false" />
  </Teleport>
</template>

<style scoped>
.settings-modal-enter-active,
.settings-modal-leave-active {
  transition: opacity 0.2s ease;
}
.settings-modal-enter-active > div:last-child,
.settings-modal-leave-active > div:last-child {
  transition: transform 0.2s ease, opacity 0.2s ease;
}
.settings-modal-enter-from,
.settings-modal-leave-to {
  opacity: 0;
}
.settings-modal-enter-from > div:last-child {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}
.settings-modal-leave-to > div:last-child {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}

/* Fade transition for update check result */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
