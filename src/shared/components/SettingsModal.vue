<script setup lang="ts">
import { Settings, FileText, CheckSquare, Globe, X, FolderOpen, Cloud, RefreshCw, Lock, Shield, Trash2, Server, Unplug, Monitor, HardDrive, Check, CalendarClock, Sparkles } from 'lucide-vue-next';
import TrashPanel from './TrashPanel.vue';
import AppDialog from './AppDialog.vue';
import { useSettings } from '../../composables/useSettings';
import { ref, computed, onMounted, watch, defineAsyncComponent } from 'vue';

const LockScreenVerify = defineAsyncComponent(() => import('./LockScreen.vue'));
const DeviceManager = defineAsyncComponent(() => import('./DeviceManager.vue'));
// How the vault is read into moments, and how to start the timeline again.
// Loaded when its tab is opened: it is a rare visit, and it brings the media
// surrogates with it.
const TimelineSettings = defineAsyncComponent(() => import('./TimelineSettings.vue'));

const SyncMobileSettings = defineAsyncComponent(() => import('./SyncMobileSettings.vue'));
// Syn's own settings — provider, model, memory, Telegram, connectors — as a tab here
// rather than a drawer inside Messages, so they are reachable from anywhere.
// Loaded when the tab is opened.
const SynSettings = defineAsyncComponent(() => import('../../mini-apps/messages/components/SynSettings.vue'));
// Family-safe answers alone, for simple mode, where the Syn tab is hidden.
const SimpleModeSynSettings = defineAsyncComponent(() => import('./SimpleModeSynSettings.vue'));
const ConfirmModal = defineAsyncComponent(() => import('./ConfirmModal.vue'));
import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import { type } from '@tauri-apps/plugin-os';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { logger } from '../../utils/logger';
import { useAppLockStore, pinErrorKey, timeoutLoosens } from '../../stores/useAppLockStore';
import { showAppNotice } from '../../composables/useAppNotice';
import { useAppUpdate } from '../../composables/useAppUpdate';
import { appInPlatformScope } from '../platformScope';
import { BUILT_IN_APPS, SELF_LOCKING_APPS, appName } from '../appRegistry';
import { appOffered } from '../appAccess';
import { SIMPLE_MODE_HOME } from '../simpleMode';
import { enable as enableAutostart, disable as disableAutostart, isEnabled as isAutostartEnabled } from '@tauri-apps/plugin-autostart';
import { useVaultArchive, formatBytes } from '../../composables/useVaultArchive';
import { UI_SCALES } from '../../utils/uiScale';
import { rovingIndex } from '../../utils/roving';
import { when } from '../../utils/when';


const {
  showSettingsModal, settingsTab, showE2eeOnboarding,
  themeMode, appLanguage, uiScale, simpleMode, defaultApp,
  taskArchiveDays,
  taskDeleteConfirm,
  enableDailyNotes, noteToolbarVisible, dailyNoteFormat, dailyNoteTag, isValidDailyFormat,
  nestedNumberListStyle, hiddenSidebarApps, codeBlockTabSize,
  codeBlockBgColorLight, codeBlockTextColorLight, codeBlockBgColorDark, codeBlockTextColorDark
} = useSettings();

const {
  updateAvailable, updateVersion, updateNotes,
  isChecking: updateChecking,
  isDownloading: updateDownloading,
  downloadProgress: updateProgress,
  lastCheckResult,
  checkForUpdates, downloadAndInstall,
} = useAppUpdate();

/**
 * The mini-apps these settings can talk about.
 *
 * Two corrections were folded in here. The list said `chat`, which is not a
 * mini-app id — the router only redirects `/chat` to `/messages` — so that
 * toggle wrote a value into `hiddenSidebarApps` that matched nothing and
 * quietly did nothing. And `feeds` was missing, so it could be neither hidden
 * nor protected by the app lock. Both now match the ids the app actually uses.
 *
 * Filtered by platform: offering to hide, or to lock, an app that this build
 * does not ship is a setting with nothing behind it.
 */
const availableApps = computed(() => BUILT_IN_APPS.filter(a => appInPlatformScope(a.id)));
const lockableApps = computed(() => availableApps.value.filter(a => !SELF_LOCKING_APPS.includes(a.id)));

/**
 * What the sidebar list offers to show or hide: in simple mode, only the apps
 * simple mode keeps. The rest are hidden by the mode already, and a switch
 * that reads "on" for an app that is nowhere to be seen would be a lie. Their
 * `hiddenSidebarApps` entries are left as they were, so turning the mode off
 * gives back the sidebar the user had arranged.
 */
const sidebarChoices = computed(() => availableApps.value.filter(a => appOffered(a.id, simpleMode.value)));

/**
 * The apps "Startup App" can be set to. Fixed rather than every app, as it
 * always was: these are the ones that make sense as a first screen.
 */
const STARTUP_CHOICES = ['nexus', 'quickcap', 'note', 'task', 'calendar', 'file'] as const;

/**
 * Whether simple mode is overriding the chosen start app, and with what.
 * Shown under the select so the stored choice is not silently ignored — it is
 * kept, and comes back when the mode is turned off.
 */
const startAppOverridden = computed(() => simpleMode.value && !appOffered(defaultApp.value, true));

/**
 * Timeline and Syn are settings for apps simple mode hides. The tabs go with
 * them; if one was open (or asked for by `openSettings('syn')`), General takes
 * its place rather than leaving a page whose button has gone.
 */
const SIMPLE_MODE_HIDDEN_TABS = ['timeline', 'syn'];
watch([simpleMode, settingsTab], ([on, tab]) => {
  if (on && SIMPLE_MODE_HIDDEN_TABS.includes(tab as string)) settingsTab.value = 'general';
}, { immediate: true });

/**
 * How many ways of looking at the vault this person has kept.
 *
 * Counted here and shown here, and it goes nowhere else. The roadmap gates its
 * next stretch of work on whether anybody actually saves a view, and this app
 * sells "zero telemetry" as a promise rather than an omission — so the number
 * is computed from nodes already in the vault and displayed to the one person
 * entitled to it. See `docs/adr-measuring-the-slope-2026-08-29.md`.
 *
 * The line is also the only place in the product that mentions views can be
 * pinned at all, which is the slope's real problem: the rungs exist and
 * nothing says so.
 */
const savedViews = ref(0);
const pinnedViews = ref(0);

const countViews = async () => {
  try {
    const nodes = await invoke<{ properties?: Record<string, unknown> }[]>(
      'get_node_summaries', { nodeType: 'view' },
    );
    savedViews.value = nodes.length;
    pinnedViews.value = nodes.filter(n => n.properties?.home === 'sidebar').length;
  } catch (e) {
    logger.warn('Could not count saved views', e);
    savedViews.value = 0;
    pinnedViews.value = 0;
  }
};

const toggleAppVisibility = (appId: string) => {
  if (defaultApp.value === appId) return;
  if (hiddenSidebarApps.value.includes(appId)) {
    hiddenSidebarApps.value = hiddenSidebarApps.value.filter(id => id !== appId);
  } else {
    hiddenSidebarApps.value.push(appId);
  }
};

const appVersion = ref('');
const isDesktop = ref(true);

onMounted(async () => {
  void readAutostart();
  void countViews();
  try {
    appVersion.value = await getVersion();
    const osType = type();
    isDesktop.value = osType === 'macos' || osType === 'windows' || osType === 'linux';
    
    // Check E2EE status
    await checkE2eeStatus();
  } catch(e) {
    logger.error("Failed to get version/os or E2EE status", e);
  }
});

// Re-check E2EE status whenever the settings modal is opened
watch(showSettingsModal, (visible) => {
  if (visible) checkE2eeStatus();
});

const openLogFolder = async () => {
  try {
    await invoke('open_app_log_folder');
  } catch (e) {
    logger.error("Failed to open log folder", e);
  }
};

type TabType = 'general' | 'notes' | 'tasks' | 'timeline' | 'security' | 'devices' | 'about';

const props = defineProps<{
  initialTab?: TabType;
  vaultPath: string;
  vaultType: 'local';
  activeSyncProvider: 'none' | 'local' | 'server';
  syncing: boolean;
  syncError: string;
  lastSyncTime: string;
  autoSyncEnabled: boolean;
  autoSyncInterval: number;
  syncServerAddr: string;
  syncServerIdHex: string;
}>();

const emit = defineEmits<{
  (e: 'clear-vault'): void;
  (e: 'sync-now'): void;
  (e: 'update:auto-sync-enabled', val: boolean): void;
  (e: 'update:auto-sync-interval', val: number): void;
  (e: 'show-setup-pin', mode: 'setup' | 'change'): void;
  (e: 'connect-server', serverAddr: string, serverIdHex: string): void;
  (e: 'disconnect-server'): void;
}>();

/** The vault-wide trash, opened from the General tab. */
const showTrash = ref(false);

const { t, locale } = useI18n();

/**
 * Interface size is a radiogroup, so it takes one tab stop and the arrows move
 * inside it — choosing as they go, as radios do.
 */
const onScaleKey = (e: KeyboardEvent, index: number) => {
  const next = rovingIndex(e.key, index, UI_SCALES.length);
  if (next === null) return;
  e.preventDefault();
  uiScale.value = UI_SCALES[next].value;
  const group = (e.currentTarget as HTMLElement).parentElement;
  (group?.children[next] as HTMLElement | undefined)?.focus();
};

/**
 * Where the legal documents live.
 *
 * The repository here used to be `synabit/synabit`, which is not where this
 * project is published — the updater endpoint in tauri.conf.json points at
 * `kid0604/synabit`, and that is the one that exists. The link therefore led
 * to a 404 for anybody who managed to open it at all.
 */
const LEGAL_BASE = 'https://github.com/kid0604/synabit/blob/main/legal';

async function openLegal(file: string) {
  try {
    await openUrl(`${LEGAL_BASE}/${file}`);
  } catch (e) {
    logger.error('Could not open the legal document', e);
  }
}



// ─── Vault backup ─────────────────────────────────────────
//
// The vault lives in app storage on Android, which the operating system
// deletes when the app is uninstalled. Sync covers anybody who turned it on;
// this is the answer for everybody else, and for a phone that is the only
// device there has ever been.
//
// The dialog returns an ordinary path on desktop and a content:// URI on
// Android. Both are passed through untouched — the Rust side resolves either.
/**
 * Whether Synabit starts with the machine.
 *
 * The operating system is the only source of truth here, deliberately: a
 * stored copy would drift the moment somebody removed the entry from their
 * own login items, and the switch would then be lying about the state of
 * their computer. So this reads the real thing and writes the real thing,
 * and keeps nothing.
 */
const autostartOn = ref(false);
const autostartBusy = ref(false);

const readAutostart = async () => {
  try {
    autostartOn.value = await isAutostartEnabled();
  } catch {
    // No login items on this platform.
  }
};

const toggleAutostart = async () => {
  if (autostartBusy.value) return;
  autostartBusy.value = true;
  try {
    if (autostartOn.value) {
      await disableAutostart();
    } else {
      await enableAutostart();
    }
  } catch (e) {
    logger.error('Could not change the login item', e);
  } finally {
    // Read back rather than assume: the request can be refused, and showing
    // a switch that disagrees with the system is worse than showing none.
    await readAutostart();
    autostartBusy.value = false;
  }
};

const archiveMessage = ref('');
const archiveError = ref('');
const diagnosticsAvailable = ref(false);

// Shared with the reminder banner so an export done from either one is the
// same event, and the reminder does not keep firing after the user has acted.
const { busy: archiveBusy, exportVault, importVault, exportDiagnostics } = useVaultArchive();

onMounted(async () => {
  try {
    const info = await invoke<{ available: boolean }>('diagnostics_info');
    diagnosticsAvailable.value = info.available;
  } catch (e) {
    logger.warn('Could not check for a log file', e);
  }
});

async function runExport() {
  archiveMessage.value = '';
  archiveError.value = '';
  try {
    const summary = await exportVault(props.vaultPath);
    if (!summary) return; // dialog closed; not a failure
    archiveMessage.value = t('settings.general.backup_exported', {
      files: summary.files,
      size: formatBytes(summary.bytes),
    });
  } catch (e) {
    archiveError.value = String(e);
    logger.error('Vault export failed', e);
  }
}

async function runImport() {
  archiveMessage.value = '';
  archiveError.value = '';
  try {
    const summary = await importVault(props.vaultPath);
    if (!summary) return;
    archiveMessage.value = summary.rejected.length
      ? t('settings.general.backup_imported_partial', {
          files: summary.files,
          skipped: summary.rejected.length,
        })
      : t('settings.general.backup_imported', { files: summary.files });
  } catch (e) {
    archiveError.value = String(e);
    logger.error('Vault import failed', e);
  }
}

async function runDiagnosticsExport() {
  archiveMessage.value = '';
  archiveError.value = '';
  try {
    const bytes = await exportDiagnostics();
    if (bytes === null) return;
    archiveMessage.value = t('settings.general.diagnostics_saved', { size: formatBytes(bytes) });
  } catch (e) {
    archiveError.value = String(e);
    logger.error('Diagnostics export failed', e);
  }
}

// Server Sync form state
const p2pFormAddr = ref('');
const p2pFormId = ref('');
const p2pServerMode = ref<'none' | 'official' | 'custom'>('none');
const serverConnecting = ref(false);

// No local tab state here: the open tab is `settingsTab` on the shared store,
// which is what every button in the template below writes to. This ref was
// seeded from `props.initialTab` and then read by nothing.

const activeSettingsProvider = ref<'none' | 'local' | 'server'>(props.activeSyncProvider);

watch(() => props.activeSyncProvider, (val) => {
  activeSettingsProvider.value = val;
});

const showConfirmDisconnectP2P = ref(false);
const showConfirmDisconnectAll = ref(false);

const handleConnectP2P = (addr: string, id: string) => {
  serverConnecting.value = true;
  emit('connect-server', addr, id);
  setTimeout(() => { serverConnecting.value = false; }, 2000);
};

const handleDisconnectAll = () => {
  if (props.activeSyncProvider === 'server') emit('disconnect-server');
  activeSettingsProvider.value = 'none';
};

// Official Synabit Sync Server.
//
// The id is the server's iroh endpoint public key, derived from server.key in
// the server's data volume. It changes whenever that volume is recreated, and
// a stale value here fails to connect while the UI still labels it "Official" —
// so read it back from the server after any redeploy that resets the volume:
//   docker exec synabit-sync-server curl -s localhost:8080/health
const OFFICIAL_SERVER = {
  addr: 'sync.synabit.net:4433',
  id: '16b18a03ce5ce91937d5856b1a233dcdbb6fdfc0c34df2e326e7df5b77ea4d24',
  available: true,
};

// ─── App Lock ─────────────────────────────────────────────────
const appLockStore = useAppLockStore();
const removingLock = ref(false);
const autoLockOptions = computed(() => [
  { value: 60, label: t('settings.security.timeout_minutes', { count: 1 }, 1) },
  { value: 300, label: t('settings.security.timeout_minutes', { count: 5 }, 5) },
  { value: 900, label: t('settings.security.timeout_minutes', { count: 15 }, 15) },
  { value: 1800, label: t('settings.security.timeout_minutes', { count: 30 }, 30) },
  { value: 0, label: t('settings.security.timeout_never') },
]);

// The PIN the prompt checked goes to the backend, which checks it again:
// `remove_app_lock` refuses without it.
const handleRemoveLock = async (pin: string) => {
  removingLock.value = true;
  try {
    await appLockStore.removeLock(pin);
  } catch (e) {
    logger.error('Failed to remove app lock:', e);
  } finally {
    removingLock.value = false;
  }
};

// ─── PIN Verification for destructive actions ─────────────
const showPinVerify = ref(false);
const pinVerifyTitle = ref('');
const pendingAction = ref<((pin: string) => void) | null>(null);

const requirePin = (title: string, action: (pin: string) => void) => {
  pinVerifyTitle.value = title;
  pendingAction.value = action;
  showPinVerify.value = true;
};

const onPinVerified = (pin: string) => {
  showPinVerify.value = false;
  if (pendingAction.value) {
    pendingAction.value(pin);
    pendingAction.value = null;
  }
};

// A change to what the PIN guards. The store changes only once the backend
// has saved it, so a refusal leaves the switch or menu where it was; the
// refusal itself is said aloud, not just logged.
const saveLockChange = async (change: () => Promise<void>) => {
  try {
    await change();
  } catch (e) {
    logger.error('Failed to change app lock settings:', e);
    showAppNotice(t(pinErrorKey(e) ?? 'settings.security.lock_change_failed'), 'error');
  }
};

// Loosening asks for the PIN when one is set, and hands it on:
// `update_app_lock_config` checks it again and refuses without it.
const handleToggleTier1 = () => {
  if (appLockStore.appLockActive) {
    requirePin(t('settings.security.pin_to_disable_lock'), pin => saveLockChange(() => appLockStore.setAppLockActive(false, pin)));
  } else {
    saveLockChange(() => appLockStore.setAppLockActive(true));
  }
};

const handleToggleProtectedApp = (appId: string, appName: string) => {
  if (appLockStore.isAppProtected(appId)) {
    requirePin(t('settings.security.pin_to_unprotect', { app: appName }), pin => saveLockChange(() => appLockStore.toggleProtectedApp(appId, pin)));
  } else {
    saveLockChange(() => appLockStore.toggleProtectedApp(appId));
  }
};

const handleAutoLockTimeout = (event: Event) => {
  const select = event.target as HTMLSelectElement;
  const next = Number(select.value);
  // Show the saved value until the change is saved: the store then moves the
  // menu, and a cancelled prompt or a refusal leaves it where it was.
  select.value = String(appLockStore.autoLockTimeoutSecs);
  if (appLockStore.isEnabled && timeoutLoosens(appLockStore.autoLockTimeoutSecs, next)) {
    requirePin(t('settings.security.pin_to_lengthen_timeout'), pin => saveLockChange(() => appLockStore.setAutoLockTimeout(next, pin)));
  } else {
    saveLockChange(() => appLockStore.setAutoLockTimeout(next));
  }
};

// ─── E2EE Security State ─────────────────────────────────
interface E2eeStatus {
  key_available: boolean;
  needs_setup: boolean;
}

const e2eeStatus = ref<E2eeStatus>({ key_available: false, needs_setup: true });
const e2eeError = ref('');

const checkE2eeStatus = async () => {
  try {
    e2eeStatus.value = await invoke<E2eeStatus>('check_e2ee_status');
    e2eeError.value = '';
  } catch (e) {
    logger.error("Failed to check E2EE status", e);
    // Said on screen as well as in the log. A failed check leaves `e2eeStatus`
    // at its defaults, which paint the panel as "not set up yet" — the one
    // reading most likely to make somebody generate a second key for a vault
    // that already has one.
    e2eeError.value = String(e);
  }
};

const setupE2ee = () => {
  showSettingsModal.value = false;
  showE2eeOnboarding.value = true;
};

// The recovery-phrase restore that used to live here is gone, not lost: it is
// `restoreFromPhrase` in E2eeOnboarding.vue, wired to a button, and `setupE2ee`
// above is what opens that screen. The copy here had lost its form in the
// template and kept only its script half, so nothing could ever call it — along
// with `restorePhrase` and `showRestoreForm`, which by then were read by nothing
// but the dead function itself.
</script>

<template>
  <AppDialog
    :show="showSettingsModal"
    :aria-label="$t('settings.title')"
    size="xl"
    unstyled
    @close="showSettingsModal = false"
  >
        <!-- Modal Container -->
        <div class="relative w-full md:w-[720px] md:max-w-[90vw] mx-auto h-[90vh] md:h-[520px] md:max-h-[85vh] bg-base dark:bg-base-dark rounded-2xl shadow-2xl border border-border-subtle dark:border-[#333] flex flex-col md:flex-row overflow-hidden" @mousedown.stop>
          
          <!-- Top/Left Tab Navigation -->
          <nav class="w-full md:w-[200px] shrink-0 bg-surface-hover dark:bg-[#1a1a1a] border-b md:border-b-0 md:border-r border-border dark:border-border-dark flex flex-col py-2 md:py-5 px-2 md:px-3 z-10">
            <h2 class="hidden md:block text-[13px] font-bold text-text dark:text-text-dark mb-5 px-2">{{ $t('settings.title') }}</h2>
            
            <div class="flex flex-row md:flex-col gap-1 md:gap-0 md:space-y-0.5 overflow-x-auto no-scrollbar">
              <button @click="settingsTab = 'general'" :aria-label="$t('settings.tabs.general')" :title="$t('settings.tabs.general')" :aria-current="settingsTab === 'general' ? 'page' : undefined" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'general' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-text-secondary dark:text-text-secondary-dark hover:bg-white/60 dark:hover:bg-[#252525] hover:text-text dark:hover:text-white']">
                <Settings class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.general') }}</span>
              </button>
              <button @click="settingsTab = 'notes'" :aria-label="$t('settings.tabs.notes')" :title="$t('settings.tabs.notes')" :aria-current="settingsTab === 'notes' ? 'page' : undefined" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'notes' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-text-secondary dark:text-text-secondary-dark hover:bg-white/60 dark:hover:bg-[#252525] hover:text-text dark:hover:text-white']">
                <FileText class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.notes') }}</span>
              </button>
              <button @click="settingsTab = 'tasks'" :aria-label="$t('settings.tabs.tasks')" :title="$t('settings.tabs.tasks')" :aria-current="settingsTab === 'tasks' ? 'page' : undefined" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'tasks' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-text-secondary dark:text-text-secondary-dark hover:bg-white/60 dark:hover:bg-[#252525] hover:text-text dark:hover:text-white']">
                <CheckSquare class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.tasks') }}</span>
              </button>
              <button v-if="!simpleMode" @click="settingsTab = 'timeline'" data-tab-timeline :aria-label="$t('settings.tabs.timeline')" :title="$t('settings.tabs.timeline')" :aria-current="settingsTab === 'timeline' ? 'page' : undefined"
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'timeline' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-text-secondary dark:text-text-secondary-dark hover:bg-white/60 dark:hover:bg-[#252525] hover:text-text dark:hover:text-white']">
                <CalendarClock class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.timeline') }}</span>
              </button>
              <button v-if="!simpleMode" @click="settingsTab = 'syn'" data-tab-syn :aria-label="$t('settings.tabs.syn')" :title="$t('settings.tabs.syn')" :aria-current="settingsTab === 'syn' ? 'page' : undefined"
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'syn' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-text-secondary dark:text-text-secondary-dark hover:bg-white/60 dark:hover:bg-[#252525] hover:text-text dark:hover:text-white']">
                <Sparkles class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.syn') }}</span>
              </button>
              <button @click="settingsTab = 'security'" :aria-label="$t('settings.tabs.security')" :title="$t('settings.tabs.security')" :aria-current="settingsTab === 'security' ? 'page' : undefined" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'security' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-text-secondary dark:text-text-secondary-dark hover:bg-white/60 dark:hover:bg-[#252525] hover:text-text dark:hover:text-white']">
                <Lock class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.security') }}</span>
              </button>
              <button @click="settingsTab = 'devices'" :aria-label="$t('settings.tabs.devices')" :title="$t('settings.tabs.devices')" :aria-current="settingsTab === 'devices' ? 'page' : undefined" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'devices' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-text-secondary dark:text-text-secondary-dark hover:bg-white/60 dark:hover:bg-[#252525] hover:text-text dark:hover:text-white']">
                <Monitor class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.devices', 'Devices') }}</span>
              </button>
              <button @click="settingsTab = 'about'" :aria-label="$t('settings.tabs.about')" :title="$t('settings.tabs.about')" :aria-current="settingsTab === 'about' ? 'page' : undefined" 
                :class="['flex-1 md:w-full text-center md:text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center justify-center md:justify-start gap-1.5 md:gap-2.5 whitespace-nowrap', settingsTab === 'about' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-text-secondary dark:text-text-secondary-dark hover:bg-white/60 dark:hover:bg-[#252525] hover:text-text dark:hover:text-white']">
                <Globe class="w-4 h-4 opacity-70 shrink-0" />
                <span class="hidden sm:inline md:inline">{{ $t('settings.tabs.about') }}</span>
              </button>
            </div>
          </nav>
          
          <!-- Content Area -->
          <div class="flex-1 flex flex-col overflow-hidden min-h-0 relative">
            <!-- Header -->
            <div class="h-12 shrink-0 flex items-center justify-between px-4 md:px-6 border-b border-border dark:border-border-dark sticky top-0 bg-base/90 dark:bg-base-dark/90 backdrop-blur-sm z-10">
              <h3 class="text-[15px] font-semibold text-text dark:text-text-dark capitalize">{{ $t(`settings.tabs.${settingsTab}`) }}</h3>
              <button @click="showSettingsModal = false" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-[#333] text-gray-500 dark:text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 transition-colors" :aria-label="$t('settings.close')" :title="$t('settings.close')">
                <X class="w-4 h-4" />
              </button>
            </div>
            
            <!-- Syn: its own scroll and its own Save bar, pinned at the bottom. -->
            <SynSettings v-if="settingsTab === 'syn'" :vault-path="vaultPath" class="flex-1" />

            <!-- Scrollable Content -->
            <div v-else class="flex-1 overflow-y-auto p-6">
              
              <!-- === GENERAL TAB === -->
              <div v-if="settingsTab === 'general'" class="space-y-6">
                <!--
                  Simple mode, first. It is for the people least likely to
                  scroll down to find it; see shared/simpleMode.ts.
                -->
                <section>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <div class="flex items-center justify-between">
                      <div class="pr-4">
                        <p id="simple-mode-label" class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.simple_mode') }}</p>
                        <p id="simple-mode-desc" class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.general.simple_mode_desc') }}</p>
                      </div>
                      <button
                        @click="simpleMode = !simpleMode"
                        class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full focus:outline-none focus-visible:ring-2 focus-visible:ring-accent transition-colors duration-200 ease-in-out"
                        :class="simpleMode ? 'bg-accent' : 'bg-gray-300 dark:bg-gray-600'"
                        role="switch"
                        :aria-checked="simpleMode"
                        aria-labelledby="simple-mode-label"
                        aria-describedby="simple-mode-desc"
                      >
                        <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="simpleMode ? 'translate-x-2' : '-translate-x-2'"></span>
                      </button>
                    </div>
                  </div>
                </section>

                <!-- Syn's family-safe switch, which the hidden Syn tab would otherwise take with it. -->
                <SimpleModeSynSettings v-if="simpleMode && vaultPath" :vault-path="vaultPath" />

                <!-- Vault Management -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.general.vault') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <div class="flex items-center gap-2 mb-2">
                      <p class="text-xs font-medium text-gray-500 dark:text-gray-400">{{ $t('settings.general.storage_type') }}</p>
                      <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 text-xs font-semibold">
                        <FolderOpen class="w-3 h-3" /> {{ $t('settings.general.storage_local') }}
                      </span>
                    </div>
                    <p class="font-mono text-[12px] break-all text-text dark:text-text-dark bg-white dark:bg-surface-hover-dark px-3 py-2 rounded-lg border border-gray-200 dark:border-transparent">{{ vaultPath }}</p>
                    <button @click="emit('clear-vault')" class="btn-secondary mt-3">
                      <FolderOpen class="w-3.5 h-3.5" /> {{ $t('settings.general.switch_vault') }}
                    </button>
                  </div>
                </section>

                <!--
                  The trash belongs to the vault, not to any one mini-app:
                  `.trash/` holds notes, whiteboards, people and captures
                  beside tasks. It sits here so it is reachable from anywhere
                  and implies nothing about which app put things in it.
                -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('trash.title') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark flex items-center gap-3">
                    <div class="flex-1 min-w-0">
                      <p class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('trash.purge_note') }}</p>
                    </div>
                    <button @click="showTrash = true" class="btn-secondary shrink-0">
                      <Trash2 class="w-3.5 h-3.5" /> {{ $t('trash.a11y_open_trash') }}
                    </button>
                  </div>
                </section>

                <!-- Backup -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.general.backup') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-3">{{ $t('settings.general.backup_hint') }}</p>

                    <div class="flex flex-wrap gap-2">
                      <button @click="runExport" :disabled="archiveBusy"
                              class="btn-primary">
                        <HardDrive class="w-3.5 h-3.5" /> {{ $t('settings.general.backup_export') }}
                      </button>
                      <button @click="runImport" :disabled="archiveBusy"
                              class="btn-secondary disabled:opacity-50">
                        <FolderOpen class="w-3.5 h-3.5" /> {{ $t('settings.general.backup_import') }}
                      </button>
                    </div>

                    <p v-if="archiveBusy" class="mt-3 text-[12px] text-gray-500 dark:text-gray-400">
                      {{ $t('settings.general.backup_working') }}
                    </p>
                    <p v-else-if="archiveMessage" class="mt-3 text-[12px] text-green-700 dark:text-green-400">
                      {{ archiveMessage }}
                    </p>
                    <p v-else-if="archiveError" class="mt-3 text-[12px] text-red-600 dark:text-red-400 break-words">
                      {{ archiveError }}
                    </p>
                  </div>
                </section>

                <!-- Diagnostics -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.general.diagnostics') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-2">{{ $t('settings.general.diagnostics_hint') }}</p>
                    <p class="text-[12px] text-amber-700 dark:text-amber-500 mb-3">{{ $t('settings.general.diagnostics_privacy') }}</p>
                    <button @click="runDiagnosticsExport" :disabled="archiveBusy || !diagnosticsAvailable"
                            class="btn-secondary disabled:opacity-50">
                      <HardDrive class="w-3.5 h-3.5" /> {{ $t('settings.general.diagnostics_export') }}
                    </button>
                    <p v-if="!diagnosticsAvailable" class="mt-3 text-[12px] text-gray-500 dark:text-gray-400">
                      {{ $t('settings.general.diagnostics_empty') }}
                    </p>
                  </div>
                </section>

                <!-- Sync Provider -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.general.sync_provider') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-2 rounded-xl border border-border dark:border-border-dark space-y-2">
                    
                    <!-- LOCAL ONLY -->
                    <div class="rounded-lg overflow-hidden border border-transparent transition-colors" :class="activeSettingsProvider === 'none' ? 'border-gray-300 dark:border-gray-600 bg-white dark:bg-surface-hover-dark shadow-sm' : ''">
                      <button @click="activeSettingsProvider = 'none'" class="w-full px-3 py-2.5 flex items-center gap-3 text-left hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                        <div class="w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors" :class="activeSettingsProvider === 'none' ? 'border-gray-500' : 'border-gray-300 dark:border-gray-600'">
                          <div v-if="activeSettingsProvider === 'none'" class="w-2 h-2 rounded-full bg-gray-500"></div>
                        </div>
                        <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 bg-gray-100 dark:bg-gray-800">
                          <HardDrive class="w-4 h-4 text-gray-500 dark:text-gray-400" />
                        </div>
                        <div class="flex-1 min-w-0">
                          <p class="text-[13px] font-semibold text-text dark:text-text-dark">{{ $t('settings.general.local_only') }}</p>
                        </div>
                        <span v-if="activeSyncProvider === 'none' || activeSyncProvider === 'local'" class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-semibold bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400">
                          <Check class="w-3 h-3" /> {{ $t('settings.general.active') }}
                        </span>
                      </button>
                      <div v-if="activeSettingsProvider === 'none'" class="px-10 pb-4 pt-1 ml-4 border-t border-[#f0f0f0] dark:border-[#333] mt-1">
                        <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-3 mt-3">{{ $t('settings.general.local_only_desc') }}</p>
                        <button v-if="activeSyncProvider === 'server'" @click="showConfirmDisconnectAll = true" class="px-3 py-1.5 bg-gray-800 hover:bg-gray-900 text-white dark:bg-gray-200 dark:text-black rounded-lg text-[12px] font-medium transition-all shadow-sm">
                          {{ $t('settings.general.disconnect_providers') }}
                        </button>
                      </div>
                    </div>

                    <!-- SYNABIT SERVER (P2P) -->
                    <div class="rounded-lg overflow-hidden border border-transparent transition-colors" :class="activeSettingsProvider === 'server' ? 'border-accent/60 bg-white dark:bg-surface-hover-dark shadow-sm' : ''">
                      <button @click="activeSettingsProvider = 'server'" class="w-full px-3 py-2.5 flex items-center gap-3 text-left hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors">
                        <div class="w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors" :class="activeSettingsProvider === 'server' ? 'border-accent' : 'border-gray-300 dark:border-gray-600'">
                          <div v-if="activeSettingsProvider === 'server'" class="w-2 h-2 rounded-full bg-accent"></div>
                        </div>
                        <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 bg-emerald-100 dark:bg-emerald-900/20">
                          <Server class="w-4 h-4 text-emerald-500" />
                        </div>
                        <div class="flex-1 min-w-0">
                          <p class="text-[13px] font-semibold text-text dark:text-text-dark">{{ $t('settings.general.p2p_sync', 'Sync Server') }}</p>
                        </div>
                        <span v-if="activeSyncProvider === 'server'" class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-semibold bg-emerald-100 dark:bg-emerald-900/40 text-emerald-600 dark:text-emerald-400">
                          <Check class="w-3 h-3" /> {{ $t('settings.general.connected') }}
                        </span>
                      </button>

                      <!-- P2P Details Accordion -->
                      <div v-if="activeSettingsProvider === 'server'" class="px-3 md:px-10 pb-4 pt-1 ml-0 md:ml-4 border-t border-[#f0f0f0] dark:border-[#333] mt-1 space-y-4">
                        <!-- Connected state -->
                        <template v-if="activeSyncProvider === 'server'">
                          <div class="flex items-center justify-between mt-3">
                            <div class="flex items-center gap-2">
                              <div class="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></div>
                              <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.connected', 'Connected') }}</p>
                            </div>
                            <button @click="emit('sync-now')" :disabled="syncing" class="btn-primary">
                              <RefreshCw class="w-3.5 h-3.5" :class="syncing ? 'animate-spin' : ''" />
                              {{ syncing ? $t('settings.general.syncing', 'Syncing...') : $t('settings.general.sync_now', 'Sync Now') }}
                            </button>
                          </div>
                          <div class="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
                            <Server class="w-3 h-3" />
                            <span class="font-mono">{{ syncServerAddr }}</span>
                            <span v-if="syncServerAddr === OFFICIAL_SERVER.addr" class="px-1.5 py-0.5 text-xs font-bold uppercase tracking-wider rounded bg-emerald-100 dark:bg-emerald-900/30 text-emerald-600 dark:text-emerald-400">{{ $t('settings.general.official') }}</span>
                            <span v-else class="px-1.5 py-0.5 text-xs font-bold uppercase tracking-wider rounded bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400">{{ $t('settings.general.self_hosted_short') }}</span>
                          </div>
                          <div v-if="lastSyncTime" class="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
                            <span>{{ $t('settings.general.last_synced_at', { time: when(lastSyncTime, locale) }) }}</span>
                          </div>
                          <div v-if="syncError" class="text-xs text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
                            ⚠️ {{ syncError }}
                          </div>
                          <!-- Auto-sync -->
                          <div class="border-t border-border dark:border-border-dark pt-4">
                            <div class="flex items-center justify-between mb-3">
                              <p class="text-[12px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.periodic_auto_sync', 'Periodic auto-sync') }}</p>
                              <label class="relative inline-flex items-center cursor-pointer">
                                <input type="checkbox" :checked="autoSyncEnabled" @change="emit('update:auto-sync-enabled', ($event.target as HTMLInputElement).checked)" class="sr-only peer">
                                <div class="w-9 h-5 bg-gray-200 peer-focus:outline-none rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all dark:border-gray-600 peer-checked:bg-accent"></div>
                              </label>
                            </div>
                            <div v-if="autoSyncEnabled" class="flex items-center justify-between">
                              <p class="text-xs text-gray-500 dark:text-gray-400">{{ $t('settings.general.sync_interval', 'Interval (minutes)') }}</p>
                              <input type="number" :value="autoSyncInterval" @input="emit('update:auto-sync-interval', Number(($event.target as HTMLInputElement).value))" min="1" max="60" class="w-16 px-2 py-1 bg-white dark:bg-surface-hover-dark border border-border dark:border-border-subtle-dark rounded text-[12px] text-center text-text dark:text-text-dark focus:outline-none focus:border-accent" />
                            </div>
                          </div>
                          <!-- Disconnect -->
                          <div class="border-t border-border dark:border-border-dark pt-4">
                            <button @click="showConfirmDisconnectP2P = true" class="px-4 py-2 rounded-lg text-[12px] font-medium border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 transition-all flex items-center gap-2">
                              <Unplug class="w-3.5 h-3.5" /> {{ $t('settings.general.disconnect_p2p', 'Disconnect') }}
                            </button>
                          </div>

                          <!-- Mobile Settings -->
                          <div class="border-t border-border dark:border-border-dark pt-4">
                            <SyncMobileSettings />
                          </div>
                        </template>

                        <!-- Disconnected state -->
                        <template v-else>
                          <!-- Option cards -->
                          <div v-if="p2pServerMode === 'none'" class="space-y-2 mt-3">
                            <!-- Synabit Cloud (Official) -->
                            <button @click="OFFICIAL_SERVER.available ? handleConnectP2P(OFFICIAL_SERVER.addr, OFFICIAL_SERVER.id) : undefined" :disabled="!OFFICIAL_SERVER.available || serverConnecting" class="w-full p-3 rounded-xl border-2 text-left transition-all flex items-start gap-3 group" :class="OFFICIAL_SERVER.available ? 'border-border dark:border-border-dark hover:border-accent dark:hover:border-accent-dark cursor-pointer' : 'border-border dark:border-border-dark opacity-60 cursor-not-allowed'">
                              <div class="w-9 h-9 rounded-lg bg-emerald-50 dark:bg-emerald-900/20 flex items-center justify-center shrink-0 mt-0.5">
                                <RefreshCw v-if="serverConnecting" class="w-4.5 h-4.5 text-emerald-500 animate-spin" />
                                <Cloud v-else class="w-4.5 h-4.5 text-emerald-500" />
                              </div>
                              <div class="flex-1 min-w-0">
                                <div class="flex items-center gap-2">
                                  <p class="text-[13px] font-semibold text-text dark:text-text-dark">{{ $t('settings.general.official_relay') }}</p>
                                  <span v-if="!OFFICIAL_SERVER.available" class="px-1.5 py-0.5 text-xs font-bold uppercase tracking-wider rounded bg-amber-100 dark:bg-amber-900/30 text-amber-600 dark:text-amber-400">{{ $t('settings.general.coming_soon', 'Coming soon') }}</span>
                                  <span v-else class="px-1.5 py-0.5 text-xs font-bold uppercase tracking-wider rounded bg-emerald-100 dark:bg-emerald-900/30 text-emerald-600 dark:text-emerald-400">{{ $t('settings.general.official', 'Official') }}</span>
                                </div>
                                <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.general.official_desc', 'One-click setup • No configuration needed') }}</p>
                              </div>
                            </button>

                            <!-- Self-hosted Server -->
                            <button @click="p2pServerMode = 'custom'" class="w-full p-3 rounded-xl border-2 border-border dark:border-border-dark hover:border-gray-400 dark:hover:border-gray-500 text-left transition-all flex items-start gap-3 cursor-pointer group">
                              <div class="w-9 h-9 rounded-lg bg-gray-100 dark:bg-gray-800 flex items-center justify-center shrink-0 mt-0.5">
                                <Server class="w-4.5 h-4.5 text-gray-500 dark:text-gray-400" />
                              </div>
                              <div class="flex-1 min-w-0">
                                <p class="text-[13px] font-semibold text-text dark:text-text-dark">{{ $t('settings.general.self_hosted', 'Self-hosted Server') }}</p>
                                <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.general.self_hosted_desc', 'Connect to your own Synabit sync server') }}</p>
                              </div>
                            </button>
                          </div>

                          <!-- Self-hosted form -->
                          <div v-else-if="p2pServerMode === 'custom'" class="space-y-3 mt-3">
                            <div class="space-y-1">
                              <label class="text-[12px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.server_address', 'Server Address') }}</label>
                              <input v-model="p2pFormAddr" type="text" :placeholder="$t('settings.general.server_address_hint', 'e.g. 1.2.3.4:4433')" class="w-full px-3 py-2 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-accent font-mono" />
                            </div>
                            <div class="space-y-1">
                              <label class="text-[12px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.server_id', 'Server ID') }}</label>
                              <input v-model="p2pFormId" type="text" :placeholder="$t('settings.general.server_id_hint', '64-character hex string')" class="w-full px-3 py-2 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-accent font-mono" />
                            </div>
                            <div class="flex gap-2">
                              <button @click="handleConnectP2P(p2pFormAddr, p2pFormId)" :disabled="serverConnecting || !p2pFormAddr || !p2pFormId" class="btn-primary flex-1">
                                <RefreshCw v-if="serverConnecting" class="w-3.5 h-3.5 animate-spin" />
                                <Server v-else class="w-3.5 h-3.5" />
                                {{ serverConnecting ? $t('settings.general.connecting', 'Connecting...') : $t('settings.general.connect', 'Connect') }}
                              </button>
                              <button @click="p2pServerMode = 'none'" class="px-4 py-2 border border-border-subtle dark:border-border-subtle-dark text-text-secondary dark:text-text-secondary-dark rounded-lg text-[13px] font-medium transition-all">
                                {{ $t('settings.security.cancel', 'Cancel') }}
                              </button>
                            </div>
                            <div v-if="syncError" class="text-xs text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
                              ⚠️ {{ syncError }}
                            </div>
                          </div>
                        </template>
                      </div>
                    </div>

                  </div>
                </section>

                <!-- Behavior -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.general.behavior') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <div class="flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.startup_app') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.general.startup_app_desc') }}</p>
                      </div>
                      <select v-model="defaultApp" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                        <option v-for="id in STARTUP_CHOICES" :key="id" :value="id" :disabled="!appOffered(id, simpleMode)">{{ appName(id) }}</option>
                      </select>
                    </div>
                    <p v-if="startAppOverridden" class="text-xs text-gray-500 dark:text-gray-400 mt-2">
                      {{ $t('settings.general.simple_mode_start_note', { app: appName(SIMPLE_MODE_HOME) }) }}
                    </p>
                  </div>
                </section>

                <!-- Sidebar Navigation -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.general.sidebar') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <p class="text-[13px] font-medium text-text dark:text-text-dark mb-1">{{ $t('settings.general.visible_apps') }}</p>
                    <p class="text-xs text-gray-500 dark:text-gray-400 mb-4">{{ $t('settings.general.visible_apps_desc') }}</p>
                    <p v-if="simpleMode" class="text-xs text-gray-500 dark:text-gray-400 mb-4">{{ $t('settings.general.simple_mode_apps_note') }}</p>
                    <p v-if="savedViews > 0" class="text-xs text-gray-500 dark:text-gray-400 mb-4">
                      {{ $t('settings.general.saved_views_count', { saved: savedViews, pinned: pinnedViews }) }}
                    </p>
                    
                    <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                      <label v-for="app in sidebarChoices" :key="app.id" 
                        class="flex items-center justify-between p-2 rounded-lg border transition-colors cursor-pointer"
                        :class="defaultApp === app.id ? 'bg-gray-100 dark:bg-[#252525] border-transparent opacity-60 cursor-not-allowed' : 'bg-white dark:bg-surface-hover-dark border-border dark:border-border-subtle-dark hover:border-gray-300 dark:hover:border-gray-500'"
                      >
                        <span class="text-[12px] font-medium text-text dark:text-text-dark flex items-center gap-2">
                          <component :is="app.icon" class="w-4 h-4 text-gray-500 dark:text-gray-400" />
                          {{ appName(app.id) }}
                          <span v-if="defaultApp === app.id" class="text-xs px-1.5 py-0.5 bg-gray-200 dark:bg-gray-700 text-gray-500 dark:text-gray-400 rounded uppercase font-bold ml-1 tracking-wide">{{ $t('settings.general.default') }}</span>
                        </span>
                        
                        <div class="relative inline-flex h-4 w-7 shrink-0 items-center justify-center rounded-full transition-colors duration-200 ease-in-out" :class="!hiddenSidebarApps.includes(app.id) ? 'bg-green-500' : 'bg-gray-300 dark:bg-gray-600'">
                          <input type="checkbox" :checked="!hiddenSidebarApps.includes(app.id)" :disabled="defaultApp === app.id" @change="toggleAppVisibility(app.id)" class="sr-only">
                          <span class="pointer-events-none inline-block h-3 w-3 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="!hiddenSidebarApps.includes(app.id) ? 'translate-x-1.5' : '-translate-x-1.5'"/>
                        </div>
                      </label>
                    </div>
                  </div>
                </section>

                <!-- Theme & Language -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.general.appearance') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark space-y-4">
                    <div>
                      <p class="text-[12px] font-medium text-text dark:text-text-dark mb-3">{{ $t('settings.general.theme') }}</p>
                      <div class="flex gap-2">
                        <button v-for="mode in (['light', 'dark', 'system'] as const)" :key="mode"
                          @click="themeMode = mode"
                          :class="['px-4 py-2 rounded-lg text-[12px] font-medium transition-all border capitalize', themeMode === mode ? 'bg-accent/10 text-accent dark:text-accent-dark border-accent/40' : 'bg-white dark:bg-surface-hover-dark border-border-subtle dark:border-border-subtle-dark text-gray-600 dark:text-gray-300 hover:border-gray-400 dark:hover:border-gray-500']">
                          {{ $t(`settings.general.themes.${mode}`) }}
                        </button>
                      </div>
                    </div>
                    
                    <div class="border-t border-border dark:border-border-dark pt-4">
                      <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.ui_scale') }}</p>
                      <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5 mb-3">{{ $t('settings.general.ui_scale_desc') }}</p>
                      <div class="flex flex-wrap gap-2" role="radiogroup" :aria-label="$t('settings.general.ui_scale')">
                        <button v-for="(s, i) in UI_SCALES" :key="s.key"
                          role="radio" :aria-checked="uiScale === s.value"
                          :tabindex="uiScale === s.value ? 0 : -1"
                          @click="uiScale = s.value"
                          @keydown="onScaleKey($event, i)"
                          :class="['px-4 py-2 rounded-lg font-medium transition-all border', uiScale === s.value ? 'bg-accent/10 text-accent dark:text-accent-dark border-accent/40' : 'bg-white dark:bg-surface-hover-dark border-border-subtle dark:border-border-subtle-dark text-gray-600 dark:text-gray-300 hover:border-gray-400 dark:hover:border-gray-500']"
                          :style="{ fontSize: `${12 * s.value}px` }">
                          {{ $t(`settings.general.ui_scales.${s.key}`) }}
                        </button>
                      </div>
                    </div>

                    <div class="border-t border-border dark:border-border-dark pt-4">
                      <div class="flex items-center justify-between">
                        <div>
                          <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.language') }}</p>
                          <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.general.language_desc') }}</p>
                        </div>
                        <select v-model="appLanguage" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                          <option value="en">English</option>
                          <option value="vi">Tiếng Việt</option>
                        </select>
                      </div>

                      <!-- Login item. Desktop only: phones have no such thing. -->
                      <div v-if="isDesktop" class="flex items-center justify-between pt-4 border-t border-border dark:border-border-dark">
                        <div class="pr-4">
                          <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.general.start_with_system') }}</p>
                          <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.general.start_with_system_desc') }}</p>
                        </div>
                        <button
                          @click="toggleAutostart"
                          :disabled="autostartBusy"
                          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full focus:outline-none transition-colors duration-200 ease-in-out disabled:opacity-50"
                          :class="autostartOn ? 'bg-accent' : 'bg-gray-300 dark:bg-gray-600'"
                          :aria-label="$t('settings.general.start_with_system')"
                          role="switch"
                          :aria-checked="autostartOn"
                        >
                          <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="autostartOn ? 'translate-x-2' : '-translate-x-2'"></span>
                        </button>
                      </div>
                    </div>
                  </div>
                </section>
              </div>
              
              <!-- === NOTES TAB === -->
              <div v-else-if="settingsTab === 'notes'" class="space-y-6">
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.notes.features') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark flex flex-col gap-4">
                    <div class="flex items-center justify-between">
                      <div class="pr-4">
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.notes.show_toolbar') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.notes.show_toolbar_desc') }}</p>
                      </div>
                      <button @click="noteToolbarVisible = !noteToolbarVisible" class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full focus:outline-none transition-colors duration-200 ease-in-out" :class="noteToolbarVisible ? 'bg-accent' : 'bg-gray-300 dark:bg-gray-600'" :aria-label="$t('settings.notes.show_toolbar')" role="switch" :aria-checked="noteToolbarVisible">
                        <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="noteToolbarVisible ? 'translate-x-2' : '-translate-x-2'"/>
                      </button>
                    </div>
                    <div class="border-t border-border dark:border-border-dark pt-4 flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.notes.enable_daily_notes') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.notes.enable_daily_notes_desc') }}</p>
                      </div>
                      <button @click="enableDailyNotes = !enableDailyNotes" class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full focus:outline-none transition-colors duration-200 ease-in-out" :class="enableDailyNotes ? 'bg-accent' : 'bg-gray-300 dark:bg-gray-600'" :aria-label="$t('settings.notes.enable_daily_notes')" role="switch" :aria-checked="enableDailyNotes">
                        <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="enableDailyNotes ? 'translate-x-2' : '-translate-x-2'"/>
                      </button>
                    </div>
                    <div class="border-t border-border dark:border-border-dark pt-4 flex items-center justify-between" :class="!enableDailyNotes ? 'opacity-50 pointer-events-none' : ''">
                      <div>
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.notes.date_format') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.notes.date_format_desc') }}</p>
                      </div>
                      <div class="flex flex-col items-end gap-1">
                        <input type="text" v-model="dailyNoteFormat" class="w-28 px-3 py-1.5 rounded-lg bg-white dark:bg-surface-hover-dark border text-[13px] text-center text-text dark:text-text-dark focus:outline-none focus:ring-1 transition-colors" :class="isValidDailyFormat ? 'border-border-subtle dark:border-border-subtle-dark focus:ring-black dark:focus:ring-white' : 'border-red-400 focus:ring-red-500'" />
                        <span v-if="!isValidDailyFormat" class="text-xs text-red-500 font-medium">{{ $t('settings.notes.date_format_req') }}</span>
                      </div>
                    </div>
                    <div class="border-t border-border dark:border-border-dark pt-4 flex items-center justify-between" :class="!enableDailyNotes ? 'opacity-50 pointer-events-none' : ''">
                      <div>
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.notes.default_tag') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.notes.default_tag_desc') }}</p>
                      </div>
                      <input type="text" v-model="dailyNoteTag" :placeholder="$t('settings.notes.default_tag_placeholder')" class="w-28 px-3 py-1.5 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-center text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors" />
                    </div>
                  </div>
                </section>
                
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3 mt-6">{{ $t('settings.notes.editor') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark flex flex-col gap-4">
                    <div class="flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.notes.list_style') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.notes.list_style_desc') }}</p>
                      </div>
                      <select v-model="nestedNumberListStyle" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                        <option value="decimal">{{ $t('settings.notes.list_style_decimal', '1. 2. 3.') }}</option>
                        <option value="alpha">{{ $t('settings.notes.list_style_alpha', 'A. B. C.') }}</option>
                        <option value="nested">{{ $t('settings.notes.list_style_nested', '1. 1.1. 1.1.1.') }}</option>
                      </select>
                    </div>
                    
                    <div class="border-t border-border dark:border-border-dark pt-4 flex items-center justify-between">
                      <div>
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.notes.code_tab_size', 'Code Block Tab Size') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.notes.code_tab_size_desc', 'Number of spaces for indentation') }}</p>
                      </div>
                      <select v-model="codeBlockTabSize" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                        <option :value="2">{{ $t('settings.notes.spaces', { count: 2 }, 2) }}</option>
                        <option :value="4">{{ $t('settings.notes.spaces', { count: 4 }, 4) }}</option>
                      </select>
                    </div>

                    <div class="border-t border-border dark:border-border-dark pt-4 flex flex-col gap-3">
                      <div class="flex items-center justify-between">
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.notes.code_theme', 'Code Block Theme (Light)') }}</p>
                        <button @click="codeBlockBgColorLight = '#f8f9fa'; codeBlockTextColorLight = '#24292e'" class="text-xs text-gray-500 dark:text-gray-400 hover:text-black dark:hover:text-white transition-colors uppercase tracking-wider font-medium">{{ $t('settings.notes.reset') }}</button>
                      </div>
                      <div class="flex items-center justify-between pl-2">
                        <p class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('settings.notes.code_bg_color') }}</p>
                        <input type="color" v-model="codeBlockBgColorLight" class="w-8 h-8 rounded border-none p-0 bg-transparent cursor-pointer" />
                      </div>
                      <div class="flex items-center justify-between pl-2">
                        <p class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('settings.notes.code_text_color') }}</p>
                        <input type="color" v-model="codeBlockTextColorLight" class="w-8 h-8 rounded border-none p-0 bg-transparent cursor-pointer" />
                      </div>
                    </div>

                    <div class="border-t border-border dark:border-border-dark pt-4 flex flex-col gap-3">
                      <div class="flex items-center justify-between">
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.notes.code_theme_dark', 'Code Block Theme (Dark)') }}</p>
                        <button @click="codeBlockBgColorDark = '#1e1e1e'; codeBlockTextColorDark = '#e4e4e7'" class="text-xs text-gray-500 dark:text-gray-400 hover:text-black dark:hover:text-white transition-colors uppercase tracking-wider font-medium">{{ $t('settings.notes.reset') }}</button>
                      </div>
                      <div class="flex items-center justify-between pl-2">
                        <p class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('settings.notes.code_bg_color') }}</p>
                        <input type="color" v-model="codeBlockBgColorDark" class="w-8 h-8 rounded border-none p-0 bg-transparent cursor-pointer" />
                      </div>
                      <div class="flex items-center justify-between pl-2">
                        <p class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('settings.notes.code_text_color') }}</p>
                        <input type="color" v-model="codeBlockTextColorDark" class="w-8 h-8 rounded border-none p-0 bg-transparent cursor-pointer" />
                      </div>
                    </div>
                  </div>
                </section>
              </div>
              
              <!-- === TASKS TAB === -->
              <div v-else-if="settingsTab === 'tasks'" class="space-y-6">
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.tasks.delete_confirm_label') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark space-y-2">
                    <label
                      v-for="option in (['dialog', 'inline', 'undo'] as const)"
                      :key="option"
                      class="flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer transition-colors"
                      :class="taskDeleteConfirm === option ? 'bg-white dark:bg-surface-hover-dark shadow-sm' : 'hover:bg-white/60 dark:hover:bg-[#252525]'"
                    >
                      <input type="radio" :value="option" v-model="taskDeleteConfirm" class="w-4 h-4 accent-accent cursor-pointer" />
                      <span class="text-[13px] text-text dark:text-text-dark">{{ $t('settings.tasks.delete_confirm_' + option) }}</span>
                    </label>
                    <p class="text-xs text-gray-500 dark:text-gray-400 px-3 pt-1">{{ $t('settings.tasks.delete_confirm_hint') }}</p>
                  </div>
                </section>

                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.tasks.auto_archive') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <div class="flex items-center justify-between mb-2">
                      <div>
                        <p class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('settings.tasks.archive_completed') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.tasks.archive_desc_1') }} <code class="px-1 py-0.5 bg-gray-200 dark:bg-[#333] rounded text-xs">Tasks/archived</code> {{ $t('settings.tasks.archive_desc_2') }}</p>
                      </div>
                    </div>
                    <div class="flex items-center gap-3 mt-3">
                      <label class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('settings.tasks.after') }}</label>
                      <input type="number" v-model.number="taskArchiveDays" min="1" max="365" class="w-20 px-3 py-1.5 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-center text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white" />
                      <span class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('settings.tasks.days') }}</span>
                    </div>
                  </div>
                </section>
              </div>
              <!-- === SECURITY TAB === -->
              <div v-else-if="settingsTab === 'security'" class="space-y-6">
                <!-- Local Security (App Lock) -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.security.app_lock') }}</h4>
                  
                  <!-- Setup or Managed States -->
                  <div v-if="!appLockStore.isEnabled" class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <div class="flex items-center gap-3 mb-3">
                      <div class="w-10 h-10 rounded-xl bg-accent/10 flex items-center justify-center">
                        <Shield class="w-5 h-5 text-accent dark:text-accent-dark" />
                      </div>
                      <div>
                        <p class="text-[13px] font-semibold text-text dark:text-text-dark">{{ $t('settings.security.protect_app') }}</p>
                        <p class="text-xs text-gray-500 dark:text-gray-400">{{ $t('settings.security.protect_app_desc') }}</p>
                      </div>
                    </div>
                    <button @click="emit('show-setup-pin', 'setup')" class="btn-primary w-full">
                      <Lock class="w-4 h-4" /> {{ $t('settings.security.setup_pin') }}
                    </button>
                  </div>

                  <template v-else>
                    <!-- Part 1: PIN Settings -->
                    <div class="mb-4 bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                      <p class="text-[13px] font-semibold text-text dark:text-text-dark mb-4">{{ $t('settings.security.pin_settings') }}</p>
                      
                      <!-- Idle Timeout -->
                      <div class="flex items-center justify-between mb-4">
                        <div>
                          <p class="text-[12px] font-medium text-text dark:text-text-dark">{{ $t('settings.security.idle_timeout') }}</p>
                          <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{{ $t('settings.security.idle_timeout_desc') }}</p>
                        </div>
                        <select :value="appLockStore.autoLockTimeoutSecs" @change="handleAutoLockTimeout" class="appearance-none px-3 py-1.5 rounded-lg bg-white dark:bg-surface-hover-dark border border-border-subtle dark:border-border-subtle-dark text-[13px] text-text dark:text-text-dark focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors cursor-pointer text-center pr-8 bg-[url('data:image/svg+xml;charset=US-ASCII,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20width%3D%22292.4%22%20height%3D%22292.4%22%3E%3Cpath%20fill%3D%22%239ca3af%22%20d%3D%22M287%2069.4a17.6%2017.6%200%200%200-13-5.4H18.4c-5%200-9.3%201.8-12.9%205.4A17.6%2017.6%200%200%200%200%2082.2c0%205%201.8%209.3%205.4%2012.9l128%20127.9c3.6%203.6%207.8%205.4%2012.8%205.4s9.2-1.8%2012.8-5.4L287%2095c3.5-3.5%205.4-7.8%205.4-12.8%200-5-1.9-9.2-5.5-12.8z%22%2F%3E%3C%2Fsvg%3E')] bg-[length:10px_10px] bg-[right_10px_center] bg-no-repeat">
                          <option v-for="opt in autoLockOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
                        </select>
                      </div>

                      <hr class="border-border dark:border-border-dark mb-4" />
                      
                      <!-- PIN Actions -->
                      <div class="flex gap-2">
                        <button @click="emit('show-setup-pin', 'change')" class="flex-1 px-4 py-2 border border-border-subtle dark:border-border-subtle-dark text-text-secondary dark:text-text-secondary-dark hover:bg-gray-100 dark:hover:bg-[#333] rounded-lg text-[12px] font-medium transition-all flex items-center justify-center gap-2">
                          <Lock class="w-3.5 h-3.5" /> {{ $t('settings.security.change_pin') }}
                        </button>
                        <button @click="requirePin($t('settings.security.pin_to_remove_lock'), handleRemoveLock)" :disabled="removingLock" class="px-4 py-2 border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-lg text-[12px] font-medium transition-all flex items-center justify-center gap-2 disabled:opacity-60">
                          <Trash2 class="w-3.5 h-3.5" /> {{ $t('settings.security.remove_pin') }}
                        </button>
                      </div>
                    </div>

                    <!-- Part 2: Lock Entire App -->
                    <div class="mb-4 bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                      <div class="flex items-center justify-between">
                        <div>
                          <p class="text-[13px] font-semibold text-text dark:text-text-dark">{{ $t('settings.security.lock_entire_app') }}</p>
                          <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">{{ $t('settings.security.lock_entire_app_desc') }}</p>
                        </div>
                        <button type="button" role="switch" :aria-checked="appLockStore.appLockActive" :aria-label="$t('settings.security.lock_entire_app')" class="relative inline-flex h-5 w-9 shrink-0 items-center justify-center rounded-full focus:outline-none focus-visible:ring-2 focus-visible:ring-accent transition-colors duration-200 ease-in-out cursor-pointer" :class="appLockStore.appLockActive ? 'bg-accent' : 'bg-gray-300 dark:bg-gray-600'" @click="handleToggleTier1">
                          <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="appLockStore.appLockActive ? 'translate-x-2' : '-translate-x-2'"/>
                        </button>
                      </div>
                    </div>

                    <!-- Part 3: Protected Mini Apps -->
                    <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                      <p class="text-[13px] font-semibold text-text dark:text-text-dark mb-2">{{ $t('settings.security.protected_mini_apps') }}</p>
                      <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-4 leading-relaxed">{{ $t('settings.security.protected_mini_apps_desc') }}</p>
                      <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                        <label v-for="app in lockableApps" :key="app.id"
                          class="flex items-center justify-between p-2 rounded-lg border transition-colors cursor-pointer"
                          :class="appLockStore.isAppProtected(app.id) ? 'bg-accent/10 border-accent/40' : 'bg-white dark:bg-surface-hover-dark border-border dark:border-border-subtle-dark hover:border-gray-300 dark:hover:border-gray-500'"
                        >
                          <span class="text-[12px] font-medium text-text dark:text-text-dark flex items-center gap-2">
                            <component :is="app.icon" class="w-4 h-4 text-gray-500 dark:text-gray-400" />
                            {{ appName(app.id) }}
                          </span>
                          <div class="relative inline-flex h-4 w-7 shrink-0 items-center justify-center rounded-full transition-colors duration-200 ease-in-out cursor-pointer" :class="appLockStore.isAppProtected(app.id) ? 'bg-accent' : 'bg-gray-300 dark:bg-gray-600'" @click="handleToggleProtectedApp(app.id, appName(app.id))">
                            <span class="pointer-events-none inline-block h-3 w-3 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="appLockStore.isAppProtected(app.id) ? 'translate-x-1.5' : '-translate-x-1.5'"/>
                          </div>
                        </label>
                      </div>
                    </div>
                  </template>
                </section>

                <!-- Cloud Security (E2EE) -->
                <section>
                  <h4 class="text-[13px] font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-3">{{ $t('settings.security.e2ee') }}</h4>
                  <div class="bg-[#f8f8f8] dark:bg-surface-dark p-4 rounded-xl border border-border dark:border-border-dark">
                    <div class="flex items-center gap-2 mb-4">
                      <div class="w-2.5 h-2.5 rounded-full" :class="e2eeStatus.key_available ? 'bg-green-500' : 'bg-amber-500'"></div>
                      <p class="text-[13px] font-semibold text-text dark:text-text-dark">{{ e2eeStatus.key_available ? $t('settings.security.e2ee_active') : $t('settings.security.e2ee_setup') }}</p>
                    </div>

                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mb-6 leading-relaxed">{{ $t('settings.security.e2ee_desc') }}</p>

                    <!-- E2EE Setup (Generate or Restore) -->
                    <div v-if="e2eeStatus.needs_setup" class="space-y-4">
                      <button @click="setupE2ee" class="btn-primary w-full">
                        <Lock class="w-4 h-4" /> {{ $t('settings.security.setup_encryption') }}
                      </button>
                    </div>

                    <!-- Key available: just show status -->
                    <div v-else-if="e2eeStatus.key_available" class="">
                      <p class="text-[12px] text-green-600 dark:text-green-400 font-medium">{{ $t('settings.security.key_stored_securely') }}</p>
                    </div>

                    <!--
                      The success line that sat here was written only by the
                      restore that now lives in E2eeOnboarding, so it could
                      never appear. The error line stays and finally has a
                      writer: a failed status check.
                    -->
                    <p v-if="e2eeError" class="mt-4 text-[12px] text-red-500 font-medium p-2 bg-red-50 dark:bg-red-900/20 rounded">{{ e2eeError }}</p>
                  </div>
                </section>
              </div>

              <!-- === TIMELINE TAB === -->
              <!-- Reading the vault into moments, and starting again. It lived
                   at the bottom of the moment review until somebody went
                   looking for the reset and had to open a queue to find it. -->
              <div v-else-if="settingsTab === 'timeline'" class="space-y-6">
                <TimelineSettings :vault-path="vaultPath" />
              </div>

              <!-- === DEVICES TAB === -->
              <div v-else-if="settingsTab === 'devices'" class="space-y-6">
                <DeviceManager />
              </div>
              
              <!-- === ABOUT TAB === -->
              <div v-else-if="settingsTab === 'about'" class="space-y-6">
                <section>
                  <div class="text-center pt-8">
                    <div class="w-16 h-16 bg-gradient-to-br from-gray-100 to-gray-200 dark:from-surface-hover-dark dark:to-[#333] rounded-2xl flex items-center justify-center mx-auto mb-4 shadow-inner">
                      <Globe class="w-8 h-8 text-gray-500 dark:text-gray-400" />
                    </div>
                    <h3 class="text-[18px] font-bold text-text dark:text-text-dark">Synabit</h3>
                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-1">{{ $t('settings.about.version') }} {{ appVersion || '...' }}</p>
                    <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-4 max-w-xs mx-auto leading-relaxed">{{ $t('settings.about.desc') }}</p>
                    
                    <!--
                      A button rather than a link, and openUrl rather than
                      target="_blank": a webview does not hand a plain external
                      anchor to the system browser, so this did nothing when
                      clicked. Everywhere else in the app already uses openUrl.

                      Google Play checks that the privacy policy is reachable
                      from inside the app, so a link that silently fails is a
                      review finding as well as a broken control.
                    -->
                    <button @click="openLegal('PRIVACY_POLICY.md')" class="text-[12px] text-accent dark:text-accent-dark hover:underline mt-4 block font-medium mx-auto">
                      {{ $t('settings.about.privacy_policy') }}
                    </button>
                    
                    <div v-if="isDesktop" class="mt-8 flex flex-col items-center gap-3">
                      <!-- Check for Updates -->
                      <button @click="checkForUpdates(false, true)" 
                              :disabled="updateChecking || updateDownloading"
                              class="btn-secondary disabled:opacity-50 disabled:cursor-not-allowed">
                        <RefreshCw v-if="updateChecking" class="w-4 h-4 animate-spin" />
                        <template v-if="updateChecking">{{ $t('update.checking') }}</template>
                        <template v-else-if="updateDownloading">{{ $t('update.downloading') }} {{ updateProgress }}%</template>
                        <template v-else-if="updateAvailable">
                          {{ $t('update.available', { version: updateVersion }) }}
                        </template>
                        <template v-else>{{ $t('update.checkNow') }}</template>
                      </button>

                      <!-- Check Result Feedback -->
                      <Transition name="fade">
                        <p v-if="lastCheckResult === 'up-to-date'" class="text-[12px] text-emerald-600 dark:text-emerald-400 flex items-center gap-1.5">
                          <Check class="w-3.5 h-3.5" /> {{ $t('update.upToDate') }}
                        </p>
                        <p v-else-if="lastCheckResult === 'error'" class="text-[12px] text-red-500 dark:text-red-400">
                          {{ $t('update.failed') }}
                        </p>
                      </Transition>

                      <!-- Install button (khi có update) -->
                      <button v-if="updateAvailable && !updateDownloading" 
                              @click="downloadAndInstall"
                              class="btn-primary">
                        {{ $t('update.installNow') }}
                      </button>

                      <!-- Release Notes (khi có update) -->
                      <p v-if="updateAvailable && updateNotes" 
                         class="text-xs text-gray-500 dark:text-gray-400 max-w-xs text-center leading-relaxed">
                        {{ updateNotes.split('\n').slice(0, 3).join('\n') }}
                      </p>

                      <!-- Open Logs -->
                      <button @click="openLogFolder" class="px-4 py-2 rounded-lg text-[12px] font-medium border border-border-subtle dark:border-border-subtle-dark text-text-secondary dark:text-text-secondary-dark hover:bg-gray-100 dark:hover:bg-[#333] transition-all flex items-center gap-2">
                        <FolderOpen class="w-4 h-4" /> {{ $t('settings.about.open_logs') }}
                      </button>
                    </div>
                  </div>
                </section>
              </div>
            </div>
          </div>
        </div>
  </AppDialog>

  <!-- PIN Verification Overlay -->
  <Teleport to="body">
    <LockScreenVerify
      v-if="showPinVerify"
      :title="pinVerifyTitle"
      @unlocked="onPinVerified"
      @cancelled="showPinVerify = false; pendingAction = null"
    />

    <!-- Confirm Disconnect Modals -->
      <ConfirmModal
        :show="showConfirmDisconnectP2P"
        :title="$t('settings.general.disconnect_server_title')"
        :message="$t('settings.general.disconnect_server_message')"
        :confirm-text="$t('settings.general.disconnect_p2p')"
        :cancel-text="$t('settings.security.cancel')"
        is-destructive
        @confirm="showConfirmDisconnectP2P = false; emit('disconnect-server')"
        @cancel="showConfirmDisconnectP2P = false"
      />
    <ConfirmModal v-if="showConfirmDisconnectAll" :show="true" :title="$t('settings.general.disconnect_all_title')" :message="$t('settings.general.disconnect_all_message')" :confirmText="$t('settings.general.disconnect_all')" @confirm="() => { handleDisconnectAll(); showConfirmDisconnectAll = false; }" @cancel="showConfirmDisconnectAll = false" />

  </Teleport>
  <TrashPanel :show="showTrash" :vaultPath="vaultPath" @close="showTrash = false" />

</template>

<style scoped>
/* Fade transition for update check result */
.fade-enter-active,
.fade-leave-active {
  transition: opacity 0.3s ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
