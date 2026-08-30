<script setup lang="ts">
/**
 * One node, whatever kind it is.
 *
 * Properties sit above the body rather than in a side rail, and that is the
 * one place this screen deliberately departs from Notes. For a note the body
 * is the content and `tags` is a remark about it; for a `book` or an `animal`
 * the frontmatter *is* the content and the body is an afterthought — Mèo Mun
 * is `species`, `colour` and `vaccinated_at` plus one sentence. Putting the
 * fields in a 280px rail would give the substance a column and the afterthought
 * the room.
 *
 * The proportion then takes care of itself: a book with six fields and no body
 * reads as a record, a note with two fields and three pages reads as a note.
 * Same layout, no mode to switch.
 */
import { Plus, X, Loader2 } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import TiptapEditor from '../../mini-apps/note/TiptapEditor.vue';
import { iconForNodeType } from './nodeTypeIcon';
import type { FieldRow } from '../../mini-apps/things/composables/useThingsNode';

defineProps<{
  nodeType: string;
  readOnlyRows: { key: string; value: string }[];
  loading?: boolean;
  saving?: boolean;
  vaultPath?: string;
}>();

const title = defineModel<string>('title', { required: true });
const body = defineModel<string>('body', { required: true });
const fields = defineModel<FieldRow[]>('fields', { required: true });

const emit = defineEmits<{ save: []; addField: []; removeField: [index: number] }>();

const { t } = useI18n();
</script>

<template>
  <div class="h-full flex flex-col min-h-0 bg-white dark:bg-[#141416]">
    <div v-if="loading" class="flex-1 flex items-center justify-center text-gray-400">
      <Loader2 class="w-5 h-5 animate-spin" />
    </div>

    <div v-else class="flex-1 overflow-y-auto min-h-0">
      <div class="max-w-3xl mx-auto px-8 py-7">

        <div class="flex items-center gap-2 mb-3">
          <component :is="iconForNodeType(nodeType)" class="w-4 h-4 text-gray-400" />
          <!--
            The type's own word. Not translated, for the reason the left rail is
            not: a type nobody wrote code for has no other name.
          -->
          <span class="text-xs font-mono text-gray-400 dark:text-gray-500">{{ nodeType }}</span>
        </div>

        <input
          v-model="title"
          type="text"
          :placeholder="t('things.untitled')"
          @blur="emit('save')"
          class="w-full bg-transparent border-0 outline-none text-3xl font-bold tracking-tight
                 text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 dark:placeholder-gray-600 mb-6"
        />

        <!-- ── Properties ─────────────────────────────────── -->
        <div class="space-y-1.5 mb-7">
          <div
            v-for="row in readOnlyRows.filter(r => r.key !== 'type' && r.key !== 'title')"
            :key="row.key"
            class="flex items-start gap-3 text-sm"
          >
            <span class="w-[130px] flex-none pt-1 text-gray-400 dark:text-gray-500 font-mono text-xs truncate">
              {{ row.key }}
            </span>
            <span class="flex-1 min-w-0 pt-1 text-gray-500 dark:text-gray-400">{{ row.value }}</span>
          </div>

          <div
            v-for="(field, index) in fields"
            :key="index"
            class="flex items-start gap-3 group"
          >
            <input
              v-model="field.key"
              spellcheck="false"
              :placeholder="t('things.field_name')"
              @blur="emit('save')"
              class="w-[130px] flex-none px-2 py-1 rounded bg-transparent hover:bg-gray-50 dark:hover:bg-white/5
                     focus:bg-gray-50 dark:focus:bg-white/5 border border-transparent focus:border-gray-200
                     dark:focus:border-gray-700 outline-none text-xs font-mono
                     text-gray-500 dark:text-gray-400 placeholder-gray-300 transition-colors"
            />
            <input
              v-model="field.value"
              spellcheck="false"
              :placeholder="t('things.field_value')"
              @blur="emit('save')"
              class="flex-1 min-w-0 px-2 py-1 rounded bg-transparent hover:bg-gray-50 dark:hover:bg-white/5
                     focus:bg-gray-50 dark:focus:bg-white/5 border border-transparent focus:border-gray-200
                     dark:focus:border-gray-700 outline-none text-sm
                     text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 transition-colors"
            />
            <button
              type="button"
              @click="emit('removeField', index); emit('save')"
              class="p-1 mt-1 rounded text-gray-300 hover:text-red-500 transition-colors cursor-pointer
                     opacity-0 group-hover:opacity-100 focus:opacity-100"
              :aria-label="t('things.remove_field')"
            >
              <X class="w-3.5 h-3.5" />
            </button>
          </div>

          <button
            type="button"
            @click="emit('addField')"
            class="flex items-center gap-1.5 mt-1 px-2 py-1 text-xs text-gray-400
                   hover:text-gray-600 dark:hover:text-gray-300 transition-colors cursor-pointer"
          >
            <Plus class="w-3 h-3" /> {{ t('things.add_field') }}
          </button>
        </div>

        <!-- ── Body ───────────────────────────────────────── -->
        <div class="border-t border-gray-100 dark:border-[#232326] pt-6">
          <TiptapEditor
            v-model="body"
            :vaultPath="vaultPath || ''"
            :minHeightClass="'min-h-[160px]'"
            :placeholder="t('things.body_placeholder')"
            class="w-full"
            @blur="emit('save')"
          />
        </div>
      </div>
    </div>

    <div
      v-if="saving"
      class="flex-shrink-0 px-8 py-1.5 text-[11px] text-gray-400 border-t border-gray-100 dark:border-[#232326]"
    >
      {{ t('things.saving') }}
    </div>
  </div>
</template>
