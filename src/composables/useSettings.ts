import { ref, computed, watch } from 'vue';
import { load } from '@tauri-apps/plugin-store';

/**
 * Composable for app-wide settings and theme management.
 * All state is module-level (singleton) so it persists across components.
 */

// --- Settings Modal ---
const showSettingsModal = ref(false);
const settingsTab = ref<'general' | 'notes' | 'tasks' | 'about'>('general');

// --- Theme ---
const themeMode = ref<'light' | 'dark' | 'system'>('system');

// --- Task Archive ---
const taskArchiveDays = ref(30);

// --- Daily Notes ---
const enableDailyNotes = ref(true);
const dailyNoteFormat = ref('YYYY-MM-DD');
const dailyNoteTag = ref('daily');

const isValidDailyFormat = computed(() => {
  const val = dailyNoteFormat.value.toUpperCase();
  return val.includes('YY') && val.includes('MM') && (val.includes('DD') || val.includes('D'));
});

let isInitialized = false;

export function useSettings() {
  async function initSettings() {
    if (isInitialized) return;
    const store = await load('settings.json', { autoSave: false });
    
    // Load
    themeMode.value = await store.get<'light' | 'dark' | 'system'>('themeMode') || 'system';
    taskArchiveDays.value = Number(await store.get<number>('taskArchiveDays') || 30);
    const hasEnableDaily = await store.has('enableDailyNotes');
    enableDailyNotes.value = hasEnableDaily ? (await store.get<boolean>('enableDailyNotes') as boolean) : true;
    dailyNoteFormat.value = await store.get<string>('dailyNoteFormat') || 'YYYY-MM-DD';
    dailyNoteTag.value = await store.get<string>('dailyNoteTag') || 'daily';
    
    applyTheme();
    isInitialized = true;

    // Watchers (idempotent — safe to call multiple times)
    watch(themeMode, async (newMode) => {
      await store.set('themeMode', newMode);
      await store.save();
      applyTheme();
    });

    watch(taskArchiveDays, async (v) => {
      await store.set('taskArchiveDays', v);
      await store.save();
    });

    watch(enableDailyNotes, async (val) => {
      await store.set('enableDailyNotes', val);
      await store.save();
    });

    watch(dailyNoteFormat, async (val) => {
      await store.set('dailyNoteFormat', val);
      await store.save();
    });

    watch(dailyNoteTag, async (val) => {
      await store.set('dailyNoteTag', val);
      await store.save();
    });
  }

  // --- Actions ---
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
    // Initialization
    initSettings,
    // Settings modal
    showSettingsModal,
    settingsTab,
    // Theme
    themeMode,
    applyTheme,
    // Tasks
    taskArchiveDays,
    // Daily notes
    enableDailyNotes,
    dailyNoteFormat,
    dailyNoteTag,
    isValidDailyFormat,
    // Actions
    openSettings,
  };
}
