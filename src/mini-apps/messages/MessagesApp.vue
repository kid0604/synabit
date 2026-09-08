<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed } from 'vue';
import { useSidebarResize } from '../../composables/useSidebarResize';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { routeForNode } from '../../shared/nodeRoutes';
import { openUrl } from '@tauri-apps/plugin-opener';
import { WEB_SOURCE } from './types';
import { Loader2, Settings, Download, ChevronLeft, Zap, ScrollText, GitBranch, PowerOff, Bell, Globe } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import synAvatar from '../../assets/syn-avatar.jpg';

import ChatSidebar, { type Selection } from './components/ChatSidebar.vue';
import ChatPanel from './components/ChatPanel.vue';
import NotificationCard from './components/NotificationCard.vue';
import ModelSelector from './components/ModelSelector.vue';
import SynSettings from './components/SynSettings.vue';
import RunInspector from './components/RunInspector.vue';
import ThreadPanel from './components/ThreadPanel.vue';
import ConfirmModal from '../../shared/components/ConfirmModal.vue';
import InstructionsPanel from './components/InstructionsPanel.vue';

import { useSynChat } from './composables/useSynChat';
import { useSynConsent } from './composables/useSynConsent';
import { useSynChoice } from './composables/useSynChoice';
import { useSynModels } from './composables/useSynModels';
import { useThreads, type ThreadState } from '../../shared/syn/useThreads';
import { useSynEnabled } from '../../shared/syn/useSynEnabled';
import { useNodeService } from '../../composables/useNodeService';
import type { ConsentAnswer, SynConversation, SynConversationFull, SynMessage } from './types';

/**
 * Trying a skill by hand.
 *
 * It writes the request into the composer and stops. The roadmap asks for a
 * skill to be runnable from the screen rather than only when the model picks
 * it, and the reason it gives is the right one: a person needs to see what a
 * skill does before trusting it. Sending on their behalf would be the opposite
 * of that — they would find out what it does by having it done.
 */
const chatPanel = ref<{ prefill: (text: string) => void } | null>(null);

/** The question Syn stopped on, if it has. Shown in the conversation. */
const { pending: consentPending, answer: answerConsent } = useSynConsent(() => props.vaultPath);

/**
 * Answer the permission question, then carry on with what was already asked.
 *
 * Not a new message: nobody typed anything. The run that stopped is over, and
 * the question still on the table is the last one the person actually asked —
 * so the stopped run's id goes back, and the backend takes both the question
 * and **the call that run was about to make** from it. Without that second
 * half, permission granted for one page was followed by a fresh search: see
 * `run::Run::pending_call`.
 *
 * A refusal carries on too. The tool comes back refused, and Syn says so; the
 * alternative is the thing this was written to fix, which is a card that asks,
 * accepts an answer, and then says nothing at all.
 */
const onConsent = async (choice: ConsentAnswer) => {
  // Read before answering: the composable clears the card, and with it the id
  // of the run whose pending call the resume has to pick up.
  const stopped = consentPending.value?.run_id;
  const wasAsked = await answerConsent(choice);
  if (!wasAsked || !stopped) return;

  const id = activeConversationId.value;
  if (!id) return;

  // The stopped run left an assistant turn with no words in it. The backend
  // drops its copy; this drops the one on screen, so the answer replaces it
  // rather than appearing under it.
  const last = activeMessages.value[activeMessages.value.length - 1];
  if (last?.role === 'assistant' && !last.content.trim()) activeMessages.value.pop();

  const response = await sendMessage(
    props.vaultPath,
    id,
    '',
    selectedModel.value || undefined,
    undefined,
    undefined,
    stopped,
  );
  if (response) {
    activeMessages.value.push(response);
    clearStreaming();
  }
};

/**
 * The *which one* Syn stopped on. A different question from consent — see
 * `syn::ambiguity` — and answered a different way: the pick goes into the
 * composer rather than starting anything, so saying *go on* stays the person's
 * move.
 */
const { pending: choicePending, answer: answerChoice } = useSynChoice(() => props.vaultPath);

const onChoice = async (nodeId: string) => {
  const named = await answerChoice(nodeId);
  if (named) chatPanel.value?.prefill(`Cái "${named}".`);
};
const trySkill = (name: string) => {
  chatPanel.value?.prefill(`Dùng skill \`${name}\` giúp tao.`);
};

const props = defineProps<{
  vaultPath: string;
}>();

const emit = defineEmits(['open-node']);
const { t } = useI18n();
const ns = useNodeService();

const handleOpenSource = (source: { id: string; title: string; node_type: string }) => {
  // A page Syn read, not a node. It opens in the user's own browser — the
  // point of a citation is that they can go and check it, and checking it
  // inside the app would be reading Syn's copy rather than the source.
  //
  // `web` is not a real node type: nothing in the vault carries it and no
  // scanner will ever see one. It exists so this chip can tell "open my note"
  // from "open that page", which are different acts behind the same control.
  if (source.node_type === WEB_SOURCE) {
    openUrl(source.id).catch(e => logger.error('[Syn] Could not open that page', e));
    return;
  }

  const route = routeForNode(source.node_type, source.id);
  if (!route) {
    logger.warn(`MessagesApp: no app owns node type '${source.node_type}' (${source.id})`);
    return;
  }
  emit('open-node', source.id, route);
};

const handleNotificationAction = (notification: any) => {
  const targetId = notification.content?.metadata?.target_id;
  if (!targetId) return;

  // `target_type` is written by `chat_engine.rs`. Notifications already sitting
  // in the vault from before it was have only the id, so the path stands in —
  // a task lives under `Tasks/`. What this must not do is fall back to `note`,
  // which is what sent every task and event reminder into the note editor.
  const route = routeForNode(notification.content?.metadata?.target_type, targetId);
  if (!route) {
    logger.warn(`MessagesApp: notification points at '${targetId}' with no usable type`);
    return;
  }
  emit('open-node', targetId, route);
};

// Composables
const {
  streamingContent,
  isStreaming,
  toolCalls,
  tempo,
  error: chatError,
  sendMessage,
  stopGeneration,
  clearStreaming,
} = useSynChat();

const {
  models,
  status,
  selectedModel,
  pullingModel,
  pullProgress,
  pullError,
  checkStatus,
  fetchModels,
  pullModel,
  formatModelSize,
  startPolling,
  stopPolling,
  startHealthCheck,
  stopHealthCheck,
  cleanup: cleanupModels,
} = useSynModels();

// State
/**
 * What the pane is showing.
 *
 * A tagged union rather than one id string. It used to be `activeChatId`, whose
 * only possible values were `'syn-main'` and `null` — and when threads arrived
 * they had to be told apart from a conversation by the shape of the id, which
 * is the kind of rule that works until somebody names something inconveniently.
 */
const selection = ref<Selection>(null);
const activeMessages = ref<SynMessage[]>([]);
const notifications = ref<any[]>([]);
const showSettings = ref(false);
/**
 * The panel that shows what Syn actually did, and what it was actually told.
 *
 * Beside Settings rather than inside it: settings are what you change, and this
 * is what you read when a change did not do what you expected.
 */
const showInspector = ref(false);
const loading = ref(true);

const isMobile = ref(window.innerWidth < 768);

// The hardcoded Syn conversation ID for the backend
const conversations = ref<SynConversation[]>([]);

const activeModelName = computed(() => {
  if (!selectedModel.value) return '';
  const model = models.value.find(m => m.name === selectedModel.value);
  return model ? model.name : selectedModel.value;
});

// Contacts list
/** How many notifications have not been read, for the badge. */
const unreadNotifications = computed(() => notifications.value.filter(n => !n.read_receipt).length);

/** The conversation open right now, if one is. */
const activeConversationId = computed(() =>
  selection.value?.kind === 'conversation' ? selection.value.id : null,
);

const activeConversationTitle = computed(
  () => conversations.value.find(c => c.id === activeConversationId.value)?.title ?? 'Syn',
);

// Load a specific conversation from backend
const loadConversation = async (id: string) => {
  try {
    const full = await invoke<SynConversationFull>('syn_get_conversation', { vaultPath: props.vaultPath, conversationId: id });
    activeMessages.value = full.messages;
    if (full.meta.model) {
      selectedModel.value = full.meta.model;
    }
  } catch (e) {
    logger.error('[Syn] Failed to load conversation', e);
    activeMessages.value = [];
  }
};

/**
 * Mark the notifications read, having actually opened them.
 *
 * It used to happen on entering the app, so a glance at a conversation cleared
 * a badge for cards nobody had seen.
 */
const markNotificationsRead = async () => {
  if (!unreadNotifications.value) return;
  try {
    await invoke('mark_chat_read', { vaultPath: props.vaultPath });
    await fetchNotifications();
  } catch (e) {
    logger.error('[Messages] Failed to mark notifications read', e);
  }
};

const fetchNotifications = async () => {
  try {
    const history = await invoke<any[]>('get_chat_history', { vaultPath: props.vaultPath });
    notifications.value = history;
  } catch (e) {
    logger.error('[Messages] Failed to fetch notifications', e);
  }
};

/**
 * Every conversation in the vault, not just the first one.
 *
 * This used to read `list[0]` and stop, which made the rest unreachable — and
 * the ask bar creates a fresh conversation every time it opens, so "the rest"
 * grew by one on most days. One conversation with no way to start another is
 * also one that grows without end.
 */
const loadConversations = async () => {
  try {
    conversations.value = await invoke<SynConversation[]>('syn_list_conversations', {
      vaultPath: props.vaultPath,
    });
  } catch (e) {
    logger.error('[Syn] Failed to list conversations', e);
  }
};

/** Start one and open it. */
const createConversation = async (): Promise<string | null> => {
  try {
    const conv = await invoke<SynConversation>('syn_create_conversation', {
      vaultPath: props.vaultPath,
      title: t('syn.new_conversation_title'),
    });
    conversations.value = [conv, ...conversations.value];
    activeMessages.value = [];
    clearStreaming();
    selection.value = { kind: 'conversation', id: conv.id };
    return conv.id;
  } catch (e) {
    logger.error('[Syn] Failed to create conversation', e);
    return null;
  }
};

/**
 * Remove one.
 *
 * The transcript of the run behind it stays in `Syn/runs/` either way, so what
 * this deletes is the conversation and not the record of what Syn did.
 */
/**
 * Rename one.
 *
 * Titles are generated from the first exchange, which is a good guess and not
 * always the right name for it afterwards.
 */
const renameConversation = async (id: string, title: string) => {
  try {
    await invoke('syn_rename_conversation', {
      vaultPath: props.vaultPath,
      conversationId: id,
      title,
    });
    conversations.value = conversations.value.map(c => (c.id === id ? { ...c, title } : c));
  } catch (e) {
    logger.error('[Syn] Failed to rename conversation', e);
  }
};

const reallyDeleteConversation = async (id: string) => {
  try {
    await invoke('syn_delete_conversation', { vaultPath: props.vaultPath, conversationId: id });
    conversations.value = conversations.value.filter(c => c.id !== id);
    if (activeConversationId.value === id) {
      selection.value = null;
      activeMessages.value = [];
    }
  } catch (e) {
    logger.error('[Syn] Failed to delete conversation', e);
  }
};

// Send message
const handleSendMessage = async (text: string, images?: string[]) => {
  // Typing into an empty screen starts a conversation rather than refusing.
  let id = activeConversationId.value;
  if (!id) id = await createConversation();
  if (!id) return;

  const cleanText = text
    .replace(/[\u200B-\u200D\uFEFF]/g, '')
    .split('\n')
    .map(l => l.trim().replace(/\s+/g, ' '))
    .filter(l => l.length > 0)
    .filter((line, i, arr) => i === 0 || line.normalize('NFC').toLowerCase() !== arr[i - 1].normalize('NFC').toLowerCase())
    .join('\n');

  if (!cleanText && !images?.length) return;

  const userMessage: SynMessage = {
    id: crypto.randomUUID(),
    role: 'user',
    content: cleanText,
    timestamp: new Date().toISOString(),
    images: images,
  };
  activeMessages.value.push(userMessage);

  const response = await sendMessage(
    props.vaultPath,
    id,
    cleanText,
    selectedModel.value || undefined,
    undefined,
    images
  );

  if (response) {
    activeMessages.value.push(response);
    clearStreaming();
    // The title is generated from the first exchange, and the count changed.
    await loadConversations();
  }
};

/**
 * Open or close the browsing pane beside the conversation.
 *
 * The way in to `syn::pane`, and for now the only one — nothing in the engine
 * reaches for it yet. It exists so the question reading the runtime source
 * could not settle can be answered by looking: does the app's own webview stay
 * where it is put when the window is resized?
 *
 * It opens on a real page rather than a blank one, because a blank pane says
 * nothing about whether text reflows sensibly at that width.
 */
const paneOpen = ref(false);
const togglePane = async () => {
  try {
    if (paneOpen.value) {
      await invoke('syn_pane_close');
      paneOpen.value = false;
    } else {
      await invoke('syn_pane_open', { url: 'https://vnexpress.net/' });
      paneOpen.value = true;
    }
  } catch (e) {
    logger.error('[Syn] The browsing pane would not open', e);
  }
};

/**
 * A sidebar somebody can pull.
 *
 * Through the shared composable rather than a fourth implementation of
 * dragging an edge — Notes lifted it out, Things uses it, and this was the one
 * app left with a hard-coded `w-[320px]`. That is the whole reason it was also
 * the one app whose layout would not stretch.
 *
 * 320 to start, which is where it has always sat, so nothing moves until
 * somebody pulls it.
 */
const sidebar = useSidebarResize({ left: { initial: 320, min: 240, max: 560 } });

onMounted(() => {
  window.addEventListener('mousemove', sidebar.onMouseMove);
  window.addEventListener('mouseup', sidebar.onMouseUp);
});
onUnmounted(() => {
  window.removeEventListener('mousemove', sidebar.onMouseMove);
  window.removeEventListener('mouseup', sidebar.onMouseUp);
});

const handlePullModel = async (name: string) => {
  await pullModel(name, props.vaultPath);
};

const refresh = async () => {
  await checkStatus(props.vaultPath);
  if (status.value.connected) {
    await fetchModels(props.vaultPath);
  }
  await loadConversations();
  await fetchNotifications();
};

const handleRegenerate = async (messageId: string) => {
  if (!activeConversationId.value) return;
  const msgIndex = activeMessages.value.findIndex(m => m.id === messageId);
  if (msgIndex <= 0) return;
  const userMsg = activeMessages.value[msgIndex - 1];
  if (userMsg.role !== 'user') return;

  activeMessages.value.splice(msgIndex, 1);
  await handleSendMessage(userMsg.content, userMsg.images);
};

const handleExportConversation = async () => {
  const id = activeConversationId.value;
  if (!id) return;
  try {
    const markdown = await invoke<string>('syn_export_conversation', {
      vaultPath: props.vaultPath,
      conversationId: id,
    });
    await navigator.clipboard.writeText(markdown);
  } catch (e) {
    logger.error('[Syn] Failed to export conversation', e);
  }
};

/**
 * Whether Syn is switched on for this vault.
 *
 * The screen reads it so it can say *why* the composer is gone; the backend is
 * what actually refuses. See `useSynEnabled`.
 */
const { enabled: synEnabled, refresh: refreshEnabled } = useSynEnabled(() => props.vaultPath);

const handleSettingsSaved = async () => {
  showSettings.value = false;
  await refreshEnabled();
  await checkStatus(props.vaultPath);
  if (status.value.connected) {
    await fetchModels(props.vaultPath);
    startHealthCheck(props.vaultPath);
  } else {
    startPolling(props.vaultPath);
  }
};

/**
 * Open whatever was clicked, whichever of the three kinds it is.
 *
 * Loading is per kind: a conversation is read from disk, a thread is already in
 * hand, and opening the notifications marks them read.
 */
const handleSelect = async (next: Selection) => {
  selection.value = next;
  if (next?.kind === 'conversation') {
    clearStreaming();
    await loadConversation(next.id);
  } else if (next?.kind === 'notifications') {
    await markNotificationsRead();
  }
};

// ─── The work that is open ──────────────────────────────────

const { threads, stats: threadStats, footing: footingTally, load: loadThreads, open: startThread, move: moveThread } =
  useThreads(() => props.vaultPath);

const activeThread = computed(() =>
  selection.value?.kind === 'thread'
    ? threads.value.find((t) => t.id === (selection.value as { id: string }).id)
    : undefined,
);

/**
 * Start one, or open the one that name already belongs to.
 *
 * The same rule the ask bar follows. Pressing this twice with one name left a
 * vault holding `General.md`, `General (1).md` and `General (2).md`, only one
 * of them with anything in it.
 */
const handleStartThread = async (title: string) => {
  const existing = threads.value.find(
    (t) =>
      t.state !== 'closed' &&
      t.title.trim().localeCompare(title, undefined, { sensitivity: 'accent' }) === 0,
  );
  if (existing) {
    selection.value = { kind: 'thread', id: existing.id };
    return;
  }
  const id = await startThread(title);
  if (id) selection.value = { kind: 'thread', id };
};

/**
 * Finish a thread, or pick it up again.
 *
 * `closed` is a state and not a delete: the file stays, Nexus still finds it,
 * and the folded section in the sidebar is there so that closing one by mistake
 * does not mean going to Things to undo it. Reopening restores it to `resting`
 * — nobody's move — because whose turn it was is a fact about a week ago.
 */
const closeThread = async (id: string) => {
  const thread = threads.value.find(t => t.id === id);
  await moveThread(id, 'closed', thread?.waiting_for ?? undefined);
};

const reopenThread = async (id: string) => {
  const thread = threads.value.find(t => t.id === id);
  await moveThread(id, 'resting', thread?.waiting_for ?? undefined);
};

/**
 * Rename a thread — the frontmatter title, not the file.
 *
 * `renameNode` moves the file, and a thread's id *is* its path: it travels into
 * `Focus.thread`, into whatever the ask bar is holding, and across the IPC call
 * with the next question. Moving the file under those would leave references
 * that resolve to nothing — and `thread::get` answers `None` quietly, so the
 * failure would be Syn silently losing the thread rather than saying anything.
 *
 * The vault already treats the frontmatter title as the name: notes here are
 * files named by uuid. A filename that no longer matches its title is normal;
 * an id that points nowhere is not.
 */
const renameThread = async (id: string, title: string) => {
  const thread = threads.value.find(t => t.id === id);
  if (!thread || !title.trim()) return;
  try {
    await ns.writeNode({
      relPath: id,
      nodeType: 'syn_thread',
      title: title.trim(),
      properties: {
        state: thread.state,
        ...(thread.waiting_for ? { waiting_for: thread.waiting_for } : {}),
      },
      content: thread.body,
    });
    await loadThreads();
  } catch (e) {
    logger.error('[Syn] Failed to rename the thread', e);
  }
};

/**
 * Put a thread in the trash.
 *
 * Distinct from closing, and the distinction is the point: **closed** means the
 * work finished, and **deleted** means the thread should not have existed. Only
 * closing was offered, so three threads created by accident were filed as
 * finished work — people press the button that is there.
 *
 * `trash_node_file`, so it is recoverable, like every other node this app
 * removes.
 */
const reallyDeleteThread = async (id: string) => {
  try {
    await ns.trashNode({ relPath: id });
    if (selection.value?.kind === 'thread' && selection.value.id === id) selection.value = null;
    await loadThreads();
  } catch (e) {
    logger.error('[Syn] Failed to trash the thread', e);
  }
};

/**
 * The question in front of both deletions.
 *
 * # Why it is asked at all
 *
 * Both delete buttons sit inside a row, appear on hover, and are a few pixels
 * from the row itself — so the click that removes a month of conversation looks
 * exactly like the click that opens it. Neither asked anything. The i18n file
 * still carried a `delete_conversation_title` key that nothing rendered, which
 * says the confirmation existed once and was lost in a rewrite.
 *
 * # Why one dialog and not two
 *
 * The two deletions are not equally severe, and the dialog says which is which
 * rather than being two components:
 *
 * * A **conversation** is `remove_file` in `Syn/`. No trash, no version
 *   history, nothing to undo — so the copy says *for good*, and says what
 *   survives it (the run transcript, which is the record of what Syn actually
 *   did and is worth knowing is not being destroyed here).
 * * A **thread** is `trash_node_file`, like every other node. The copy says
 *   Trash, because a warning that overstates the damage teaches people to click
 *   through warnings.
 *
 * That difference is the whole reason to name the destination in the message:
 * the same red button doing two different things silently is what made this
 * worth fixing.
 */
const pendingDelete = ref<{ kind: 'conversation' | 'thread'; id: string; title: string } | null>(
  null
);

const askDeleteConversation = (id: string) => {
  const conv = conversations.value.find(c => c.id === id);
  // The backend names every conversation on creation, so a blank one means a
  // rename cleared it. Falling back to the default name keeps the dialog from
  // asking about `""`.
  pendingDelete.value = {
    kind: 'conversation',
    id,
    title: conv?.title?.trim() || t('syn.new_conversation_title'),
  };
};

const askDeleteThread = (id: string) => {
  const thread = threads.value.find(t => t.id === id);
  if (!thread) return;
  pendingDelete.value = { kind: 'thread', id, title: thread.title };
};

const confirmDelete = async () => {
  const pending = pendingDelete.value;
  if (!pending) return;
  // Cleared first: the dialog is answered the moment it is answered, and a
  // modal that lingers over a slow filesystem invites a second click.
  pendingDelete.value = null;
  if (pending.kind === 'conversation') await reallyDeleteConversation(pending.id);
  else await reallyDeleteThread(pending.id);
};

const handleMoveThread = async (state: ThreadState, waitingFor: string | undefined) => {
  if (!activeThread.value) return;
  await moveThread(activeThread.value.id, state, waitingFor);
};

/** The way in from Nexus and from Things. */
const openThread = async (id: string) => {
  if (!threads.value.length) await loadThreads();
  selection.value = { kind: 'thread', id };
};

/** Ask Syn inside this thread — the ask bar belongs to `App.vue`. */
const askInThread = (id: string) => {
  window.dispatchEvent(new CustomEvent('syn-ask-in-thread', { detail: { id } }));
};

watch(() => status.value.connected, (connected, wasConnected) => {
  if (connected && !wasConnected) {
    stopPolling();
    startHealthCheck(props.vaultPath);
  } else if (!connected && wasConnected) {
    stopHealthCheck();
    startPolling(props.vaultPath);
  }
});

onMounted(async () => {
  loading.value = true;
  try {
    // The vault first, and only the vault. Conversations and notifications are
    // files on this machine; everything the screen needs to draw is here.
    await loadConversations();
    await fetchNotifications();
    await loadThreads();

    // Land on the most recent conversation, which is what this screen used to
    // do by having only one. Not on a phone, where the sidebar is the screen.
    if (!isMobile.value && conversations.value.length) {
      await handleSelect({ kind: 'conversation', id: conversations.value[0].id });
    }
  } catch (e) {
    logger.error('[Syn] Failed to initialize', e);
  } finally {
    loading.value = false;
  }

  // Reaching the provider is deliberately *not* awaited before the screen is
  // shown, and deliberately not inside the block above.
  //
  // It used to be first, and the whole app hung behind it: `list_models` was
  // built on the generation client, which waits five minutes, so an endpoint
  // that accepted the connection and went quiet left Messages showing a
  // spinner and "No chat selected" with nothing clickable. The Rust side was
  // idle and a Web Inspector timeline of that state was empty — the main
  // thread was not busy, it was waiting on an IPC call that had not come back.
  //
  // The timeout is fixed too, in `catalogue_client`. The ordering is the part
  // that matters: whether the network answers is not a precondition for
  // reading your own conversations.
  void (async () => {
    try {
      await checkStatus(props.vaultPath);
      if (status.value.connected) {
        await fetchModels(props.vaultPath);
        startHealthCheck(props.vaultPath);
      } else {
        startPolling(props.vaultPath);
      }
    } catch (e) {
      logger.error('[Syn] Could not reach the provider', e);
    }
  })();

  const handleResize = () => {
      isMobile.value = window.innerWidth < 768;
  };
  window.addEventListener('resize', handleResize);

  const handleKeydown = (e: KeyboardEvent) => {
    // The dialog first: Escape dismisses the thing on top, and a question about
    // deleting something is on top of everything else on this screen.
    if (e.key === 'Escape' && pendingDelete.value) {
      pendingDelete.value = null;
      return;
    }
    if (e.key === 'Escape' && isStreaming.value) {
      stopGeneration();
    }
  };
  window.addEventListener('keydown', handleKeydown);
  
  onUnmounted(() => {
    window.removeEventListener('resize', handleResize);
    window.removeEventListener('keydown', handleKeydown);
    cleanupModels();
  });
});

/**
 * Show one conversation, named by its backend id.
 *
 * The way in from the ask bar: an exchange started over a note continues here,
 * in the screen built for a conversation. Without this the bar would be a dead
 * end — the exchange is saved either way, and being unable to reach a saved
 * thing is worse than not saving it.
 */
const openConversation = async (id: string) => {
  if (!conversations.value.some(c => c.id === id)) await loadConversations();
  await handleSelect({ kind: 'conversation', id });
};

/**
 * Open what Syn remembers, or what it knows how to do.
 *
 * The way in from `syn::notice`, and the reason those notices can now carry a
 * link at all: `syn_memory` and `syn_skill` had no route, so a card saying *"I
 * am holding two contradictory things about you"* arrived with nowhere to go
 * and look. Half of noticing pointed at nothing.
 *
 * It opens the inspector rather than a screen of its own, because the inspector
 * already has both lists — a third place to read memories would be a third
 * place for them to disagree.
 */
const openSynItem = (tab: 'memory' | 'skills', id: string) => {
  inspectorTab.value = tab;
  inspectorHighlight.value = id;
  showInspector.value = true;
};

const inspectorTab = ref<'memory' | 'skills' | null>(null);
const inspectorHighlight = ref<string | null>(null);

defineExpose({ refresh, fetchNotifications, openConversation, openThread, openSynItem });
</script>

<template>
  <div class="flex-1 w-full h-full flex bg-gray-50 dark:bg-[#0f1115] text-text dark:text-text-dark relative overflow-hidden">
    
    <!-- Sidebar -->
    <div
        class="flex-shrink-0 h-full border-r border-border dark:border-border-dark z-20 relative"
        :class="[isMobile ? (selection ? 'hidden' : 'w-full') : '', sidebar.isDraggingLeft.value ? '' : 'transition-[width] duration-300']"
        :style="isMobile ? undefined : { width: `${sidebar.leftWidth.value}px` }"
    >
        <!--
          The edge, draggable, the same way Notes and Things have always been.
          This app was the only one where the sidebar was a hard 320px, which is
          why it was the only one that would not stretch.

          Wider than the border it sits on: a hairline target is a target people
          miss. Desktop only — below `md` the sidebar is the whole screen and
          there is no second pane to take width from.
        -->
        <div
          v-if="!isMobile"
          class="hidden md:block absolute top-0 right-0 w-1.5 h-full z-10
                 cursor-col-resize opacity-0 hover:opacity-100 transition-opacity
                 hover:bg-black/10 dark:hover:bg-white/10"
          @mousedown.stop="sidebar.startDragLeft($event)"
        ></div>
        <ChatSidebar
            :threads="threads"
            :conversations="conversations"
            :selection="selection"
            :unread="unreadNotifications"
            :stats="threadStats"
            :footing="footingTally"
            :online="status.connected"
            @select="handleSelect"
            @start-thread="handleStartThread"
            @close-thread="closeThread"
            @reopen-thread="reopenThread"
            @rename-thread="renameThread"
            @delete-thread="askDeleteThread"
            @new-conversation="createConversation"
            @delete-conversation="askDeleteConversation"
            @rename-conversation="renameConversation"
        />
    </div>

    <!-- Main Chat Area -->
    <div 
        class="flex-1 flex flex-col min-w-0 h-full bg-white dark:bg-[#15161a] transition-all duration-300"
        :class="[isMobile ? (selection ? 'w-full block' : 'hidden') : 'block']"
    >
        <!-- Header for Chat -->
        <div class="h-14 border-b border-border dark:border-border-dark flex items-center justify-between px-4 flex-shrink-0 bg-surface dark:bg-surface-dark shadow-sm">
            <!-- Left: Back button (mobile) + Contact info -->
            <div class="flex items-center gap-3">
                <button v-if="isMobile" @click="selection = null" class="p-1.5 -ml-2 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 cursor-pointer" :aria-label="t('syn.back_to_list')">
                    <ChevronLeft class="w-5 h-5" />
                </button>
                
                <template v-if="selection?.kind === 'conversation'">
                    <div class="w-8 h-8 rounded-xl overflow-hidden shadow-sm ring-1 ring-violet-500/30 flex-shrink-0 relative">
                        <img :src="synAvatar" alt="Syn" class="w-full h-full object-cover" />
                        <div v-if="status.connected" class="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 bg-green-500 rounded-full border-2 border-surface dark:border-surface-dark"></div>
                    </div>
                    <div class="flex flex-col justify-center min-w-0">
                        <span class="text-sm font-semibold tracking-tight text-gray-900 dark:text-white leading-tight truncate">{{ activeConversationTitle }}</span>
                        <div class="flex items-center gap-1.5 text-[11px] text-gray-500">
                           <span v-if="status.connected" class="text-green-500 font-medium">Online</span>
                           <span v-else class="text-gray-400">Offline</span>
                           
                           <template v-if="status.connected">
                              <span class="text-gray-300 dark:text-gray-600">·</span>
                              <span class="truncate">{{ activeModelName }}</span>
                           </template>
                        </div>
                    </div>
                </template>
                <template v-else-if="activeThread">
                    <GitBranch class="w-5 h-5 text-gray-400 flex-shrink-0" />
                    <div class="flex flex-col justify-center min-w-0">
                        <span class="text-sm font-semibold tracking-tight text-gray-900 dark:text-white leading-tight truncate">
                            {{ activeThread.title }}
                        </span>
                        <span class="text-[11px] text-gray-500">
                            {{ t(`syn.thread_state_${activeThread.state}`) }}
                        </span>
                    </div>
                </template>
                <template v-else-if="selection?.kind === 'instructions'">
                    <ScrollText class="w-5 h-5 text-gray-400 flex-shrink-0" />
                    <span class="text-sm font-semibold text-gray-900 dark:text-white">{{ t('syn.settings_instructions') }}</span>
                </template>
                <template v-else-if="selection?.kind === 'notifications'">
                    <Bell class="w-5 h-5 text-gray-400 flex-shrink-0" />
                    <span class="text-sm font-semibold text-gray-900 dark:text-white">{{ t('syn.notifications') }}</span>
                </template>
                <template v-else-if="!selection">
                    <span class="font-medium text-gray-400">{{ t('syn.nothing_selected') }}</span>
                </template>
            </div>

            <!-- Right: Actions -->
            <div class="flex items-center gap-1.5" v-if="selection?.kind === 'conversation'">
                
                <ModelSelector
                  v-if="status.connected && models.length > 0"
                  v-model="selectedModel"
                  :models="models"
                  :format-size="formatModelSize"
                  :pulling-model="pullingModel"
                  :pull-progress="pullProgress"
                  :pull-error="pullError"
                  :can-pull-models="status.supports_model_management !== false"
                  @pull-model="handlePullModel"
                />

                <button
                  v-if="activeMessages.length > 0"
                  @click="handleExportConversation"
                  class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 transition-colors cursor-pointer"
                  :title="$t('syn.export')"
                >
                  <Download class="w-4 h-4" />
                </button>

                <!-- The gate for `syn::pane`. Here rather than in devtools
                     because the person who has to answer *does it hold when you
                     resize the window* is the person looking at the screen, and
                     making them paste an invoke to find out puts the friction on
                     the wrong side. See docs/syn-the-pane-2026-09-08.md. -->
                <button
                  @click="togglePane"
                  class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 transition-colors cursor-pointer"
                  :class="paneOpen
                    ? 'text-violet-600 dark:text-violet-400 bg-violet-50 dark:bg-violet-500/10'
                    : 'text-gray-500 dark:text-gray-400'"
                  :title="paneOpen ? t('syn.pane_close') : t('syn.pane_open')"
                >
                  <Globe class="w-4 h-4" />
                </button>

                <button
                  @click="showInspector = !showInspector"
                  class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 transition-colors cursor-pointer"
                  :title="$t('syn.inspector')"
                >
                  <ScrollText class="w-4 h-4" />
                </button>

                <button
                  @click="showSettings = !showSettings"
                  class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-white/5 text-gray-500 dark:text-gray-400 transition-colors cursor-pointer"
                  title="Syn AI Settings"
                >
                  <Settings class="w-4 h-4" />
                </button>
            </div>
        </div>

        <!-- Chat Panel -->
        <div class="flex-1 flex min-h-0 overflow-hidden relative">
            
            <div v-if="loading" class="absolute inset-0 flex items-center justify-center bg-white/50 dark:bg-[#15161a]/50 z-10 backdrop-blur-sm">
                <Loader2 class="w-8 h-8 text-violet-500 animate-spin" />
            </div>

            <!-- Switched off, and saying so where the composer used to be.
                 Above the conversation branch on purpose: the screen still
                 opens, the threads and the transcript are still readable, and
                 the only thing missing is the part that would have been
                 refused. A blank screen or a red error would both be lies
                 about a choice somebody made. -->
            <div
              v-if="!synEnabled"
              class="flex-1 flex flex-col items-center justify-center text-center px-8"
            >
                <div class="w-16 h-16 rounded-3xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center mb-4">
                    <PowerOff class="w-8 h-8 text-gray-400 dark:text-gray-500" />
                </div>
                <p class="text-sm font-medium text-gray-600 dark:text-gray-300">{{ t('syn.syn_off_title') }}</p>
                <p class="mt-2 text-[12px] max-w-sm text-gray-400 dark:text-gray-500 leading-relaxed">
                    {{ t('syn.syn_off_body') }}
                </p>
                <button
                  @click="showSettings = true"
                  class="mt-4 px-3 py-1.5 text-[12px] font-medium rounded-lg border border-gray-200 dark:border-gray-700/50 text-gray-600 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 transition-colors cursor-pointer"
                >
                    {{ t('syn.syn_off_turn_on') }}
                </button>
            </div>
            <template v-else-if="selection?.kind === 'conversation'">
                <ChatPanel
                  ref="chatPanel"
                  :messages="activeMessages"
                  :streaming-content="streamingContent"
                  :is-streaming="isStreaming"
                  :tool-calls="toolCalls"
                  :tempo="tempo"
                  :vault-path="vaultPath"
                  :connection-lost="!status.connected"
                  :chat-error="chatError"
                  :consent-ask="consentPending?.ask ?? null"
                  :choice-ask="choicePending?.choice ?? null"
                  @send="handleSendMessage"
                  @stop="stopGeneration"
                  @open-source="handleOpenSource"
                  @regenerate="handleRegenerate"
                  @notification-action="handleNotificationAction"
                  @consent="onConsent"
                  @choice="onChoice"
                />
            </template>
            <!-- A thread: the other kind of thing this sidebar lists. -->
            <ThreadPanel
              v-else-if="activeThread"
              :thread="activeThread"
              :usage="threadStats?.per_thread[activeThread.id]"
              @move="handleMoveThread"
              @saved="loadThreads"
              @ask="askInThread"
            />
            <!-- How the two of them work together. A screen, because the file
                 it edits was created to stop this being a settings field and
                 for a while it was still only reachable as one. -->
            <InstructionsPanel
              v-else-if="selection?.kind === 'instructions'"
              :vault-path="vaultPath"
            />
            <!-- Notifications, gathered. Reading a month of them no longer means
                 scrolling a month of conversation. -->
            <div v-else-if="selection?.kind === 'notifications'" class="flex-1 overflow-y-auto p-4 space-y-3">
                <p v-if="!notifications.length" class="text-center text-[13px] text-gray-400 py-10">
                    {{ t('syn.no_notifications') }}
                </p>
                <NotificationCard
                    v-for="n in notifications"
                    :key="n.id"
                    :notification="n"
                    @action="handleNotificationAction"
                />
            </div>
            <template v-else>
                <div class="flex-1 flex flex-col items-center justify-center text-gray-400 dark:text-gray-500 px-8 text-center">
                    <div class="w-16 h-16 rounded-3xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center mb-4">
                        <Zap class="w-8 h-8 text-gray-300 dark:text-gray-600" />
                    </div>
                    <p class="text-sm">{{ t('syn.pick_something') }}</p>
                    <p v-if="!threads.length" class="mt-3 text-[12px] max-w-sm">{{ t('threads.empty_body') }}</p>
                </div>
            </template>
        </div>
    </div>

    <!-- What Syn did, and what it was told -->
    <RunInspector
      v-if="showInspector"
      :vault-path="props.vaultPath"
      :initial-tab="inspectorTab"
      :highlight="inspectorHighlight"
      @close="showInspector = false; inspectorTab = null; inspectorHighlight = null"
      @use="trySkill"
    />

    <!-- Asked before anything is removed. The message names where it goes,
         because a conversation goes nowhere and a thread goes to the Trash. -->
    <ConfirmModal
      :show="!!pendingDelete"
      :title="pendingDelete?.kind === 'thread' ? t('syn.delete_thread_title') : t('syn.delete_conversation_title')"
      :message="pendingDelete?.kind === 'thread'
        ? t('syn.delete_thread_body', { title: pendingDelete?.title })
        : t('syn.delete_conversation_body', { title: pendingDelete?.title })"
      :confirm-text="t('syn.delete')"
      :cancel-text="t('syn.cancel')"
      is-destructive
      @confirm="confirmDelete"
      @cancel="pendingDelete = null"
    />

    <!-- Settings Panel -->
    <SynSettings
      v-if="showSettings"
      :vault-path="props.vaultPath"
      :models="models"
      @close="showSettings = false"
      @saved="handleSettingsSaved"
    />
  </div>
</template>
