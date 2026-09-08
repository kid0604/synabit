<script setup lang="ts">
/**
 * A line at the bottom of whatever you are doing, that Syn answers into.
 *
 * # Why this exists beside the Messages app
 *
 * Syn was the second of twelve mini-apps, which meant that asking it anything
 * started with leaving whatever you were looking at. That turns every small
 * question into a decision, and small decisions are mostly answered "never
 * mind". The app it was in is the right place for a conversation; it is the
 * wrong place for a question about the paragraph in front of you.
 *
 * So this is the other shape: summoned by a key, sitting over the work rather
 * than replacing it, gone on Escape. It carries what is on screen with the
 * question — see `focus.ts` — which is the whole reason it can be short.
 *
 * # What it deliberately does not do
 *
 * It does not become a chat window. There is no history pane, no model picker,
 * no attachments. The exchange it holds is real and is saved like any other, so
 * "Mở trong Messages" continues it in the place built for that — but the bar
 * itself stays one question wide. Every control added here is a reason to
 * hesitate before pressing the key, and hesitating is the failure it exists to
 * remove.
 */
import { ref, computed, watch, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { useI18n } from 'vue-i18n';
import { X, CornerDownLeft, Loader2, ArrowUpRight, GitBranch, Plus, Check, Zap } from 'lucide-vue-next';

import { logger } from '../../utils/logger';
import { describeFocus, type SynFocus } from './focus';
import { useThreads, openThreads, type Thread } from './useThreads';

const props = defineProps<{
  open: boolean;
  vaultPath: string;
  /**
   * What was on screen when the key was pressed.
   *
   * Passed in rather than read here, and that is load-bearing: focusing this
   * component's textarea collapses the document selection, so by the time this
   * component exists there is nothing left to read. `App.vue` captures it in
   * the keydown handler, before the bar is shown.
   */
  focus?: SynFocus;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  /** Continue this exchange in the Messages app. */
  (e: 'open-in-messages', conversationId: string): void;
  /**
   * The thread this bar is now working in, or `undefined` for none.
   *
   * Raised rather than kept here, because the thread outlives the bar: it is
   * the piece of work, and the bar is one question asked inside it. `App.vue`
   * holds it and puts it back into the focus on the next capture.
   */
  (e: 'thread', id: string | undefined): void;
}>();

const { t } = useI18n();

const question = ref('');
const answer = ref('');
const busy = ref(false);
const working = ref<string | null>(null);
/**
 * How heavy this turn is, once the backend has said.
 *
 * The bar is where a wrong impression costs most: it is summoned mid-sentence,
 * so a spinner that implies work for a count answered from the index is the
 * difference between reaching for it again and not.
 */
const tempo = ref<'instant' | 'working' | null>(null);
const failed = ref<string | null>(null);
const conversationId = ref<string | null>(null);
const inputRef = ref<HTMLTextAreaElement | null>(null);

let stopStream: (() => void) | null = null;
let stopTools: (() => void) | null = null;
let stopTempo: (() => void) | null = null;

const picked = computed(() => describeFocus(props.focus));
const hasAnswer = computed(() => answer.value.length > 0 || busy.value || !!failed.value);

/**
 * Whether there is anywhere else to continue this.
 *
 * Not while the user is already in Messages, which is where the link goes. A
 * way out that leads to the room you are standing in is not a way out — it
 * reads as a broken link, and the first screenshot of this bar working had it
 * offering exactly that.
 */
const canContinueElsewhere = computed(
  () => !!conversationId.value && !busy.value && !!answer.value && props.focus?.app !== 'messages',
);

const rendered = computed(() => {
  if (!answer.value) return '';
  try {
    return DOMPurify.sanitize(marked.parse(answer.value, { async: false }) as string);
  } catch (e) {
    logger.error('[Syn] Could not render the answer', e);
    return '';
  }
});

/**
 * Wipe the exchange and start a new conversation next time.
 *
 * Opening the bar is a new question, not a continuation of one asked an hour
 * ago over a different note. Follow-ups within one opening still work, because
 * the conversation id survives until it closes — "ngắn hơn" has to mean
 * something.
 */
const reset = () => {
  question.value = '';
  answer.value = '';
  working.value = null;
  failed.value = null;
  conversationId.value = null;
  tempo.value = null;
  detach();
};

const detach = () => {
  stopStream?.();
  stopTools?.();
  stopTempo?.();
  stopStream = null;
  stopTools = null;
  stopTempo = null;
};

const close = () => {
  reset();
  emit('close');
};

watch(
  () => props.open,
  async (open) => {
    if (open) {
      await nextTick();
      inputRef.value?.focus();
      // Loaded on open rather than on mount: the bar is mounted for the whole
      // session and reading the vault for a panel nobody has summoned is work
      // nobody asked for.
      void loadThreads();
    } else {
      pickingThread.value = false;
      reset();
    }
  },
);

/** The conversation this exchange belongs to, made on first use. */
const conversation = async (): Promise<string> => {
  if (conversationId.value) return conversationId.value;
  const conv = await invoke<{ id: string }>('syn_create_conversation', {
    vaultPath: props.vaultPath,
    title: t('syn.ask_conversation_title'),
  });
  conversationId.value = conv.id;
  return conv.id;
};

const ask = async () => {
  const text = question.value.trim();
  if (!text || busy.value) return;

  busy.value = true;
  answer.value = '';
  failed.value = null;
  working.value = null;
  tempo.value = null;

  try {
    const id = await conversation();

    // Listened for here rather than in a shared composable because this bar
    // shows one exchange and nothing else: no message list to append to, no
    // ids to match up beyond its own.
    const { listen } = await import('@tauri-apps/api/event');
    detach();
    stopStream = await listen<{ conversation_id: string; token: string; done: boolean }>(
      'syn-stream-token',
      (event) => {
        if (event.payload.conversation_id !== id) return;
        if (event.payload.done) {
          busy.value = false;
          working.value = null;
        } else {
          working.value = null;
          answer.value += event.payload.token;
        }
      },
    );
    stopTempo = await listen<{ conversation_id: string; tempo: 'instant' | 'working' }>(
      'syn-tempo',
      (event) => {
        if (event.payload.conversation_id !== id) return;
        tempo.value = event.payload.tempo;
      },
    );
    stopTools = await listen<{ conversation_id: string; tool_name: string }>(
      'syn-tool-call',
      (event) => {
        if (event.payload.conversation_id !== id) return;
        // One line, replaced each time. A running list of tool names is a
        // transcript, and the transcript already exists in the run panel.
        working.value = event.payload.tool_name;
      },
    );

    question.value = '';
    const reply = await invoke<{ content: string }>('syn_send_message', {
      vaultPath: props.vaultPath,
      request: {
        conversation_id: id,
        message: text,
        focus: props.focus,
      },
    });

    // Ollama cannot stream a turn that used tools, so the streamed text may
    // never have arrived. The returned message is the one that is always right.
    if (reply?.content) answer.value = reply.content;
  } catch (e: unknown) {
    logger.error('[Syn] The ask bar could not get an answer', e);
    failed.value = (e as { message?: string })?.message ?? String(e);
  } finally {
    busy.value = false;
    working.value = null;
  }
};

const stop = async () => {
  try {
    await invoke('syn_stop_generation', { conversationId: conversationId.value ?? undefined });
  } catch (e) {
    logger.error('[Syn] Could not stop', e);
  } finally {
    busy.value = false;
    working.value = null;
  }
};

// ─── The thread this question belongs to ─────────────────────
//
// A picker, not a screen. It exists so that starting a piece of work costs one
// click in the middle of a sentence — which is the whole bet: a thread nobody
// starts is a thread nobody has, and a model asked to notice that some work
// deserves a thread will not, the way `recall` was never called.

const { threads, load: loadThreads, open: startThread } = useThreads(() => props.vaultPath);
const pickingThread = ref(false);
const newThreadTitle = ref('');
/**
 * The thread created in this opening of the picker, if any.
 *
 * Kept only to mark the row, so that "it worked" is visible in the list rather
 * than inferred from the list having changed length.
 */
const justStarted = ref<string | null>(null);

const choices = computed(() => openThreads(threads.value));
const current = computed<Thread | undefined>(() =>
  threads.value.find((t) => t.id === props.focus?.thread),
);

const togglePicker = async () => {
  pickingThread.value = !pickingThread.value;
  if (pickingThread.value) {
    justStarted.value = null;
    if (!threads.value.length) await loadThreads();
  }
};

/**
 * Work in this thread from now on, and put the picker away.
 *
 * Closing is right for *choosing*: the decision is made and the list has done
 * its job. It is wrong for *creating* — see `startAndChoose`.
 */
const choose = (id: string | undefined) => {
  emit('thread', id);
  pickingThread.value = false;
};

/**
 * Start a thread, or switch to the one that name already belongs to.
 *
 * "Start" used to always create. Pressing it twice with the same name left two
 * threads called *General* and then three, indistinguishable in this list, only
 * one of them holding anything — and no way to tell from here which. The name
 * is how a person identifies a piece of work, so a name that is already taken
 * means they are pointing at the work rather than asking for a second one.
 *
 * Only open threads are matched. A name reused a year after that work was
 * closed is a new piece of work, not a resurrection.
 */
const startAndChoose = async () => {
  const title = newThreadTitle.value.trim();
  if (!title) return;

  const existing = choices.value.find(
    (t) => t.title.trim().localeCompare(title, undefined, { sensitivity: 'accent' }) === 0,
  );
  if (existing) {
    newThreadTitle.value = '';
    choose(existing.id);
    return;
  }

  const id = await startThread(title);
  if (id) {
    newThreadTitle.value = '';
    // Selected but *not* closed. Creating used to close the picker, which made
    // the one thing worth seeing — the thread that now exists — the one thing
    // nobody saw: the only other signal was eleven grey pixels in the corner,
    // and somebody watching the field they had just typed in pressed Start
    // twice more and ended up with three threads called General.
    //
    // Left open, the new one appears at the top of the list with a tick beside
    // it. Escape or the button puts it away, which is a decision rather than a
    // side effect.
    emit('thread', id);
    justStarted.value = id;
  }
};

const onKeydown = (event: KeyboardEvent) => {
  if (event.key === 'Escape') {
    event.preventDefault();
    // Escape closes what is open, innermost first: the picker, then a running
    // answer, then the bar. One key that did all three at once would throw away
    // an answer that was most of the way there.
    if (pickingThread.value) pickingThread.value = false;
    else if (busy.value) void stop();
    else close();
    return;
  }
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault();
    void ask();
  }
};
</script>

<template>
  <Transition
    enter-active-class="transition duration-150 ease-out"
    enter-from-class="opacity-0 translate-y-3"
    leave-active-class="transition duration-100 ease-in"
    leave-to-class="opacity-0 translate-y-3"
  >
    <div
      v-if="open"
      class="fixed inset-x-0 bottom-0 z-[70] flex justify-center px-4 pb-5 pointer-events-none"
    >
      <div
        class="pointer-events-auto w-full max-w-2xl rounded-2xl border border-black/10 dark:border-white/10 bg-white/95 dark:bg-[#1c1c1e]/95 backdrop-blur-xl shadow-2xl overflow-hidden"
      >
        <!-- What Syn came back with. Above the input, so the question stays
             where the eye already is. -->
        <div v-if="hasAnswer" class="max-h-[45vh] overflow-y-auto px-5 pt-4">
          <p v-if="failed" class="text-[13px] text-red-500">{{ failed }}</p>

          <div
            v-else-if="rendered"
            class="prose prose-sm dark:prose-invert max-w-none text-[14px] leading-relaxed"
            v-html="rendered"
          ></div>

          <p
            v-else-if="working"
            class="flex items-center gap-2 text-[13px] text-gray-500 dark:text-gray-400"
          >
            <Loader2 class="w-3.5 h-3.5 animate-spin" />
            {{ t('syn.ask_working', { tool: working }) }}
          </p>

          <!-- Answered from the index: one round, no tools. A spinner here
               would imply work that is not happening. -->
          <p
            v-else-if="busy && tempo === 'instant'"
            class="flex items-center gap-2 text-[13px] text-emerald-600 dark:text-emerald-400"
          >
            <Zap class="w-3.5 h-3.5" />
            {{ t('syn.tempo_instant') }}
          </p>

          <p
            v-else-if="busy"
            class="flex items-center gap-2 text-[13px] text-gray-500 dark:text-gray-400"
          >
            <Loader2 class="w-3.5 h-3.5 animate-spin" />
            {{ tempo === 'working' ? t('syn.tempo_working') : t('syn.ask_thinking') }}
          </p>

          <button
            v-if="canContinueElsewhere"
            class="my-3 inline-flex items-center gap-1 text-[12px] text-gray-500 hover:text-gray-800 dark:hover:text-gray-200"
            @click="emit('open-in-messages', conversationId!)"
          >
            {{ t('syn.ask_open_in_messages') }}
            <ArrowUpRight class="w-3.5 h-3.5" />
          </button>
        </div>

        <!-- The thread picker. Above the input so the work is named before the
             question is typed, and closed by default so the bar stays one line. -->
        <div v-if="pickingThread" class="border-t border-black/5 dark:border-white/5 px-4 py-3">
          <div class="flex items-center gap-2 mb-2">
            <input
              v-model="newThreadTitle"
              :placeholder="t('syn.thread_new_placeholder')"
              class="flex-1 bg-transparent text-[13px] outline-none placeholder-gray-400"
              @keydown.enter.prevent="startAndChoose"
              @keydown.stop
            />
            <button
              class="shrink-0 inline-flex items-center gap-1 px-2 py-1 rounded-md text-[12px] text-gray-600 dark:text-gray-300 hover:bg-black/5 dark:hover:bg-white/10 disabled:opacity-40"
              :disabled="!newThreadTitle.trim()"
              @click="startAndChoose"
            >
              <Plus class="w-3.5 h-3.5" />
              {{ t('syn.thread_start') }}
            </button>
          </div>

          <ul class="max-h-40 overflow-y-auto -mx-1">
            <li v-if="props.focus?.thread">
              <button
                class="w-full text-left px-2 py-1.5 rounded-md text-[13px] text-gray-500 hover:bg-black/5 dark:hover:bg-white/10"
                @click="choose(undefined)"
              >
                {{ t('syn.thread_none') }}
              </button>
            </li>
            <li v-for="thread in choices" :key="thread.id">
              <button
                class="w-full flex items-center gap-2 text-left px-2 py-1.5 rounded-md text-[13px] hover:bg-black/5 dark:hover:bg-white/10"
                @click="choose(thread.id)"
              >
                <Check
                  class="w-3.5 h-3.5 shrink-0"
                  :class="thread.id === props.focus?.thread ? 'opacity-100' : 'opacity-0'"
                />
                <span class="truncate">{{ thread.title }}</span>
                <span
                  v-if="thread.id === justStarted"
                  class="shrink-0 text-[11px] text-emerald-600 dark:text-emerald-400"
                >
                  {{ t('syn.thread_just_started') }}
                </span>
                <span class="ml-auto shrink-0 text-[11px] text-gray-400">
                  {{ t(`syn.thread_state_${thread.state}`) }}
                </span>
              </button>
            </li>
            <li
              v-if="!choices.length"
              class="px-2 py-1.5 text-[12px] text-gray-400"
            >
              {{ t('syn.thread_none_yet') }}
            </li>
          </ul>
        </div>

        <div class="flex items-end gap-2 px-4 py-3">
          <textarea
            ref="inputRef"
            v-model="question"
            rows="1"
            :placeholder="t('syn.ask_placeholder')"
            class="flex-1 resize-none bg-transparent text-[15px] leading-relaxed outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 max-h-32"
            spellcheck="false"
            @keydown="onKeydown"
          ></textarea>

          <button
            class="shrink-0 p-1.5 rounded-lg text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10"
            :title="t('syn.ask_close')"
            @click="close"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <!-- What was picked up off the screen. Said out loud, because the
             difference between Syn ignoring the paragraph and never having
             been given it is otherwise invisible. -->
        <div
          class="flex items-center justify-between gap-3 px-5 pb-3 text-[11px] text-gray-400 dark:text-gray-500 select-none"
        >
          <button
            class="shrink-0 inline-flex items-center gap-1 px-1.5 py-0.5 -ml-1.5 rounded-md hover:bg-black/5 dark:hover:bg-white/10"
            :class="current ? 'text-indigo-500 dark:text-indigo-400' : ''"
            :title="t('syn.thread_pick')"
            @click="togglePicker"
          >
            <GitBranch class="w-3 h-3" />
            <span class="max-w-[16ch] truncate">
              {{ current ? current.title : t('syn.thread_none') }}
            </span>
          </button>

          <span class="truncate">
            <template v-if="picked.chars">
              {{ t('syn.ask_sees_selection', { chars: picked.chars }) }}
              <template v-if="picked.node"> · {{ picked.node }}</template>
            </template>
            <template v-else-if="picked.node">
              {{ t('syn.ask_sees_node', { node: picked.node }) }}
            </template>
            <template v-else>{{ t('syn.ask_sees_nothing') }}</template>
          </span>

          <span class="shrink-0 inline-flex items-center gap-1">
            <template v-if="busy">{{ t('syn.ask_esc_stops') }}</template>
            <template v-else>
              <CornerDownLeft class="w-3 h-3" />
              {{ t('syn.ask_enter_sends') }}
            </template>
          </span>
        </div>
      </div>
    </div>
  </Transition>
</template>
