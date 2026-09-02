<script setup lang="ts">
/**
 * Taking a kind out of the vault, which is two different acts wearing one name.
 *
 * A kind is not stored anywhere to delete. It is the fact that files say
 * `type: x`, so it goes away when they stop saying it — and there are exactly
 * two ways to make them stop: change the word, or remove the files. Those are
 * opposite in consequence, one keeps everything and one ends it, and no button
 * can work out which somebody meant.
 *
 * So the fork is here rather than guessed. Retyping leads because the kind
 * people want gone is usually the one they made by accident — `abc` from a
 * slipped keystroke, with a real note underneath it. Nobody creates a kind by
 * mistake and wants their writing destroyed along with the mistake.
 *
 * A kind nothing carries has no fork: there is only a declaration, and it goes.
 */
import { ref, computed, onMounted, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { ArrowRight, Trash2, Loader2 } from 'lucide-vue-next';
import { logger } from '../../utils/logger';

const props = defineProps<{
  vaultPath: string;
  nodeType: string;
  /** Whether a declared structure exists, which goes either way. */
  declared: boolean;
  /** Kinds this could become, for the retype list. */
  candidates: string[];
}>();

const emit = defineEmits<{ done: []; close: [] }>();

const { t } = useI18n();

const nodes = ref<number | null>(null);
const busy = ref(false);

/** `null` until somebody picks a side; then which side. */
const choice = ref<'retype' | 'delete' | null>(null);
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
    // Nothing carries it: there is no fork, only a declaration to drop.
    // Otherwise the safe outcome is chosen already — it is the one people
    // nearly always mean, and the button still waits on a destination, so
    // nothing is one careless click from happening.
    choice.value = plan.nodes ? 'retype' : 'delete';
  } catch (e) {
    logger.error('[Things] Could not count the kind', e);
    emit('close');
  }
});

const startTyping = async () => {
  typing.value = true;
  target.value = '';
  await nextTick();
  freshBox.value?.focus();
};

const useFresh = () => {
  const name = fresh.value.trim().toLowerCase();
  if (name) target.value = name;
};

const canGo = computed(() => {
  if (choice.value === 'delete') return true;
  return choice.value === 'retype' && !!target.value && target.value !== props.nodeType;
});

const apply = async () => {
  if (!canGo.value || busy.value) return;
  busy.value = true;
  try {
    if (choice.value === 'retype') {
      await invoke('retype_kind', {
        vaultPath: props.vaultPath,
        fromType: props.nodeType,
        toType: target.value,
      });
    } else if (nodes.value) {
      await invoke('delete_kind', {
        vaultPath: props.vaultPath,
        nodeType: props.nodeType,
      });
    }
    emit('done');
  } catch (e) {
    logger.error('[Things] Could not remove the kind', e);
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
            {{ t('things.remove_kind_title', { type: nodeType }) }}
          </h3>
          <p class="text-xs text-gray-500 dark:text-gray-400 mt-1.5 leading-relaxed">
            {{ nodes
              ? t('things.remove_kind_why', { n: nodes, type: nodeType }, nodes)
              : t('things.remove_kind_empty', { type: nodeType }) }}
          </p>
        </div>

        <div v-if="nodes" class="px-5 py-4 space-y-2">
          <!--
            Retyping first. The kind somebody wants gone is usually the one
            they made by accident, and their writing is underneath it.
          -->
          <button
            type="button"
            @click="choice = 'retype'"
            class="w-full text-left px-3 py-2.5 rounded-lg border cursor-pointer transition-colors"
            :class="choice === 'retype'
              ? 'border-blue-400 dark:border-blue-500/60 bg-blue-50/60 dark:bg-blue-900/20'
              : 'border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-white/5'"
          >
            <span class="flex items-center gap-2 text-sm text-text dark:text-text-dark">
              <ArrowRight class="w-3.5 h-3.5 text-gray-400" />
              {{ t('things.remove_kind_retype') }}
            </span>
            <span class="block mt-0.5 ml-5.5 text-[11px] text-gray-500 dark:text-gray-400">
              {{ t('things.remove_kind_retype_why', { n: nodes }, nodes) }}
            </span>
          </button>

          <div v-if="choice === 'retype'" class="pl-3 space-y-1 max-h-40 overflow-y-auto">
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
            <input
              v-if="typing"
              ref="freshBox"
              v-model="fresh"
              spellcheck="false"
              :placeholder="t('things.kind_name_hint')"
              @input="useFresh"
              class="w-full px-2.5 py-1.5 rounded-md font-mono text-xs outline-none
                     bg-gray-50 dark:bg-white/5 border border-blue-300 dark:border-blue-500/40
                     text-text dark:text-text-dark"
            />
            <button
              v-else
              type="button"
              @click="startTyping"
              class="w-full text-left px-2.5 py-1.5 rounded-md text-xs cursor-pointer
                     text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/5"
            >
              + {{ t('things.rename_new_name') }}
            </button>
          </div>

          <button
            type="button"
            @click="choice = 'delete'"
            class="w-full text-left px-3 py-2.5 rounded-lg border cursor-pointer transition-colors"
            :class="choice === 'delete'
              ? 'border-red-400 dark:border-red-500/60 bg-red-50/60 dark:bg-red-900/20'
              : 'border-gray-200 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-white/5'"
          >
            <span class="flex items-center gap-2 text-sm text-red-600 dark:text-red-400">
              <Trash2 class="w-3.5 h-3.5" />
              {{ t('things.remove_kind_delete', { n: nodes }, nodes) }}
            </span>
            <span class="block mt-0.5 ml-5.5 text-[11px] text-gray-500 dark:text-gray-400">
              {{ t('things.remove_kind_delete_why', nodes) }}
            </span>
          </button>
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
            class="px-3 py-1.5 rounded-md text-xs font-medium text-white cursor-pointer
                   flex items-center gap-1.5 disabled:opacity-40 disabled:cursor-not-allowed"
            :class="choice === 'delete' && nodes
              ? 'bg-red-600 hover:bg-red-700'
              : 'bg-blue-600 hover:bg-blue-700'"
          >
            <Loader2 v-if="busy" class="w-3.5 h-3.5 animate-spin" />
            {{ choice === 'retype' ? t('things.remove_kind_move_apply') : t('things.delete') }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
