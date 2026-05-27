import { ref, computed, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { useAppStore } from '../stores/useAppStore';

// UI State (singleton)
const showSettingsModal = ref(false);
const settingsTab = ref<'general' | 'notes' | 'tasks' | 'about'>('general');

let isInitialized = false;

export function useSettings() {
  const appStore = useAppStore();
  const { themeMode, taskArchiveDays, enableDailyNotes, dailyNoteFormat, dailyNoteTag, nestedNumberListStyle, defaultApp, hiddenSidebarApps } = storeToRefs(appStore);

  const isValidDailyFormat = computed(() => {
    const val = dailyNoteFormat.value.toUpperCase();
    return val.includes('YY') && val.includes('MM') && (val.includes('DD') || val.includes('D'));
  });

  async function initSettings() {
    if (isInitialized) return;
    await appStore.initialize();
    applyTheme();
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
  };
}
