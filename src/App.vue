<script setup lang="ts">
import { ref, computed, provide, onMounted, onUnmounted, watch } from 'vue';
import { FileText, FolderOpen, Calendar, CheckSquare, Zap, Globe, Cloud, RefreshCw, CloudOff, Settings, Users, Wallet, MessageCircle, Palette, MoreHorizontal, Rss, Server } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { initEventBus, destroyEventBus, useEventBus } from './composables/useEventBus';
import { useNodeService } from './composables/useNodeService';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { documentDir } from '@tauri-apps/api/path';

import { defineAsyncComponent } from 'vue';
import { useRouter, useRoute } from 'vue-router';

// Settings Modal is the only async component kept here
const SettingsModal = defineAsyncComponent(() => import('./shared/components/SettingsModal.vue'));
const E2eeOnboarding = defineAsyncComponent(() => import('./shared/components/E2eeOnboarding.vue'));
const LockScreen = defineAsyncComponent(() => import('./shared/components/LockScreen.vue'));
const SetupPinModal = defineAsyncComponent(() => import('./shared/components/SetupPinModal.vue'));
const SyncConflictToast = defineAsyncComponent(() => import('./shared/components/SyncConflictToast.vue'));
const GDriveMigrationModal = defineAsyncComponent(() => import('./shared/components/GDriveMigrationModal.vue'));
const RecoveryModal = defineAsyncComponent(() => import('./shared/components/RecoveryModal.vue'));
const VaultBackupNotice = defineAsyncComponent(() => import('./shared/components/VaultBackupNotice.vue'));

// Composables
import { useSettings } from './composables/useSettings';
import { useGDrive } from './composables/useGDrive';
import { useSync } from './composables/useSync';
import { useAppLock } from './composables/useAppLock';
import { usePlatform } from './composables/usePlatform';
import { useBackGuard } from './composables/useBackGuard';
import { appInPlatformScope } from './shared/platformScope';
import { ensureNotificationPermission } from './composables/useNotificationPermission';
import { useAppUpdate } from './composables/useAppUpdate';
import { useCaptureIntake } from './mini-apps/quickcap/useQuickCapWriter';
import { isComposeUrl } from './mini-apps/quickcap/captureUrl';
import { onOpenUrl, getCurrent } from '@tauri-apps/plugin-deep-link';


import DesktopLayout from './layouts/DesktopLayout.vue';
import MobileLayout from './layouts/MobileLayout.vue';

// Stores
import { useAppStore } from './stores/useAppStore';
import { useNavigationStore, type NavEntry } from './stores/useNavigationStore';
import { useAppLockStore } from './stores/useAppLockStore';
import { useLicenseStore } from './stores/useLicenseStore';
import { storeToRefs } from 'pinia';

const LicenseModal = defineAsyncComponent(() => import('./shared/components/LicenseModal.vue'));
const showLicenseModal = ref(false);

const licenseStore = useLicenseStore();

const bus = useEventBus();
const ns = useNodeService();

// ─── Auto-Update ──────────────────────────────────────────
const {
  updateAvailable, updateVersion, updateNotes,
  isDownloading: updateDownloading,
  downloadProgress: updateProgress,
  downloadAndInstall, dismissUpdate,
} = useAppUpdate();

// ─── Settings ─────────────────────────────────────────────
const {
  showSettingsModal, openSettings, initSettings, applyTheme, defaultApp, hiddenSidebarApps, showE2eeOnboarding, showRecoveryModal
} = useSettings();

const ALL_APPS = [
  { id: 'nexus', name: 'Nexus', icon: Globe },
  { id: 'messages', name: 'Messages', icon: MessageCircle },
  { id: 'quickcap', name: 'QuickCap', icon: Zap },
  { id: 'note', name: 'Notes', icon: FileText },
  { id: 'task', name: 'Tasks', icon: CheckSquare },
  { id: 'calendar', name: 'Calendar', icon: Calendar },
  { id: 'file', name: 'Files', icon: FolderOpen },
  { id: 'whiteboard', name: 'Whiteboard', icon: Palette },
  { id: 'people', name: 'People', icon: Users },
  { id: 'finance', name: 'Finance', icon: Wallet },
  { id: 'feeds', name: 'Feeds', icon: Rss },
];

const getAppName = (appId: string): string => {
  return ALL_APPS.find(a => a.id === appId)?.name || appId;
};

/**
 * The mini-apps this platform ships at all.
 *
 * Distinct from what the user chose to hide, and from what fits on screen. An
 * app outside the platform's scope is not in the product here — it never
 * reaches the bottom bar, the More menu, or the router.
 *
 * Note this keys off `isMobileOS` and not `useMobileLayout`: a desktop window
 * dragged narrow adopts the mobile layout, and must keep every app.
 */
const platformApps = computed(() => ALL_APPS.filter(a => appInPlatformScope(a.id)));

const mobileVisibleApps = computed(() => {
    return platformApps.value
        .filter(a => !hiddenSidebarApps.value.includes(a.id))
        .slice(0, 4)
        .map(a => a.id);
});

const isAppVisible = (appId: string) => {
    if (!appInPlatformScope(appId)) return false;
    if (hiddenSidebarApps.value.includes(appId)) return false;
    if (useMobileLayout.value && !mobileVisibleApps.value.includes(appId)) return false;
    return true;
};

const moreMenuApps = computed(() => {
    return platformApps.value.filter(a => {
        const isUserHidden = hiddenSidebarApps.value.includes(a.id);
        const isMobileHidden = useMobileLayout.value && !mobileVisibleApps.value.includes(a.id);
        return isUserHidden || isMobileHidden;
    });
});

// ─── App Lock ─────────────────────────────────────────────
const appLockStore = useAppLockStore();
const currentAppIdRef = computed(() => (route.name as string) || null);
useAppLock(currentAppIdRef); // Activity monitoring + session refresh
const showSetupPinModal = ref(false);
const setupPinMode = ref<'setup' | 'change'>('setup');

const showHiddenAppsMenu = ref(false);

const appStore = useAppStore();
const { vaultPath, vaultType, activeSyncProvider } = storeToRefs(appStore);

const { useMobileLayout, isMobileOS, initOS } = usePlatform();

// ─── App View State (Vue Router) ──────────────────────────
const router = useRouter();
const route = useRoute();

const activeTool = computed({
  get: () => (route.name as string) || 'nexus',
  set: (val: string) => { 
      if (route.name !== val) {
          router.push({ name: val }).catch(err => {
              logger.warn('Router navigation error:', err);
          });
      }
  }
});

// ─── Navigation History (Back/Forward) — declared early so watcher can use them ─────
const navStore = useNavigationStore();
let isRestoringNav = false;

const getItemIdForApp = (app: string): string | undefined => {
    switch (app) {
        case 'note': return noteAppRef.value?.currentNoteId || undefined;
        case 'whiteboard': return whiteboardAppRef.value?.currentBoardId || undefined;
        case 'file': return filesAppRef.value?.activeTabId || undefined;
        default: return undefined;
    }
};

const getCurrentItemId = (): string | undefined => getItemIdForApp(activeTool.value);

const getCurrentScrollTop = (): number => {
    const el = document.querySelector('[data-app-scroll]') as HTMLElement;
    return el?.scrollTop || 0;
};

watch(activeTool, async (newTool, oldTool) => {
  if (oldTool !== newTool) {
    logger.debug(`Navigated to mini-app: ${newTool} (from ${oldTool})`);
    // Push old location onto the back stack (unless we're restoring from nav history)
    if (!isRestoringNav && oldTool) {
      navStore.pushNavigation({
        app: oldTool,
        itemId: getItemIdForApp(oldTool),
        scrollTop: getCurrentScrollTop(),
      });
    }
  }
  
  if (newTool === 'messages' && vaultPath.value) {
     if (messagesAppRef.value) {
         messagesAppRef.value.fetchNotifications();
     }
     
     if (unreadNotificationCount.value > 0) {
         unreadNotificationCount.value = 0;
         try {
             await invoke('mark_chat_read', { vaultPath: vaultPath.value });
         } catch (e) {
             logger.error('Failed to mark chat as read', e);
         }
     }
  }

  if (newTool === 'whiteboard' && vaultPath.value) {
     if (whiteboardAppRef.value && typeof whiteboardAppRef.value.refreshBoards === 'function') {
         whiteboardAppRef.value.refreshBoards();
     }
  }
});


// ─── Mini App Refs for cross-app navigation ─────────────────
const messagesAppRef = ref<any>(null);
const noteAppRef = ref<any>(null);
const quickCapAppRef = ref<any>(null);
const taskAppRef = ref<any>(null);
const calendarAppRef = ref<any>(null);
const whiteboardAppRef = ref<any>(null);
const peopleAppRef = ref<any>(null);
const financeAppRef = ref<any>(null);
const feedsAppRef = ref<any>(null);
const filesAppRef = ref<any>(null);

const setAppRef = (el: any, name: string) => {
    if (!el) return;
    if (name === 'messages') messagesAppRef.value = el;
    else if (name === 'note') noteAppRef.value = el;
    else if (name === 'quickcap') quickCapAppRef.value = el;
    else if (name === 'task') taskAppRef.value = el;
    else if (name === 'calendar') calendarAppRef.value = el;
    else if (name === 'whiteboard') whiteboardAppRef.value = el;
    else if (name === 'people') peopleAppRef.value = el;
    else if (name === 'finance') financeAppRef.value = el;
    else if (name === 'feeds') feedsAppRef.value = el;
    else if (name === 'file') filesAppRef.value = el;
};

// ─── Floating Note (opened in new window) ─────────────────

watch(activeTool, (newTool) => {
    if (newTool === 'task') {
        taskAppRef.value?.refresh?.();
    }
});
const floatingNoteId = ref<string | null>(null);
const isFloatingView = ref(false);

// ─── GDrive ─────────────────────────────────────────────────
const gdrive = useGDrive(vaultPath, vaultType);
const showGDriveMigrationModal = ref(false);

const handleGDriveMigrated = async (newPath: string) => {
    showGDriveMigrationModal.value = false;
    await appStore.setVaultPath(newPath, 'local');

    invoke('start_vault_watcher', { vaultPath: newPath }).catch(logger.error);
    syncState.sync();
};

/**
 * How many caps are waiting to be turned into something.
 *
 * Promotion trashes the cap it came from, so everything still in QuickCap is
 * by definition unprocessed — the count is the inbox. It is here rather than
 * inside QuickCapApp because the whole point is to be visible when that tab
 * is *not* open: a fleeting note only stays fleeting if something reminds you
 * it is still sitting there.
 */
const quickCapCount = ref(0);

const refreshQuickCapCount = async () => {
    if (!vaultPath.value) {
        quickCapCount.value = 0;
        return;
    }
    try {
        quickCapCount.value = await invoke<number>('count_inbox_caps');
    } catch (e) {
        logger.error('Could not count quick caps', e);
    }
};

// ─── Captures from outside the app ───────────────────────
//
// A share sheet, a widget or a hotkey can hand over a thought at a moment
// when no vault is open — locked, unchosen, or the process only just
// started by an intent. Those are queued rather than written, and this is
// where they finally land. It lives here rather than in QuickCapApp so a
// capture arrives even when the user never opens that tab.
const { drainCaptures } = useCaptureIntake();
let stopCaptureListener: (() => void) | null = null;
let stopComposeListener: (() => void) | null = null;

watch(
    vaultPath,
    (path) => {
        if (path) {
            void drainCaptures();
            void refreshQuickCapCount();
        } else {
            quickCapCount.value = 0;
        }
    },
    { immediate: true },
);

// ─── Sync ────────────────────────────────────────────────
const syncState = useSync(vaultPath, activeSyncProvider);

// Files another device's version displaced. Held until dismissed rather than
// cleared on the next sync: syncs run on their own, and a notice that clears
// itself is a notice nobody sees.
const syncConflictCount = computed(() => syncState.syncConflicts.value.length);
const showSyncConflicts = ref(false);
let lastAutoSyncTriggerTime = 0;

// The Android back button closes the topmost layer rather than the app. Every
// dismissible thing owned by this component is registered here, after all of
// them exist; see useBackGuard for why this cooperates with Tauri's back
// handling instead of replacing it.
//
// E2EE onboarding and the recovery modal are deliberately absent: both are
// flows where leaving half way puts the vault in a state the user cannot see,
// and a stray back press is exactly how that happens.
useBackGuard(showSettingsModal, () => { showSettingsModal.value = false; });
useBackGuard(showLicenseModal, () => { showLicenseModal.value = false; });
useBackGuard(showSetupPinModal, () => { showSetupPinModal.value = false; });
const hiddenAppsGuard = useBackGuard(showHiddenAppsMenu, () => { showHiddenAppsMenu.value = false; });
useBackGuard(showSyncConflicts, () => { showSyncConflicts.value = false; });
useBackGuard(showGDriveMigrationModal, () => { showGDriveMigrationModal.value = false; });

/**
 * Open an app the sidebar is not showing.
 *
 * The menu has to give up its back-guard entry before the route changes.
 * Closing it the ordinary way schedules a `history.back()`, and the router
 * pushes the new route a few microtasks later — so the press lands on the
 * navigation and undoes it, and the click looks ignored. This was how People
 * and Finance became unreachable once they were hidden from the sidebar.
 */
const openHiddenApp = (appId: string) => {
    hiddenAppsGuard.detach();
    showHiddenAppsMenu.value = false;
    activeTool.value = appId;
};


const selectVault = async () => {
    try {
        if (isMobileOS.value) {
            // No directory picker exists on mobile, so the backend decides and
            // reports where the vault lives.
            const resolved = await invoke<string>('resolve_mobile_vault_path');
            await appStore.setVaultPath(resolved, 'local');
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(logger.error);

     // Feeds refresh on a timer for as long as the app is running, not only
     // while the Feeds tab happens to be open. Starting it here rather than in
     // the mini-app is the difference between "background refresh" and
     // "refresh whenever you go and look".
     invoke('feed_start_scheduler', { vaultPath: vaultPath.value })
         .catch((e) => logger.error('Failed to start feed scheduler', e));
            return;
        }

        const defaultPath = await documentDir().catch(() => undefined);
        const selected = await open({ 
            title: 'Select Note Vault Directory', 
            defaultPath,
            directory: true, 
            multiple: false 
        });
        if (selected) {
            await appStore.setVaultPath(selected as string, 'local');
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(logger.error);
        }
    } catch(err) { logger.error(String(err)); }
};

const clearVault = () => {
    vaultPath.value = '';
    vaultType.value = 'local';
    activeTool.value = 'nexus';
    syncState.setupAutoSync();
};

// ─── Navigation History (Back/Forward) — continued ───────

/** Build a NavEntry snapshot of the current state */
const buildCurrentEntry = (): NavEntry => ({
    app: activeTool.value,
    itemId: getCurrentItemId(),
    scrollTop: getCurrentScrollTop(),
});

/** Navigate to a NavEntry — switch tool and restore item + scroll */
const navigateToEntry = (entry: NavEntry) => {
    isRestoringNav = true;
    activeTool.value = entry.app as any;
    if (entry.itemId) {
        navigateToItem(entry.app, entry.itemId, entry.scrollTop, true);
    } else if (entry.scrollTop) {
        setTimeout(() => {
            const el = document.querySelector('[data-app-scroll]') as HTMLElement;
            if (el) el.scrollTop = entry.scrollTop!;
        }, 150);
    }
    setTimeout(() => { isRestoringNav = false; }, 300);
};

const handleGoBack = () => {
    const entry = navStore.goBack(buildCurrentEntry());
    if (entry) navigateToEntry(entry);
};

const handleGoForward = () => {
    const entry = navStore.goForward(buildCurrentEntry());
    if (entry) navigateToEntry(entry);
};

// Provide navigation to all child mini-apps via inject
// NOTE: Pinia auto-unwraps computed refs, so navStore.canGoBack returns a plain boolean.
// We must wrap in computed() to keep reactivity through provide/inject.
provide('canGoBack', computed(() => navStore.canGoBack));
provide('canGoForward', computed(() => navStore.canGoForward));
provide('goBack', handleGoBack);
provide('goForward', handleGoForward);
provide('pushNavigation', (entry?: NavEntry) => {
    navStore.pushNavigation(entry || buildCurrentEntry());
});

// ─── Cross-app Navigation (Nexus → Note/Task/QuickCap) ───

const callWhenReady = (getRef: () => any, method: string, ...args: any[]) => {
    let attempts = 0;
    const interval = setInterval(() => {
        const componentRef = getRef();
        if (componentRef && typeof componentRef[method] === 'function') {
            clearInterval(interval);
            componentRef[method](...args);
        } else if (attempts >= 40) { // 2 seconds max
            clearInterval(interval);
            logger.warn(`Component ref or method ${method} not ready after 2s`);
        }
        attempts++;
    }, 50);
};

/** Navigate to a specific item within an app, optionally restoring scroll */
const navigateToItem = (app: string, itemId: string, scrollTop?: number, skipNavPush = false) => {
    const restoreScroll = () => {
        if (scrollTop) {
            setTimeout(() => {
                const el = document.querySelector('[data-app-scroll]') as HTMLElement;
                if (el) el.scrollTop = scrollTop;
            }, 200);
        }
    };

    if (app === 'note') { callWhenReady(() => noteAppRef.value, 'openNoteById', itemId, skipNavPush); restoreScroll(); }
    else if (app === 'quickcap') { callWhenReady(() => quickCapAppRef.value, 'openEditById', itemId); }
    else if (app === 'task') { callWhenReady(() => taskAppRef.value, 'openEditById', itemId); }
    else if (app === 'calendar') { callWhenReady(() => calendarAppRef.value, 'openEventById', itemId); }
    else if (app === 'whiteboard') { callWhenReady(() => whiteboardAppRef.value, 'openBoardById', itemId, skipNavPush); restoreScroll(); }
    else if (app === 'people') { callWhenReady(() => peopleAppRef.value, 'openPersonById', itemId); }
    else if (app === 'finance') { callWhenReady(() => financeAppRef.value, 'openMonthById', itemId); }
    else if (app === 'feeds') { callWhenReady(() => feedsAppRef.value, 'openFeedById', itemId); }
    else if (app === 'file') { callWhenReady(() => filesAppRef.value, 'openFileById', itemId, skipNavPush); }
};

const handleEditFromNexus = async (id: string, type: string, query?: string) => {
    logger.debug(`App.vue: handleEditFromNexus received id: ${id}, type: ${type}`);
    // Note: watcher on activeTool now handles pushing to back stack automatically
    if (type === 'note') { 
        activeTool.value = 'note'; 
        callWhenReady(() => noteAppRef.value, 'openNoteById', id);
    }
    else if (type === 'quickcap') { 
        activeTool.value = 'quickcap'; 
        callWhenReady(() => quickCapAppRef.value, 'openEditById', id);
    }
    else if (type === 'task') { 
        activeTool.value = 'task'; 
        callWhenReady(() => taskAppRef.value, 'openEditById', id);
    }
    else if (type === 'calendar') { 
        activeTool.value = 'calendar'; 
        callWhenReady(() => calendarAppRef.value, 'openEventById', id);
    }
    else if (type === 'whiteboard') {
        activeTool.value = 'whiteboard';
        callWhenReady(() => whiteboardAppRef.value, 'openBoardById', id);
    }
    else if (type === 'person') {
        activeTool.value = 'people';
        callWhenReady(() => peopleAppRef.value, 'openPersonById', id);
    }
    else if (type === 'finance_month') {
        activeTool.value = 'finance';
        callWhenReady(() => financeAppRef.value, 'openMonthById', id);
    }
    else if (type === 'feed_source') {
        activeTool.value = 'feeds';
        callWhenReady(() => feedsAppRef.value, 'openFeedById', id);
    }
    else if (type === 'project') {
        activeTool.value = 'task';
        callWhenReady(() => taskAppRef.value, 'openProjectById', id);
    }
    else if (type === 'pdf' || type === 'pdf_highlight' || type === 'file') {
        activeTool.value = 'file';
        // The query rides along so a hit inside a document opens on its page.
        callWhenReady(() => filesAppRef.value, 'openFileById', id, false, query);
    }
};

import { logger } from './utils/logger';
import type { ScanReport } from './types/ipc';

// ─── Notifications & Initial Scan ─────────────────────────
const unreadNotificationCount = ref(0);
const feedsUnreadCount = ref(0);

/**
 * Rescan the vault, and say something when part of it did not make it in.
 *
 * A scan never fails as a whole over one bad file — it steps over it and keeps
 * going, which is what you want, but it used to mean a note could quietly stop
 * being findable with nothing anywhere to say why. The count comes back from
 * the scan now; the Rust log names the individual files.
 */
/**
 * Re-read the vault when the app comes back to the foreground.
 *
 * Desktop has a filesystem watcher; `notify` has no Android backend, so the
 * mobile stub in watcher.rs does nothing and its comment says the frontend
 * re-scans on resume instead. Nothing did. Anything that changed the vault
 * while the app was backgrounded — a sync that ran, a file pulled in, a restore
 * — stayed invisible to search and backlinks until something else happened to
 * trigger a scan.
 *
 * Only on a mobile OS: on the desktop the watcher already covers this, and
 * re-scanning every time the window regains focus would be a large amount of
 * work for nothing.
 */
const rescanOnResume = () => {
    if (document.visibilityState !== 'visible') return;
    if (!isMobileOS.value || !vaultPath.value) return;
    scanVaultNodes().catch(logger.error);
};

const scanVaultNodes = async (): Promise<void> => {
    if (!vaultPath.value) return;
    const report = await invoke<ScanReport>('scan_all_nodes', { vaultPath: vaultPath.value });
    if (report && report.failed > 0) {
        logger.warn(
            `Vault scan: ${report.failed} file(s) could not be fully indexed and will not appear in search until the next scan.`
        );
    }
};

const checkUnreadNotifications = async () => {
    if (!vaultPath.value) return;
    try {
        const msgs = await invoke<any[]>('get_chat_history', { vaultPath: vaultPath.value });
        unreadNotificationCount.value = msgs.filter(m => m.read_receipt === false).length;
    } catch(e) {
        logger.error('Failed to check unread messages', e);
    }
};

/**
 * The 60-second feeds poll, kept where `onUnmounted` can reach it.
 *
 * It used to be a `const` inside `onMounted`, with a note saying the component
 * lifecycle cleaned it up. Nothing cleans up a `setInterval` but a matching
 * `clearInterval` — Vue does not track timers — so the poll outlived the
 * component. On the root component that is invisible in a packaged app, which
 * is why it survived; under dev HMR every reload left another copy running,
 * each one still invoking `feed_get_total_unread` once a minute.
 */
let feedsUnreadInterval: ReturnType<typeof setInterval> | undefined;

const updateFeedsUnreadCount = async () => {
    if (!vaultPath.value) return;
    try {
        feedsUnreadCount.value = await invoke<number>('feed_get_total_unread', { vaultPath: vaultPath.value });
    } catch(e) {
        logger.error('Failed to check feeds unread count', e);
    }
};

// ─── Keyboard shortcuts for navigation ───────────────────
const handleKeyboardNav = (e: KeyboardEvent) => {
    const isMeta = e.metaKey || e.ctrlKey;
    if (isMeta && e.key === '[') {
        e.preventDefault();
        handleGoBack();
    } else if (isMeta && e.key === ']') {
        e.preventDefault();
        handleGoForward();
    }
};



// ─── Lifecycle ────────────────────────────────────────────
onMounted(async () => {
  logger.info("Synabit Frontend App Mounting...");

  // A capture can arrive while the app is already open — a share sheet hands
  // one over to a running process. Without this it would sit in the queue
  // until the next launch, which is the one thing the fast path must not do.
  stopCaptureListener = await listen('capture-queued', () => {
    if (vaultPath.value) void drainCaptures();
  });

  // "Let me write something" — from the Android launcher shortcut, and from
  // the desktop global hotkey. Three entry points, one destination.
  const openCompose = () => {
    activeTool.value = 'quickcap';
    callWhenReady(() => quickCapAppRef.value, 'focusCompose');
  };

  // The hotkey has already raised and focused the window by the time this
  // arrives; all that is left is to land in the right place.
  stopComposeListener = await listen('quickcap:compose', openCompose);

  // Both deep-link paths are needed: `getCurrent` for a cold start, where the
  // URL arrived before anything was listening, and `onOpenUrl` for a shortcut
  // used while the app is already running.
  await onOpenUrl((urls) => {
    if (urls.some(isComposeUrl)) openCompose();
  });

  getCurrent()
    .then((urls) => {
      if (urls?.some(isComposeUrl)) openCompose();
    })
    .catch((e) => logger.error('Could not read the launch deep link', e));
  // Before anything reads `isMobileOS`. `usePlatform` starts this during setup
  // but does not wait for it, so until it resolves the app believes it is on a
  // desktop: the vault location below would be skipped on a phone, and a tablet
  // would paint the desktop layout and then jump to the mobile one. Awaiting the
  // same promise costs one tick and removes both.
  await initOS();
  await appStore.initialize();
  await initSettings();
  await initEventBus();
  await licenseStore.checkState();
  if (licenseStore.licenseStatus.type === 'NoLicense') {
      showLicenseModal.value = true;
  }
  applyTheme();
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  window.addEventListener('keydown', handleKeyboardNav);
  document.addEventListener('visibilitychange', rescanOnResume);

  const params = new URLSearchParams(window.location.search);
  const floatingId = params.get('floatingNote');
  if (floatingId) {
      isFloatingView.value = true;
      floatingNoteId.value = floatingId;
      activeTool.value = 'note';
  } else {
      activeTool.value = defaultApp.value;
  }

  // Runs whether or not a vault is already configured, because an install made
  // by an earlier version has one in app-private storage — invisible to the
  // user and unreachable over USB. The backend moves it and hands back where
  // it ended up; calling this again once it has moved does nothing.
  if (isMobileOS.value) {
      try {
          const resolved = await invoke<string>('resolve_mobile_vault_path');
          if (resolved !== vaultPath.value) {
              await appStore.setVaultPath(resolved, 'local');
          }
      } catch (e) {
          logger.error('Could not resolve the vault location on this device', e);
      }
  }

  if (vaultPath.value) {
     // Only once there is a vault: task reminders are the reason to want
     // notifications, and they cannot happen before one exists. Not awaited —
     // the permission dialog must not hold up the rest of startup.
     ensureNotificationPermission();

     invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(logger.error);
     
     // Scan all nodes on startup so Nexus sees fresh Indexed DB data
     scanVaultNodes().then(async () => {
         await checkUnreadNotifications();
         await updateFeedsUnreadCount();
     }).catch(logger.error);
     
     // Trigger GC for FTS5 on startup
     invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
     invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);

     // Empty what has been in `.trash/` longer than the delete dialogs promise.
     // This used to run when QuickCap mounted, back when captures were the only
     // thing that went in there. Notes go there too now, and the dialog that
     // says "removed for good after 30 days" is a promise the app has to keep
     // whether or not the user ever opens that tab.
     invoke('purge_trash', { vaultPath: vaultPath.value, maxAgeDays: 30 })
         .catch((e) => logger.error('Failed to purge trash', e));
     
     // Feeds unread count polling (every 60s)
     feedsUnreadInterval = setInterval(() => updateFeedsUnreadCount(), 60 * 1000);
     
     if (noteAppRef.value) noteAppRef.value.scanVault();
  }

  gdrive.checkGDriveAuth();
  
  if (activeSyncProvider.value === 'server' && appStore.syncServerAddr) {
      invoke('sync_connect', { serverAddr: appStore.syncServerAddr, serverIdHex: appStore.syncServerIdHex }).catch(logger.error);
  }

  if (vaultType.value === 'gdrive') {
      showGDriveMigrationModal.value = true;
  }

  // Anything that creates or retires a cap moves this number.
  bus.on('node:created', () => void refreshQuickCapCount());
  bus.on('node:deleted', () => void refreshQuickCapCount());
  bus.on('vault:sync-completed', () => void refreshQuickCapCount());

  bus.on('vault:file-created-deleted', async (payload: any) => {
      void refreshQuickCapCount();
      if (noteAppRef.value) noteAppRef.value.scanVault();
      const paths = (payload as string[] | undefined) || [];
      if (paths && paths.length > 0) {
          await invoke('scan_specific_nodes', { vaultPath: vaultPath.value, paths }).catch(logger.error);
          
          const hasFiles = paths.some(p => p.startsWith('assets/') || p.includes('Files/'));
          const hasWhiteboards = paths.some(p => p.startsWith('Whiteboards/'));
          if (hasFiles) await invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
          if (hasWhiteboards) await invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
      } else {
          await scanVaultNodes().catch(logger.error);
          await invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
          await invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
      }
      
      setTimeout(() => checkUnreadNotifications(), 500);
      
      if (appStore.syncAutoEnabled && !syncState.isSyncing.value) {
          const now = Date.now();
          if (now - lastAutoSyncTriggerTime > 5000) {
              lastAutoSyncTriggerTime = now;
              syncState.sync('watcher_create_delete');
          }
      }
  });

  bus.on('vault:file-modified', async (payload: any) => {
      if (noteAppRef.value) noteAppRef.value.scanVault();
      const paths = (payload as string[] | undefined) || [];
      if (paths && paths.length > 0) {
          await invoke('scan_specific_nodes', { vaultPath: vaultPath.value, paths }).catch(logger.error);
          
          const hasFiles = paths.some(p => p.startsWith('assets/') || p.includes('Files/'));
          const hasWhiteboards = paths.some(p => p.startsWith('Whiteboards/'));
          if (hasFiles) await invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
          if (hasWhiteboards) await invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
      } else {
          await scanVaultNodes().catch(logger.error);
          await invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
          await invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
      }
      
      setTimeout(() => checkUnreadNotifications(), 500);

      if (appStore.syncAutoEnabled && !syncState.isSyncing.value) {
          const now = Date.now();
          if (now - lastAutoSyncTriggerTime > 5000) {
              lastAutoSyncTriggerTime = now;
              syncState.sync('watcher_modified');
          }
      }
  });

  bus.on('chat:new-message', () => {
      checkUnreadNotifications();
      if (messagesAppRef.value) {
          messagesAppRef.value.fetchNotifications();
      }
  });

  // ─── Feeds Unread Badge via Event Bus ──────────────────
  bus.on('feed:refreshed', () => updateFeedsUnreadCount());
  bus.on('node:updated', ({ nodeType }: any) => {
      if (nodeType === 'feed_article') updateFeedsUnreadCount();
  });

  // ─── Cross-App Navigation via Event Bus ──────────────────
  bus.on('navigate:to-item', ({ app, itemId }) => {
      activeTool.value = app;
      navigateToItem(app, itemId);
  });

  getCurrentWindow().onCloseRequested(async (event) => {
      // Closing puts Synabit in the background rather than ending it: the
      // global hotkey only exists while the process does, and the tray's Quit
      // is the way out.
      //
      // This has to happen in JavaScript. Registering this listener is what
      // makes Tauri hand the close decision to the front end, and from that
      // moment the Rust-side `CloseRequested` handler stops being called at
      // all — so preventing the close there quietly did nothing.
      event.preventDefault();

      // NoteApp handles its own save-on-close internally
      // But we trigger a final save here for safety
      if (noteAppRef.value?.currentNoteId) {
          const nApp = noteAppRef.value;
          const noteId = nApp.currentNoteId;
          if (noteId && nApp.tabContents[noteId]) {
              const note = nApp.notes.find((n: any) => n.id === noteId);
              if (note) {
                  try {
                      await ns.writeNode({
                          relPath: note.id,
                          nodeType: 'note',
                          title: note.title,
                          properties: {
                              pinned: note.pinned,
                              tags: note.tags
                          },
                          content: nApp.tabContents[noteId],
                          silent: true,
                      });
                      emit('note-updated', { id: note.id, content: nApp.tabContents[noteId] });
                  } catch(e) { logger.error('Save before close failed', e); }
              }
          }
      }

      // After the save, not before: hiding first would let the window go while
      // a note was still being written to disk.
      invoke('hide_to_background').catch(logger.error);
  });

  logger.info("Synabit Frontend App Mount Complete.");
  
  // Show window smoothly after everything is initialized
  setTimeout(() => {
      getCurrentWindow().show().catch(logger.error);
  }, 100);
});

onUnmounted(() => {
  stopCaptureListener?.();
  stopComposeListener?.();
  window.matchMedia('(prefers-color-scheme: dark)').removeEventListener('change', applyTheme);
  window.removeEventListener('keydown', handleKeyboardNav);
  document.removeEventListener('visibilitychange', rescanOnResume);
  destroyEventBus();
  clearInterval(feedsUnreadInterval);
});
</script>

<template>
  <div class="flex h-screen w-full bg-base text-text dark:bg-base-dark dark:text-text-dark font-sans overflow-hidden select-none">

    <!-- ═══ Auto-Update Banner ═══ -->
    <Transition name="slide-down">
      <div v-if="updateAvailable && !updateDownloading"
           class="fixed top-0 left-0 right-0 z-[9999] bg-indigo-600 text-white px-4 py-2.5 flex items-center justify-between shadow-lg">
        <div class="flex items-center gap-2.5 min-w-0">
          <svg class="w-4 h-4 flex-shrink-0 animate-bounce" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
          </svg>
          <div class="min-w-0">
            <span class="text-sm font-medium truncate block">{{ $t('update.available', { version: updateVersion }) }}</span>
            <span v-if="updateNotes" class="text-xs text-indigo-200 truncate block mt-0.5">{{ updateNotes.split('\n')[0] }}</span>
          </div>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <button @click="downloadAndInstall"
                  class="bg-white text-indigo-600 px-3 py-1 rounded-md text-xs font-semibold hover:bg-indigo-50 transition cursor-pointer">
            {{ $t('update.installNow') }}
          </button>
          <button @click="dismissUpdate"
                  class="text-indigo-200 hover:text-white px-2 py-1 text-xs transition cursor-pointer">
            {{ $t('update.later') }}
          </button>
        </div>
      </div>
    </Transition>

    <!-- ═══ Update Download Progress ═══ -->
    <div v-if="updateDownloading"
         class="fixed top-0 left-0 right-0 z-[9999] bg-indigo-600 text-white px-4 py-2.5 shadow-lg">
      <div class="flex items-center justify-between mb-1.5">
        <span class="text-xs font-medium">{{ $t('update.downloading') }}</span>
        <span class="text-xs tabular-nums">{{ updateProgress }}%</span>
      </div>
      <div class="w-full bg-indigo-400/50 rounded-full h-1.5">
        <div class="bg-white h-1.5 rounded-full transition-all duration-300 ease-out"
             :style="{ width: updateProgress + '%' }"/>
      </div>
    </div>

    <!-- Application State 0: Initializing -->
    <div v-if="!appStore.isReady" class="flex-1 flex flex-col items-center justify-center p-8 bg-base dark:bg-base-dark" data-tauri-drag-region>
    </div>

    <!-- Application State 1: No Vault Selected -->
    <div v-else-if="!vaultPath" class="flex-1 flex flex-col items-center justify-center p-8 bg-base dark:bg-base-dark" data-tauri-drag-region>
        <div class="max-w-lg w-full text-center space-y-8">
            <div class="w-20 h-20 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mx-auto shadow-inner">
               <FileText class="w-10 h-10 text-gray-400" />
            </div>
            <div>
               <h1 class="text-2xl font-bold mb-2">Welcome to Synabit</h1>
               <p class="text-text-secondary dark:text-text-secondary-dark text-sm">Choose how you want to store your vault.</p>
            </div>
            
            <div class="flex gap-4 justify-center" @mousedown.stop>
              <button @click="selectVault" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-border dark:border-[#333] hover:border-black dark:hover:border-white bg-surface dark:bg-surface-dark transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer">
                <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center group-hover:bg-gray-200 dark:group-hover:bg-gray-700 transition-colors">
                  <FolderOpen class="w-6 h-6 text-gray-600 dark:text-gray-300" />
                </div>
                <div>
                  <p class="font-semibold text-sm">Local Folder</p>
                  <p class="text-[11px] text-gray-400 mt-1">Store on this computer</p>
                </div>
              </button>
              
              <button @click="gdrive.connectGDrive()" :disabled="gdrive.gdriveAuthLoading.value" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-border dark:border-[#333] hover:border-blue-500 dark:hover:border-blue-400 bg-surface dark:bg-surface-dark transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer disabled:opacity-60 disabled:pointer-events-none">
                <div class="w-12 h-12 rounded-xl bg-blue-50 dark:bg-blue-900/30 flex items-center justify-center group-hover:bg-blue-100 dark:group-hover:bg-blue-900/50 transition-colors">
                  <Cloud v-if="!gdrive.gdriveAuthLoading.value" class="w-6 h-6 text-blue-500" />
                  <RefreshCw v-else class="w-6 h-6 text-blue-500 animate-spin" />
                </div>
                <div>
                  <p class="font-semibold text-sm">Google Drive</p>
                  <p class="text-[11px] text-gray-400 mt-1">Sync across devices</p>
                </div>
              </button>
            </div>
            
            <p v-if="gdrive.gdriveSyncError.value" class="text-red-500 text-xs px-4">{{ gdrive.gdriveSyncError.value }}</p>
        </div>
    </div>

    <!-- Application State 2: Vault Selected -->
    <template v-else>

      <component :is="useMobileLayout ? MobileLayout : DesktopLayout" :activeTool="activeTool" @update:activeTool="activeTool = $event">

        <template #banner>
          <VaultBackupNotice v-if="vaultPath" :vaultPath="vaultPath" />
        </template>
        
        <!--
          SIDEBAR / BOTTOMBAR

          `z-[55]` on the desktop sidebar is load-bearing, not decoration. The
          nav is a flex item with a z-index, so it is a stacking context and
          nothing inside it — the hover tooltips, the More Apps menu — can
          paint above something outside it that sits higher. The content area
          is `relative` with no z-index, so a mini-app's own panels land in the
          root stacking context: People and Finance both hold their list at
          `z-[49]`, which used to swallow the menu whole.

          55 is chosen to sit above every in-content layer (the highest is 50)
          and below every modal (the lowest is 60), so a dialog still covers
          the sidebar as it should.
        -->
        <template v-if="!isFloatingView" #[useMobileLayout?`bottombar`:`sidebar`]>
          <nav :class="useMobileLayout ? 'w-full flex justify-around items-center h-full' : 'w-16 flex-shrink-0 bg-sidebar dark:bg-sidebar-dark border-r border-border dark:border-border-dark flex flex-col items-center py-4 z-[55] h-full'" data-tauri-drag-region>
              <div :class="useMobileLayout ? 'flex justify-around items-center w-full' : 'flex-1 flex flex-col items-center gap-3 mt-4 w-full'" @mousedown.stop>
                <button v-if="isAppVisible('nexus')" @click="activeTool = 'nexus'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'nexus' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Globe class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Nexus</span>
                </button>

                <button v-if="isAppVisible('messages')" @click="activeTool = 'messages'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'messages' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <MessageCircle class="w-5 h-5" />
                   <div v-if="unreadNotificationCount > 0" class="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 bg-red-500 text-white text-[10px] font-bold rounded-full flex items-center justify-center ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a] shadow-sm">{{ unreadNotificationCount > 99 ? '99+' : unreadNotificationCount }}</div>
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Messages</span>
                </button>

                <button v-if="isAppVisible('quickcap')" @click="activeTool = 'quickcap'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'quickcap' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <!--
                     Caps waiting to be turned into something. Grey rather than
                     red: an inbox with things in it is the normal state, not an
                     alarm, and a colour that shouts gets ignored within a week.
                   -->
                   <span v-if="quickCapCount > 0" class="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 bg-gray-400 dark:bg-gray-600 text-white text-[10px] font-bold rounded-full flex items-center justify-center ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a] shadow-sm">{{ quickCapCount > 99 ? '99+' : quickCapCount }}</span>
                   <Zap class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">QuickCap</span>
                </button>
                <button v-if="isAppVisible('note')" @click="activeTool = 'note'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'note' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <FileText class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Notes</span>
                </button>
                <button v-if="isAppVisible('task')" @click="activeTool = 'task'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'task' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <CheckSquare class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Tasks</span>
                </button>
                <button v-if="isAppVisible('calendar')" @click="activeTool = 'calendar'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'calendar' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Calendar class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Calendar</span>
                </button>
                <button v-if="isAppVisible('file')" @click="activeTool = 'file'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'file' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <FolderOpen class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Files</span>
                </button>
                <button v-if="isAppVisible('whiteboard')" @click="activeTool = 'whiteboard'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'whiteboard' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Palette class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Whiteboard</span>
                </button>
                <button v-if="isAppVisible('people')" @click="activeTool = 'people'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'people' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Users class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">People</span>
                </button>

                <button v-if="isAppVisible('finance')" @click="activeTool = 'finance'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'finance' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Wallet class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Finance</span>
                </button>

                <button v-if="isAppVisible('feeds')" @click="activeTool = 'feeds'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'feeds' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Rss class="w-5 h-5" />
                   <span v-if="feedsUnreadCount > 0" class="absolute -top-1 -right-1 min-w-[18px] h-[18px] bg-orange-500 text-white text-[10px] font-bold rounded-full flex items-center justify-center px-1 shadow-sm ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a]">{{ feedsUnreadCount > 99 ? '99+' : feedsUnreadCount }}</span>
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Feeds</span>
                </button>

                
                <div v-if="moreMenuApps.length > 0" class="relative flex justify-center">
                  <button @click="showHiddenAppsMenu = !showHiddenAppsMenu" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showHiddenAppsMenu ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                    <MoreHorizontal class="w-5 h-5" />
                    <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">More Apps</span>
                  </button>
                  
                  <!-- Overlay for clicking outside -->
                  <div v-if="showHiddenAppsMenu" class="fixed inset-0 z-40" @click="showHiddenAppsMenu = false"></div>
                  
                  <div v-if="showHiddenAppsMenu" :class="useMobileLayout ? 'absolute bottom-full mb-4 right-0 w-48' : 'absolute left-full top-0 ml-2 w-48'" class="py-2 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-xl border border-gray-200 dark:border-[#2c2c2c] z-50 max-h-[60vh] overflow-y-auto">
                    <button v-for="app in moreMenuApps" :key="app.id" @click="openHiddenApp(app.id)" class="w-full flex items-center gap-3 px-4 py-3 text-sm text-[#1c1c1e] dark:text-[#f4f4f5] hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors">
                      <component :is="app.icon" class="w-5 h-5 text-gray-500" />
                      <span class="font-medium">{{ app.name }}</span>
                    </button>
                  </div>
                </div>
                
                <button v-if="useMobileLayout" @click="openSettings" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showSettingsModal ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']" aria-label="Open Settings">
                   <Settings class="w-5 h-5" />
                </button>
             </div>
             
             <!-- Settings & Sync bottom icons for desktop -->
             <div v-if="!useMobileLayout" class="flex-shrink-0 w-full flex flex-col items-center gap-3 mb-2" @mousedown.stop>
                <button v-if="activeSyncProvider === 'gdrive'" @click="syncState.sync()" :disabled="syncState.isSyncing.value" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', syncState.syncError.value ? 'text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30' : gdrive.gdriveConnected.value ? 'text-blue-500 hover:bg-blue-100 dark:hover:bg-blue-900/30' : 'text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-800']" :title="syncState.isSyncing.value ? 'Syncing...' : appStore.syncLastSuccessful ? `Last sync: ${appStore.syncLastSuccessful}` : 'Sync with Google Drive'">
                   <RefreshCw v-if="syncState.isSyncing.value" class="w-5 h-5 animate-spin" />
                   <CloudOff v-else-if="syncState.syncError.value" class="w-5 h-5" />
                   <Cloud v-else class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ syncState.isSyncing.value ? 'Syncing…' : syncState.syncError.value ? 'Sync Error' : appStore.syncLastSuccessful ? `Synced ${appStore.syncLastSuccessful}` : 'Sync Now' }}</span>
                </button>
                <button v-if="activeSyncProvider === 'server'" @click="syncConflictCount > 0 ? (showSyncConflicts = true) : syncState.sync()" :disabled="syncState.isSyncing.value" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', syncState.syncError.value ? 'text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30' : syncConflictCount > 0 ? 'text-amber-500 hover:bg-amber-100 dark:hover:bg-amber-900/30' : 'text-emerald-500 hover:bg-emerald-100 dark:hover:bg-emerald-900/30']" :title="syncState.isSyncing.value ? 'Syncing...' : appStore.syncLastSuccessful ? `P2P synced ${appStore.syncLastSuccessful}` : 'Sync Server'">
                   <RefreshCw v-if="syncState.isSyncing.value" class="w-5 h-5 animate-spin" />
                   <Server v-else class="w-5 h-5" />
                   <span v-if="syncConflictCount > 0" class="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 rounded-full bg-amber-500 text-white text-[10px] font-bold leading-4 text-center">{{ syncConflictCount }}</span>
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ syncState.isSyncing.value ? 'Syncing…' : syncState.syncError.value ? 'Sync Error' : syncConflictCount > 0 ? `${syncConflictCount} file(s) kept aside — click to see` : appStore.syncLastSuccessful ? `Synced ${appStore.syncLastSuccessful}` : 'Sync Now' }}</span>
                </button>

                 <button @click="openSettings" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showSettingsModal ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Settings class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Settings</span>
                </button>
             </div>
          </nav>
        </template>

        <!-- MINI APP CONTENT AREA (Vue Router + KeepAlive) -->
        <div class="flex-1 h-full overflow-hidden relative">
            <router-view v-slot="{ Component, route }">
                <!-- Tier 2: Show PIN pad directly for protected mini-apps -->
                <LockScreen
                    v-if="appLockStore.isEnabled && appLockStore.isAppProtected(route.name as string) && !appLockStore.isMiniAppAccessible(route.name as string)"
                    :title="`Enter PIN to access ${getAppName(route.name as string)}`"
                    @unlocked="appLockStore.unlockMiniApp(route.name as string)"
                    @cancelled="router.back()"
                />
                <keep-alive v-else>
                    <component 
                        :is="Component" 
                        :key="route.name"
                        :vault-path="vaultPath" 
                        :is-floating-view="isFloatingView" 
                        :floating-note-id="floatingNoteId" 
                        @open-node="handleEditFromNexus"
                        @edit-item="handleEditFromNexus"
                        :ref="(el: any) => setAppRef(el, route.name as string)"
                    />
                </keep-alive>
            </router-view>
            
            <!-- Conflict Toast -->
            <SyncConflictToast />

            <!-- GDrive Migration Modal -->
            <GDriveMigrationModal 
              :show="showGDriveMigrationModal" 
              @migrated="handleGDriveMigrated" 
            />
        </div>


        <!-- SETTINGS MODAL -->
        <template #modal>
          <SettingsModal
            :vault-path="vaultPath"
            :vault-type="vaultType"
            :active-sync-provider="activeSyncProvider"
            :gdrive-connected="gdrive.gdriveConnected.value"
            :gdrive-auth-loading="gdrive.gdriveAuthLoading.value"
            :syncing="syncState.isSyncing.value"
            :sync-error="syncState.syncError.value"
            :last-sync-time="appStore.syncLastSuccessful"
            :auto-sync-enabled="appStore.syncAutoEnabled"
            :auto-sync-interval="appStore.syncAutoInterval"
            :sync-server-addr="appStore.syncServerAddr"
            :sync-server-id-hex="appStore.syncServerIdHex"
            @clear-vault="clearVault"
            @sync-now="syncState.sync()"
            @connect-gdrive="gdrive.connectGDrive()"
            @disconnect-gdrive="gdrive.disconnectGDrive()"
            @connect-server="(addr: string, id: string) => { appStore.syncServerAddr = addr; appStore.syncServerIdHex = id; invoke('sync_connect', { serverAddr: addr, serverIdHex: id }).then(() => appStore.activeSyncProvider = 'server').catch(logger.error); }"
            @disconnect-server="() => { invoke('sync_disconnect').then(() => appStore.activeSyncProvider = 'none').catch(logger.error); }"
            @update:auto-sync-enabled="appStore.syncAutoEnabled = $event"
            @update:auto-sync-interval="appStore.syncAutoInterval = $event"
            @show-setup-pin="(mode: 'setup' | 'change') => { setupPinMode = mode; showSetupPinModal = true; }"
          />
        </template>
      </component>

      <!-- Sync Conflict Toast (floating bottom-right) -->
      <SyncConflictToast />
    </template>

    <!-- E2EE Onboarding Modal -->
    <E2eeOnboarding v-if="showE2eeOnboarding" @done="showE2eeOnboarding = false" />
    
    <RecoveryModal
      :is-open="showRecoveryModal"
      @update:is-open="showRecoveryModal = $event"
    />

    <LicenseModal :isOpen="showLicenseModal" @close="showLicenseModal = false" />

    <!-- Tier 1: App Lock Screen -->
    <LockScreen
      v-if="appLockStore.isEnabled && appLockStore.isAppLocked"
      title="Enter PIN to unlock Synabit"
      :cancellable="false"
      @unlocked="appLockStore.unlockApp()"
    />

    <!-- Setup PIN Modal -->
    <SetupPinModal
      v-if="showSetupPinModal"
      :mode="setupPinMode"
      @done="showSetupPinModal = false; appLockStore.refreshConfig();"
      @cancel="showSetupPinModal = false"
    />

  </div>

    <!-- Files kept aside during sync. Deliberately not styled as an error: the
         sync worked, and the only thing the user needs is where their file went. -->
    <div v-if="showSyncConflicts" class="fixed inset-0 z-[100] flex items-center justify-center bg-black/40 p-4" @click.self="showSyncConflicts = false">
      <div class="w-full max-w-lg rounded-2xl bg-white dark:bg-gray-900 shadow-xl border border-amber-200 dark:border-amber-900/50 overflow-hidden">
        <div class="px-5 py-4 border-b border-gray-200 dark:border-gray-800">
          <h2 class="text-base font-semibold text-gray-900 dark:text-gray-100">{{ syncConflictCount }} file(s) kept</h2>
          <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
            Another device saved its own file to the same place. Yours was not lost — it was renamed and is still in your vault.
          </p>
        </div>
        <ul class="max-h-72 overflow-y-auto px-5 py-3 space-y-3">
          <li v-for="c in syncState.syncConflicts.value" :key="c.kept_as" class="text-sm">
            <div class="text-gray-500 dark:text-gray-400 line-through break-all">{{ c.rel_path }}</div>
            <div class="font-medium text-gray-900 dark:text-gray-100 break-all">{{ c.kept_as }}</div>
          </li>
        </ul>
        <div class="px-5 py-3 bg-gray-50 dark:bg-gray-800/50 flex justify-end">
          <button @click="syncState.dismissConflicts(); showSyncConflicts = false" class="px-4 py-2 rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 text-sm font-medium hover:opacity-90 transition-opacity cursor-pointer">
            Got it
          </button>
        </div>
      </div>
    </div>
</template>

<style scoped>
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}

/* Auto-Update banner slide transition */
.slide-down-enter-active,
.slide-down-leave-active {
  transition: transform 0.3s ease, opacity 0.3s ease;
}
.slide-down-enter-from,
.slide-down-leave-to {
  transform: translateY(-100%);
  opacity: 0;
}
</style>
