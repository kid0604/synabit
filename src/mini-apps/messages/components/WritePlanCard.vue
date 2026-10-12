<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Send } from 'lucide-vue-next';
import type { WritePlan, WriteRequest } from '../composables/useSynWrites';

/**
 * Syn's plan of sends, laid out for the person to approve.
 *
 * Every line is built by Rust from the requests as they will go — the tool,
 * every argument, a table by its handle and where it came from — and what
 * Rust can see is wrong is shown on the request and cannot be approved. Only
 * `why` is Syn's own words, and it is labelled as such.
 */
const props = defineProps<{ plan: WritePlan; busy?: boolean; error?: string | null }>();
const emit = defineEmits<{ answer: [approved: string[]] }>();
const { t } = useI18n();

const approvable = (r: WriteRequest) => !r.problems?.length && r.state === 'proposed';
const chosen = ref<Set<string>>(new Set(props.plan.requests.filter(approvable).map((r) => r.hash)));
watch(
  () => props.plan,
  (p) => {
    chosen.value = new Set(p.requests.filter(approvable).map((r) => r.hash));
  },
);

const toggle = (hash: string, on: boolean) => {
  const next = new Set(chosen.value);
  if (on) next.add(hash);
  else next.delete(hash);
  chosen.value = next;
};

const count = computed(() => chosen.value.size);
</script>

<template>
  <div class="rounded-xl border border-violet-300/60 dark:border-violet-500/30 bg-violet-50/40 dark:bg-violet-500/5 p-3 space-y-3" data-test="write-plan">
    <div class="flex items-center gap-2">
      <Send class="w-4 h-4 text-violet-600 dark:text-violet-300" aria-hidden="true" />
      <p class="text-sm font-medium text-text dark:text-text-dark">{{ t('syn.writes_plan_title', { n: plan.requests.length }, plan.requests.length) }}</p>
    </div>
    <p v-if="plan.why" class="text-xs text-gray-600 dark:text-gray-300">
      <span class="italic">{{ t('syn.plan_why') }}</span> “{{ plan.why }}”
    </p>

    <ul class="space-y-2">
      <li
        v-for="r in plan.requests"
        :key="r.hash"
        class="p-2.5 rounded-lg bg-white/70 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50 space-y-1.5"
        :data-test="`request-${r.hash.slice(0, 8)}`"
      >
        <label class="flex items-start gap-2 cursor-pointer" :class="{ 'opacity-60 cursor-not-allowed': !approvable(r) }">
          <input
            type="checkbox"
            class="mt-0.5 accent-violet-600"
            :disabled="!approvable(r) || busy"
            :checked="chosen.has(r.hash)"
            @change="toggle(r.hash, ($event.target as HTMLInputElement).checked)"
          />
          <span class="min-w-0 flex-1">
            <span class="text-xs font-medium text-text dark:text-text-dark">{{ r.connector }} · {{ r.operation }}</span>
            <span
              class="ml-2 px-1.5 py-0.5 rounded text-xs font-medium"
              :class="r.effect === 'irreversible'
                ? 'bg-red-50 text-red-700 dark:bg-red-500/10 dark:text-red-300'
                : 'bg-amber-50 text-amber-800 dark:bg-amber-500/10 dark:text-amber-300'"
            >
              {{ r.effect === 'irreversible' ? t('syn.plan_irreversible') : t('syn.plan_write') }}
            </span>
          </span>
        </label>
        <dl class="pl-6 space-y-0.5 text-xs">
          <div v-for="s in r.shown" :key="s.param" class="flex gap-2 min-w-0">
            <dt class="font-mono text-gray-500 dark:text-gray-400 shrink-0">{{ s.param }}</dt>
            <dd class="min-w-0 break-words text-text dark:text-text-dark">
              <template v-if="s.table">{{ t('syn.plan_table', { what: s.table }) }}</template>
              <template v-else>{{ s.value }}</template>
              <span v-if="s.role" class="ml-1 text-gray-500 dark:text-gray-400">({{ t(`syn.plan_role_${s.role}`) }})</span>
            </dd>
          </div>
        </dl>
        <ul v-if="r.problems?.length" class="pl-6 text-xs text-red-600 dark:text-red-400 space-y-0.5">
          <li v-for="(p, i) in r.problems" :key="i">⚠ {{ p }}</li>
        </ul>
      </li>
    </ul>

    <p v-if="error" class="text-xs text-red-600 dark:text-red-400">{{ error }}</p>
    <div class="flex items-center justify-end gap-2">
      <button
        type="button"
        class="px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer border border-gray-200 dark:border-gray-700/50 text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 disabled:opacity-40"
        :disabled="busy"
        data-test="decline-all"
        @click="emit('answer', [])"
      >
        {{ t('syn.plan_decline') }}
      </button>
      <button type="button" class="btn-primary" :disabled="busy || count === 0" data-test="approve-chosen" @click="emit('answer', [...chosen])">
        {{ t('syn.plan_approve', { n: count }, count) }}
      </button>
    </div>
  </div>
</template>
