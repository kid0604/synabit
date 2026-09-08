<script setup lang="ts">
/**
 * Everything Syn keeps, in one list.
 *
 * # The three kinds, and why they are not one
 *
 * This used to show a single hardcoded row called "Syn", behind which sat one
 * conversation that grew forever, with the system's own notifications merged
 * into it. Three separate problems wearing one row:
 *
 * * **Conversations were a list of one.** `syn_list_conversations` returns
 *   every conversation in the vault; the screen read `list[0]` and stopped. So
 *   a second one could be created — the ask bar makes a fresh one every time it
 *   opens — and never be opened again. There was no way to start one, either,
 *   which left the first one growing without end.
 * * **Notifications were messages.** A task falling overdue is not something
 *   anybody said, and putting the card in the transcript meant reading a month
 *   of them required scrolling a month of conversation.
 * * **Threads had nowhere to be**, which is what started all this.
 *
 * So: three sections, in the order they ask for attention. Open work first,
 * because a thread waiting on somebody outranks a transcript of a question
 * already answered. Conversations next. Notifications last, gathered rather
 * than interleaved.
 */
import { ref, computed } from 'vue';
import { Search, GitBranch, Plus, MessageSquare, Bell, Trash2, Check, Pencil, ChevronRight, ScrollText } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import NavButtons from '../../../shared/components/NavButtons.vue';
import type { Thread, FootingTally } from '../../../shared/syn/useThreads';
import type { SynConversation } from '../types';
import { grouped, closed, subtitle } from '../threads';

/** What the pane is showing. Explicit, so nothing has to guess it from an id. */
export type Selection =
  | { kind: 'conversation'; id: string }
  | { kind: 'thread'; id: string }
  | { kind: 'notifications' }
  /**
   * `{vault}/SYN.md` — how the two of them work together.
   *
   * A row here rather than a field in the settings modal. `instructions.rs`
   * opens by condemning exactly that arrangement, and moving the storage into
   * a file fixed where the words are kept without fixing how anybody reaches
   * them. This is the other half.
   */
  | { kind: 'instructions' }
  | null;

const props = defineProps<{
  threads: Thread[];
  conversations: SynConversation[];
  selection: Selection;
  /** How many notifications have not been read. */
  unread: number;
  /** Whether Syn's provider is reachable, for the dot beside the name. */
  online: boolean;
  /**
   * Whether any of this is used, across every run on disk.
   *
   * Shown, and not only collected. The two things this app keeps finding are a
   * feature nobody reaches and a measurement nobody looked at — `recall` went
   * uncalled across fifteen runs, the skill detector fired on none of
   * seventeen — and both were invisible until somebody counted. A line saying
   * "0 of 12 wrote anything back" is the earliest possible warning that threads
   * are going the same way.
   */
  stats: { runs_total: number; runs_in_a_thread: number; runs_that_wrote_back: number } | null;
  /**
   * How often Syn was standing on something, across the runs still on disk.
   *
   * On this screen rather than behind the inspector's tabs for the same reason
   * `stats` is: the failure this app keeps repeating is a measurement that gets
   * collected and never looked at, and a number two clicks away is a number
   * nobody looks at. This is the one question `syn::footing` exists to answer —
   * *how often is Syn guessing?* — so it goes where the answer is unavoidable.
   */
  footing: FootingTally | null;
}>();

/**
 * The reading, once there is enough of it to mean anything.
 *
 * Amber when a fifth or more of the answers were standing on nothing. Not a
 * threshold anybody measured — nothing has run for a week yet — so it is a
 * starting guess, and calling it that here is the honest version of picking a
 * number.
 */
const guessRate = computed(() => {
  const t = props.footing;
  if (!t || t.measured < 5) return null;
  return t.guessing / t.measured;
});

const emit = defineEmits<{
  select: [selection: Selection];
  startThread: [title: string];
  closeThread: [id: string];
  reopenThread: [id: string];
  renameThread: [id: string, title: string];
  deleteThread: [id: string];
  newConversation: [];
  deleteConversation: [id: string];
  renameConversation: [id: string, title: string];
}>();

const { t } = useI18n();
const searchQuery = ref('');
const newThreadTitle = ref('');

const matches = (text: string) =>
  !searchQuery.value.trim() || text.toLowerCase().includes(searchQuery.value.trim().toLowerCase());

const openWork = computed(() => grouped(props.threads.filter((thread) => matches(thread.title))));

/**
 * Finished work, folded away.
 *
 * Shown at all because closing is one click and reopening should not mean
 * leaving for Things to find the file. Folded because a list that keeps
 * everything that ever happened stops being a list about now.
 */
const finished = computed(() => closed(props.threads.filter((thread) => matches(thread.title))));
const showClosed = ref(false);

/**
 * What is being renamed, and the text so far.
 *
 * One at a time and shared between the two kinds, because only one thing can be
 * renamed at once and two drafts would need a rule about which one a blur
 * belongs to.
 */
const renaming = ref<{ kind: 'conversation' | 'thread'; id: string } | null>(null);
const renameDraft = ref('');

const beginRename = (kind: 'conversation' | 'thread', id: string, title: string) => {
  renaming.value = { kind, id };
  renameDraft.value = title;
};

const isRenaming = (kind: string, id: string) =>
  renaming.value?.kind === kind && renaming.value.id === id;

const commitRename = () => {
  const at = renaming.value;
  const title = renameDraft.value.trim();
  renaming.value = null;
  if (!at || !title) return;
  if (at.kind === 'conversation') emit('renameConversation', at.id, title);
  else emit('renameThread', at.id, title);
};

const chats = computed(() =>
  props.conversations
    .filter((c) => matches(c.title))
    // The backend already sorts by recency; this only lifts what was pinned.
    .slice()
    .sort((a, b) => Number(b.pinned) - Number(a.pinned)),
);

const isOn = (kind: string, id?: string) =>
  props.selection?.kind === kind
  && (id === undefined || (props.selection as { id?: string }).id === id);

const start = () => {
  const title = newThreadTitle.value.trim();
  if (!title) return;
  emit('startThread', title);
  newThreadTitle.value = '';
};

const when = (iso?: string) => {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  const now = new Date();
  return d.toDateString() === now.toDateString()
    ? d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
    : d.toLocaleDateString([], { month: 'short', day: 'numeric' });
};
</script>

<template>
  <div class="flex flex-col h-full bg-surface dark:bg-surface-dark border-r border-border dark:border-border-dark">
    <div class="h-14 flex items-center gap-3 px-4 flex-shrink-0 border-b border-border dark:border-border-dark" data-tauri-drag-region>
      <NavButtons />
      <h2 class="font-bold text-lg text-text dark:text-text-dark">{{ t('syn.title') }}</h2>
      <span
        class="w-2 h-2 rounded-full flex-shrink-0"
        :class="online ? 'bg-green-500' : 'bg-gray-300 dark:bg-gray-600'"
        :title="online ? t('syn.status_connected') : t('syn.status_disconnected')"
      ></span>
    </div>

    <div class="px-3 py-3 border-b border-border dark:border-border-dark flex-shrink-0">
      <div class="relative">
        <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
          <Search class="w-4 h-4 text-gray-400" />
        </div>
        <input
          v-model="searchQuery"
          type="text"
          class="w-full bg-gray-100 dark:bg-[#1a1a1e] text-sm text-text dark:text-text-dark rounded-xl pl-9 pr-4 py-2 outline-none focus:ring-2 focus:ring-violet-500/50 transition-shadow placeholder-gray-400"
          :placeholder="t('syn.search_placeholder')"
        />
      </div>
    </div>

    <div class="flex-1 overflow-y-auto px-2 py-2">
      <!-- ─── The work that is open ───────────────────────── -->
      <div v-for="group in openWork" :key="group.state">
        <p class="px-2.5 pt-3 pb-1 text-[11px] uppercase tracking-wide text-gray-400">
          {{ t(`syn.thread_state_${group.state}`) }} · {{ group.threads.length }}
        </p>
        <div
          v-for="thread in group.threads"
          :key="thread.id"
          class="w-full flex items-start gap-2.5 px-2.5 py-2 rounded-xl text-left transition-colors cursor-pointer group"
          :class="isOn('thread', thread.id) ? 'bg-violet-50 dark:bg-violet-500/10' : 'hover:bg-gray-100 dark:hover:bg-white/5'"
          @click="emit('select', { kind: 'thread', id: thread.id })"
        >
          <GitBranch class="w-4 h-4 mt-0.5 flex-shrink-0 text-gray-400" />
          <span class="flex-1 min-w-0">
            <input
              v-if="isRenaming('thread', thread.id)"
              v-model="renameDraft"
              class="w-full bg-transparent text-[13px] font-medium outline-none border-b border-violet-400"
              @click.stop
              @keydown.enter.prevent="commitRename"
              @keydown.esc.prevent="renaming = null"
              @blur="commitRename"
            />
            <span v-else class="block text-[13px] font-medium truncate text-gray-900 dark:text-gray-100">{{ thread.title }}</span>
            <span v-if="subtitle(thread)" class="block text-[12px] truncate text-gray-500 dark:text-gray-400">
              {{ subtitle(thread) }}
            </span>
          </span>
          <!--
            Three, and the middle one is the distinction that was missing.
            **Finished** keeps the work and folds it away; **delete** says the
            thread should not have existed. Only the first was offered, so three
            threads made by accident were filed as finished work — people press
            the button that is there.
          -->
          <button
            class="shrink-0 p-1 rounded-md text-gray-300 opacity-0 group-hover:opacity-100 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer"
            :title="t('syn.rename_thread')"
            @click.stop="beginRename('thread', thread.id, thread.title)"
          >
            <Pencil class="w-3.5 h-3.5" />
          </button>
          <button
            class="shrink-0 p-1 rounded-md text-gray-300 opacity-0 group-hover:opacity-100 hover:text-red-500 hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer"
            :title="t('syn.delete_thread')"
            @click.stop="emit('deleteThread', thread.id)"
          >
            <Trash2 class="w-3.5 h-3.5" />
          </button>
          <button
            class="shrink-0 p-1 rounded-md text-gray-300 opacity-0 group-hover:opacity-100 hover:text-emerald-600 hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer"
            :title="t('syn.close_thread')"
            @click.stop="emit('closeThread', thread.id)"
          >
            <Check class="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      <div class="flex items-center gap-1.5 px-2.5 py-2">
        <input
          v-model="newThreadTitle"
          :placeholder="t('threads.new_placeholder')"
          class="flex-1 min-w-0 bg-transparent text-[12px] outline-none placeholder-gray-400"
          @keydown.enter.prevent="start"
        />
        <button
          class="shrink-0 p-1 rounded-md text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 disabled:opacity-30 cursor-pointer"
          :disabled="!newThreadTitle.trim()"
          :title="t('threads.start')"
          @click="start"
        >
          <Plus class="w-3.5 h-3.5" />
        </button>
      </div>

      <!-- ─── Finished work ──────────────────────────────── -->
      <div v-if="finished.length">
        <button
          class="w-full flex items-center gap-1 px-2.5 py-1.5 text-[11px] uppercase tracking-wide text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 cursor-pointer"
          @click="showClosed = !showClosed"
        >
          <ChevronRight class="w-3 h-3 transition-transform" :class="showClosed ? 'rotate-90' : ''" />
          {{ t('syn.thread_state_closed') }} · {{ finished.length }}
        </button>
        <div
          v-for="thread in showClosed ? finished : []"
          :key="thread.id"
          class="w-full flex items-center gap-2.5 px-2.5 py-1.5 rounded-xl text-left transition-colors cursor-pointer group"
          :class="isOn('thread', thread.id) ? 'bg-violet-50 dark:bg-violet-500/10' : 'hover:bg-gray-100 dark:hover:bg-white/5'"
          @click="emit('select', { kind: 'thread', id: thread.id })"
        >
          <span class="flex-1 min-w-0 text-[13px] truncate text-gray-500 dark:text-gray-400">{{ thread.title }}</span>
          <button
            class="shrink-0 p-1 rounded-md text-gray-300 opacity-0 group-hover:opacity-100 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer"
            :title="t('syn.reopen_thread')"
            @click.stop="emit('reopenThread', thread.id)"
          >
            <GitBranch class="w-3.5 h-3.5" />
          </button>
          <button
            class="shrink-0 p-1 rounded-md text-gray-300 opacity-0 group-hover:opacity-100 hover:text-red-500 hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer"
            :title="t('syn.delete_thread')"
            @click.stop="emit('deleteThread', thread.id)"
          >
            <Trash2 class="w-3.5 h-3.5" />
          </button>
        </div>
      </div>

      <!-- What the runs say about all of it. One line, under the work it is
           about, because a number nobody is shown is a number nobody acts on. -->
      <p
        v-if="stats && stats.runs_in_a_thread"
        class="px-2.5 pb-2 text-[11px]"
        :class="stats.runs_that_wrote_back ? 'text-gray-400' : 'text-amber-600 dark:text-amber-500'"
      >
        {{ t('threads.stats', {
          inThread: stats.runs_in_a_thread,
          total: stats.runs_total,
          wrote: stats.runs_that_wrote_back,
        }) }}
      </p>

      <!-- What the answers were standing on. The question `syn::footing`
           exists to answer, on a screen rather than in a field on disk. -->
      <p
        v-if="guessRate !== null && footing"
        class="px-2.5 pb-2 text-[11px]"
        :class="guessRate >= 0.2 ? 'text-amber-600 dark:text-amber-500' : 'text-gray-400'"
        :title="footing.unmeasured
          ? t('syn.footing_tally_unmeasured', { n: footing.unmeasured })
          : undefined"
      >
        {{ t('syn.footing_tally', {
          measured: footing.measured,
          grounded: footing.grounded,
          inferred: footing.inferred,
          guessing: footing.guessing,
        }) }}
      </p>

      <!-- ─── Conversations ──────────────────────────────── -->
      <div class="flex items-center gap-1.5 px-2.5 pt-3 pb-1">
        <p class="text-[11px] uppercase tracking-wide text-gray-400">
          {{ t('syn.conversations') }} · {{ conversations.length }}
        </p>
        <button
          class="ml-auto shrink-0 p-1 rounded-md text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer"
          :title="t('syn.new_conversation')"
          @click="emit('newConversation')"
        >
          <Plus class="w-3.5 h-3.5" />
        </button>
      </div>

      <p v-if="!chats.length" class="px-2.5 py-2 text-[12px] text-gray-400">
        {{ t('syn.no_conversations') }}
      </p>

      <div
        v-for="chat in chats"
        :key="chat.id"
        class="w-full flex items-start gap-2.5 px-2.5 py-2 rounded-xl text-left transition-colors cursor-pointer group"
        :class="isOn('conversation', chat.id) ? 'bg-violet-50 dark:bg-violet-500/10' : 'hover:bg-gray-100 dark:hover:bg-white/5'"
        @click="emit('select', { kind: 'conversation', id: chat.id })"
      >
        <MessageSquare class="w-4 h-4 mt-0.5 flex-shrink-0 text-gray-400" />
        <span class="flex-1 min-w-0">
          <input
            v-if="isRenaming('conversation', chat.id)"
            v-model="renameDraft"
            class="w-full bg-transparent text-[13px] outline-none border-b border-violet-400"
            @click.stop
            @keydown.enter.prevent="commitRename"
            @keydown.esc.prevent="renaming = null"
            @blur="commitRename"
          />
          <span v-else class="block text-[13px] truncate text-gray-900 dark:text-gray-100">{{ chat.title }}</span>
          <span class="block text-[11px] text-gray-400">
            {{ when(chat.updated_at) }} · {{ t('syn.message_count', { n: chat.message_count }) }}
          </span>
        </span>
        <button
          class="shrink-0 p-1 rounded-md text-gray-300 opacity-0 group-hover:opacity-100 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer"
          :title="t('syn.rename_conversation')"
          @click.stop="beginRename('conversation', chat.id, chat.title)"
        >
          <Pencil class="w-3.5 h-3.5" />
        </button>
        <button
          class="shrink-0 p-1 rounded-md text-gray-300 opacity-0 group-hover:opacity-100 hover:text-red-500 hover:bg-black/5 dark:hover:bg-white/10 cursor-pointer"
          :title="t('syn.delete_conversation')"
          @click.stop="emit('deleteConversation', chat.id)"
        >
          <Trash2 class="w-3.5 h-3.5" />
        </button>
      </div>

      <!-- ─── Notifications ──────────────────────────────────
           Their own place rather than merged into a transcript: a task falling
           overdue is not something anybody said. -->
      <button
        class="mt-3 w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-left transition-colors cursor-pointer"
        :class="isOn('notifications') ? 'bg-violet-50 dark:bg-violet-500/10' : 'hover:bg-gray-100 dark:hover:bg-white/5'"
        @click="emit('select', { kind: 'notifications' })"
      >
        <Bell class="w-4 h-4 flex-shrink-0 text-gray-400" />
        <span class="flex-1 text-[13px] text-gray-900 dark:text-gray-100">{{ t('syn.notifications') }}</span>
        <span
          v-if="unread"
          class="shrink-0 min-w-[20px] h-[20px] rounded-full bg-red-500 text-white text-[11px] font-bold flex items-center justify-center px-1.5"
        >
          {{ unread > 99 ? '99+' : unread }}
        </span>
      </button>

      <!-- ─── How we work together ───────────────────────────
           `{vault}/SYN.md`. Last in the list because it is read rarely and
           changed rarely — and here at all because until now the only way to
           it was a textarea inside a settings modal, which is the arrangement
           the file was created to end. -->
      <button
        class="mt-1 w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-left transition-colors cursor-pointer"
        :class="isOn('instructions') ? 'bg-violet-50 dark:bg-violet-500/10' : 'hover:bg-gray-100 dark:hover:bg-white/5'"
        @click="emit('select', { kind: 'instructions' })"
      >
        <ScrollText class="w-4 h-4 flex-shrink-0 text-gray-400" />
        <span class="flex-1 text-[13px] text-gray-900 dark:text-gray-100">{{ t('syn.settings_instructions') }}</span>
      </button>
    </div>
  </div>
</template>
