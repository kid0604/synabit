import { defineStore } from 'pinia';
import { ref } from 'vue';
import { load } from '@tauri-apps/plugin-store';

export const useAppStore = defineStore('app', () => {
  // Vault & Sync
  const vaultPath = ref<string>('');
  const vaultType = ref<'local' | 'gdrive'>('local');
  const taskArchiveDays = ref<number>(30);
  
  // Daily Notes
  const enableDailyNotes = ref<boolean>(true);
  const dailyNoteFormat = ref<string>('YYYY-MM-DD');
  const dailyNoteTag = ref<string>('daily');
  
  // Theme
  const themeMode = ref<'light' | 'dark' | 'system'>('system');
  
  // GDrive
  const gdriveAutoSyncEnabled = ref<boolean>(true);
  const gdriveAutoSyncInterval = ref<number>(5);
  const gdriveLastSyncTime = ref<string>('');

  let storeInstance: any = null;
  const isReady = ref(false);

  async function initialize() {
    if (isReady.value) return;
    
    // Load store
    storeInstance = await load('settings.json', { autoSave: true });
    
    // Read values
    vaultPath.value = (await storeInstance.get<string>('vaultPath')) || '';
    vaultType.value = (await storeInstance.get<'local' | 'gdrive'>('vaultType')) || 'local';
    
    themeMode.value = (await storeInstance.get<'light' | 'dark' | 'system'>('themeMode')) || 'system';
    
    const arcDays = await storeInstance.get<number>('taskArchiveDays');
    if (arcDays) taskArchiveDays.value = Number(arcDays);
    
    const enDaily = await storeInstance.has('enableDailyNotes');
    if (enDaily) enableDailyNotes.value = (await storeInstance.get<boolean>('enableDailyNotes')) as boolean;
    
    const dailyFmt = await storeInstance.get<string>('dailyNoteFormat');
    if (dailyFmt) dailyNoteFormat.value = dailyFmt;
    
    const dailyTag = await storeInstance.get<string>('dailyNoteTag');
    if (dailyTag) dailyNoteTag.value = dailyTag;
    
    const autoSync = await storeInstance.get<boolean>('gdriveAutoSyncEnabled');
    if (autoSync !== null && autoSync !== undefined) gdriveAutoSyncEnabled.value = autoSync;
    
    const syncInt = await storeInstance.get<number>('gdriveAutoSyncInterval');
    if (syncInt) gdriveAutoSyncInterval.value = Number(syncInt);
    
    const lastTime = await storeInstance.get<string>('gdriveLastSyncTime');
    if (lastTime) gdriveLastSyncTime.value = lastTime;

    isReady.value = true;
  }

  // Setters wrapper that automatically persist to Tauri Store
  async function setVaultPath(path: string, type: 'local' | 'gdrive') {
    vaultPath.value = path;
    vaultType.value = type;
    if (storeInstance) {
      await storeInstance.set('vaultPath', path);
      await storeInstance.set('vaultType', type);
      await storeInstance.save(); // if autoSave is somehow disabled
    }
  }

  async function setTheme(mode: 'light' | 'dark' | 'system') {
    themeMode.value = mode;
    if (storeInstance) {
        await storeInstance.set('themeMode', mode);
        await storeInstance.save();
    }
  }

  return {
    isReady,
    initialize,
    vaultPath,
    vaultType,
    taskArchiveDays,
    enableDailyNotes,
    dailyNoteFormat,
    dailyNoteTag,
    themeMode,
    gdriveAutoSyncEnabled,
    gdriveAutoSyncInterval,
    gdriveLastSyncTime,
    setVaultPath,
    setTheme,
    // Add reference access to the store instance if needed outside
    getStoreInstance: () => storeInstance
  };
});
