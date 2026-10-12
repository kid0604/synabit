<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { ChevronDown, ChevronRight, MessageSquarePlus } from 'lucide-vue-next';
import { useSynDeclared } from '../composables/useSynDeclared';
import { choicesFrom, hostOf, inputsOf, sends, type DeclaredView } from '../declared';
import { askSynWith } from '../../../shared/syn/selectionAsk';
import ConnectCard from './ConnectCard.vue';
import SynConnectorSettings from './SynConnectorSettings.vue';

/**
 * Settings → Connections: what Syn can reach, and stopping it.
 *
 * Connecting happens in the chat — "connect Syn to my calendar" — or here,
 * from the library, through the same connect card. Nothing on this screen
 * asks a technical question; MCP servers and pasted declarations are under
 * Advanced. See `docs/syn-connectors-simple-2026-10-12.md`.
 */
const props = defineProps<{ vaultPath: string }>();
const { t, te } = useI18n();
const { declared, bundled, busy, error, load, loadBundled, validate, save, approve, remove } = useSynDeclared(() => props.vaultPath);

onMounted(() => {
  void load();
  void loadBundled();
});

/** The connection whose card is open. */
const opened = ref<string | null>(null);
const advanced = ref(false);
const pasting = ref('');
const pasteProblems = ref<string[] | null>(null);

const idOf = (v: DeclaredView) => v.manifest?.id ?? v.node_id;
const name = (v: DeclaredView) => {
  const m = v.manifest;
  if (!m) return v.title;
  const key = `syn.library_${m.id}_name`;
  return m.provenance === 'bundled' && te(key) ? t(key) : m.name;
};
const source = (v: DeclaredView) => {
  const m = v.manifest;
  if (!m) return t('syn.source_user');
  if (m.transport === 'device') return t('syn.source_device');
  return t(`syn.source_${m.provenance}`);
};
const where = (v: DeclaredView) => {
  const m = v.manifest;
  if (!m) return '';
  if (m.transport === 'device') return v.instance?.folder ?? t('syn.connect_this_device');
  const address = inputsOf(m).find((i) => i.kind === 'address');
  if (address) {
    try {
      return new URL(v.instance?.inputs?.[address.name] ?? '').host;
    } catch {
      return t('syn.connect_your_server');
    }
  }
  return hostOf(m);
};
const state = (v: DeclaredView) => {
  switch (v.standing.state) {
    case 'approved':
      return { text: t('syn.state_ok'), cls: 'bg-green-50 text-green-700 dark:bg-green-500/10 dark:text-green-300' };
    case 'changed':
      return { text: t('syn.state_changed'), cls: 'bg-amber-50 text-amber-800 dark:bg-amber-500/10 dark:text-amber-300' };
    case 'broken':
      return { text: t('syn.state_broken'), cls: 'bg-red-50 text-red-700 dark:bg-red-500/10 dark:text-red-300' };
    default:
      return { text: t('syn.state_new'), cls: 'bg-gray-100 text-gray-600 dark:bg-white/10 dark:text-gray-300' };
  }
};
/** What it keeps in Synabit on its own, and how that last went. */
const kept = (v: DeclaredView) => {
  const ran = Object.values(v.mirrors_ran ?? {})[0];
  if (!ran) return null;
  const when = new Date(ran.at * 1000).toLocaleString();
  return ran.ok ? t('syn.connections_kept', { when }) : `${t('syn.state_failing')}: ${ran.said ?? ''}`;
};

/** The library, less what is already in the vault. */
const library = computed(() => bundled.value.filter((b) => !declared.value.some((d) => d.manifest?.id === b.id)));
const libName = (id: string, fallback: string) => (te(`syn.library_${id}_name`) ? t(`syn.library_${id}_name`) : fallback);
const libAbout = (id: string, fallback: string) => (te(`syn.library_${id}_about`) ? t(`syn.library_${id}_about`) : fallback);

const add = async (id: string) => {
  const b = bundled.value.find((x) => x.id === id);
  if (b && (await save(b.yaml, b.about))) opened.value = id;
};

const toggleKeep = async (v: DeclaredView, on: boolean) => {
  if (!v.manifest) return;
  const choices = choicesFrom(v);
  choices.mirrors = on ? (v.manifest.mirrors ?? []).map((m) => m.name) : [];
  await approve(v.manifest.id, choices, '');
};

const keepPasted = async () => {
  const found = await validate(pasting.value);
  pasteProblems.value = (found ?? []).map((p) => (p.at ? `${p.at}: ${p.said}` : p.said));
  if (found && found.length === 0 && (await save(pasting.value))) {
    pasting.value = '';
    pasteProblems.value = null;
  }
};
</script>

<template>
  <section class="space-y-3" data-test="connections">
    <div class="flex items-start justify-between gap-3">
      <p class="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">{{ t('syn.connections_intro') }}</p>
      <button type="button" class="btn-secondary flex items-center gap-1.5 shrink-0" data-test="ask-syn" @click="askSynWith(t('syn.connections_ask_prompt'))">
        <MessageSquarePlus class="w-4 h-4" aria-hidden="true" />{{ t('syn.connections_ask') }}
      </button>
    </div>

    <ul v-if="declared.length" class="rounded-xl border border-gray-200 dark:border-gray-700/50 divide-y divide-gray-200 dark:divide-gray-700/50">
      <li v-for="v in declared" :key="idOf(v)" :data-test="`connection-${idOf(v)}`">
        <button type="button" class="w-full flex items-center gap-3 px-3 py-2.5 text-left cursor-pointer" @click="opened = opened === idOf(v) ? null : idOf(v)">
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium text-text dark:text-text-dark truncate">
              {{ name(v) }}
              <span class="ml-1.5 px-1.5 py-0.5 rounded text-xs font-normal bg-gray-100 text-gray-600 dark:bg-white/10 dark:text-gray-300">{{ source(v) }}</span>
            </p>
            <p class="text-xs text-gray-500 dark:text-gray-400 truncate">
              {{ where(v) }}<template v-if="v.manifest"> · {{ sends(v.manifest) ? t('syn.connections_reads_sends') : t('syn.connections_reads') }}</template><template v-if="kept(v)"> · {{ kept(v) }}</template>
            </p>
          </div>
          <span class="px-2 py-0.5 rounded text-xs shrink-0" :class="state(v).cls">{{ state(v).text }}</span>
          <component :is="opened === idOf(v) ? ChevronDown : ChevronRight" class="w-4 h-4 text-gray-500 shrink-0" aria-hidden="true" />
        </button>
        <div v-if="opened === idOf(v)" class="px-3 pb-3 space-y-2">
          <ConnectCard v-if="v.manifest" :vault-path="vaultPath" :connector-id="v.manifest.id" @done="opened = null; load()" />
          <ul v-else class="text-xs text-red-600 dark:text-red-400 space-y-0.5">
            <li v-for="p in v.problems" :key="p.code + p.at">{{ p.said }}</li>
          </ul>
          <div v-if="v.standing.state === 'approved'" class="flex flex-wrap items-center gap-3 text-xs">
            <label v-if="v.manifest?.mirrors?.length" class="flex items-center gap-1.5 cursor-pointer text-text dark:text-text-dark">
              <input type="checkbox" class="accent-accent" :checked="!!v.instance?.mirrors?.length" data-test="keep" @change="toggleKeep(v, ($event.target as HTMLInputElement).checked)" />
              {{ t('syn.connections_keep') }}
            </label>
            <button type="button" class="ml-auto text-red-600 dark:text-red-400 hover:underline cursor-pointer" :disabled="busy" data-test="stop" @click="v.manifest && remove(v.manifest.id)">
              {{ t('syn.connections_stop') }}
            </button>
          </div>
        </div>
      </li>
    </ul>
    <p v-else class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.connections_none') }}</p>

    <div v-if="library.length" class="space-y-1.5">
      <p class="text-xs font-medium text-gray-600 dark:text-gray-300">{{ t('syn.connections_library') }}</p>
      <div class="grid gap-2 sm:grid-cols-2">
        <button
          v-for="b in library"
          :key="b.id"
          type="button"
          class="text-left rounded-lg bg-gray-50 dark:bg-white/5 px-3 py-2 hover:bg-gray-100 dark:hover:bg-white/10 cursor-pointer"
          :data-test="`library-${b.id}`"
          :disabled="busy"
          @click="add(b.id)"
        >
          <p class="text-sm text-text dark:text-text-dark">{{ libName(b.id, b.name) }}</p>
          <p class="text-xs text-gray-500 dark:text-gray-400">{{ libAbout(b.id, b.about) }}</p>
        </button>
      </div>
    </div>

    <p v-if="error" class="text-xs text-red-600 dark:text-red-400 break-words">{{ error }}</p>

    <button type="button" class="flex items-center gap-1 text-xs text-gray-500 dark:text-gray-400 cursor-pointer" data-test="advanced" @click="advanced = !advanced">
      <component :is="advanced ? ChevronDown : ChevronRight" class="w-3.5 h-3.5" aria-hidden="true" />{{ t('syn.connections_advanced') }}
    </button>
    <div v-if="advanced" class="space-y-4 pl-4 border-l border-gray-200 dark:border-gray-700/50">
      <SynConnectorSettings :vault-path="vaultPath" />
      <div class="space-y-1.5">
        <p class="text-xs font-medium text-gray-600 dark:text-gray-300">{{ t('syn.declared_add') }}</p>
        <textarea v-model="pasting" rows="6" class="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50 text-xs font-mono text-text dark:text-text-dark" :placeholder="t('syn.declared_paste')" />
        <ul v-if="pasteProblems?.length" class="text-xs text-red-600 dark:text-red-400 space-y-0.5">
          <li v-for="p in pasteProblems" :key="p">{{ p }}</li>
        </ul>
        <button type="button" class="btn-secondary" :disabled="busy || !pasting.trim()" @click="keepPasted">{{ t('syn.declared_save') }}</button>
      </div>
    </div>
  </section>
</template>
