<script setup lang="ts">
/**
 * One piece of open work, in the pane where a conversation would be.
 *
 * # Why this is a pane in Messages and not a screen of its own
 *
 * Everything else Syn keeps is already here: the conversations, the run
 * transcripts, what it remembers, what it knows how to do, what it has been
 * allowed to do. A thread was briefly a thirteenth mini-app with its own
 * sidebar slot, which split one family of features across two entries and spent
 * a permanent slot on a feature one day old.
 *
 * The sidebar already lists things you can open and the pane already shows
 * whichever one you picked. A thread is a second kind of thing in that list,
 * not a second app: *conversations are what was said, threads are what the work
 * is.*
 *
 * # What it deliberately is not
 *
 * Not an editor. The body is Markdown in an ordinary node, which Notes, any
 * text editor and `update_node` all already write; a second full editor here
 * would be a second thing to keep correct. The textarea is for correcting a
 * line without leaving, which is what the skills panel offers for the same
 * reason.
 */
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Loader2, Sparkles, Check } from 'lucide-vue-next';
import { marked } from 'marked';
import DOMPurify from 'dompurify';

import { logger } from '../../../utils/logger';
import { useNodeService } from '../../../composables/useNodeService';
import { THREAD_STATES, type Thread, type ThreadState } from '../../../shared/syn/useThreads';

const props = defineProps<{
  thread: Thread;
  /**
   * What the runs say about this one, when it is known.
   *
   * Shown because the interesting number is not how many times somebody asked
   * inside a thread but how many of those asks left anything behind. A thread
   * Syn reads and never writes to is a thread that quietly stops being worth
   * having, and nothing else on screen would say so.
   */
  usage?: { runs: number; wrote_back: number };
}>();

const emit = defineEmits<{
  /** Whose move it is, or what it is waiting for, has changed. */
  (e: 'move', state: ThreadState, waitingFor: string | undefined): void;
  /** The body was saved and the list should be read again. */
  (e: 'saved'): void;
  /** Open the ask bar inside this thread. */
  (e: 'ask', id: string): void;
}>();

const { t } = useI18n();
const ns = useNodeService();

const editing = ref(false);
const draft = ref('');
const saving = ref(false);

const rendered = computed(() => {
  const body = props.thread.body ?? '';
  if (!body.trim()) return '';
  try {
    return DOMPurify.sanitize(marked.parse(body, { async: false }) as string);
  } catch (e) {
    logger.error('[Syn] Could not render a thread', e);
    return '';
  }
});

const beginEdit = () => {
  draft.value = props.thread.body ?? '';
  editing.value = true;
};

/**
 * Save the body back to the node it is.
 *
 * Through `writeNode`, the path every other screen writes a node by, rather
 * than a command of its own. The frontmatter goes back unchanged: this edits
 * the document, and whose move it is has its own control above.
 */
const saveBody = async () => {
  saving.value = true;
  try {
    await ns.writeNode({
      relPath: props.thread.id,
      nodeType: 'syn_thread',
      title: props.thread.title,
      properties: {
        state: props.thread.state,
        ...(props.thread.waiting_for ? { waiting_for: props.thread.waiting_for } : {}),
      },
      content: draft.value,
    });
    editing.value = false;
    emit('saved');
  } catch (e) {
    logger.error('[Syn] Could not save the thread', e);
  } finally {
    saving.value = false;
  }
};

const onWaitingFor = (value: string) => {
  if ((props.thread.waiting_for ?? '') === value.trim()) return;
  emit('move', props.thread.state, value.trim() || undefined);
};

const when = (iso: string) => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleDateString();
};
</script>

<template>
  <div class="flex-1 min-h-0 flex flex-col">
    <div class="shrink-0 px-6 pt-5 pb-3 border-b border-border dark:border-border-dark">
      <h2 class="text-[19px] font-semibold">{{ thread.title }}</h2>
      <p class="mt-0.5 text-[12px] text-gray-400">
        {{ t('threads.opened', { date: when(thread.opened) }) }} ·
        {{ t('threads.moved', { date: when(thread.last_moved) }) }} ·
        <code class="text-[11px]">{{ thread.id }}</code>
      </p>
      <p v-if="usage" class="mt-0.5 text-[12px]" :class="usage.runs && !usage.wrote_back ? 'text-amber-600 dark:text-amber-500' : 'text-gray-400'">
        {{ t('threads.usage', { runs: usage.runs, wrote: usage.wrote_back }) }}
      </p>

      <!-- Whose move it is. The one field with a closed set of values, and the
           reason threads are grouped rather than tabulated. -->
      <div class="mt-3 flex flex-wrap items-center gap-1.5">
        <button
          v-for="state in THREAD_STATES"
          :key="state"
          class="px-2.5 py-1 rounded-full text-[12px] border transition-colors cursor-pointer"
          :class="state === thread.state
            ? 'bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 border-transparent'
            : 'border-gray-200 dark:border-gray-700 text-gray-600 dark:text-gray-300 hover:bg-black/5 dark:hover:bg-white/10'"
          @click="emit('move', state, thread.waiting_for ?? undefined)"
        >
          <Check v-if="state === thread.state" class="inline w-3 h-3 -mt-0.5 mr-0.5" />
          {{ t(`syn.thread_state_${state}`) }}
        </button>

        <button
          class="ml-auto inline-flex items-center gap-1 px-2.5 py-1 rounded-lg text-[12px] text-indigo-600 dark:text-indigo-400 hover:bg-indigo-50 dark:hover:bg-indigo-500/10 cursor-pointer"
          @click="emit('ask', thread.id)"
        >
          <Sparkles class="w-3.5 h-3.5" />
          {{ t('threads.ask_here') }}
        </button>
      </div>

      <input
        :value="thread.waiting_for ?? ''"
        :placeholder="t('threads.waiting_placeholder')"
        class="mt-3 w-full bg-transparent text-[13px] outline-none placeholder-gray-400 border-b border-transparent focus:border-gray-200 dark:focus:border-gray-700 pb-1"
        @change="onWaitingFor(($event.target as HTMLInputElement).value)"
      />
    </div>

    <div class="flex-1 overflow-y-auto px-6 py-5">
      <template v-if="editing">
        <textarea
          v-model="draft"
          class="w-full h-[55vh] resize-none bg-transparent text-[14px] leading-relaxed font-mono outline-none"
          spellcheck="false"
        ></textarea>
        <div class="flex items-center gap-2 mt-2">
          <button
            class="px-3 py-1.5 rounded-lg bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 text-[13px] disabled:opacity-50 cursor-pointer"
            :disabled="saving"
            @click="saveBody"
          >
            <Loader2 v-if="saving" class="inline w-3.5 h-3.5 animate-spin mr-1" />
            {{ t('threads.save') }}
          </button>
          <button class="px-3 py-1.5 text-[13px] text-gray-500 cursor-pointer" @click="editing = false">
            {{ t('threads.cancel') }}
          </button>
        </div>
      </template>

      <template v-else>
        <div v-if="rendered" class="prose prose-sm dark:prose-invert max-w-none" v-html="rendered"></div>
        <p v-else class="text-[13px] text-gray-400">{{ t('threads.body_empty') }}</p>

        <button
          class="mt-5 text-[12px] text-gray-500 hover:text-gray-800 dark:hover:text-gray-200 cursor-pointer"
          @click="beginEdit"
        >
          {{ t('threads.edit') }}
        </button>
      </template>
    </div>
  </div>
</template>
