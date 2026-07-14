import { ref, computed, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { useAppStore } from '../stores/useAppStore';
import { useAppLockStore } from '../stores/useAppLockStore';
import { useEventBus } from './useEventBus';
import { invoke } from '@tauri-apps/api/core';
import { i18n } from '../i18n';

// UI State (singleton)
const showSettingsModal = ref(false);
const settingsTab = ref<'general' | 'notes' | 'tasks' | 'about' | 'security' | 'devices'>('general');
const showE2eeOnboarding = ref(false);

let isInitialized = false;

export function useSettings() {
  const appStore = useAppStore();
  const appLockStore = useAppLockStore();
  const { 
    themeMode, appLanguage, taskArchiveDays, enableDailyNotes, dailyNoteFormat, 
    dailyNoteTag, nestedNumberListStyle, codeBlockTabSize, defaultApp, hiddenSidebarApps,
    codeBlockBgColorLight, codeBlockTextColorLight, codeBlockBgColorDark, codeBlockTextColorDark
  } = storeToRefs(appStore);

  const isValidDailyFormat = computed(() => {
    const val = dailyNoteFormat.value.toUpperCase();
    return val.includes('YY') && val.includes('MM') && (val.includes('DD') || val.includes('D'));
  });

  const bus = useEventBus();

  async function initSettings() {
    if (isInitialized) return;
    await appStore.initialize();
    applyTheme();
    
    // Initialize App Lock state
    await appLockStore.initialize();
    
    // Listen for E2EE setup required from backend
    bus.on('e2ee:setup-required', () => {
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

    // Sync initial language
    i18n.global.locale.value = appLanguage.value as any;

    // Watch for theme changes to apply class
    watch(themeMode, () => {
      applyTheme();
    });

    // Watch for language changes to update i18n
    watch(appLanguage, (newLang) => {
      i18n.global.locale.value = newLang as any;
    });

    // Watch for code block theme changes
    watch([codeBlockBgColorLight, codeBlockTextColorLight, codeBlockBgColorDark, codeBlockTextColorDark], () => {
      applyCodeBlockTheme();
    });

    // Apply code block theme initially
    applyCodeBlockTheme();
  }

  function applyCodeBlockTheme() {
    const root = document.documentElement;
    root.style.setProperty('--code-block-bg-light', codeBlockBgColorLight.value);
    root.style.setProperty('--code-block-color-light', codeBlockTextColorLight.value);
    root.style.setProperty('--code-block-bg-dark', codeBlockBgColorDark.value);
    root.style.setProperty('--code-block-color-dark', codeBlockTextColorDark.value);
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
    appLanguage,
    applyTheme,
    taskArchiveDays,
    enableDailyNotes,
    dailyNoteFormat,
    dailyNoteTag,
    nestedNumberListStyle,
    codeBlockTabSize,
    codeBlockBgColorLight,
    codeBlockTextColorLight,
    codeBlockBgColorDark,
    codeBlockTextColorDark,
    defaultApp,
    hiddenSidebarApps,
    isValidDailyFormat,
    openSettings,
    showE2eeOnboarding,
  };
}
