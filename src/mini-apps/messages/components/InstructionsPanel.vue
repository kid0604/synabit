<script setup lang="ts">
/**
 * The contract between the two of them, as a screen rather than a field.
 *
 * # Why this exists
 *
 * `instructions.rs` opens by condemning a specific thing: *"the thing that
 * shapes **every answer Syn ever gives** was a field in a JSON blob, reachable
 * only through a textarea in a settings modal."* Moving the storage to
 * `{vault}/SYN.md` fixed where it is kept. It did not fix that sentence, which
 * is about how you reach it — and for a while afterwards there was still
 * exactly one way in, and it was still a textarea in a settings modal.
 *
 * So it gets a row in the sidebar, beside the threads and the conversations,
 * which is where everything else Syn keeps already lives. It is not a setting.
 * It is a document about how two people work together, and it is the only one
 * of them that Syn is not allowed to write.
 *
 * # What the screen has to say that a textarea did not
 *
 * Three things, and each is a thing somebody would otherwise find out the hard
 * way:
 *
 * * **Where the file is.** It syncs, it has version history, it opens in any
 *   editor, and it can be edited on a phone with this app closed. A textarea
 *   implies none of that.
 * * **How much of it Syn actually gets.** `instructions::BUDGET_CHARS` is
 *   4,000, and past that the text is cut. An instruction somebody believes is
 *   in force and is not is worse than no instruction.
 * * **That it costs something on every single turn**, which is the honest
 *   reason not to write an essay here.
 */
import { ref, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { FileText, Save, Check, Loader2 } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';

const props = defineProps<{ vaultPath: string }>();

const { t } = useI18n();

/**
 * What Syn gets before the rest is cut.
 *
 * The same number as `instructions::BUDGET_CHARS`. Duplicated rather than
 * fetched because it is a label on a progress bar and not a decision — the
 * cut happens in Rust either way, and a wrong number here shows a misleading
 * bar rather than sending the wrong text.
 */
const BUDGET = 4000;

const body = ref('');
/** What was last saved, so the button can say whether there is anything to do. */
const saved = ref('');
const path = ref('');
const isLoading = ref(true);
const isSaving = ref(false);
const justSaved = ref(false);

const dirty = computed(() => body.value !== saved.value);
const used = computed(() => body.value.trim().length);
const over = computed(() => used.value > BUDGET);

const load = async () => {
  isLoading.value = true;
  try {
    body.value = await invoke<string>('syn_get_instructions', { vaultPath: props.vaultPath });
    saved.value = body.value;
    path.value = await invoke<string>('syn_instructions_path', { vaultPath: props.vaultPath });
  } catch (e) {
    logger.error('[Syn] Could not read the standing instructions', e);
  } finally {
    isLoading.value = false;
  }
};

const save = async () => {
  isSaving.value = true;
  try {
    await invoke('syn_save_instructions', { vaultPath: props.vaultPath, body: body.value });
    saved.value = body.value;
    justSaved.value = true;
    setTimeout(() => (justSaved.value = false), 2000);
  } catch (e) {
    logger.error('[Syn] Could not write the standing instructions', e);
  } finally {
    isSaving.value = false;
  }
};

/**
 * Put the four-question draft in the box.
 *
 * Fetched rather than duplicated in TypeScript: two copies of a contract drift,
 * and the half the user reads would stop being the half Syn is held to. See
 * `instructions::TEMPLATE`.
 */
const useTemplate = async () => {
  try {
    body.value = await invoke<string>('syn_instructions_template');
  } catch (e) {
    logger.error('[Syn] Could not read the instructions template', e);
  }
};

watch(() => props.vaultPath, load, { immediate: true });
</script>

<template>
  <div class="flex-1 flex flex-col min-h-0">
    <div v-if="isLoading" class="flex-1 flex items-center justify-center">
      <Loader2 class="w-6 h-6 text-violet-500 animate-spin" />
    </div>

    <template v-else>
      <div class="flex-1 overflow-y-auto p-6 space-y-4">
        <p class="text-sm text-gray-500 dark:text-gray-400 leading-relaxed max-w-prose">
          {{ t('syn.instructions_explainer') }}
        </p>

        <!-- Offered only on an empty file: it is a draft to argue with, not a
             contract signed on somebody's behalf. -->
        <button
          v-if="!body.trim()"
          @click="useTemplate"
          class="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs rounded-lg border
                 border-gray-200 dark:border-gray-700/50 text-gray-600 dark:text-gray-300
                 hover:bg-gray-50 dark:hover:bg-white/5 transition-colors cursor-pointer"
        >
          {{ t('syn.instructions_use_template') }}
        </button>

        <textarea
          v-model="body"
          :placeholder="t('syn.instructions_placeholder')"
          class="w-full min-h-[320px] px-3 py-2.5 rounded-xl resize-y
                 bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                 text-[13px] leading-relaxed font-mono text-text dark:text-text-dark
                 placeholder-gray-400 dark:placeholder-gray-500 outline-none
                 focus:border-violet-400 dark:focus:border-violet-500/50
                 focus:ring-1 focus:ring-violet-400/20 transition-all"
        />

        <!-- What Syn actually gets. A silently truncated instruction is one
             somebody believes is in force and is not. -->
        <p class="text-[11px]" :class="over ? 'text-amber-600 dark:text-amber-500' : 'text-gray-400'">
          {{ over
            ? t('syn.instructions_over_budget', { used, budget: BUDGET })
            : t('syn.instructions_budget', { used, budget: BUDGET }) }}
        </p>

        <!-- Where it really is. This screen is a convenience over the file, not
             the place the file lives. -->
        <div
          v-if="path"
          class="flex items-start gap-2 text-[11px] text-gray-400 dark:text-gray-500"
        >
          <FileText class="w-3.5 h-3.5 shrink-0 mt-px" />
          <span class="min-w-0">
            <span class="font-mono break-all">{{ path }}</span>
            <span class="block mt-0.5">{{ t('syn.instructions_where') }}</span>
          </span>
        </div>
      </div>

      <div class="border-t border-gray-100 dark:border-gray-800/60 px-6 py-3 flex items-center gap-3">
        <button
          @click="save"
          :disabled="!dirty || isSaving"
          class="inline-flex items-center gap-1.5 px-3.5 py-2 text-sm font-medium rounded-lg
                 bg-violet-500 text-white hover:bg-violet-600 transition-colors cursor-pointer
                 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <Loader2 v-if="isSaving" class="w-4 h-4 animate-spin" />
          <Check v-else-if="justSaved" class="w-4 h-4" />
          <Save v-else class="w-4 h-4" />
          {{ justSaved ? t('syn.instructions_saved') : t('threads.save') }}
        </button>
        <span v-if="dirty" class="text-[11px] text-gray-400">{{ t('syn.instructions_unsaved') }}</span>
      </div>
    </template>
  </div>
</template>
