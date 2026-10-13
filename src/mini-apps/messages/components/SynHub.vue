<script setup lang="ts">
/**
 * Everything about Syn that belongs to the person, in one place.
 *
 * It was spread over five: the instructions on a screen of their own, routines
 * on another, connections in Settings, and what Syn remembers, its skills and
 * its permissions as tabs four to six of a drawer called "What Syn did", which
 * opened only from inside a conversation. Those three are the core of a
 * personal assistant — what it believes about you, what it knows how to do,
 * what you let it do — and they were filed under debugging.
 *
 * Grouped as the person thinks of them: what Syn knows (how to work with you,
 * what it remembers), what it can do (skills, connections), what it does on
 * its own (routines), and what it is allowed. The brain — the model — is in
 * Settings, one press away. What is waiting for an answer is in "Syn's work",
 * beside this.
 *
 * Every panel is the one that was already there, so there is one copy of each.
 */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import InstructionsPanel from './InstructionsPanel.vue';
import RoutinesPanel from './RoutinesPanel.vue';
import SynConnections from './SynConnections.vue';
import RunInspector from './RunInspector.vue';

export type HubTab = 'how' | 'memory' | 'skills' | 'connections' | 'routines' | 'permissions';

const props = defineProps<{
  vaultPath: string;
  tab?: HubTab | null;
  /** A memory or skill to scroll to and ring, when a notice named one. */
  highlight?: string | null;
}>();
const emit = defineEmits<{
  'open-conversation': [id: string];
  'open-settings': [];
}>();
const { t } = useI18n();

const GROUPS: { title: string; tabs: HubTab[] }[] = [
  { title: 'syn.hub_knows', tabs: ['how', 'memory'] },
  { title: 'syn.hub_can', tabs: ['skills', 'connections'] },
  { title: 'syn.hub_does', tabs: ['routines'] },
  { title: 'syn.hub_allowed', tabs: ['permissions'] },
];

const current = ref<HubTab>(props.tab ?? 'memory');
watch(() => props.tab, (next) => { if (next) current.value = next; });
</script>

<template>
  <div class="flex-1 flex flex-col min-h-0" data-syn-hub>
    <nav class="flex gap-4 overflow-x-auto px-4 py-2 border-b border-gray-100 dark:border-gray-800/60" :aria-label="t('syn.hub_title')">
      <div v-for="group in GROUPS" :key="group.title" class="shrink-0">
        <p class="text-xs uppercase tracking-wide text-gray-500 dark:text-gray-400 mb-1">{{ t(group.title) }}</p>
        <div class="flex gap-1">
          <button
            v-for="option in group.tabs"
            :key="option"
            type="button"
            class="px-3 py-1 text-xs font-medium rounded-md whitespace-nowrap transition-colors"
            :class="current === option
              ? 'bg-accent/10 text-accent dark:text-accent-dark'
              : 'text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/5'"
            :aria-pressed="current === option"
            :data-hub-tab="option"
            @click="current = option"
          >
            {{ t(`syn.hub_tab_${option}`) }}
          </button>
        </div>
      </div>
      <div class="shrink-0 self-end">
        <button
          type="button"
          class="px-3 py-1 text-xs font-medium rounded-md whitespace-nowrap text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-white/5"
          data-hub-tab="brain"
          @click="emit('open-settings')"
        >
          {{ t('syn.hub_tab_brain') }} ↗
        </button>
      </div>
    </nav>

    <InstructionsPanel v-if="current === 'how'" :vault-path="vaultPath" />
    <RunInspector
      v-else-if="current === 'memory' || current === 'skills' || current === 'permissions'"
      :key="current"
      embedded
      :vault-path="vaultPath"
      :initial-tab="current"
      :highlight="highlight ?? null"
    />
    <div v-else-if="current === 'connections'" class="flex-1 overflow-y-auto px-6 py-6">
      <div class="max-w-3xl mx-auto"><SynConnections :vault-path="vaultPath" /></div>
    </div>
    <RoutinesPanel
      v-else-if="current === 'routines'"
      :vault-path="vaultPath"
      @open-conversation="(id) => emit('open-conversation', id)"
    />
  </div>
</template>
