<script setup lang="ts">
import { Settings, FileText, CheckSquare, Globe, X, FolderOpen, Cloud, CloudOff, RefreshCw, MessageSquare, Zap, Calendar, Palette, Users, Wallet, Lock } from 'lucide-vue-next';
import { useSettings } from '../../composables/useSettings';
import { ref, onMounted } from 'vue';
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { type } from '@tauri-apps/plugin-os';
import { logger } from '../../utils/logger';

const {
  showSettingsModal, settingsTab,
  themeMode, defaultApp,
  taskArchiveDays,
  enableDailyNotes, dailyNoteFormat, dailyNoteTag, isValidDailyFormat,
  nestedNumberListStyle, hiddenSidebarApps
} = useSettings();

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
    e2eeEnabled.value = await invoke<boolean>('is_e2ee_enabled');
  } catch(e) {
    logger.error("Failed to get version/os or E2EE status", e);
  }
});

const openLogFolder = async () => {
  try {
    await invoke('open_app_log_folder');
  } catch (e) {
    logger.error("Failed to open log folder", e);
  }
};

defineProps<{
  vaultPath: string;
  vaultType: 'local' | 'gdrive';
  gdriveConnected: boolean;
  gdriveSyncing: boolean;
  gdriveSyncError: string;
  lastSyncTime: string;
  gdriveAutoSyncEnabled: boolean;
  gdriveAutoSyncInterval: number;
}>();

const emit = defineEmits<{
  (e: 'clear-vault'): void;
  (e: 'sync-gdrive'): void;
  (e: 'disconnect-gdrive'): void;
  (e: 'connect-gdrive'): void;
  (e: 'update:gdriveAutoSyncEnabled', val: boolean): void;
  (e: 'update:gdriveAutoSyncInterval', val: number): void;
}>();

// ─── E2EE Security State ─────────────────────────────────
const e2eeEnabled = ref(false);
const e2eePassword = ref('');
const e2eeConfirmPassword = ref('');
const e2eeError = ref('');
const e2eeSuccess = ref('');

const setE2EEPassword = async () => {
  e2eeError.value = '';
  e2eeSuccess.value = '';
  if (!e2eePassword.value) {
    e2eeError.value = 'Password cannot be empty';
    return;
  }
  if (e2eePassword.value !== e2eeConfirmPassword.value) {
    e2eeError.value = 'Passwords do not match';
    return;
  }
  try {
    await invoke('set_e2ee_password', { password: e2eePassword.value });
    e2eeEnabled.value = true;
    e2eeSuccess.value = 'Master password set successfully.';
    e2eePassword.value = '';
    e2eeConfirmPassword.value = '';
  } catch (err) {
    e2eeError.value = String(err);
  }
};

const clearE2EEPassword = async () => {
  e2eeError.value = '';
  e2eeSuccess.value = '';
  try {
    await invoke('clear_e2ee_password');
    e2eeEnabled.value = false;
    e2eeSuccess.value = 'Master password removed.';
  } catch (err) {
    e2eeError.value = String(err);
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
            <h2 class="hidden md:block text-[13px] font-bold text-[#1c1c1e] dark:text-[#f4f4f5] mb-5 px-2">Settings</h2>
            
            <div class="flex flex-row md:flex-col gap-1 md:gap-0 md:space-y-0.5 overflow-x-auto no-scrollbar">
              <button @click="settingsTab = 'general'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'general' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Settings class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">General</span>
              </button>
              <button @click="settingsTab = 'notes'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'notes' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <FileText class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">Notes</span>
              </button>
              <button @click="settingsTab = 'tasks'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'tasks' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <CheckSquare class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">Tasks</span>
              </button>
              <button @click="settingsTab = 'security'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'security' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Lock class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">Security</span>
              </button>
              <button @click="settingsTab = 'about'" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'about' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Globe class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">About</span>
              </button>
            </div>
          </nav>
          
          <!-- Content Area -->
          <div class="flex-1 flex flex-col overflow-hidden min-h-0 relative">
            <!-- Header -->
            <div class="h-12 shrink-0 flex items-center justify-between px-4 md:px-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c] sticky top-0 bg-[#fdfdfc]/90 dark:bg-[#242424]/90 backdrop-blur-sm z-10">
              <h3 class="text-[15px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] capitalize">{{ settingsTab }}</h3>
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
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Vault</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center gap-2 mb-2">
                      <p class="text-[11px] font-medium text-gray-400 dark:text-gray-500">Storage Type</p>
                      <span v-if="vaultType === 'gdrive'" class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-blue-100 dark:bg-blue-900/40 text-blue-600 dark:text-blue-400 text-[10px] font-semibold">
                        <Cloud class="w-3 h-3" /> Google Drive
                      </span>
                      <span v-else class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 text-[10px] font-semibold">
                        <FolderOpen class="w-3 h-3" /> Local
                      </span>
                    </div>
                    <p class="font-mono text-[12px] break-all text-[#1c1c1e] dark:text-[#f4f4f5] bg-white dark:bg-[#2a2a2a] px-3 py-2 rounded-lg border border-gray-200 dark:border-transparent">{{ vaultPath }}</p>
                    <button @click="emit('clear-vault')" class="mt-3 px-4 py-2 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-lg text-[12px] font-medium transition-all shadow-sm flex items-center gap-2">
                      <FolderOpen class="w-3.5 h-3.5" /> Switch Vault
                    </button>
                  </div>
                </section>

                <!-- Google Drive Sync -->
                <section v-if="vaultType === 'gdrive'">
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Google Drive Sync</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] space-y-4">
                    <div class="flex items-center justify-between">
                      <div class="flex items-center gap-2">
                        <div :class="['w-2 h-2 rounded-full', gdriveConnected ? 'bg-green-500' : 'bg-red-500']"></div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ gdriveConnected ? 'Connected' : 'Disconnected' }}</p>
                      </div>
                      <button v-if="gdriveConnected" @click="emit('sync-gdrive')" :disabled="gdriveSyncing" class="px-3 py-1.5 rounded-lg text-[12px] font-medium transition-all flex items-center gap-1.5 bg-blue-500 hover:bg-blue-600 text-white disabled:opacity-60">
                        <RefreshCw class="w-3.5 h-3.5" :class="gdriveSyncing ? 'animate-spin' : ''" />
                        {{ gdriveSyncing ? 'Syncing…' : 'Sync Now' }}
                      </button>
                      <button v-else @click="emit('connect-gdrive')" class="px-3 py-1.5 rounded-lg text-[12px] font-medium transition-all flex items-center gap-1.5 bg-blue-500 hover:bg-blue-600 text-white">
                        <Cloud class="w-3.5 h-3.5" />
                        Reconnect
                      </button>
                    </div>
                    <div v-if="lastSyncTime" class="flex items-center gap-2 text-[11px] text-gray-400">
                      <span>Last synced: {{ lastSyncTime }}</span>
                    </div>
                    <div v-if="gdriveSyncError" class="text-[11px] text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
                      ⚠️ {{ gdriveSyncError }}
                    </div>
                    <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                      <div class="flex items-center justify-between mb-3">
                        <p class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Periodic Auto Sync</p>
                        <label class="relative inline-flex items-center cursor-pointer">
                          <input type="checkbox" :checked="gdriveAutoSyncEnabled" @change="emit('update:gdriveAutoSyncEnabled', ($event.target as HTMLInputElement).checked)" class="sr-only peer">
                          <div class="w-9 h-5 bg-gray-200 peer-focus:outline-none rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all dark:border-gray-600 peer-checked:bg-blue-500"></div>
                        </label>
                      </div>
                      <div v-if="gdriveAutoSyncEnabled" class="flex items-center justify-between">
                        <p class="text-[11px] text-gray-500 dark:text-gray-400">Sync interval (minutes)</p>
                        <input type="number" :value="gdriveAutoSyncInterval" @input="emit('update:gdriveAutoSyncInterval', Number(($event.target as HTMLInputElement).value))" min="1" max="60" class="w-16 px-2 py-1 bg-white dark:bg-[#2a2a2a] border border-[#e6e6e6] dark:border-[#3a3a3a] rounded text-[12px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:border-blue-500" />
                      </div>
                    </div>
                    <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                      <button @click="emit('disconnect-gdrive')" class="px-4 py-2 rounded-lg text-[12px] font-medium border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 transition-all flex items-center gap-2">
                        <CloudOff class="w-3.5 h-3.5" /> Disconnect Google Drive
                      </button>
                    </div>
                  </div>
                </section>

                <!-- Behavior -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Behavior</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Startup App</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Which mini-app to open when Synabit starts.</p>
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
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Sidebar</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] mb-1">Visible Apps</p>
                    <p class="text-[11px] text-gray-400 dark:text-gray-500 mb-4">Choose which apps to show on the sidebar. The default startup app cannot be hidden.</p>
                    
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                      <label v-for="app in availableApps" :key="app.id" 
                        class="flex items-center justify-between p-2 rounded-lg border transition-colors cursor-pointer"
                        :class="defaultApp === app.id ? 'bg-gray-100 dark:bg-[#252525] border-transparent opacity-60 cursor-not-allowed' : 'bg-white dark:bg-[#2a2a2a] border-[#e6e6e6] dark:border-[#3a3a3a] hover:border-gray-300 dark:hover:border-gray-500'"
                      >
                        <span class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2">
                          <component :is="app.icon" class="w-4 h-4 text-gray-500" />
                          {{ app.name }}
                          <span v-if="defaultApp === app.id" class="text-[9px] px-1.5 py-0.5 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded uppercase font-bold ml-1 tracking-wide">Default</span>
                        </span>
                        
                        <div class="relative inline-flex h-4 w-7 shrink-0 items-center justify-center rounded-full transition-colors duration-200 ease-in-out" :class="!hiddenSidebarApps.includes(app.id) ? 'bg-green-500' : 'bg-gray-300 dark:bg-gray-600'">
                          <input type="checkbox" :checked="!hiddenSidebarApps.includes(app.id)" :disabled="defaultApp === app.id" @change="toggleAppVisibility(app.id)" class="sr-only">
                          <span class="pointer-events-none inline-block h-3 w-3 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="!hiddenSidebarApps.includes(app.id) ? 'translate-x-1.5' : '-translate-x-1.5'"/>
                        </div>
                      </label>
                    </div>
                  </div>
                </section>

                <!-- Theme -->
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Appearance</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <p class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] mb-3">Theme</p>
                    <div class="flex gap-2">
                      <button v-for="mode in (['light', 'dark', 'system'] as const)" :key="mode"
                        @click="themeMode = mode"
                        :class="['px-4 py-2 rounded-lg text-[12px] font-medium transition-all border capitalize', themeMode === mode ? 'bg-black text-white dark:bg-white dark:text-black border-transparent shadow-sm' : 'bg-white dark:bg-[#2a2a2a] border-[#e0e0e0] dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 hover:border-gray-400 dark:hover:border-gray-500']">
                        {{ mode }}
                      </button>
                    </div>
                  </div>
                </section>
              </div>
              
              <!-- === NOTES TAB === -->
              <div v-else-if="settingsTab === 'notes'" class="space-y-6">
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Features</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col gap-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Enable Daily Notes</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Show the "Today" button to quickly create and access daily notes.</p>
                      </div>
                      <button @click="enableDailyNotes = !enableDailyNotes" class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full focus:outline-none transition-colors duration-200 ease-in-out" :class="enableDailyNotes ? 'bg-purple-600' : 'bg-gray-300 dark:bg-gray-600'">
                        <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="enableDailyNotes ? 'translate-x-2' : '-translate-x-2'"/>
                      </button>
                    </div>
                    <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4 flex items-center justify-between" :class="!enableDailyNotes ? 'opacity-50 pointer-events-none' : ''">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Date Format</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Format of the daily note filename (e.g. YYYY-MM-DD or DD-MM-YYYY).</p>
                      </div>
                      <div class="flex flex-col items-end gap-1">
                        <input type="text" v-model="dailyNoteFormat" class="w-28 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 transition-colors" :class="isValidDailyFormat ? 'border-[#e0e0e0] dark:border-[#3a3a3a] focus:ring-black dark:focus:ring-white' : 'border-red-400 focus:ring-red-500'" />
                        <span v-if="!isValidDailyFormat" class="text-[10px] text-red-500 font-medium">Requires YY, MM, DD</span>
                      </div>
                    </div>
                    <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4 flex items-center justify-between" :class="!enableDailyNotes ? 'opacity-50 pointer-events-none' : ''">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Default Tag</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Tag automatically assigned to new daily notes.</p>
                      </div>
                      <input type="text" v-model="dailyNoteTag" placeholder="daily" class="w-28 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors" />
                    </div>
                  </div>
                </section>
                
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3 mt-6">Editor</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col gap-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Nested Numbered List Style</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Style of ordered lists when indented (sub-lists).</p>
                      </div>
                      <select v-model="nestedNumberListStyle" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                        <option value="decimal">Default (1, 2, 3)</option>
                        <option value="alpha">Alphabetical (a, b, c)</option>
                        <option value="nested">Nested (1.1, 1.2)</option>
                      </select>
                    </div>
                  </div>
                </section>
              </div>
              
              <!-- === TASKS TAB === -->
              <div v-else-if="settingsTab === 'tasks'" class="space-y-6">
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Auto Archive</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center justify-between mb-2">
                      <div>
                        <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Archive completed tasks</p>
                        <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Tasks marked as "done" for longer than this period will be moved to the <code class="px-1 py-0.5 bg-gray-200 dark:bg-[#333] rounded text-[10px]">Tasks/archived</code> folder.</p>
                      </div>
                    </div>
                    <div class="flex items-center gap-3 mt-3">
                      <label class="text-[12px] text-gray-500 dark:text-gray-400">After</label>
                      <input type="number" v-model.number="taskArchiveDays" min="1" max="365" class="w-20 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white" />
                      <span class="text-[12px] text-gray-500 dark:text-gray-400">days</span>
                    </div>
                  </div>
                </section>
              </div>
              <!-- === SECURITY TAB === -->
              <div v-else-if="settingsTab === 'security'" class="space-y-6">
                <section>
                  <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">End-to-End Encryption</h4>
                  <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                    <div class="flex items-center gap-2 mb-4">
                      <div :class="['w-2.5 h-2.5 rounded-full', e2eeEnabled ? 'bg-green-500' : 'bg-gray-400']"></div>
                      <p class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">
                        Status: {{ e2eeEnabled ? 'Enabled' : 'Disabled' }}
                      </p>
                    </div>

                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-6 leading-relaxed">
                      Enable End-to-End Encryption (E2EE) by setting a Master Password. 
                      Your data will be encrypted locally before syncing to Google Drive. 
                      <strong class="text-red-500 dark:text-red-400 block mt-2">
                        ⚠️ WARNING: If you lose this password, your data on Google Drive cannot be recovered.
                      </strong>
                    </p>

                    <!-- Set Password Form (if disabled) -->
                    <div v-if="!e2eeEnabled" class="space-y-4">
                      <div class="space-y-1">
                        <label class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Master Password</label>
                        <input type="password" v-model="e2eePassword" class="w-full px-3 py-2 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white" />
                      </div>
                      <div class="space-y-1">
                        <label class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Confirm Password</label>
                        <input type="password" v-model="e2eeConfirmPassword" class="w-full px-3 py-2 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white" />
                      </div>
                      <button @click="setE2EEPassword" class="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg text-[13px] font-medium transition-all shadow-sm w-full flex items-center justify-center gap-2">
                        <Lock class="w-4 h-4" /> Enable Encryption
                      </button>
                    </div>

                    <!-- Remove Password (if enabled) -->
                    <div v-else class="space-y-4 pt-4 border-t border-[#e6e6e6] dark:border-[#333]">
                      <button @click="clearE2EEPassword" class="px-4 py-2 border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg text-[13px] font-medium transition-all w-full">
                        Disable Encryption / Remove Password
                      </button>
                    </div>

                    <!-- Messages -->
                    <p v-if="e2eeError" class="mt-4 text-[12px] text-red-500 font-medium p-2 bg-red-50 dark:bg-red-900/20 rounded">{{ e2eeError }}</p>
                    <p v-if="e2eeSuccess" class="mt-4 text-[12px] text-green-600 dark:text-green-400 font-medium p-2 bg-green-50 dark:bg-green-900/20 rounded">{{ e2eeSuccess }}</p>
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
                    <p class="text-[12px] text-gray-400 dark:text-gray-500 mt-1">Version {{ appVersion || '...' }}</p>
                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-4 max-w-xs mx-auto leading-relaxed">A unified, local-first productivity workspace for notes, tasks, quick captures, and more.</p>
                    
                    <div v-if="isDesktop" class="mt-8 flex justify-center">
                      <button @click="openLogFolder" class="px-4 py-2 rounded-lg text-[12px] font-medium border border-[#e0e0e0] dark:border-[#3a3a3a] text-[#52525b] dark:text-[#a1a1aa] hover:bg-gray-100 dark:hover:bg-[#333] transition-all flex items-center gap-2">
                        <FolderOpen class="w-4 h-4" /> Open Application Logs
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
</style>
