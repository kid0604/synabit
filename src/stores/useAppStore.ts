import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
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
    const codeBlockTabSize = ref<number>(4);
    const codeBlockBgColorLight = ref<string>('#f8f9fa');
    const codeBlockTextColorLight = ref<string>('#24292e');
    const codeBlockBgColorDark = ref<string>('#1e1e1e');
    const codeBlockTextColorDark = ref<string>('#e4e4e7');
    
    // App Settings
    const defaultApp = ref<'nexus' | 'note' | 'task' | 'quickcap' | 'file' | 'calendar' | 'whiteboard' | 'pdf'>('nexus');
    const hiddenSidebarApps = ref<string[]>([]);
    
    // Theme
    const themeMode = ref<'light' | 'dark' | 'system'>('system');
    const appLanguage = ref<'en' | 'vi'>('en');
    
    // GDrive
    const gdriveAutoSyncEnabled = ref<boolean>(true);
    const gdriveAutoSyncInterval = ref<number>(5);
    const gdriveLastSyncTime = ref<string>('');
  
    // P2P Sync
    const p2pServerAddr = ref<string>('');
    const p2pServerIdHex = ref<string>('');
    const p2pAutoSyncEnabled = ref<boolean>(true);
    const p2pAutoSyncInterval = ref<number>(5);
    const p2pLastSyncTime = ref<string>('');
    const p2pCellularPolicy = ref<'all' | 'text_only' | 'off'>('all');
  
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
      
      const lang = await storeInstance.get('appLanguage');
      if (lang) appLanguage.value = lang as 'en' | 'vi';
      
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
          
      const tabSize = await storeInstance.get('codeBlockTabSize');
      if (tabSize !== null && tabSize !== undefined) codeBlockTabSize.value = Number(tabSize);
      
      const cbBgLight = await storeInstance.get('codeBlockBgColorLight');
      if (cbBgLight) codeBlockBgColorLight.value = cbBgLight as string;
      const cbTextLight = await storeInstance.get('codeBlockTextColorLight');
      if (cbTextLight) codeBlockTextColorLight.value = cbTextLight as string;
      const cbBgDark = await storeInstance.get('codeBlockBgColorDark');
      if (cbBgDark) codeBlockBgColorDark.value = cbBgDark as string;
      const cbTextDark = await storeInstance.get('codeBlockTextColorDark');
      if (cbTextDark) codeBlockTextColorDark.value = cbTextDark as string;
      
      const defApp = await storeInstance.get('defaultApp');
      if (defApp) defaultApp.value = defApp as any;
      
      const hApps = await storeInstance.get('hiddenSidebarApps');
      if (hApps && Array.isArray(hApps)) hiddenSidebarApps.value = hApps as string[];
      
      const autoSync = await storeInstance.get('gdriveAutoSyncEnabled');
      if (autoSync !== null && autoSync !== undefined) gdriveAutoSyncEnabled.value = autoSync as boolean;
      
      const syncInt = await storeInstance.get('gdriveAutoSyncInterval');
      if (syncInt) gdriveAutoSyncInterval.value = Number(syncInt);
      
      const lastTime = await storeInstance.get('gdriveLastSyncTime');
      if (lastTime) gdriveLastSyncTime.value = lastTime as string;
  
      // P2P Sync
      const p2pAddr = await storeInstance.get('p2pServerAddr');
      if (p2pAddr) p2pServerAddr.value = p2pAddr as string;
      const p2pId = await storeInstance.get('p2pServerIdHex');
      if (p2pId) p2pServerIdHex.value = p2pId as string;
      const p2pAutoSync = await storeInstance.get('p2pAutoSyncEnabled');
      if (p2pAutoSync !== null && p2pAutoSync !== undefined) p2pAutoSyncEnabled.value = p2pAutoSync as boolean;
      const p2pSyncInt = await storeInstance.get('p2pAutoSyncInterval');
      if (p2pSyncInt) p2pAutoSyncInterval.value = Number(p2pSyncInt);
      const p2pLast = await storeInstance.get('p2pLastSyncTime');
      if (p2pLast) p2pLastSyncTime.value = p2pLast as string;
      const p2pCellPolicy = await storeInstance.get('p2pCellularPolicy');
      if (p2pCellPolicy) p2pCellularPolicy.value = p2pCellPolicy as any;
  
      isReady.value = true;
  
      // Set up watchers for auto-save
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
      watch(codeBlockTabSize, async (v) => {
        if (storeInstance) await storeInstance.set('codeBlockTabSize', v);
      });
      watch(codeBlockBgColorLight, async (v) => {
        if (storeInstance) await storeInstance.set('codeBlockBgColorLight', v);
      });
      watch(codeBlockTextColorLight, async (v) => {
        if (storeInstance) await storeInstance.set('codeBlockTextColorLight', v);
      });
      watch(codeBlockBgColorDark, async (v) => {
        if (storeInstance) await storeInstance.set('codeBlockBgColorDark', v);
      });
      watch(codeBlockTextColorDark, async (v) => {
        if (storeInstance) await storeInstance.set('codeBlockTextColorDark', v);
      });

    watch(defaultApp, async (v) => {
      if (storeInstance) await storeInstance.set('defaultApp', v);
    });
    watch(hiddenSidebarApps, async (v) => {
      if (storeInstance) await storeInstance.set('hiddenSidebarApps', v);
    }, { deep: true });
    watch(themeMode, async (v) => {
      if (storeInstance) await storeInstance.set('themeMode', v);
    });
    watch(appLanguage, async (v) => {
      if (storeInstance) await storeInstance.set('appLanguage', v);
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
    watch(p2pServerAddr, async (v) => {
      if (storeInstance) await storeInstance.set('p2pServerAddr', v);
    });
    watch(p2pServerIdHex, async (v) => {
      if (storeInstance) await storeInstance.set('p2pServerIdHex', v);
    });
    watch(p2pAutoSyncEnabled, async (v) => {
      if (storeInstance) await storeInstance.set('p2pAutoSyncEnabled', v);
    });
    watch(p2pAutoSyncInterval, async (v) => {
      if (storeInstance) await storeInstance.set('p2pAutoSyncInterval', v);
    });
    watch(p2pLastSyncTime, async (v) => {
      if (storeInstance) await storeInstance.set('p2pLastSyncTime', v);
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
    codeBlockTabSize,
    codeBlockBgColorLight,
    codeBlockTextColorLight,
    codeBlockBgColorDark,
    codeBlockTextColorDark,
    defaultApp,
    hiddenSidebarApps,
    themeMode,
    appLanguage,
    gdriveAutoSyncEnabled,
    gdriveAutoSyncInterval,
    gdriveLastSyncTime,
    p2pServerAddr,
    p2pServerIdHex,
    p2pAutoSyncEnabled,
    p2pAutoSyncInterval,
    p2pLastSyncTime,
    p2pCellularPolicy,
    setVaultPath,
    setTheme,
    // Add reference access to the store instance if needed outside
    getStoreInstance: () => storeInstance
  };
});
