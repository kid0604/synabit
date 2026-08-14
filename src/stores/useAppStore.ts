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
    
    // Unified Sync Settings
    const activeSyncProvider = ref<'none' | 'local' | 'gdrive' | 'server'>('none');
    const syncAutoEnabled = ref<boolean>(true);
    const syncAutoInterval = ref<number>(5);
    const syncLastAttempted = ref<string>('');
    const syncLastSuccessful = ref<string>('');
  
    // Sync Server specific
    const syncServerAddr = ref<string>('');
    const syncServerIdHex = ref<string>('');
    const syncCellularPolicy = ref<'all' | 'text_only' | 'off'>('all');
  
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
      
      // Migration script: Check if old settings exist and migrate them to unified settings
      let didMigrate = false;
      
      const hasOldProvider = await storeInstance.has('activeSyncProvider');
      if (!hasOldProvider) {
        // Assume default provider logic based on old config presence
        const hasGDrive = await storeInstance.has('gdriveLastSyncTime');
        const hasP2P = await storeInstance.has('p2pServerAddr');
        
        if (hasP2P) {
          activeSyncProvider.value = 'server';
          
          // Migrate P2P settings
          const p2pAddr = await storeInstance.get('p2pServerAddr');
          if (p2pAddr) syncServerAddr.value = p2pAddr as string;
          const p2pId = await storeInstance.get('p2pServerIdHex');
          if (p2pId) syncServerIdHex.value = p2pId as string;
          const p2pAutoSync = await storeInstance.get('p2pAutoSyncEnabled');
          if (p2pAutoSync !== null) syncAutoEnabled.value = p2pAutoSync as boolean;
          const p2pSyncInt = await storeInstance.get('p2pAutoSyncInterval');
          if (p2pSyncInt) syncAutoInterval.value = Number(p2pSyncInt);
          const p2pLastAtt = await storeInstance.get('syncLastAttempted');
          const p2pLastSucc = await storeInstance.get('syncLastSuccessful');
          if (p2pLastAtt) syncLastAttempted.value = p2pLastAtt as string;
          if (p2pLastSucc) syncLastSuccessful.value = p2pLastSucc as string;
          const p2pCellPolicy = await storeInstance.get('p2pCellularPolicy');
          if (p2pCellPolicy) syncCellularPolicy.value = p2pCellPolicy as any;
          
        } else if (hasGDrive) {
          activeSyncProvider.value = 'gdrive';
          
          // Migrate GDrive settings
          const autoSync = await storeInstance.get('gdriveAutoSyncEnabled');
          if (autoSync !== null) syncAutoEnabled.value = autoSync as boolean;
          const syncInt = await storeInstance.get('gdriveAutoSyncInterval');
          if (syncInt) syncAutoInterval.value = Number(syncInt);
          const lastAtt = await storeInstance.get('syncLastAttempted');
          const lastSucc = await storeInstance.get('syncLastSuccessful');
          if (lastAtt) syncLastAttempted.value = lastAtt as string;
          if (lastSucc) syncLastSuccessful.value = lastSucc as string;
        } else {
          activeSyncProvider.value = 'none';
        }
        didMigrate = true;
      } else {
        // Load unified settings normally
        const provider = await storeInstance.get('activeSyncProvider');
        if (provider) activeSyncProvider.value = provider as any;
        
        const autoSync = await storeInstance.get('syncAutoEnabled');
        if (autoSync !== null && autoSync !== undefined) syncAutoEnabled.value = autoSync as boolean;
        const syncInt = await storeInstance.get('syncAutoInterval');
        if (syncInt) syncAutoInterval.value = Number(syncInt);
        const lastAtt = await storeInstance.get('syncLastAttempted');
        const lastSucc = await storeInstance.get('syncLastSuccessful');
        if (lastAtt) syncLastAttempted.value = lastAtt as string;
        if (lastSucc) syncLastSuccessful.value = lastSucc as string;
        
        const srvAddr = await storeInstance.get('syncServerAddr');
        if (srvAddr) syncServerAddr.value = srvAddr as string;
        const srvId = await storeInstance.get('syncServerIdHex');
        if (srvId) syncServerIdHex.value = srvId as string;
        const cellPolicy = await storeInstance.get('syncCellularPolicy');
        if (cellPolicy) syncCellularPolicy.value = cellPolicy as any;
      }
      
      if (didMigrate) {
        // Clean up old keys and save
        await storeInstance.delete('p2pServerAddr');
        await storeInstance.delete('p2pServerIdHex');
        await storeInstance.delete('p2pAutoSyncEnabled');
        await storeInstance.delete('p2pAutoSyncInterval');
        await storeInstance.delete('p2pLastSyncTime');
        await storeInstance.delete('p2pCellularPolicy');
        await storeInstance.delete('gdriveAutoSyncEnabled');
        await storeInstance.delete('gdriveAutoSyncInterval');
        await storeInstance.delete('gdriveLastSyncTime');
        
        // Save new keys
        await storeInstance.set('activeSyncProvider', activeSyncProvider.value);
        await storeInstance.set('syncAutoEnabled', syncAutoEnabled.value);
        await storeInstance.set('syncAutoInterval', syncAutoInterval.value);
        await storeInstance.set('syncLastAttempted', syncLastAttempted.value);
        await storeInstance.set('syncLastSuccessful', syncLastSuccessful.value);
        await storeInstance.set('syncServerAddr', syncServerAddr.value);
        await storeInstance.set('syncServerIdHex', syncServerIdHex.value);
        await storeInstance.set('syncCellularPolicy', syncCellularPolicy.value);
        
        await storeInstance.save();
      }
  
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
    watch(activeSyncProvider, async (v) => {
      if (storeInstance) await storeInstance.set('activeSyncProvider', v);
    });
    watch(syncAutoEnabled, async (v) => {
      if (storeInstance) await storeInstance.set('syncAutoEnabled', v);
    });
    watch(syncAutoInterval, async (v) => {
      if (storeInstance) await storeInstance.set('syncAutoInterval', v);
    });
    watch(syncLastAttempted, async (v) => {
      if (storeInstance) await storeInstance.set('syncLastAttempted', v);
      await storeInstance?.save();
    });
    watch(syncLastSuccessful, async (v) => {
      if (storeInstance) await storeInstance.set('syncLastSuccessful', v);
      await storeInstance?.save();
    });
    watch(syncServerAddr, async (v) => {
      if (storeInstance) await storeInstance.set('syncServerAddr', v);
    });
    watch(syncServerIdHex, async (v) => {
      if (storeInstance) await storeInstance.set('syncServerIdHex', v);
    });
    watch(syncCellularPolicy, async (v) => {
      if (storeInstance) await storeInstance.set('syncCellularPolicy', v);
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
    activeSyncProvider,
    syncAutoEnabled,
    syncAutoInterval,
    syncLastAttempted,
    syncLastSuccessful,
    syncServerAddr,
    syncServerIdHex,
    syncCellularPolicy,
    setVaultPath,
    setTheme,
    // Add reference access to the store instance if needed outside
    getStoreInstance: () => storeInstance
  };
});
