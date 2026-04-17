import { ref, computed, watch } from 'vue';

/**
 * Composable for app-wide settings and theme management.
 * All state is module-level (singleton) so it persists across components.
 */

// --- Settings Modal ---
const showSettingsModal = ref(false);
const settingsTab = ref<'general' | 'notes' | 'tasks' | 'about'>('general');

// --- Theme ---
const themeMode = ref<'light' | 'dark' | 'system'>(
  localStorage.getItem('synabitThemeMode') as 'light' | 'dark' | 'system' || 'system'
);

// --- Task Archive ---
const taskArchiveDays = ref(Number(localStorage.getItem('synabitTaskArchiveDays') || '30'));

// --- Daily Notes ---
const enableDailyNotes = ref(localStorage.getItem('synabitConfig_enableDailyNotes') !== 'false');
const dailyNoteFormat = ref(localStorage.getItem('synabitConfig_dailyNoteFormat') || 'YYYY-MM-DD');
const dailyNoteTag = ref(localStorage.getItem('synabitConfig_dailyNoteTag') ?? 'daily');

const isValidDailyFormat = computed(() => {
  const val = dailyNoteFormat.value.toUpperCase();
  return val.includes('YY') && val.includes('MM') && (val.includes('DD') || val.includes('D'));
});

export function useSettings() {
  // --- Watchers (idempotent — safe to call multiple times) ---
  watch(themeMode, (newMode) => {
    localStorage.setItem('synabitThemeMode', newMode);
    applyTheme();
  });

  watch(taskArchiveDays, (v) => {
    localStorage.setItem('synabitTaskArchiveDays', String(v));
  });

  watch(enableDailyNotes, (val) =>
    localStorage.setItem('synabitConfig_enableDailyNotes', String(val))
  );

  watch(dailyNoteFormat, (val) =>
    localStorage.setItem('synabitConfig_dailyNoteFormat', val)
  );

  watch(dailyNoteTag, (val) =>
    localStorage.setItem('synabitConfig_dailyNoteTag', val)
  );

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
