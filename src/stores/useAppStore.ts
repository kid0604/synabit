import { defineStore } from 'pinia';
import { ref } from 'vue';
import { load, Store } from '@tauri-apps/plugin-store';

export const useAppStore = defineStore('app', () => {
  // Vault & Sync
  const vaultPath = ref<string>('');
  const vaultType = ref<'local' | 'gdrive'>('local');
  const taskArchiveDays = ref<number>(30);
  
  // Daily Notes
  const enableDailyNotes = ref<boolean>(true);
  const dailyNoteFormat = ref<string>('YYYY-MM-DD');
  const dailyNoteTag = ref<string>('daily');
  
  // Editor Settings
  const nestedNumberListStyle = ref<'decimal' | 'alpha' | 'nested'>('decimal');
  
  // App Settings
  const defaultApp = ref<'nexus' | 'note' | 'task' | 'quickcap' | 'file' | 'calendar'>('nexus');
  
  // Theme
  const themeMode = ref<'light' | 'dark' | 'system'>('system');
  
  // GDrive
  const gdriveAutoSyncEnabled = ref<boolean>(true);
  const gdriveAutoSyncInterval = ref<number>(5);
  const gdriveLastSyncTime = ref<string>('');

  let storeInstance: Store | null = null;
  const isReady = ref(false);

  async function initialize() {
    if (isReady.value) return;
    
    // Load store
    storeInstance = await load('settings.json', { autoSave: true } as any);
    
    // Read values
    if (!storeInstance) return;
    
    // Read values
    vaultPath.value = (await storeInstance.get('vaultPath') as string) || '';
    vaultType.value = (await storeInstance.get('vaultType') as 'local' | 'gdrive') || 'local';
    
    themeMode.value = (await storeInstance.get('themeMode') as 'light' | 'dark' | 'system') || 'system';
    
    const arcDays = await storeInstance.get('taskArchiveDays');
    if (arcDays) taskArchiveDays.value = Number(arcDays);
    
    const enDaily = await storeInstance.has('enableDailyNotes');
    if (enDaily) enableDailyNotes.value = (await storeInstance.get('enableDailyNotes')) as boolean;
    
    const dailyFmt = await storeInstance.get('dailyNoteFormat');
    if (dailyFmt) dailyNoteFormat.value = dailyFmt as string;
    
    const dailyTag = await storeInstance.get('dailyNoteTag');
    if (dailyTag) dailyNoteTag.value = dailyTag as string;
    
    const nestedListStyle = await storeInstance.get('nestedNumberListStyle');
    if (nestedListStyle) nestedNumberListStyle.value = nestedListStyle as 'decimal' | 'alpha' | 'nested';
        
    const defApp = await storeInstance.get('defaultApp');
    if (defApp) defaultApp.value = defApp as any;
    
    const autoSync = await storeInstance.get('gdriveAutoSyncEnabled');
    if (autoSync !== null && autoSync !== undefined) gdriveAutoSyncEnabled.value = autoSync as boolean;
    
    const syncInt = await storeInstance.get('gdriveAutoSyncInterval');
    if (syncInt) gdriveAutoSyncInterval.value = Number(syncInt);
    
    const lastTime = await storeInstance.get('gdriveLastSyncTime');
    if (lastTime) gdriveLastSyncTime.value = lastTime as string;

    isReady.value = true;

    // Set up watchers for auto-save
    import('vue').then(({ watch }) => {
      watch(taskArchiveDays, async (v) => {
        if (storeInstance) await storeInstance.set('taskArchiveDays', v);
      });
      watch(enableDailyNotes, async (v) => {
        if (storeInstance) await storeInstance.set('enableDailyNotes', v);
      });
      watch(dailyNoteFormat, async (v) => {
        if (storeInstance) await storeInstance.set('dailyNoteFormat', v);
      });
      watch(dailyNoteTag, async (v) => {
        if (storeInstance) await storeInstance.set('dailyNoteTag', v);
      });
      watch(nestedNumberListStyle, async (v) => {
        if (storeInstance) await storeInstance.set('nestedNumberListStyle', v);
      });

      watch(defaultApp, async (v) => {
        if (storeInstance) await storeInstance.set('defaultApp', v);
      });
      watch(themeMode, async (v) => {
        if (storeInstance) await storeInstance.set('themeMode', v);
      });
      watch(gdriveAutoSyncEnabled, async (v) => {
        if (storeInstance) await storeInstance.set('gdriveAutoSyncEnabled', v);
      });
      watch(gdriveAutoSyncInterval, async (v) => {
        if (storeInstance) await storeInstance.set('gdriveAutoSyncInterval', v);
      });
      watch(gdriveLastSyncTime, async (v) => {
        if (storeInstance) await storeInstance.set('gdriveLastSyncTime', v);
      });
    });
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
    nestedNumberListStyle,
    defaultApp,
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
