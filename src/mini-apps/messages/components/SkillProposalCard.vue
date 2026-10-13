<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { BookMarked } from 'lucide-vue-next';
import SynCard from '../../../shared/syn/SynCard.vue';
import { toolLabel } from '../../../shared/syn/toolLabel';
import { useSynSkillProposals, type SkillProposal } from '../composables/useSynSkillProposals';

/**
 * A way of doing a job Syn wrote down, asked about under the answer: keep it?
 *
 * Everything on it is read back from the skill as written — its steps, each
 * call it makes and whether that reads or changes things — not from what the
 * model said about it. What was read from outside before it was written is
 * named, because a skill rides into every later prompt. See `syn::drafting`.
 */
const props = defineProps<{ vaultPath: string; conversationId: string | null | undefined }>();
const { t } = useI18n();
const { proposedIn, answer, error, answering } = useSynSkillProposals(() => props.vaultPath);

const proposals = computed<SkillProposal[]>(() => proposedIn(props.conversationId));

/** What became of the last answer, until the conversation changes. */
const said = ref<{ name: string; kept: boolean } | null>(null);
watch(() => props.conversationId, () => { said.value = null; });

const decide = async (p: SkillProposal, keep: boolean) => {
  if (await answer(p.skill_id, keep)) said.value = { name: p.name, kept: keep };
};

const changes = (does: string) => does === 'write' || does === 'structural';
</script>

<template>
  <div v-if="proposals.length || said" class="space-y-2" data-test="skill-proposals">
    <SynCard
      v-for="p in proposals"
      :key="p.skill_id"
      tone="change"
      :icon="BookMarked"
      :title="t('syn.skill_proposal_title', { name: p.name })"
      :lead="p.when_to_use ? t('syn.skill_proposal_when', { when: p.when_to_use }) : undefined"
      :error="error"
      :data-test="`skill-${p.name}`"
    >
      <p v-if="p.description" class="text-xs text-text dark:text-text-dark">{{ p.description }}</p>

      <ul v-if="p.calls.length" class="space-y-1 text-xs">
        <li v-for="(c, i) in p.calls" :key="`${c.tool}-${i}`" class="flex items-center gap-2">
          <span class="text-text dark:text-text-dark">{{ toolLabel(t, c.tool) }}</span>
          <span
            v-if="changes(c.does)"
            class="px-1.5 py-0.5 rounded font-medium bg-amber-50 text-amber-800 dark:bg-amber-500/10 dark:text-amber-300"
          >{{ t('syn.skill_proposal_changes') }}</span>
        </li>
      </ul>

      <details v-if="p.steps" class="text-xs">
        <summary class="cursor-pointer text-gray-600 dark:text-gray-300">{{ t('syn.skill_proposal_steps') }}</summary>
        <pre class="mt-1 whitespace-pre-wrap break-words max-h-56 overflow-auto p-2 rounded bg-white/70 dark:bg-white/5 text-text dark:text-text-dark">{{ p.steps }}</pre>
      </details>

      <p v-if="p.read_before.length" class="text-xs text-amber-800 dark:text-amber-300">
        {{ t('syn.skill_proposal_read_before', { what: p.read_before.join(', ') }) }}
      </p>

      <template #actions>
        <button
          type="button"
          class="px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer border border-gray-200 dark:border-gray-700/50 text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 disabled:opacity-40"
          :disabled="answering"
          data-test="skill-let-go"
          @click="decide(p, false)"
        >
          {{ t('syn.skill_proposal_let_go') }}
        </button>
        <button type="button" class="btn-primary" :disabled="answering" data-test="skill-keep" @click="decide(p, true)">
          {{ t('syn.skill_proposal_keep') }}
        </button>
      </template>
    </SynCard>

    <p v-if="said && !proposals.length" class="text-xs text-gray-600 dark:text-gray-300 px-1" data-test="skill-said">
      {{ said.kept ? t('syn.skill_proposal_kept', { name: said.name }) : t('syn.skill_proposal_let_go_done', { name: said.name }) }}
    </p>
  </div>
</template>
