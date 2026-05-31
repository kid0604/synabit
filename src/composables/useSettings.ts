import { ref, computed, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { useAppStore } from '../stores/useAppStore';
import { useAppLockStore } from '../stores/useAppLockStore';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

// UI State (singleton)
const showSettingsModal = ref(false);
const settingsTab = ref<'general' | 'notes' | 'tasks' | 'about' | 'security'>('general');
const showE2eeOnboarding = ref(false);

let isInitialized = false;

export function useSettings() {
  const appStore = useAppStore();
  const appLockStore = useAppLockStore();
  const { themeMode, taskArchiveDays, enableDailyNotes, dailyNoteFormat, dailyNoteTag, nestedNumberListStyle, defaultApp, hiddenSidebarApps } = storeToRefs(appStore);

  const isValidDailyFormat = computed(() => {
    const val = dailyNoteFormat.value.toUpperCase();
    return val.includes('YY') && val.includes('MM') && (val.includes('DD') || val.includes('D'));
  });

  async function initSettings() {
    if (isInitialized) return;
    await appStore.initialize();
    applyTheme();
    
    // Initialize App Lock state
    await appLockStore.initialize();
    
    // Listen for E2EE setup required from backend
    listen('e2ee-setup-required', () => {
      showE2eeOnboarding.value = true;
    });
    
    // Check E2EE status on startup
    try {
      const status = await invoke<{ key_available: boolean; needs_setup: boolean }>('check_e2ee_status');
      if (status.needs_setup) {
        showE2eeOnboarding.value = true;
      }
    } catch (e) {
      // E2EE check may fail if DB not ready yet — will be caught on first sync
    }
    
    isInitialized = true;

    // Watch for theme changes to apply class
    watch(themeMode, () => {
      applyTheme();
    });
  }

  function openSettings() {
    showSettingsModal.value = true;
    settingsTab.value = 'general';
  }

  function applyTheme() {
    const isDark =
      themeMode.value === 'dark' ||
      (themeMode.value === 'system' &&
        window.matchMedia('(prefers-color-scheme: dark)').matches);
    if (isDark) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }

  return {
    initSettings,
    showSettingsModal,
    settingsTab,
    themeMode,
    applyTheme,
    taskArchiveDays,
    enableDailyNotes,
    dailyNoteFormat,
    dailyNoteTag,
    nestedNumberListStyle,
    defaultApp,
    hiddenSidebarApps,
    isValidDailyFormat,
    openSettings,
    showE2eeOnboarding,
  };
}
