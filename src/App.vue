<script setup lang="ts">
import { ref, computed, provide, onMounted, onUnmounted, watch, nextTick } from 'vue';
import {
  paneShare as synPaneShare, panePage, PANE_BAR, dragPaneTo, openPane, closePane,
  panePageBack, panePageForward, typedAddress, leavesTheApp, openBeside, followLink, SOMEWHERE_TO_START,
} from './shared/syn/pane';

/**
 * Dragging the browser pane's edge.
 *
 * Listeners on `window` rather than the handle, because a pointer that leaves a
 * six-pixel strip mid-drag is the normal case, not the exception — the same
 * reason `useSidebarResize` does it that way for the DOM sidebars.
 *
 * The pane follows the pointer live. It used to be parked off the right edge
 * for the length of the pull with a line drawn where it would land, and that
 * looked exactly as bad as it sounds: the column went white while the pane was
 * away and flashed as it came back, on every drag. See `shared/syn/pane` for
 * why the live version works now and did not before.
 */
/**
 * A link that would take the app somewhere that is not the app.
 *
 * # What happened without this
 *
 * Syn answered with a link to an article. Clicking it navigated **the app's own
 * webview** to that article: the entire window became a news site, with no
 * sidebar, no conversation and no way back, because the way back is the app and
 * the app was gone. Quitting was the only exit.
 *
 * One listener, at the document, rather than a handler in each component that
 * renders markdown — there are four already and the next one would arrive
 * without this thought attached. Notes had the same hole: the editor lets
 * `synabit://` through and falls through on everything else.
 *
 * On the bubble, and skipping anything already handled: `ArticleReader` and the
 * wiki-link handler both stop their own clicks, and this must compose with them
 * rather than race them. A bubble listener can still call `preventDefault`,
 * because the browser follows the link only once the event has finished
 * propagating.
 */
const followExternalLink = (e: MouseEvent) => {
  if (e.defaultPrevented) return;
  const link = (e.target as HTMLElement | null)?.closest?.('a[href]') as HTMLAnchorElement | null;
  if (!link) return;

  const href = link.getAttribute('href') ?? '';
  if (!leavesTheApp(href)) return;

  e.preventDefault();
  void followLink(href);
};

const draggingPane = ref(false);

/**
 * What the address bar is showing, which is not always what the pane is on.
 *
 * A separate ref because the moment somebody starts typing, the bar stops being
 * a report and becomes a draft. Overwriting it from `syn-pane-page` while they
 * type — and that event fires on every title change — would delete the address
 * halfway through being entered.
 */
const paneAddress = ref('');
const paneAddressFocused = ref(false);

watch(panePage, page => {
  if (paneAddressFocused.value) return;
  paneAddress.value = page?.url ?? '';
}, { immediate: true });

const goToTypedAddress = () => {
  const url = typedAddress(paneAddress.value);
  if (!url) return;
  // Their own browser, so their own address. The backend's `may_go_to` is the
  // rule about where it may go, and it is the same rule for Syn and for them.
  openPane(url);
  paneAddressFocused.value = false;
};

const onPaneDrag = (e: MouseEvent) => {
  if (!draggingPane.value) return;
  // What is left of the window to the right of the pointer. Rust decides
  // whether that is allowed; this only says what is being asked for.
  dragPaneTo((window.innerWidth - e.clientX) / window.innerWidth);
};

const endPaneDrag = () => {
  if (!draggingPane.value) return;
  draggingPane.value = false;
  document.body.style.cursor = '';
};

const startPaneDrag = () => {
  draggingPane.value = true;
  // Held for the whole drag: without it the cursor flickers back to a caret
  // every time the pointer crosses text.
  document.body.style.cursor = 'col-resize';
};

onMounted(() => {
  window.addEventListener('mousemove', onPaneDrag);
  window.addEventListener('mouseup', endPaneDrag);
});
onUnmounted(() => {
  window.removeEventListener('mousemove', onPaneDrag);
  window.removeEventListener('mouseup', endPaneDrag);
});
import { Loader2, FileText, FolderOpen, Calendar, CheckSquare, Zap, Globe, Waypoints, RefreshCw, Settings, Users, Wallet, MessageCircle, Palette, MoreHorizontal, Rss, Server, Boxes, KeyRound, X, ArrowLeft, ArrowRight } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { runBeforeQuit } from './composables/useBeforeQuit';
import { initEventBus, destroyEventBus, useEventBus } from './composables/useEventBus';
import { useNodeService } from './composables/useNodeService';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { defineAsyncComponent } from 'vue';
import { useRouter, useRoute } from 'vue-router';

// Settings Modal is the only async component kept here
const SettingsModal = defineAsyncComponent(() => import('./shared/components/SettingsModal.vue'));
const E2eeOnboarding = defineAsyncComponent(() => import('./shared/components/E2eeOnboarding.vue'));
const LockScreen = defineAsyncComponent(() => import('./shared/components/LockScreen.vue'));
const SetupPinModal = defineAsyncComponent(() => import('./shared/components/SetupPinModal.vue'));
import AppNotice from './shared/components/AppNotice.vue';
import DeleteConfirmHost from './shared/components/DeleteConfirmHost.vue';
const SafeRequestCard = defineAsyncComponent(() => import('./mini-apps/safe/SafeRequestCard.vue'));
const SshApproveCard = defineAsyncComponent(() => import('./mini-apps/safe/SshApproveCard.vue'));
const CliApproveCard = defineAsyncComponent(() => import('./mini-apps/safe/CliApproveCard.vue'));
const RecoveryModal = defineAsyncComponent(() => import('./shared/components/RecoveryModal.vue'));
const VaultBackupNotice = defineAsyncComponent(() => import('./shared/components/VaultBackupNotice.vue'));

// Composables
import { useSettings } from './composables/useSettings';
import { useSync } from './composables/useSync';
import { useAppLock } from './composables/useAppLock';
import { usePlatform } from './composables/usePlatform';
import { useBackGuard } from './composables/useBackGuard';
import { BUILT_IN_APPS, appName } from './shared/appRegistry';
import { appOffered, redirectTarget, startApp, appForOpenType, hiddenBySimpleMode } from './shared/appAccess';
import { simpleModePass, modeChoiceCopy } from './shared/simpleMode';
import { appInPlatformScope } from './shared/platformScope';
import { showAppNotice } from './composables/useAppNotice';
import { when } from './utils/when';
import { useNow } from '@vueuse/core';
import { railOverflow, railSlots } from './shared/railFit';
import { ensureNotificationPermission } from './composables/useNotificationPermission';
import { useAppUpdate } from './composables/useAppUpdate';
import { useCaptureIntake } from './mini-apps/quickcap/useQuickCapWriter';
import { isComposeUrl } from './mini-apps/quickcap/captureUrl';
import { onOpenUrl, getCurrent } from '@tauri-apps/plugin-deep-link';


import DesktopLayout from './layouts/DesktopLayout.vue';
import MobileLayout from './layouts/MobileLayout.vue';
import AskBar from './shared/syn/AskBar.vue';
import AppDialog from './shared/components/AppDialog.vue';
import { captureFocus, buildFocus, focusWithSelection, type SynFocus } from './shared/syn/focus';
import { SYN_ASK, SYN_ASK_ABOUT, type AskAboutDetail } from './shared/syn/selectionAsk';
import { routeQuickQuestion, QUICK_QUESTION_EVENT } from './shared/syn/quickAsk';
import synAvatar from './assets/syn-avatar.jpg';
import { useSynEnabled } from './shared/syn/useSynEnabled';

// Stores
import { useAppStore } from './stores/useAppStore';
import { useNavigationStore, type NavEntry } from './stores/useNavigationStore';
import { useAppLockStore } from './stores/useAppLockStore';
import { storeToRefs } from 'pinia';



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
  showSettingsModal, openSettings, initSettings, applyTheme, defaultApp, hiddenSidebarApps, simpleMode, showE2eeOnboarding, showRecoveryModal
} = useSettings();

const getAppName = (appId: string): string => appName(appId);

/**
 * When the last sync worked, in words ("5 minutes ago"), for the sync button's
 * tooltip. It used to print the stored ISO string. Ticks once a minute so an
 * open tooltip does not go on saying "now" for an hour.
 */
const now = useNow({ interval: 60_000 });
const lastSyncedText = computed(() => appStore.syncLastSuccessful
    ? i18n.global.t('shell.sync.synced_at', { time: when(appStore.syncLastSuccessful, i18n.global.locale.value, now.value.getTime()) })
    : '');

/**
 * The mini-apps this platform ships at all, less the ones simple mode hides.
 *
 * Distinct from what the user chose to hide, and from what fits on screen. An
 * app outside the platform's scope, or outside simple mode while it is on, is
 * not in the product here — it never reaches the bottom bar, the More menu, or
 * the router. See `shared/appAccess.ts`.
 *
 * Note this keys off `isMobileOS` and not `useMobileLayout`: a desktop window
 * dragged narrow adopts the mobile layout, and must keep every app.
 */
const platformApps = computed(() => BUILT_IN_APPS.filter(a => appOffered(a.id, simpleMode.value)));

const mobileVisibleApps = computed(() => {
    return platformApps.value
        .filter(a => !hiddenSidebarApps.value.includes(a.id))
        .slice(0, 4)
        .map(a => a.id);
});

/**
 * The desktop rail's app list, measured, and the apps that do not fit in it.
 * They move to More Apps rather than overflowing the shell — see
 * `shared/railFit.ts` for why the rail does not simply scroll.
 */
const railList = ref<HTMLElement | null>(null);
const railHeight = ref(0);
// A plain ResizeObserver, re-aimed whenever the element is replaced — the same
// `ref` is the phone's bottom bar or the sidebar depending on the layout.
// `useElementSize` was tried first and, after a full reload, reported 0px for a
// sidebar that was 400px tall, so nothing ever moved to More.
let railObserver: ResizeObserver | null = null;
watch(railList, (el) => {
    railObserver?.disconnect();
    railObserver = null;
    railHeight.value = 0;
    if (!el) return;
    railObserver = new ResizeObserver(([entry]) => { railHeight.value = entry.contentRect.height; });
    railObserver.observe(el);
}, { flush: 'post' });
onUnmounted(() => railObserver?.disconnect());
const railOverflowed = computed(() => {
    // Nothing is measured on the first render; better to show every app for
    // one frame than to hide them all.
    if (useMobileLayout.value || railHeight.value <= 0) return [];
    const onRail = platformApps.value.map(a => a.id).filter(id => !hiddenSidebarApps.value.includes(id));
    const moreShown = onRail.length < platformApps.value.length;
    return railOverflow(onRail, railSlots(railHeight.value), moreShown);
});

const isAppVisible = (appId: string) => {
    if (!appOffered(appId, simpleMode.value)) return false;
    if (hiddenSidebarApps.value.includes(appId)) return false;
    if (useMobileLayout.value && !mobileVisibleApps.value.includes(appId)) return false;
    if (railOverflowed.value.includes(appId)) return false;
    return true;
};

const moreMenuApps = computed(() => {
    return platformApps.value.filter(a => {
        const isUserHidden = hiddenSidebarApps.value.includes(a.id);
        const isMobileHidden = useMobileLayout.value && !mobileVisibleApps.value.includes(a.id);
        return isUserHidden || isMobileHidden || railOverflowed.value.includes(a.id);
    });
});

/** Light up More Apps when the app on screen is one of the ones inside it. */
const activeInMore = computed(() => moreMenuApps.value.some(a => a.id === activeTool.value));

// ─── App Lock ─────────────────────────────────────────────
const appLockStore = useAppLockStore();
const currentAppIdRef = computed(() => (route.name as string) || null);
useAppLock(currentAppIdRef); // Activity monitoring + session refresh
const showSetupPinModal = ref(false);
const setupPinMode = ref<'setup' | 'change'>('setup');

const showHiddenAppsMenu = ref(false);

const appStore = useAppStore();
const { vaultPath, vaultType, activeSyncProvider } = storeToRefs(appStore);

const { useMobileLayout, isMobileOS, isMac, initOS } = usePlatform();

// How much the phone's tab bar takes from the bottom, so the toast stack
// (style.css, `.app-toasts`) sits above it rather than on it.
watch(useMobileLayout, (mobile) => {
  document.documentElement.style.setProperty('--bottom-bar', mobile ? '4rem' : '0px');
}, { immediate: true });

/**
 * A rail button's shape. On a phone the icon carries its name underneath —
 * the hover tooltip the desktop uses has no hover to open it there.
 */
const railButtonShape = computed(() => useMobileLayout.value
    ? 'relative group flex-1 min-w-0 h-14 px-0.5 rounded-xl flex flex-col items-center justify-center gap-1 transition-all cursor-pointer'
    : 'relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer');

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

/**
 * Leave an app the moment simple mode hides it.
 *
 * The router guard only runs on the next navigation, so without this somebody
 * who turns simple mode on while in Whiteboard would stay in a Whiteboard that
 * has no sidebar button — the one state the mode exists to prevent. `replace`,
 * so Back does not lead straight back into it.
 */
watch(simpleMode, (on) => {
    const target = redirectTarget(route.name as string | undefined, on, (name) => BUILT_IN_APPS.some(a => a.id === name));
    if (target) router.replace({ name: target }).catch(err => logger.warn('Router navigation error:', err));
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

/**
 * What the open item is *called*, when its path does not say.
 *
 * Notes are files named by uuid in a vault that has been synced, so
 * `Notes/4e0bc181-e384-40d2-….md` names the file and tells nobody which note
 * it is. Only Notes exposes a list to look the title up in; the rest fall back
 * to the path, which for a board or a file is readable anyway.
 */
const getCurrentItemTitle = (): string | undefined => {
    if (activeTool.value !== 'note') return undefined;
    const id = noteAppRef.value?.currentNoteId;
    if (!id) return undefined;
    const note = noteAppRef.value?.notes?.find((n: { id: string }) => n.id === id);
    return note?.title || undefined;
};

/**
 * The article open in Feeds.
 *
 * Not in `getItemIdForApp`: an article is not a vault node, and that function
 * answers with paths the node tools take. Asked to "summarise this" in the
 * reader, Syn was told only that the user was in Feeds, and asked them to open
 * the article they were already reading.
 */
const getCurrentArticle = (): { id: string; title?: string } | undefined => {
    if (activeTool.value !== 'feeds') return undefined;
    return feedsAppRef.value?.currentArticle?.() ?? undefined;
};

/** Where a question is being asked from, for `captureFocus`. */
const askingFrom = (thread?: string) => ({
    app: activeTool.value,
    node: getCurrentItemId(),
    nodeTitle: getCurrentItemTitle(),
    article: getCurrentArticle(),
    thread,
});

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
     // Deliberately *not* marking them read here any more. Entering the app is
     // not reading the notifications: the cards used to be interleaved into the
     // one conversation, so opening the screen did put them in front of
     // somebody. They have their own place now, and it marks them read when it
     // is opened — see `markNotificationsRead` in MessagesApp.
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
const nexusAppRef = ref<any>(null);
const safeAppRef = ref<any>(null);

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
    else if (name === 'nexus') nexusAppRef.value = el;
    else if (name === 'safe') safeAppRef.value = el;
};

// ─── Floating Note (opened in new window) ─────────────────

watch(activeTool, (newTool) => {
    if (newTool === 'task') {
        taskAppRef.value?.refresh?.();
    }
});
const floatingNoteId = ref<string | null>(null);
const isFloatingView = ref(false);


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
let stopOpenUrlListener: (() => void) | null = null;

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

// The Android back button closes the topmost layer rather than the app; see
// useBackGuard for why this cooperates with Tauri's back handling instead of
// replacing it.
//
// Settings, the PIN setup and the sync-conflict notice are `AppDialog`s, which
// register themselves — on the same rule as Escape, so E2EE onboarding, which
// cannot be dismissed at all, is not closed by Back either. Registering them
// here as well would push two history entries per dialog. Only the More menu,
// which is not a dialog, is left to do here.
const hiddenAppsGuard = useBackGuard(showHiddenAppsMenu, () => { showHiddenAppsMenu.value = false; });

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


/**
 * The first-run question, "Simple or Everything", asked once a folder exists.
 *
 * Asked of anyone who has never answered it or touched the setting — which in
 * practice is a new install, since `simpleModeChosen` only reads false before
 * the first answer. Somebody upgrading keeps their vault open and never sees
 * this screen; somebody who switches to a new folder later is asked once.
 *
 * Nothing is preselected. "Everything" is what the app has always been, so it
 * would be the natural default — but a default is a choice made for the people
 * least likely to change it, and they are exactly who simple mode is for.
 */
const choosingMode = ref(false);

/** Which words the mode question uses: a phone does not have every app the desktop names. */
const modeCopy = computed(() => modeChoiceCopy(appInPlatformScope));

/** Back from the mode question to the folder question. */
const backToFolder = () => {
    choosingMode.value = false;
    clearVault();
};

/**
 * Whether startup is still finding out where the folder is.
 *
 * A phone picks its own folder (`resolve_mobile_vault_path`), but that answer
 * arrives after the first paint, and until it does there is no folder — so the
 * welcome screen's "choose a folder" step flashed up for a moment on every
 * first launch, and then vanished. Until the answer is in, the screen stays
 * neutral instead.
 */
const resolvingFolder = ref(true);

const chooseMode = async (simple: boolean) => {
    await appStore.chooseSimpleMode(simple);
    choosingMode.value = false;
    enterVault();
};

/**
 * Open a vault that was just chosen: the start app for the current mode, then
 * a scan so the index is this folder's, then the empty-vault rule.
 */
const enterVault = () => {
    activeTool.value = openingApp.value = startApp({
        defaultApp: defaultApp.value,
        defaultAppChosen: appStore.defaultAppChosen,
        simpleMode: simpleMode.value,
        vaultEmpty: null,
    });
    scanVaultNodes().then(() => landOnQuickCapIfEmpty()).catch(logger.error);
};

/** After a folder is set: ask the mode question, or go straight in. */
const afterVaultChosen = () => {
    if (!appStore.simpleModeChosen) choosingMode.value = true;
    else enterVault();
};

const selectVault = async () => {
    try {
        if (isMobileOS.value) {
            // No directory picker exists on mobile, so the backend decides and
            // reports where the vault lives.
            const resolved = await invoke<string>('resolve_mobile_vault_path');
            await appStore.setVaultPath(resolved, 'local');
            afterVaultChosen();
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(logger.error);

     // Feeds refresh on a timer for as long as the app is running, not only
     // while the Feeds tab happens to be open. Starting it here rather than in
     // the mini-app is the difference between "background refresh" and
     // "refresh whenever you go and look".
     invoke('feed_start_scheduler', { vaultPath: vaultPath.value })
         .catch((e) => logger.error('Failed to start feed scheduler', e));
            return;
        }

        // Rust opens the dialog and the folder, so the vault is the person's
        // answer to a dialog and never a path this window names. See
        // `app_shell::vault`.
        const selected = await invoke<string | null>('pick_vault_folder', {
            title: i18n.global.t('shell.welcome.pick_folder'),
        });
        if (selected) {
            await appStore.setVaultPath(selected, 'local');
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(logger.error);
            afterVaultChosen();
        }
    } catch(err) { logger.error(err); }
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
        // An app the keep-alive dropped (`:max`) leaves its ref pointing at the
        // unmounted instance until the new one mounts; calling into that would
        // be heard by nobody. Wait for the live one instead.
        if (componentRef && !componentRef.$?.isUnmounted && typeof componentRef[method] === 'function') {
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

/**
 * Say why a link goes nowhere, and offer the two ways through.
 *
 * A link into Whiteboard, Files, Safe or Syn used to do nothing at all in
 * simple mode: the router sent the app back to Notes and the item never
 * opened. Now the person hears that the item lives in an app simple mode
 * hides, and can open it this once or turn the mode off.
 */
const offerHiddenApp = (app: string, retry: () => void) => {
    const t = i18n.global.t;
    showAppNotice(t('shell.simple_link.body', { app: appName(app) }), 'info', [
        { label: t('shell.simple_link.open_anyway'), run: () => { simpleModePass.value = app; retry(); } },
        { label: t('shell.simple_link.turn_off'), run: () => { simpleMode.value = false; retry(); } },
    ]);
};

const handleEditFromNexus = async (id: string, type: string, query?: string) => {
    logger.debug(`App.vue: handleEditFromNexus received id: ${id}, type: ${type}`);
    const target = appForOpenType(type);
    if (target && simpleModePass.value !== target && hiddenBySimpleMode(target, simpleMode.value)) {
        offerHiddenApp(target, () => void handleEditFromNexus(id, type, query));
        return;
    }
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
    // A thread opened from Nexus or Things lands in Messages, where every other
    // thing Syn keeps already lives. `syn_thread` is the only type that routes
    // here, so this arm is unambiguous; the day a second one does, the node
    // type has to travel with the id rather than only the route.
    else if (type === 'messages') {
        activeTool.value = 'messages';
        callWhenReady(() => messagesAppRef.value, 'openThread', id);
    }
    // What Syn remembers, and what it knows how to do. Both live in the run
    // inspector, which already lists them — so this opens the panel on the
    // right tab rather than building a third screen for two lists that exist.
    else if (type === 'syn_memory' || type === 'syn_skill') {
        activeTool.value = 'messages';
        callWhenReady(
            () => messagesAppRef.value,
            'openSynItem',
            type === 'syn_memory' ? 'memory' : 'skills',
            id,
        );
    }
    // A moment is a file of its own (`timeline::moments`). Nexus opens it in a
    // sheet of its own; anywhere else — a `type:moment` search, a link — opens
    // the file, where its frontmatter is all there is to read.
    else if (type === 'moment') {
        activeTool.value = 'note';
        callWhenReady(() => noteAppRef.value, 'openNoteById', id);
    }
    // A `synabit://safe/<id>` link in a note. It names the item by id only —
    // the note never holds its title — and Safe opens it, after unlocking if
    // it has to.
    else if (type === 'safe') {
        activeTool.value = 'safe';
        callWhenReady(() => safeAppRef.value, 'openItemById', id);
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
        // A recording cited at a moment arrives as `Files/<hash>.md#t=192,230`.
        const [fileId, fragment] = id.split('#');
        callWhenReady(() => filesAppRef.value, 'openFileById', fileId, false, fragment ?? query);
    }
};

import { logger } from './utils/logger';
import { i18n } from './i18n';
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

/**
 * The app the shell opened on, so the empty-vault rule can tell whether the
 * user has gone anywhere since. Empty once it has had its say.
 */
const openingApp = ref('');

/**
 * Move a new user from the start screen to QuickCap when the vault is empty.
 *
 * The rule is `startApp` in `shared/appAccess.ts`: only when no start app was
 * ever chosen in Settings, and only when the vault holds no notes and no
 * tasks. This is the half that asks the vault.
 *
 * Asked after `scan_all_nodes`, never before: until the scan has run the index
 * need not match the folder — a folder somebody just chose has not been read
 * at all — so an earlier answer could call a full folder empty.
 * On an empty vault the scan is instant, so the move lands before anybody
 * could have started reading Nexus. It never moves somebody who has already
 * gone somewhere else, and it only runs once per start.
 *
 * `get_node_summaries` is the cheapest existing question — there is no count
 * command — and it is only asked of people who never chose a start app.
 */
const landOnQuickCapIfEmpty = async () => {
    const from = openingApp.value;
    openingApp.value = '';
    if (!from || appStore.defaultAppChosen || !vaultPath.value) return;
    if (activeTool.value !== from) return;
    try {
        const [notes, tasks] = await Promise.all([
            invoke<unknown[]>('get_node_summaries', { nodeType: 'note' }),
            invoke<unknown[]>('get_node_summaries', { nodeType: 'task' }),
        ]);
        const target = startApp({
            defaultApp: defaultApp.value,
            defaultAppChosen: appStore.defaultAppChosen,
            simpleMode: simpleMode.value,
            vaultEmpty: notes.length === 0 && tasks.length === 0,
        });
        if (activeTool.value === from && target !== from) activeTool.value = target;
    } catch (e) {
        logger.warn('Could not tell whether the vault is empty', e);
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

// ─── Ask Syn, from wherever you are ───────────────────────
//
// The bar over the work, rather than the app you have to travel to. See
// `shared/syn/AskBar.vue` for why it is a second surface rather than a
// shortcut into Messages.
const askBarOpen = ref(false);
const askFocus = ref<SynFocus | undefined>(undefined);
/**
 * The thread the bar is working in.
 *
 * Held here rather than in the bar, because the piece of work outlives the
 * question: closing the bar and pressing the key again five minutes later is
 * still the same pricing problem. It is cleared only when the user picks
 * "no thread", or when the vault is locked.
 */
const askThread = ref<string | undefined>(undefined);

/**
 * Whether a question is allowed to be asked right now.
 *
 * A lock screen is a promise that what is behind it stays behind it, and Syn
 * reads the whole vault. A bar summoned over the lock would answer questions
 * about protected notes to whoever pressed the key — which is not a smaller
 * hole for being a convenient one. The same goes for a mini-app the user
 * protected individually: routing around that lock through a keyboard shortcut
 * would make the setting mean nothing.
 *
 * And the switch, for a different reason than the locks: those are about who is
 * allowed to ask, this is about whether Syn exists on this vault at all. A bar
 * that opened with Syn off would take a question, send it, and be refused by
 * the backend — which is a worse way to learn about a setting than the bar
 * simply not being there. See `useSynEnabled`.
 */
const { enabled: synEnabled } = useSynEnabled(() => vaultPath.value ?? '');

/** A protected mini-app that has not been unlocked this session. */
const isRouteLocked = (name: string): boolean =>
    appLockStore.isEnabled && appLockStore.isAppProtected(name) && !appLockStore.isMiniAppAccessible(name);

const askBarAllowed = computed(() => {
    if (!vaultPath.value) return false;
    if (!synEnabled.value) return false;
    // Simple mode hides Syn along with its apps. Its buttons went with it, but
    // the key, the selection button and the editor's toolbar all ask this, so
    // this is the one place that has to say no for all of them.
    if (simpleMode.value) return false;
    if (!appLockStore.isEnabled) return true;
    if (appLockStore.isAppLocked) return false;
    return appLockStore.isMiniAppAccessible(activeTool.value);
});

/**
 * Open the bar with whatever is on screen right now.
 *
 * The capture happens *here*, in the keydown handler, and not inside the bar.
 * Showing the bar moves the caret into its textarea, and focusing an input
 * collapses the document selection — so a bar that read the selection itself
 * would read an empty one every single time, while working perfectly in any
 * test that never focused anything.
 */
const openAskBar = () => {
    if (!askBarAllowed.value) return;
    askFocus.value = captureFocus(askingFrom(askThread.value));
    askBarOpen.value = true;
};

// Locking while the bar is open has to put it away, and take the exchange with
// it. A bar that survives the lock is the same hole reached from the other side.
watch(askBarAllowed, (allowed) => {
    if (!allowed) {
        askBarOpen.value = false;
        // The thread goes too. Which piece of work somebody is in the middle of
        // is a fact about them, and it should not be sitting in memory waiting
        // for whoever unlocks the screen next.
        askThread.value = undefined;
        askFocus.value = undefined;
    }
});

/**
 * Switch the thread, and put it into the focus the bar is already holding.
 *
 * Without the second half, picking a thread would do nothing until the bar was
 * closed and reopened — the focus was captured before the choice was made.
 */
const chooseThread = (id: string | undefined) => {
    askThread.value = id;
    askFocus.value = captureFocus(askingFrom(id)) ?? { app: activeTool.value, thread: id };
};

/**
 * Open the bar inside a thread, asked for by the Threads screen.
 *
 * A window event rather than a prop, because mini-apps are mounted generically
 * by the router — the same reason `synabit-navigate` is one. Without it the
 * Threads screen could show the work and offer no way to do any of it.
 */
const onAskInThread = (e: Event) => {
    const id = (e as CustomEvent).detail?.id;
    if (!id || !askBarAllowed.value) return;
    askThread.value = id;
    askFocus.value = captureFocus(askingFrom(id));
    askBarOpen.value = true;
};

const continueInMessages = (conversationId: string) => {
    askBarOpen.value = false;
    activeTool.value = 'messages';
    callWhenReady(() => messagesAppRef.value, 'openConversation', conversationId);
};

// ─── Other ways in than the key ───────────────────────────
//
// Cmd+J is invisible until somebody is told about it, and it does not exist on
// a phone. So the bar can also be opened from a button in the app's own chrome,
// from a button beside a selection, and from the quick-entry box. Every one of
// them goes through `askBarAllowed`, so none of them is a way around a lock or
// the switch.

/**
 * The key, as this platform spells it, for the tooltips that teach it. `null`
 * on a phone, where there is no key to teach.
 */
const askShortcut = computed<string | null>(() =>
    isMobileOS.value ? null : isMac.value ? '⌘J' : 'Ctrl+J',
);

// The selection button in Files and the toolbar in the editor ask this, rather
// than working it out again. See `SYN_ASK`.
provide(SYN_ASK, { allowed: askBarAllowed, shortcut: askShortcut });

/**
 * The chrome button. The same thing as the key, including closing the bar
 * when it is already open — a button that only ever opened would leave a
 * touch screen with no way to put the bar away but the small ✕ inside it.
 */
const toggleAskBar = () => {
    if (askBarOpen.value) askBarOpen.value = false;
    else openAskBar();
};

/**
 * Words to put in the bar as it opens, from the quick-entry box.
 *
 * Cleared as the bar closes, so pressing the key later opens an empty bar
 * rather than the last question from another window.
 */
const askPrefill = ref<string | undefined>(undefined);
watch(askBarOpen, (open) => {
    if (!open) askPrefill.value = undefined;
});

/**
 * "Ask Syn" beside a selection. The text arrives already read — see
 * `focusWithSelection` for why it is not read again here — and everything
 * else about where the person is comes from the same place the key reads it.
 */
const onAskAbout = (e: Event) => {
    const selection = (e as CustomEvent<AskAboutDetail>).detail?.selection;
    if (!selection || !askBarAllowed.value) return;
    askFocus.value = focusWithSelection(askingFrom(askThread.value), selection);
    askBarOpen.value = true;
};

/**
 * Open the bar with a question typed in the quick-entry box.
 *
 * The focus is deliberately *not* read off the screen. The box floats over
 * other applications, so whatever this window happens to be showing is not
 * what the person was looking at when they asked — telling Syn "the user is
 * in Notes, with Pricing open" would be a claim about a screen they could not
 * see. Only the thread travels, because the thread is the piece of work the
 * person chose, not something read off a window.
 *
 * Put in the box rather than sent. The bar is opened over whatever the app was
 * left on, carrying a thread the box never showed; seeing the question sitting
 * there, with what it will carry, costs one Enter — the same call `onChoice`
 * in the bar makes for a question it wrote on the person's behalf.
 */
const askWithQuestion = async (text: string) => {
    if (askBarOpen.value) {
        // A bar already open holds an exchange about something else. Closing it
        // first gives the question a clean one, and lets the bar's own "on
        // open" run again to put the words in.
        askBarOpen.value = false;
        await nextTick();
    }
    askPrefill.value = text;
    askFocus.value = buildFocus({ app: '', thread: askThread.value }, undefined);
    askBarOpen.value = true;
};

/**
 * Collect a question the quick-entry box left in Rust, if there is one and it
 * can be dealt with now. See `routeQuickQuestion` for the three outcomes, and
 * `QuickQuestion` in `capture.rs` for why the words wait there rather than
 * arriving on the event.
 */
const collectQuickQuestion = async () => {
    const route = routeQuickQuestion({
        hasVault: !!vaultPath.value,
        // In simple mode there is no bar to put the question in, and waiting
        // for one would hold it until the mode is turned off. It is kept as a
        // capture instead, the same as with Syn switched off.
        synEnabled: synEnabled.value && !simpleMode.value,
        allowed: askBarAllowed.value,
    });
    if (route === 'wait') return;

    let text: string | null = null;
    try {
        text = await invoke<string | null>('take_quick_question');
    } catch (e) {
        logger.error('[Syn] Could not collect the question from quick entry', e);
        return;
    }
    if (!text) return;

    if (route === 'capture') {
        invoke('queue_capture', { text, source: 'quick-entry' }).catch((e) =>
            logger.error('[Syn] Could not keep an unaskable question as a capture', e),
        );
        return;
    }
    await askWithQuestion(text);
};

// A question typed while the app was locked is asked once it is unlocked —
// the other half of the `wait` route.
watch(askBarAllowed, (allowed) => {
    if (allowed) void collectQuickQuestion();
});
let stopQuickQuestionListener: (() => void) | null = null;

// ─── Keyboard shortcuts for navigation ───────────────────
const handleKeyboardNav = (e: KeyboardEvent) => {
    const isMeta = e.metaKey || e.ctrlKey;
    if (isMeta && e.key === '[') {
        e.preventDefault();
        handleGoBack();
    } else if (isMeta && e.key === ']') {
        e.preventDefault();
        handleGoForward();
    } else if (isMeta && (e.key === 'j' || e.key === 'J')) {
        // Cmd/Ctrl+J. Clear of the neighbours that matter: Cmd+K is a search
        // box in enough apps that taking it would surprise people, and the
        // global capture hotkey already owns Cmd+Shift+Space.
        // Only taken when there is a bar to open: in simple mode, or with Syn
        // off or locked away, the key is left to whatever else wants it.
        if (!askBarOpen.value && !askBarAllowed.value) return;
        e.preventDefault();
        if (askBarOpen.value) askBarOpen.value = false;
        else openAskBar();
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

  // "Let me write something" — from the Android launcher shortcut, as a deep
  // link. The desktop hotkey used to land here too, through a
  // `quickcap:compose` event; it opens the quick-entry window instead now
  // (`surface_quick_entry` in lib.rs), and nothing sends that event any more.
  const openCompose = () => {
    activeTool.value = 'quickcap';
    callWhenReady(() => quickCapAppRef.value, 'focusCompose');
  };

  // Both deep-link paths are needed: `getCurrent` for a cold start, where the
  // URL arrived before anything was listening, and `onOpenUrl` for a shortcut
  // used while the app is already running.
  stopOpenUrlListener = await onOpenUrl((urls) => {
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
  applyTheme();
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  // Capture-phase would fire before the components that handle their own
  // links; `auxclick` is the middle button, which opens a link just as well.
  document.addEventListener('click', followExternalLink);
  document.addEventListener('auxclick', followExternalLink);
  window.addEventListener('keydown', handleKeyboardNav);
  window.addEventListener('syn-ask-in-thread', onAskInThread as EventListener);
  window.addEventListener(SYN_ASK_ABOUT, onAskAbout);
  // After the vault is known, so a question that arrived during startup is
  // routed by the vault's real state rather than by "nothing loaded yet".
  stopQuickQuestionListener = await listen(QUICK_QUESTION_EVENT, () => void collectQuickQuestion());
  void collectQuickQuestion();
  document.addEventListener('visibilitychange', rescanOnResume);

  const params = new URLSearchParams(window.location.search);
  const floatingId = params.get('floatingNote');
  if (floatingId) {
      isFloatingView.value = true;
      floatingNoteId.value = floatingId;
      activeTool.value = 'note';
  } else {
      // Emptiness is not known yet — the scan below has not run — so this is
      // the chosen app or the mode's home. `landOnQuickCapIfEmpty` may move a
      // brand-new user on once the scan has answered.
      activeTool.value = openingApp.value = startApp({
          defaultApp: defaultApp.value,
          defaultAppChosen: appStore.defaultAppChosen,
          simpleMode: simpleMode.value,
          vaultEmpty: null,
      });
  }

  // Runs whether or not a vault is already configured, because an install made
  // by an earlier version has one in app-private storage — invisible to the
  // user and unreachable over USB. The backend moves it and hands back where
  // it ended up; calling this again once it has moved does nothing.
  if (!isMobileOS.value) resolvingFolder.value = false;
  if (isMobileOS.value) {
      try {
          const resolved = await invoke<string>('resolve_mobile_vault_path');
          if (resolved !== vaultPath.value) {
              // No folder before this call is a first run: a phone picks the
              // folder itself, so this is where the welcome screen's mode
              // question gets its turn.
              const firstRun = !vaultPath.value;
              await appStore.setVaultPath(resolved, 'local');
              if (firstRun && !appStore.simpleModeChosen) choosingMode.value = true;
          }
      } catch (e) {
          logger.error('Could not resolve the vault location on this device', e);
      } finally {
          resolvingFolder.value = false;
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
         void landOnQuickCapIfEmpty();
         // Record what changed while the app was closed. See `timeline/ledger.rs`.
         invoke('ledger_sweep', { vaultPath: vaultPath.value }).catch(logger.error);
         readNotesInBackground();
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

  if (activeSyncProvider.value === 'server' && appStore.syncServerAddr) {
      invoke('sync_connect', { serverAddr: appStore.syncServerAddr, serverIdHex: appStore.syncServerIdHex }).catch(logger.error);
  }


  // Anything that creates or retires a cap moves this number.
  bus.on('node:created', () => void refreshQuickCapCount());
  bus.on('node:deleted', () => void refreshQuickCapCount());
  bus.on('vault:sync-completed', () => void refreshQuickCapCount());

  // The evidence ledger looks again once a burst of file changes has settled.
  // Not on every event: a note being typed saves every few seconds, and one
  // record of where it ended up says as much as forty of how it got there.
  // Reads a few settled notes into timeline proposals, when the person turned
// that on. Never on a phone: a model call is battery. Rust refuses the same
// and stays quiet about it. See `timeline/extract.rs`.
let lastBackgroundRead = 0;
const readNotesInBackground = () => {
    if (isMobileOS.value || !vaultPath.value) return;
    // At most every ten minutes. What it writes under `Timeline/` is a file
    // event too, and without this each pass would start the next.
    if (Date.now() - lastBackgroundRead < 10 * 60_000) return;
    lastBackgroundRead = Date.now();
    const vault = vaultPath.value;
    // One after the other: both may use the same local model.
    invoke('timeline_extract_run', { vaultPath: vault, scope: 'new', auto: true, limit: null })
        .catch(logger.error)
        .finally(() => invoke('timeline_media_run', { vaultPath: vault, auto: true, limit: null, locale: i18n.global.locale.value }).catch(logger.error));
};
let ledgerSweepTimer: ReturnType<typeof setTimeout> | undefined;
  const scheduleLedgerSweep = () => {
      clearTimeout(ledgerSweepTimer);
      ledgerSweepTimer = setTimeout(() => {
          if (vaultPath.value) invoke('ledger_sweep', { vaultPath: vaultPath.value }).catch(logger.error);
          readNotesInBackground();
      }, 30_000);
  };

  /** Only the timeline's own files changed: nothing for the ledger or a reading to look at. */
  const onlyTimeline = (payload: any) => {
      const paths = (payload as string[] | undefined) || [];
      return paths.length > 0 && paths.every(p => p.startsWith('Timeline/'));
  };

  bus.on('vault:file-created-deleted', async (payload: any) => {
      if (!onlyTimeline(payload)) scheduleLedgerSweep();
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
      if (!onlyTimeline(payload)) scheduleLedgerSweep();
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

  // Quitting waits for what each app still has to write (see `quit.rs` and
  // `useBeforeQuit`), then says it is done.
  listen('app:before-quit', async () => {
      await runBeforeQuit();
      await invoke('quit_ready').catch(() => {});
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
  stopOpenUrlListener?.();
  window.matchMedia('(prefers-color-scheme: dark)').removeEventListener('change', applyTheme);
  document.removeEventListener('click', followExternalLink);
  document.removeEventListener('auxclick', followExternalLink);
  window.removeEventListener('keydown', handleKeyboardNav);
  window.removeEventListener('syn-ask-in-thread', onAskInThread as EventListener);
  window.removeEventListener(SYN_ASK_ABOUT, onAskAbout);
  stopQuickQuestionListener?.();
  document.removeEventListener('visibilitychange', rescanOnResume);
  destroyEventBus();
  clearInterval(feedsUnreadInterval);
});
</script>

<template>
  <!--
    `syn-pane-open` is what makes room for the browsing pane. See the rule at
    the bottom of this file, and `syn::pane` for why the app draws itself
    narrower instead of the app's webview being moved: on macOS it cannot be
    moved, and `set_bounds` says so by returning `Ok` and doing nothing.
  -->
  <div
    :class="['flex h-screen w-full bg-base text-text dark:bg-base-dark dark:text-text-dark font-sans overflow-hidden select-none', { 'syn-pane-open': synPaneShare > 0 }]"
    :style="synPaneShare > 0 ? { '--syn-pane': `${(synPaneShare * 100).toFixed(4)}%` } : undefined"
  >
    <!--
      The edge between the conversation and the browser, draggable — the same
      thing Notes and Things give their side panels, and the reason this one
      needs its own handle is that the pane is not a DOM element. It is an OS
      webview, so the handle lives here at the app's right edge and the drag is
      sent to `set_bounds`, which *does* move a child webview.

      Sits inside the transformed root, so `right-0` is the app's edge, which is
      exactly the pane's edge. z above everything: it has to be grabbable over
      whatever the mini-app has drawn there.
    -->
    <div
      v-if="synPaneShare > 0"
      class="group fixed top-0 right-0 w-1.5 h-full z-[10000] cursor-col-resize
             hover:bg-black/10 dark:hover:bg-white/10 transition-colors"
      @mousedown.prevent="startPaneDrag"
    >
      <!--
        The way out used to live here, as a button that appeared on hovering the
        edge. It is in the address bar now, where it is always visible and where
        every browser keeps it. A control you have to find by hovering is a
        control somebody in a hurry does not have.
      -->
    </div>


    <!-- ═══ Auto-Update Banner ═══ -->
    <Transition name="slide-down">
      <div v-if="updateAvailable && !updateDownloading"
           class="fixed top-0 left-0 right-0 z-[9999] bg-accent text-white px-4 py-2.5 flex items-center justify-between shadow-lg">
        <div class="flex items-center gap-2.5 min-w-0">
          <svg class="w-4 h-4 flex-shrink-0 animate-bounce" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
          </svg>
          <div class="min-w-0">
            <span class="text-sm font-medium truncate block">{{ $t('update.available', { version: updateVersion }) }}</span>
            <span v-if="updateNotes" class="text-xs text-white/80 truncate block mt-0.5">{{ updateNotes.split('\n')[0] }}</span>
          </div>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <button @click="downloadAndInstall"
                  class="bg-white text-accent px-3 py-1 rounded-md text-xs font-semibold hover:bg-white/90 transition cursor-pointer">
            {{ $t('update.installNow') }}
          </button>
          <button @click="dismissUpdate"
                  class="text-white/80 hover:text-white px-2 py-1 text-xs transition cursor-pointer">
            {{ $t('update.later') }}
          </button>
        </div>
      </div>
    </Transition>

    <!-- ═══ Update Download Progress ═══ -->
    <div v-if="updateDownloading"
         class="fixed top-0 left-0 right-0 z-[9999] bg-accent text-white px-4 py-2.5 shadow-lg">
      <div class="flex items-center justify-between mb-1.5">
        <span class="text-xs font-medium">{{ $t('update.downloading') }}</span>
        <span class="text-xs tabular-nums">{{ updateProgress }}%</span>
      </div>
      <div class="w-full bg-white/30 rounded-full h-1.5">
        <div class="bg-white h-1.5 rounded-full transition-all duration-300 ease-out"
             :style="{ width: updateProgress + '%' }"/>
      </div>
    </div>

    <!-- Application State 0: Initializing -->
    <div v-if="!appStore.isReady || (!vaultPath && resolvingFolder)" class="flex-1 flex flex-col items-center justify-center p-8 bg-base dark:bg-base-dark" data-tauri-drag-region>
      <!-- Neutral while startup finds the folder, rather than flashing the
           "choose a folder" step a phone never needs. -->
      <div v-if="appStore.isReady" role="status" class="flex items-center justify-center">
        <Loader2 class="w-6 h-6 animate-spin text-gray-500 dark:text-gray-400" aria-hidden="true" />
        <span class="sr-only">{{ $t('shell.welcome.loading') }}</span>
      </div>
    </div>

    <!-- Application State 1: No Vault Selected -->
    <div v-else-if="!vaultPath || choosingMode" class="flex-1 flex flex-col items-center justify-center p-8 bg-base dark:bg-base-dark" data-tauri-drag-region>
        <!-- Step 2: simple or everything. Two cards, nothing preselected. -->
        <div v-if="vaultPath" class="max-w-lg w-full text-center space-y-8">
            <div>
               <h1 class="text-2xl font-bold mb-2">{{ $t('shell.welcome.mode_title') }}</h1>
               <p class="text-text-secondary dark:text-text-secondary-dark text-sm">{{ $t('shell.welcome.mode_subtitle') }}</p>
            </div>
            <div class="flex flex-col sm:flex-row gap-4 justify-center" @mousedown.stop>
              <button @click="chooseMode(true)" class="group flex flex-col items-center gap-3 p-6 sm:w-56 rounded-2xl border-2 border-border dark:border-[#333] hover:border-black dark:hover:border-white bg-surface dark:bg-surface-dark transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer">
                <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center group-hover:bg-gray-200 dark:group-hover:bg-gray-700 transition-colors">
                  <FileText class="w-6 h-6 text-gray-600 dark:text-gray-300" />
                </div>
                <div>
                  <p class="font-semibold text-sm">{{ $t(`shell.welcome.mode_simple${modeCopy}`) }}</p>
                  <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">{{ $t(`shell.welcome.mode_simple_desc${modeCopy}`) }}</p>
                </div>
              </button>
              <button @click="chooseMode(false)" class="group flex flex-col items-center gap-3 p-6 sm:w-56 rounded-2xl border-2 border-border dark:border-[#333] hover:border-black dark:hover:border-white bg-surface dark:bg-surface-dark transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer">
                <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center group-hover:bg-gray-200 dark:group-hover:bg-gray-700 transition-colors">
                  <Waypoints class="w-6 h-6 text-gray-600 dark:text-gray-300" />
                </div>
                <div>
                  <p class="font-semibold text-sm">{{ $t('shell.welcome.mode_full') }}</p>
                  <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">{{ $t(`shell.welcome.mode_full_desc${modeCopy}`) }}</p>
                </div>
              </button>
            </div>
            <div @mousedown.stop>
              <button type="button" class="btn-secondary" @click="backToFolder">
                <ArrowLeft class="w-4 h-4" aria-hidden="true" /> {{ $t('shell.welcome.back_to_folder') }}
              </button>
            </div>
        </div>

        <!-- Step 1: where the vault lives. -->
        <div v-else class="max-w-lg w-full text-center space-y-8">
            <div class="w-20 h-20 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mx-auto shadow-inner">
               <FileText class="w-10 h-10 text-gray-500 dark:text-gray-400" />
            </div>
            <div>
               <h1 class="text-2xl font-bold mb-2">{{ $t('shell.welcome.title') }}</h1>
               <p class="text-text-secondary dark:text-text-secondary-dark text-sm">{{ $t('shell.welcome.subtitle') }}</p>
            </div>
            
            <div class="flex gap-4 justify-center" @mousedown.stop>
              <button @click="selectVault" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-border dark:border-[#333] hover:border-black dark:hover:border-white bg-surface dark:bg-surface-dark transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer">
                <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center group-hover:bg-gray-200 dark:group-hover:bg-gray-700 transition-colors">
                  <FolderOpen class="w-6 h-6 text-gray-600 dark:text-gray-300" />
                </div>
                <div>
                  <p class="font-semibold text-sm">{{ $t('shell.welcome.local_folder') }}</p>
                  <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">{{ isMobileOS ? $t('shell.welcome.store_on_device') : $t('shell.welcome.store_on_computer') }}</p>
                </div>
              </button>
              
            </div>
            
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
              <div ref="railList" :class="useMobileLayout ? 'flex justify-around items-center w-full' : 'flex-1 min-h-0 flex flex-col items-center gap-3 mt-4 w-full *:shrink-0'" @mousedown.stop>
                <button v-if="isAppVisible('nexus')" @click="activeTool = 'nexus'" :aria-label="getAppName('nexus')" :aria-current="activeTool === 'nexus' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'nexus' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <!--
                     A globe here and a globe on the browser button were the
                     same picture for two different things. The globe belongs to
                     the browser — it is the web. Nexus is the vault's own graph
                     with a search over it, which is what this draws.
                   -->
                   <Waypoints class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('nexus') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('nexus') }}</span>
                </button>

                <button v-if="isAppVisible('messages')" @click="activeTool = 'messages'" :aria-label="getAppName('messages')" :aria-current="activeTool === 'messages' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'messages' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <MessageCircle class="w-5 h-5" />
                   <div v-if="unreadNotificationCount > 0" class="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 bg-red-500 text-white text-xs font-bold rounded-full flex items-center justify-center ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a] shadow-sm">{{ unreadNotificationCount > 99 ? '99+' : unreadNotificationCount }}</div>
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('messages') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('messages') }}</span>
                </button>

                <button v-if="isAppVisible('quickcap')" @click="activeTool = 'quickcap'" :aria-label="getAppName('quickcap')" :aria-current="activeTool === 'quickcap' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'quickcap' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <!--
                     Caps waiting to be turned into something. Grey rather than
                     red: an inbox with things in it is the normal state, not an
                     alarm, and a colour that shouts gets ignored within a week.
                   -->
                   <span v-if="quickCapCount > 0" class="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 bg-gray-400 dark:bg-gray-600 text-white text-xs font-bold rounded-full flex items-center justify-center ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a] shadow-sm">{{ quickCapCount > 99 ? '99+' : quickCapCount }}</span>
                   <Zap class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('quickcap') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('quickcap') }}</span>
                </button>
                <button v-if="isAppVisible('note')" @click="activeTool = 'note'" :aria-label="getAppName('note')" :aria-current="activeTool === 'note' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'note' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <FileText class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('note') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('note') }}</span>
                </button>
                <button v-if="isAppVisible('task')" @click="activeTool = 'task'" :aria-label="getAppName('task')" :aria-current="activeTool === 'task' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'task' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <CheckSquare class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('task') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('task') }}</span>
                </button>
                <button v-if="isAppVisible('calendar')" @click="activeTool = 'calendar'" :aria-label="getAppName('calendar')" :aria-current="activeTool === 'calendar' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'calendar' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Calendar class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('calendar') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('calendar') }}</span>
                </button>
                <button v-if="isAppVisible('file')" @click="activeTool = 'file'" :aria-label="getAppName('file')" :aria-current="activeTool === 'file' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'file' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <FolderOpen class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('file') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('file') }}</span>
                </button>
                <button v-if="isAppVisible('whiteboard')" @click="activeTool = 'whiteboard'" :aria-label="getAppName('whiteboard')" :aria-current="activeTool === 'whiteboard' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'whiteboard' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Palette class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('whiteboard') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('whiteboard') }}</span>
                </button>
                <button v-if="isAppVisible('people')" @click="activeTool = 'people'" :aria-label="getAppName('people')" :aria-current="activeTool === 'people' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'people' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Users class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('people') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('people') }}</span>
                </button>

                <button v-if="isAppVisible('finance')" @click="activeTool = 'finance'" :aria-label="getAppName('finance')" :aria-current="activeTool === 'finance' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'finance' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Wallet class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('finance') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('finance') }}</span>
                </button>

                <button v-if="isAppVisible('feeds')" @click="activeTool = 'feeds'" :aria-label="getAppName('feeds')" :aria-current="activeTool === 'feeds' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'feeds' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Rss class="w-5 h-5" />
                   <span v-if="feedsUnreadCount > 0" class="absolute -top-1 -right-1 min-w-[18px] h-[18px] bg-orange-700 text-white text-xs font-bold rounded-full flex items-center justify-center px-1 shadow-sm ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a]">{{ feedsUnreadCount > 99 ? '99+' : feedsUnreadCount }}</span>
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('feeds') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('feeds') }}</span>
                </button>


                <button v-if="isAppVisible('things')" @click="activeTool = 'things'" :aria-label="getAppName('things')" :aria-current="activeTool === 'things' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'things' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Boxes class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('things') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('things') }}</span>
                </button>

                <button v-if="isAppVisible('safe')" @click="activeTool = 'safe'" :aria-label="getAppName('safe')" :aria-current="activeTool === 'safe' ? 'page' : undefined" :class="[railButtonShape, activeTool === 'safe' ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <KeyRound class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ getAppName('safe') }}</span>
                   <span v-else class="text-xs leading-none truncate max-w-full">{{ getAppName('safe') }}</span>
                </button>

                <div v-if="moreMenuApps.length > 0" :class="['relative flex justify-center', useMobileLayout && 'flex-1 min-w-0']">
                  <button @click="showHiddenAppsMenu = !showHiddenAppsMenu" :aria-label="$t('shell.nav.more_apps')" :aria-expanded="showHiddenAppsMenu" :aria-current="activeInMore ? 'page' : undefined" :class="[railButtonShape, useMobileLayout && 'w-full', showHiddenAppsMenu || activeInMore ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                    <MoreHorizontal class="w-5 h-5" />
                    <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ $t('shell.nav.more_apps') }}</span>
                    <span v-else class="text-xs leading-none truncate max-w-full">{{ $t('shell.nav.more') }}</span>
                  </button>
                  
                  <!--
                    When the rail is full, More sits at the bottom of it, so its
                    menu opens upwards; otherwise it would run off a short window.
                  -->
                  <!-- Overlay for clicking outside -->
                  <div v-if="showHiddenAppsMenu" class="fixed inset-0 z-40" @click="showHiddenAppsMenu = false"></div>
                  
                  <div v-if="showHiddenAppsMenu" :class="useMobileLayout ? 'absolute bottom-full mb-4 right-0 w-48' : railOverflowed.length ? 'absolute left-full bottom-0 ml-2 w-48' : 'absolute left-full top-0 ml-2 w-48'" class="py-2 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-xl border border-gray-200 dark:border-border-dark z-50 max-h-[60vh] overflow-y-auto">
                    <button v-for="app in moreMenuApps" :key="app.id" @click="openHiddenApp(app.id)" class="w-full flex items-center gap-3 px-4 py-3 text-sm text-text dark:text-text-dark hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors">
                      <component :is="app.icon" class="w-5 h-5 text-gray-500 dark:text-gray-400" />
                      <span class="font-medium">{{ getAppName(app.id) }}</span>
                    </button>
                  </div>
                </div>
                
                <!--
                  Ask Syn, on a phone. The key does not exist here, so without
                  this the bar did not either — on the platform where leaving
                  what you are reading to go and ask costs the most.
                -->
                <button v-if="useMobileLayout && askBarAllowed && !simpleMode" @mousedown.prevent @click="toggleAskBar" :class="[railButtonShape, askBarOpen ? 'bg-accent/10' : 'text-gray-500 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-800']" :aria-label="$t('syn.open_ask_bar')" :aria-pressed="askBarOpen">
                   <img :src="synAvatar" alt="" class="w-6 h-6 rounded-full object-cover" />
                   <span class="text-xs leading-none truncate max-w-full">{{ $t('shell.nav.ask_syn') }}</span>
                </button>

                <button v-if="useMobileLayout" @click="openSettings()" :class="[railButtonShape, showSettingsModal ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']" :aria-label="$t('shell.nav.open_settings')">
                   <Settings class="w-5 h-5" />
                   <span class="text-xs leading-none truncate max-w-full">{{ $t('settings.title') }}</span>
                </button>
             </div>
             
             <!-- Settings & Sync bottom icons for desktop -->
             <div v-if="!useMobileLayout" class="flex-shrink-0 w-full flex flex-col items-center gap-3 mb-2" @mousedown.stop>
                <!--
                  Ask Syn: the same bar Cmd+J opens, for everybody who has not
                  been told about Cmd+J. Down here with the other things that
                  belong to the whole app rather than to one mini-app, and
                  Syn's face rather than a chat bubble, because the bubble
                  above is the Messages app and this is not a way into it.

                  `mousedown.prevent` keeps the selection. Pressing a button
                  moves focus to it, and focus leaving an editor can collapse
                  the text somebody highlighted to ask about — so the button
                  would read an empty selection exactly when it mattered. With
                  the press not taking focus, `openAskBar` reads the screen as
                  it was, the same way the key does.
                -->
                <button
                  v-if="askBarAllowed && !simpleMode"
                  @mousedown.prevent
                  @click="toggleAskBar"
                  :aria-label="$t('syn.open_ask_bar')"
                  :aria-pressed="askBarOpen"
                  :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer',
                           askBarOpen ? 'bg-accent/10' : 'hover:bg-gray-200 dark:hover:bg-gray-800']"
                >
                   <img :src="synAvatar" alt="" class="w-6 h-6 rounded-full object-cover" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ askShortcut ? $t('syn.open_ask_bar_hint', { shortcut: askShortcut }) : $t('syn.open_ask_bar') }}</span>
                </button>

                <!--
                  The browser belongs to the whole app, not to Syn. Syn's header
                  has one too, because that is where somebody asking a question
                  wants it — but a pane opened there and left open has to be
                  reachable from wherever you went next, and the mini-apps are
                  where you went next.

                  Simple mode hides it, as it hides Syn's button above — the
                  shortcut still works for whoever knows it. A pane already open
                  keeps its button, or it would have no way to close.
                -->
                <button
                  v-if="!simpleMode || synPaneShare > 0"
                  @click="synPaneShare > 0 ? closePane() : openBeside(SOMEWHERE_TO_START)"
                  :aria-label="synPaneShare > 0 ? $t('shell.browser.close') : $t('shell.browser.open')"
                  :aria-pressed="synPaneShare > 0"
                  :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer',
                           synPaneShare > 0 ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']"
                >
                   <Globe class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ synPaneShare > 0 ? $t('shell.browser.close') : $t('shell.browser.open') }}</span>
                </button>

                <button v-if="activeSyncProvider === 'server'" @click="syncConflictCount > 0 ? (showSyncConflicts = true) : syncState.sync()" :disabled="syncState.isSyncing.value" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', syncState.syncError.value ? 'text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30' : syncConflictCount > 0 ? 'text-amber-500 hover:bg-amber-100 dark:hover:bg-amber-900/30' : 'text-emerald-500 hover:bg-emerald-100 dark:hover:bg-emerald-900/30']" :title="syncState.isSyncing.value ? $t('shell.sync.syncing') : lastSyncedText || $t('shell.sync.server')">
                   <RefreshCw v-if="syncState.isSyncing.value" class="w-5 h-5 animate-spin" />
                   <Server v-else class="w-5 h-5" />
                   <span v-if="syncConflictCount > 0" class="absolute -top-0.5 -right-0.5 min-w-[16px] h-4 px-1 rounded-full bg-amber-700 text-white text-xs font-bold leading-4 text-center">{{ syncConflictCount }}</span>
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ syncState.isSyncing.value ? $t('shell.sync.syncing') : syncState.syncError.value ? $t('shell.sync.error') : syncConflictCount > 0 ? $t('shell.sync.kept_aside', { count: syncConflictCount }, syncConflictCount) : lastSyncedText || $t('settings.general.sync_now') }}</span>
                </button>

                 <button @click="openSettings()" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showSettingsModal ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Settings class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ $t('settings.title') }}</span>
                </button>
             </div>
          </nav>
        </template>

        <!-- MINI APP CONTENT AREA (Vue Router + KeepAlive) -->
        <div class="flex-1 h-full overflow-hidden relative">
            <router-view v-slot="{ Component, route }">
                <!--
                  Tier 2: Show PIN pad directly for protected mini-apps.

                  Beside the keep-alive, not instead of it. When this was the
                  `v-if` and the keep-alive its `v-else`, opening a locked app
                  unmounted the keep-alive and every app cached in it — each
                  one's open note, scroll and draft gone for a PIN prompt.

                  The locked app itself is not rendered: its `<component>` is
                  `v-if`ed out below, so the keep-alive renders nothing. If it
                  had been opened before, its cached instance is deactivated,
                  which takes its DOM out of the document — nothing of it sits
                  behind the PIN pad to be read or tabbed to.
                -->
                <LockScreen
                    v-if="isRouteLocked(route.name as string)"
                    :title="$t('shell.lock.enter_pin_for_app', { app: getAppName(route.name as string) })"
                    @unlocked="appLockStore.unlockMiniApp(route.name as string)"
                    @cancelled="router.back()"
                />
                <!--
                  Five apps kept warm, the least recently used dropped. Every
                  cached app stays mounted and subscribed to the vault, so
                  without a bound a long session holds all twelve.
                -->
                <keep-alive :max="5">
                    <component 
                        v-if="!isRouteLocked(route.name as string)"
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
        </div>


        <!-- SETTINGS MODAL -->
        <template #modal>
          <SettingsModal
            :vault-path="vaultPath"
            :vault-type="vaultType"
            :active-sync-provider="activeSyncProvider"
            :syncing="syncState.isSyncing.value"
            :sync-error="syncState.syncError.value"
            :last-sync-time="appStore.syncLastSuccessful"
            :auto-sync-enabled="appStore.syncAutoEnabled"
            :auto-sync-interval="appStore.syncAutoInterval"
            :sync-server-addr="appStore.syncServerAddr"
            :sync-server-id-hex="appStore.syncServerIdHex"
            @clear-vault="clearVault"
            @sync-now="syncState.sync()"
            @connect-server="(addr: string, id: string) => { appStore.syncServerAddr = addr; appStore.syncServerIdHex = id; invoke('sync_connect', { serverAddr: addr, serverIdHex: id }).then(() => appStore.activeSyncProvider = 'server').catch(logger.error); }"
            @disconnect-server="() => { invoke('sync_disconnect').then(() => appStore.activeSyncProvider = 'none').catch(logger.error); }"
            @update:auto-sync-enabled="appStore.syncAutoEnabled = $event"
            @update:auto-sync-interval="appStore.syncAutoInterval = $event"
            @show-setup-pin="(mode: 'setup' | 'change') => { setupPinMode = mode; showSetupPinModal = true; }"
          />
        </template>
      </component>

      <Teleport to="#app-toasts">
        <AppNotice />
      </Teleport>
      <DeleteConfirmHost />
    </template>

    <!-- Syn asking for a secret: the value goes from this card to the Safe, never through Syn. -->
    <SafeRequestCard v-if="vaultPath && !isFloatingView" :vault-path="vaultPath" />
    <!-- Safe's SSH agent asking before it signs. -->
    <SshApproveCard v-if="!isFloatingView" />
    <CliApproveCard v-if="!isFloatingView" />

    <!-- E2EE Onboarding Modal -->
    <E2eeOnboarding v-if="showE2eeOnboarding" @done="showE2eeOnboarding = false" />
    
    <RecoveryModal
      :is-open="showRecoveryModal"
      @update:is-open="showRecoveryModal = $event"
    />


    <!-- Tier 1: App Lock Screen -->
    <LockScreen
      v-if="appLockStore.isEnabled && appLockStore.isAppLocked"
      :title="$t('shell.lock.enter_pin_for_app', { app: 'Synabit' })"
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
    <AppDialog
      :show="showSyncConflicts"
      labelledby="sync-conflicts-title"
      unstyled
      @close="showSyncConflicts = false"
    >
      <div class="w-full rounded-2xl bg-white dark:bg-gray-900 shadow-xl border border-amber-200 dark:border-amber-900/50 overflow-hidden">
        <div class="px-5 py-4 border-b border-gray-200 dark:border-gray-800">
          <h2 id="sync-conflicts-title" class="text-base font-semibold text-gray-900 dark:text-gray-100">{{ $t('shell.conflicts.title', { count: syncConflictCount }, syncConflictCount) }}</h2>
          <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
            {{ $t('shell.conflicts.body') }}
          </p>
        </div>
        <ul class="max-h-72 overflow-y-auto px-5 py-3 space-y-3">
          <li v-for="c in syncState.syncConflicts.value" :key="c.kept_as" class="text-sm">
            <div class="text-gray-500 dark:text-gray-400 line-through break-all">{{ c.rel_path }}</div>
            <div class="font-medium text-gray-900 dark:text-gray-100 break-all">{{ c.kept_as }}</div>
          </li>
        </ul>
        <div class="px-5 py-3 bg-gray-50 dark:bg-gray-800/50 flex justify-end">
          <button @click="syncState.dismissConflicts(); showSyncConflicts = false" class="btn-primary">
            {{ $t('shell.conflicts.got_it') }}
          </button>
        </div>
      </div>
    </AppDialog>

    <!-- ═══ Ask Syn (Cmd/Ctrl+J) ═══ -->
    <!--
      Last in the tree and fixed to the bottom, so it sits over whatever screen
      is open rather than inside one of them. It needs a vault: with none
      chosen there is nothing for Syn to read and nothing to save an exchange
      into.
    -->
    <!--
      The browsing pane's address bar.

      Outside the app's root element, and that is not tidiness: the root carries
      a `transform`, which makes it the containing block for every `position:
      fixed` descendant — the same property the 69 overlays rely on to stay
      inside the app's half of the window. A bar written in there would be
      confined to the app's half, which is exactly where the pane is not.

      Its height is `PANE_BAR`, which Rust reserves out of the pane's rectangle.
      Nothing overlaps: the pane is an OS webview and draws over anything this
      app puts in the same place.
    -->
    <div
      v-if="synPaneShare > 0"
      class="fixed top-0 right-0 z-[10001] flex items-center gap-1 px-1.5
             bg-base dark:bg-base-dark border-b border-l border-border dark:border-border-dark"
      :style="{ width: `${(synPaneShare * 100).toFixed(4)}%`, height: `${PANE_BAR}px` }"
    >
      <button
        class="w-7 h-7 shrink-0 rounded flex items-center justify-center cursor-pointer
               text-text/60 dark:text-text-dark/60 hover:bg-black/5 dark:hover:bg-white/10"
        :title="$t('shell.nav.back')"
        :aria-label="$t('shell.nav.back')"
        @click="panePageBack()"
      >
        <ArrowLeft class="w-4 h-4" />
      </button>
      <button
        class="w-7 h-7 shrink-0 rounded flex items-center justify-center cursor-pointer
               text-text/60 dark:text-text-dark/60 hover:bg-black/5 dark:hover:bg-white/10"
        :title="$t('shell.nav.forward')"
        :aria-label="$t('shell.nav.forward')"
        @click="panePageForward()"
      >
        <ArrowRight class="w-4 h-4" />
      </button>
      <!--
        The address, and it is editable. A browser you cannot type an address
        into is a viewer, and the whole reason the jar starts empty is that the
        person is expected to go and log into things in here themselves.

        `title` carries the page's own title: at three hundred pixels the bar
        has room for one line, and an address is the line that can be acted on.
      -->
      <input
        v-model="paneAddress"
        :title="panePage?.title || panePage?.url || ''"
        spellcheck="false"
        class="flex-1 min-w-0 h-7 px-2 rounded text-xs bg-black/5 dark:bg-white/10
               text-text dark:text-text-dark outline-none select-text
               focus:ring-1 focus:ring-accent"
        @focus="paneAddressFocused = true"
        @blur="paneAddressFocused = false; paneAddress = panePage?.url ?? ''"
        @keydown.enter="goToTypedAddress()"
        @keydown.esc="paneAddress = panePage?.url ?? ''; ($event.target as HTMLInputElement).blur()"
      />
      <!--
        The stop button, and it is the one every person already reaches for.
        `browser::CLOSED_ON_IT` is what Syn is told when a read ends this way.
      -->
      <button
        class="w-7 h-7 shrink-0 rounded flex items-center justify-center cursor-pointer
               text-text/60 dark:text-text-dark/60 hover:bg-black/5 dark:hover:bg-white/10"
        :title="$t('shell.browser.close')"
        :aria-label="$t('shell.browser.close')"
        @click="closePane()"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <AskBar
      v-if="askBarAllowed"
      :open="askBarOpen"
      :vault-path="vaultPath"
      :focus="askFocus"
      :prefill="askPrefill"
      @close="askBarOpen = false"
      @open-in-messages="continueInMessages"
      @thread="chooseThread"
    />
</template>

<style>
/*
  Room for the browsing pane, and the whole of the layout half of `syn::pane`.

  Two declarations, and the second is the one that earns its place. `width`
  shrinks what the app draws. `transform` makes this element the **containing
  block for `position: fixed` descendants** — so all 69 `fixed inset-0`
  overlays in this app are measured against *this box* rather than the viewport,
  and stay clear of the pane without one of them being edited.

  Not scoped: `position: fixed` containment is about this element, and the rule
  has to survive whatever the child components do.

  Only while the pane is open. A permanent `transform` on the app root changes
  how every fixed overlay behaves for people who never open a browser, which is
  a large change to make for nothing.
*/
.syn-pane-open {
  width: calc(100% - var(--syn-pane, 0px));
  transform: translateZ(0);
}
</style>

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
