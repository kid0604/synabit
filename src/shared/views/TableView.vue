<script setup lang="ts">
/**
 * Rows and columns, of whatever type.
 *
 * Lifted out of `mini-apps/note/QueryResultTable.vue`, which was the only
 * generic view this app had and was living in the Notes folder because that is
 * where it was first needed. The markup is unchanged; what changed is that it
 * no longer fetches.
 *
 * That split is the contract in `types.ts`, and it is what makes the same
 * table usable in two places that load their data completely differently: a
 * query block inside a note re-runs on every keystroke and on every vault
 * event, while Things runs one query when you pick something in the rail.
 * Neither of those belongs in a table.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { iconForNodeType } from './nodeTypeIcon';
import type { QueryResult, QueryRow } from './types';

const props = defineProps<{
  result: QueryResult | null;
  selectedId?: string | null;
  /** Shown in the first column when a node has no title of its own. */
  untitledLabel?: string;
}>();

const emit = defineEmits<{ open: [row: QueryRow] }>();

const { t } = useI18n();

const hasMore = computed(
  () => !!props.result && props.result.total > props.result.rows.length,
);
</script>

<template>
  <div v-if="result" class="overflow-x-auto">
    <table class="w-full text-[13px] border-collapse">
      <thead>
        <tr class="border-b border-[#e6e6e6] dark:border-[#3f3f46]">
          <th
            v-for="column in result.columns"
            :key="column"
            class="text-left font-semibold text-[11px] uppercase tracking-wider text-gray-500 py-2 px-3 whitespace-nowrap"
          >
            {{ column }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="row in result.rows"
          :key="row.id"
          @click="emit('open', row)"
          class="border-b last:border-b-0 border-[#f0f0f0] dark:border-[#2c2c2c] cursor-pointer
                 hover:bg-gray-50 dark:hover:bg-[#252525] transition-colors"
          :class="row.id === selectedId ? 'bg-gray-100 dark:bg-white/10' : ''"
        >
          <td
            v-for="(cell, i) in row.cells"
            :key="i"
            class="py-2 px-3 text-[#1c1c1e] dark:text-[#f4f4f5] align-top"
          >
            <span v-if="i === 0" class="flex items-center gap-2">
              <component :is="iconForNodeType(row.node_type)" class="w-3.5 h-3.5 text-gray-400 shrink-0" />
              <span class="truncate">{{ cell || untitledLabel || row.id }}</span>
            </span>
            <span v-else class="text-gray-500 dark:text-gray-400">{{ cell }}</span>
          </td>
        </tr>
      </tbody>
    </table>

    <!--
      `total` counts what matched, `rows` is what came back. Saying so beats a
      table that silently stops, which reads as "that is all there is".
    -->
    <p class="text-[11px] text-gray-400 pt-2 px-3">
      {{ t('note.query_summary', { shown: result.rows.length, ms: result.query_time_ms }) }}
      <span v-if="hasMore"> · {{ t('note.query_more') }}</span>
    </p>
  </div>
</template>
