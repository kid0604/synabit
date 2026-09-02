<script setup lang="ts">
/**
 * Renaming one key across every node of a kind.
 *
 * Renaming and merging are one operation wearing two names, and which one you
 * are doing depends entirely on whether the destination already exists. This
 * used to offer only the existing keys, which made the ordinary case — a field
 * called `due` that should have been `deadline` — the one thing you could not
 * do, while the rarer case was the only one on offer.
 *
 * This is the only thing in schema editing that rewrites the vault, and it
 * rewrites all of one kind at once: `màu` into `colour` reaches one file, the
 * same gesture on `task` reaches 127 and sends 127 nodes through sync. So it
 * asks the backend what it would do, shows the count, and only then offers the
 * button — the same discipline the frontmatter repair used, for the same
 * reason.
 *
 * Nodes already holding the target key are not merged and not overwritten.
 * They are counted and named, because a value somebody wrote is not something
 * to resolve automatically, and the person looking at this screen is the only
 * one who knows which of the two they meant.
 */
import { ref, computed, watch, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { Loader2, ArrowRight } from 'lucide-vue-next';
import { logger } from '../../utils/logger';

interface RenamePlan {
  renaming: number;
  skipped: number;
  skipped_sample: string[];
}

const props = defineProps<{
  /** Required by the backend command, which writes files. */
  vaultPath: string;
  nodeType: string;
  /** The key being merged away. */
  from: string;
  /** Every other key on this kind, as possible destinations. */
  candidates: string[];
}>();

/** `done` carries the destination, so the caller can mend the shape. */
const emit = defineEmits<{ done: [to: string]; close: [] }>();

const { t } = useI18n();

const target = ref<string>('');
const typing = ref(false);
const fresh = ref('');
const freshBox = ref<HTMLInputElement | null>(null);

/** A destination the kind has never seen is a rename; one it has is a merge. */
const isMerge = computed(() => props.candidates.includes(target.value));

const startTyping = async () => {
  typing.value = true;
  target.value = '';
  await nextTick();
  freshBox.value?.focus();
};

const useFresh = () => {
  const name = fresh.value.trim();
  if (name) target.value = name;
};
const plan = ref<RenamePlan | null>(null);
const busy = ref(false);
const failed = ref<string | null>(null);

/** Ask what would happen, every time the destination changes. */
watch(target, async next => {
  plan.value = null;
  failed.value = null;
  if (!next) return;
  try {
    plan.value = await invoke<RenamePlan>('preview_rename_property', {
      nodeType: props.nodeType,
      from: props.from,
      to: next,
    });
  } catch (e) {
    logger.error('[Things] Could not preview the merge', e);
    failed.value = String(e);
  }
});

const apply = async () => {
  if (!target.value || busy.value) return;
  busy.value = true;
  failed.value = null;
  try {
    await invoke<RenamePlan>('rename_property', {
      // `rename_property` writes through the vault, so it needs the path.
      // Without it Tauri rejects the call before any of this runs.
      vaultPath: props.vaultPath,
      nodeType: props.nodeType,
      from: props.from,
      to: target.value,
    });
    emit('done', target.value);
  } catch (e) {
    logger.error('[Things] Could not merge the field', e);
    failed.value = String(e);
  } finally {
    busy.value = false;
  }
};
</script>

<template>
  <Teleport to="body">
    <div
      class="fixed inset-0 z-[10000] flex items-center justify-center p-4 bg-black/40 backdrop-blur-sm"
      @click.self="emit('close')"
    >
      <div
        class="w-full max-w-md rounded-xl shadow-2xl overflow-hidden
               bg-white dark:bg-[#1c1c1e] border border-gray-200 dark:border-gray-700"
      >
        <div class="px-5 py-4 border-b border-gray-100 dark:border-gray-700">
          <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">
            {{ isMerge ? t('things.merge_title') : t('things.rename_title') }}
          </h3>
          <div class="flex items-center gap-2 mt-2 font-mono text-xs">
            <span class="text-gray-500 dark:text-gray-400">{{ from }}</span>
            <ArrowRight class="w-3.5 h-3.5 text-gray-400" />
            <span :class="target ? 'text-[#1c1c1e] dark:text-[#f4f4f5]' : 'text-gray-300'">
              {{ target || '…' }}
            </span>
          </div>
        </div>

        <div class="px-5 py-4 max-h-[45vh] overflow-y-auto">
          <p class="text-xs text-gray-400 mb-2">{{ t('things.rename_pick_target') }}</p>

          <!--
            The new name first, because it is the ordinary case. Picking an
            existing key below is the same operation arriving at a name that is
            already taken, which is what makes it a merge.
          -->
          <div v-if="typing" class="flex items-center gap-2 mb-2">
            <input
              ref="freshBox"
              v-model="fresh"
              spellcheck="false"
              :placeholder="t('things.field_name')"
              @input="useFresh"
              @keydown.esc="typing = false; fresh = ''; target = ''"
              class="flex-1 min-w-0 px-2.5 py-1.5 rounded-md font-mono text-xs outline-none
                     bg-gray-50 dark:bg-white/5 border border-blue-300 dark:border-blue-500/40
                     text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300"
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
            v-for="key in candidates"
            :key="key"
            type="button"
            @click="typing = false; fresh = ''; target = key"
            class="w-full text-left px-2.5 py-1.5 rounded-md font-mono text-xs cursor-pointer
                   transition-colors"
            :class="key === target
              ? 'bg-blue-50 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300'
              : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/5'"
          >
            {{ key }}
          </button>

          <!-- What will happen, in files, before anything happens. -->
          <div
            v-if="plan"
            class="mt-4 pt-3 border-t border-gray-100 dark:border-gray-700 text-xs space-y-1.5"
          >
            <!--
              A field that was declared and never filled in carries no files at
              all, and merging it is a change to the shape and nothing else.
              Saying "0 nodes will be changed" and greying the button out told
              somebody their merge was impossible when it was simply cheap.
            -->
            <p class="text-[#1c1c1e] dark:text-[#f4f4f5]">
              {{ plan.renaming
                ? t('things.merge_will_change', { count: plan.renaming })
                : t('things.merge_only_shape', { field: from }) }}
            </p>
            <p v-if="isMerge" class="text-gray-500 dark:text-gray-400">
              {{ t('things.merge_because_taken', { field: target }) }}
            </p>
            <template v-if="plan.skipped">
              <p class="text-amber-700 dark:text-amber-400">
                {{ t('things.merge_will_skip', { count: plan.skipped }) }}
              </p>
              <p
                v-for="path in plan.skipped_sample"
                :key="path"
                class="font-mono text-[11px] text-gray-400 truncate"
              >
                {{ path }}
              </p>
            </template>
          </div>

          <p v-if="failed" class="mt-3 text-xs text-red-600 dark:text-red-400">{{ failed }}</p>
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
            :disabled="!plan || busy"
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
