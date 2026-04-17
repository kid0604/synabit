<script setup lang="ts">
import { Settings, FileText, CheckSquare, Globe, X, FolderOpen, Cloud, CloudOff, RefreshCw } from 'lucide-vue-next';
import { useSettings } from '../composables/useSettings';

const {
  showSettingsModal, settingsTab,
  themeMode,
  taskArchiveDays,
  enableDailyNotes, dailyNoteFormat, dailyNoteTag, isValidDailyFormat,
} = useSettings();

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
  (e: 'update:gdriveAutoSyncEnabled', val: boolean): void;
  (e: 'update:gdriveAutoSyncInterval', val: number): void;
}>();
</script>

<template>
  <Teleport to="body">
    <Transition name="settings-modal">
      <div v-if="showSettingsModal" class="fixed inset-0 z-[200] flex items-center justify-center">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-black/40 dark:bg-black/60 backdrop-blur-sm" @mousedown="showSettingsModal = false"></div>
        
        <!-- Modal Container -->
        <div class="relative w-[720px] max-w-[90vw] h-[520px] max-h-[85vh] bg-[#fdfdfc] dark:bg-[#242424] rounded-2xl shadow-2xl border border-[#e0e0e0] dark:border-[#333] flex overflow-hidden" @mousedown.stop>
          
          <!-- Left Tab Navigation -->
          <nav class="w-[200px] shrink-0 bg-[#f5f5f5] dark:bg-[#1a1a1a] border-r border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col py-5 px-3">
            <h2 class="text-[13px] font-bold text-[#1c1c1e] dark:text-[#f4f4f5] mb-5 px-2">Settings</h2>
            
            <div class="space-y-0.5">
              <button @click="settingsTab = 'general'" 
                :class="['w-full text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center gap-2.5', settingsTab === 'general' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Settings class="w-4 h-4 opacity-70" />
                General
              </button>
              <button @click="settingsTab = 'notes'" 
                :class="['w-full text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center gap-2.5', settingsTab === 'notes' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <FileText class="w-4 h-4 opacity-70" />
                Notes
              </button>
              <button @click="settingsTab = 'tasks'" 
                :class="['w-full text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center gap-2.5', settingsTab === 'tasks' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <CheckSquare class="w-4 h-4 opacity-70" />
                Tasks
              </button>
              <button @click="settingsTab = 'about'" 
                :class="['w-full text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center gap-2.5', settingsTab === 'about' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                <Globe class="w-4 h-4 opacity-70" />
                About
              </button>
            </div>
          </nav>
          
          <!-- Right Content Area -->
          <div class="flex-1 flex flex-col overflow-hidden">
            <!-- Header -->
            <div class="h-12 shrink-0 flex items-center justify-between px-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
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
                      <button @click="emit('sync-gdrive')" :disabled="gdriveSyncing" class="px-3 py-1.5 rounded-lg text-[12px] font-medium transition-all flex items-center gap-1.5 bg-blue-500 hover:bg-blue-600 text-white disabled:opacity-60">
                        <RefreshCw class="w-3.5 h-3.5" :class="gdriveSyncing ? 'animate-spin' : ''" />
                        {{ gdriveSyncing ? 'Syncing…' : 'Sync Now' }}
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
              
              <!-- === ABOUT TAB === -->
              <div v-else-if="settingsTab === 'about'" class="space-y-6">
                <section>
                  <div class="text-center pt-8">
                    <div class="w-16 h-16 bg-gradient-to-br from-gray-100 to-gray-200 dark:from-[#2a2a2a] dark:to-[#333] rounded-2xl flex items-center justify-center mx-auto mb-4 shadow-inner">
                      <Globe class="w-8 h-8 text-gray-400" />
                    </div>
                    <h3 class="text-[18px] font-bold text-[#1c1c1e] dark:text-[#f4f4f5]">Synabit</h3>
                    <p class="text-[12px] text-gray-400 dark:text-gray-500 mt-1">Version 1.0.0-alpha</p>
                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-4 max-w-xs mx-auto leading-relaxed">A unified, local-first productivity workspace for notes, tasks, quick captures, and more.</p>
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
