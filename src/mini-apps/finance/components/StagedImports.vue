<script setup lang="ts">
import { Sparkles } from 'lucide-vue-next';
import type { Staged } from '../useStagedImports';

/**
 * What Syn staged, above the ledger it is not yet in. The description is
 * Syn's and is quoted as Syn's; the count and the source are Rust's.
 */
defineProps<{ staged: Staged[] }>();
const emit = defineEmits<{ preview: [id: string]; discard: [id: string] }>();
</script>

<template>
  <div v-if="staged.length" class="mx-4 md:mx-6 mt-4 space-y-2" data-test="staged-imports">
    <div
      v-for="s in staged"
      :key="s.id"
      class="flex flex-wrap items-center gap-3 rounded-xl border border-violet-300/60 dark:border-violet-500/30 bg-violet-50/50 dark:bg-violet-500/5 px-4 py-3 text-sm"
      :data-test="`staged-${s.id}`"
    >
      <Sparkles class="w-4 h-4 text-violet-600 dark:text-violet-300 shrink-0" aria-hidden="true" />
      <div class="flex-1 min-w-0">
        <p class="font-medium text-text dark:text-text-dark">{{ $t('finance.staged_title', { n: s.rows }, s.rows) }}</p>
        <p class="text-xs text-gray-600 dark:text-gray-300 truncate">
          <template v-if="s.why"><span class="italic">{{ $t('finance.staged_why') }}</span> “{{ s.why }}” · </template>{{ s.source }}
        </p>
      </div>
      <button type="button" class="btn-secondary" data-test="staged-discard" @click="emit('discard', s.id)">{{ $t('finance.staged_discard') }}</button>
      <button type="button" class="btn-primary" data-test="staged-preview" @click="emit('preview', s.id)">{{ $t('finance.staged_preview') }}</button>
    </div>
  </div>
</template>
