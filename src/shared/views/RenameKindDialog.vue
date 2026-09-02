<script setup lang="ts">
/**
 * Renaming a kind, which is saying a different word on every node of it.
 *
 * The operation already existed and was only reachable from a dialog headed
 * "Remove abc?", under the option to move its nodes elsewhere. That is the
 * same act, and it was behind a bin — nobody wanting to correct a typed kind
 * name goes looking under delete. Same command, its own door.
 *
 * Renaming into a name already in use is a merge, and the dialog says so
 * rather than pretending otherwise: `abc` becoming `book` when books exist
 * puts its nodes among them for good. The field rename says the same thing one
 * level down, for the same reason.
 */
import { ref, computed, onMounted, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { ArrowRight, Loader2 } from 'lucide-vue-next';
import { logger } from '../../utils/logger';

const props = defineProps<{
  vaultPath: string;
  nodeType: string;
  /** Kinds that already exist, so a merge can be named as one. */
  candidates: string[];
}>();

const emit = defineEmits<{ done: [to: string]; close: [] }>();

const { t } = useI18n();

const nodes = ref<number | null>(null);
const busy = ref(false);
const target = ref('');
const typing = ref(false);
const fresh = ref('');
const freshBox = ref<HTMLInputElement | null>(null);

onMounted(async () => {
  try {
    const plan = await invoke<{ nodes: number }>('preview_delete_kind', {
      nodeType: props.nodeType,
    });
    nodes.value = plan.nodes;
  } catch (e) {
    logger.error('[Things] Could not count the kind', e);
    emit('close');
  }
});

const isMerge = computed(() => props.candidates.includes(target.value));

const startTyping = async () => {
  typing.value = true;
  target.value = '';
  await nextTick();
  freshBox.value?.focus();
};

/** Lower-cased, so `Book` and `book` never become two kinds. */
const useFresh = () => {
  const name = fresh.value.trim().toLowerCase();
  if (name) target.value = name;
};

const canGo = computed(() => !!target.value && target.value !== props.nodeType);

const apply = async () => {
  if (!canGo.value || busy.value) return;
  busy.value = true;
  try {
    await invoke('retype_kind', {
      vaultPath: props.vaultPath,
      fromType: props.nodeType,
      toType: target.value,
    });
    emit('done', target.value);
  } catch (e) {
    logger.error('[Things] Could not rename the kind', e);
    emit('close');
  } finally {
    busy.value = false;
  }
};
</script>

<template>
  <Teleport to="body">
    <div
      v-if="nodes !== null"
      class="fixed inset-0 z-[10000] flex items-center justify-center p-4 bg-black/40 backdrop-blur-sm"
      @click.self="emit('close')"
    >
      <div
        class="w-full max-w-md rounded-xl shadow-2xl overflow-hidden
               bg-white dark:bg-[#1c1c1e] border border-gray-200 dark:border-gray-700"
      >
        <div class="px-5 py-4 border-b border-gray-100 dark:border-gray-700">
          <h3 class="text-base font-semibold text-text dark:text-text-dark">
            {{ isMerge ? t('things.merge_kind_title') : t('things.rename_kind_title') }}
          </h3>
          <div class="flex items-center gap-2 mt-2 font-mono text-xs">
            <span class="text-gray-500 dark:text-gray-400">{{ nodeType }}</span>
            <ArrowRight class="w-3.5 h-3.5 text-gray-400" />
            <span :class="target ? 'text-text dark:text-text-dark' : 'text-gray-300'">
              {{ target || '…' }}
            </span>
          </div>
        </div>

        <div class="px-5 py-4 max-h-[45vh] overflow-y-auto">
          <p class="text-xs text-gray-400 mb-2">{{ t('things.rename_kind_pick') }}</p>

          <div v-if="typing" class="mb-2">
            <input
              ref="freshBox"
              v-model="fresh"
              spellcheck="false"
              :placeholder="t('things.kind_name_hint')"
              @input="useFresh"
              class="w-full px-2.5 py-1.5 rounded-md font-mono text-xs outline-none
                     bg-gray-50 dark:bg-white/5 border border-blue-300 dark:border-blue-500/40
                     text-text dark:text-text-dark"
            />
          </div>
          <button
            v-else
            type="button"
            @click="startTyping"
            class="w-full text-left px-2.5 py-1.5 mb-2 rounded-md text-xs cursor-pointer
                   text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/5"
          >
            + {{ t('things.rename_new_name') }}
          </button>

          <button
            v-for="kind in candidates"
            :key="kind"
            type="button"
            @click="typing = false; fresh = ''; target = kind"
            class="w-full text-left px-2.5 py-1.5 rounded-md font-mono text-xs cursor-pointer"
            :class="kind === target
              ? 'bg-blue-50 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300'
              : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/5'"
          >
            {{ kind }}
          </button>

          <div v-if="target" class="mt-4 pt-3 border-t border-gray-100 dark:border-gray-700 space-y-1.5">
            <p class="text-xs text-text dark:text-text-dark">
              {{ t('things.rename_kind_count', { n: nodes, type: nodeType }, nodes ?? 0) }}
            </p>
            <!--
              A name already in use makes this a merge, and a merge does not
              come apart again. Said here because the two are one gesture and
              only the destination tells them apart.
            -->
            <p v-if="isMerge" class="text-xs text-amber-700 dark:text-amber-400">
              {{ t('things.merge_kind_because', { type: target }) }}
            </p>
          </div>
        </div>

        <div class="px-5 py-3 flex justify-end gap-2 border-t border-gray-100 dark:border-gray-700">
          <button
            type="button"
            @click="emit('close')"
            class="px-3 py-1.5 rounded-md text-xs text-gray-600 dark:text-gray-300
                   hover:bg-gray-100 dark:hover:bg-white/5 cursor-pointer"
          >
            {{ t('things.cancel') }}
          </button>
          <button
            type="button"
            :disabled="!canGo || busy"
            @click="apply"
            class="px-3 py-1.5 rounded-md text-xs font-medium text-white bg-blue-600
                   hover:bg-blue-700 disabled:opacity-40 disabled:cursor-not-allowed
                   cursor-pointer flex items-center gap-1.5"
          >
            <Loader2 v-if="busy" class="w-3.5 h-3.5 animate-spin" />
            {{ isMerge ? t('things.merge_apply') : t('things.rename_apply') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
