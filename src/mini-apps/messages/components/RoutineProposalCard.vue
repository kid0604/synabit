<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { CalendarClock } from 'lucide-vue-next';
import SynCard from '../../../shared/syn/SynCard.vue';
import { daysInWords, type Routine } from '../routines';
import { useSynRoutineProposals } from '../composables/useSynRoutineProposals';

/**
 * Something Syn would do on its own, written from the conversation, asked
 * about under the answer: turn it on?
 *
 * Says what it will do, in the person's own words of it; when — the days and
 * the time, or what it waits for; that it runs on this device; and what it may
 * and may not do while nobody is watching. Turning it on is also agreeing to
 * it on this device. See `routine::draft`.
 */
const props = defineProps<{ vaultPath: string; conversationId: string | null | undefined }>();
const { t } = useI18n();
const { proposedIn, answer, error, answering } = useSynRoutineProposals(() => props.vaultPath);

const routines = computed<Routine[]>(() => proposedIn(props.conversationId));

const said = ref<{ name: string; kept: boolean } | null>(null);
watch(() => props.conversationId, () => { said.value = null; });

const decide = async (r: Routine, keep: boolean) => {
  if (await answer(r.id, keep)) said.value = { name: r.name, kept: keep };
};

/** When it runs, in words. */
const when = (r: Routine) => {
  const parts: string[] = [];
  if (r.schedule.at) parts.push(t('syn.routine_proposal_at', { days: daysInWords(r.schedule.weekdays, t), at: r.schedule.at }));
  if (r.on_new) parts.push(t('syn.routine_proposal_on_new', { what: r.on_new }));
  return parts.join(' · ');
};
</script>

<template>
  <div v-if="routines.length || said" class="space-y-2" data-test="routine-proposals">
    <SynCard
      v-for="r in routines"
      :key="r.id"
      tone="change"
      :icon="CalendarClock"
      :title="t('syn.routine_proposal_title', { name: r.name })"
      :lead="when(r)"
      :error="error"
      :note="t('syn.routine_proposal_may')"
      :data-test="`routine-${r.name}`"
    >
      <p class="text-sm text-text dark:text-text-dark whitespace-pre-wrap">“{{ r.ask }}”</p>
      <p v-if="r.to_phone" class="text-xs text-gray-600 dark:text-gray-300">{{ t('syn.routine_proposal_phone') }}</p>
      <p v-if="r.read_before?.length" class="text-xs text-amber-800 dark:text-amber-300">
        {{ t('syn.skill_proposal_read_before', { what: r.read_before.join(', ') }) }}
      </p>

      <template #actions>
        <button
          type="button"
          class="px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer border border-gray-200 dark:border-gray-700/50 text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 disabled:opacity-40"
          :disabled="answering"
          data-test="routine-let-go"
          @click="decide(r, false)"
        >
          {{ t('syn.routine_proposal_let_go') }}
        </button>
        <button type="button" class="btn-primary" :disabled="answering" data-test="routine-turn-on" @click="decide(r, true)">
          {{ t('syn.routine_proposal_turn_on') }}
        </button>
      </template>
    </SynCard>

    <p v-if="said && !routines.length" class="text-xs text-gray-600 dark:text-gray-300 px-1" data-test="routine-said">
      {{ said.kept ? t('syn.routine_proposal_on', { name: said.name }) : t('syn.routine_proposal_gone', { name: said.name }) }}
    </p>
  </div>
</template>
