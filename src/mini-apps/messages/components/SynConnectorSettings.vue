<script setup lang="ts">
import { onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Loader2, Plus, X } from 'lucide-vue-next';
import { isMobileOS } from '../../../shared/platformScope';
import { useSynConnectors } from '../composables/useSynConnectors';
import { draftFrom, statusLine, type ConnectorDraft, type ConnectorView, type ConnectorTested } from '../connector';

const props = defineProps<{ vaultPath: string }>();

const { t } = useI18n();
const { servers, busy, error, load, reconnect, save, remove, test } = useSynConnectors(() => props.vaultPath);

/** The server being added or edited, or `null` when the form is closed. */
const draft = ref<ConnectorDraft | null>(null);
/** What Test found for the draft as it stands. Cleared when it changes. */
const tested = ref<ConnectorTested | null>(null);
/** The server whose Remove was pressed once, waiting for the second press. */
const removing = ref<string | null>(null);

const open = (view?: ConnectorView) => {
  draft.value = draftFrom(view);
  tested.value = null;
  removing.value = null;
};

const close = () => {
  draft.value = null;
  tested.value = null;
};

const addSecret = () => draft.value?.secrets.push({ name: '', value: '', stored: false });
const dropSecret = (i: number) => draft.value?.secrets.splice(i, 1);

const runTest = async () => {
  if (!draft.value) return;
  tested.value = await test(draft.value);
};

const saveDraft = async () => {
  if (draft.value && (await save(draft.value))) close();
};

/** Switching a server on or off is a save with nothing else changed. */
const toggle = async (view: ConnectorView) => {
  const flipped = draftFrom(view);
  flipped.enabled = !flipped.enabled;
  await save(flipped);
};

const confirmRemove = async (view: ConnectorView) => {
  if (removing.value !== view.server.id) {
    removing.value = view.server.id;
    return;
  }
  removing.value = null;
  await remove(view.server.id);
};

// A result for a different address or program is not a result for this one.
watch(
  () => draft.value && [draft.value.kind, draft.value.url, draft.value.command, draft.value.args],
  () => {
    tested.value = null;
  },
  { deep: true },
);

onMounted(load);
watch(() => props.vaultPath, load);

const field =
  'w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50 ' +
  'text-sm text-text dark:text-text-dark placeholder-gray-500 dark:placeholder-gray-400 outline-none ' +
  'focus:border-accent dark:focus:border-accent-dark focus:ring-1 focus:ring-accent/20 dark:focus:ring-accent-dark/20 transition-all';
const plainButton =
  'px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer border border-gray-200 dark:border-gray-700/50 ' +
  'text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 transition-colors ' +
  'disabled:opacity-40 disabled:cursor-not-allowed';
const primaryButton = 'btn-primary';
const subtleDanger =
  'px-2 py-1 rounded-lg text-xs font-medium cursor-pointer text-red-600 dark:text-red-400 ' +
  'hover:bg-red-50 dark:hover:bg-red-500/10 transition-colors disabled:opacity-40';
</script>

<template>
  <!-- CONNECTORS
       Tools on other services. Its own section, like Telegram, and for the
       same reason: nothing here waits for the Save at the bottom. A server is
       saved, its secrets go to the keychain and it is connected the moment
       its own button is pressed, so the list always shows what is true. -->
  <section>
    <!-- Titled by the folding section around it. -->
    <div class="flex items-center justify-end mb-3">
      <button
        v-if="servers.length"
        type="button"
        class="text-xs text-accent dark:text-accent-dark hover:underline cursor-pointer disabled:opacity-40"
        :disabled="busy"
        @click="reconnect"
      >
        {{ t('syn.connector_reconnect') }}
      </button>
    </div>
    <p class="text-xs text-gray-500 dark:text-gray-400 leading-relaxed mb-3">{{ t('syn.connector_intro') }}</p>

    <ul v-if="servers.length" class="space-y-2 mb-3">
      <li
        v-for="view in servers"
        :key="view.server.id"
        class="p-3 rounded-xl border border-gray-200 dark:border-gray-700/50 space-y-1.5"
      >
        <div class="flex items-center gap-2 min-w-0">
          <input
            type="checkbox"
            :checked="view.server.enabled"
            :disabled="busy"
            :aria-label="t('syn.connector_enabled')"
            class="w-4 h-4 accent-accent cursor-pointer shrink-0"
            @change="toggle(view)"
          />
          <span class="text-sm font-medium text-text dark:text-text-dark truncate">{{ view.server.name }}</span>
          <span class="ml-auto flex items-center gap-1 shrink-0">
            <button type="button" :class="plainButton" :disabled="busy" @click="open(view)">
              {{ t('syn.connector_edit') }}
            </button>
            <button type="button" :class="subtleDanger" :disabled="busy" @click="confirmRemove(view)">
              {{ t('syn.connector_remove') }}
            </button>
          </span>
        </div>
        <div class="flex items-start gap-2 min-w-0">
          <span class="w-2 h-2 mt-1.5 rounded-full shrink-0" :class="statusLine(t, view).dot" />
          <span class="text-xs text-gray-500 dark:text-gray-400 break-words min-w-0">{{ statusLine(t, view).text }}</span>
        </div>
        <p v-if="removing === view.server.id" class="text-xs text-red-600 dark:text-red-400">
          {{ t('syn.connector_remove_confirm', { name: view.server.name }) }}
        </p>
      </li>
    </ul>
    <p v-else-if="!draft" class="text-xs text-gray-500 dark:text-gray-400 mb-3">{{ t('syn.connector_none') }}</p>

    <button v-if="!draft" type="button" :class="[plainButton, 'flex items-center gap-1.5']" :disabled="busy" @click="open()">
      <Plus class="w-3.5 h-3.5" />
      {{ t('syn.connector_add') }}
    </button>

    <!-- The form. -->
    <div v-else class="p-3 rounded-xl border border-accent/30 dark:border-accent-dark/30 space-y-3">
      <div>
        <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">{{ t('syn.connector_name') }}</label>
        <input v-model="draft.name" type="text" spellcheck="false" :class="field" :placeholder="t('syn.connector_name_placeholder')" />
      </div>

      <div class="flex flex-col gap-1.5 text-sm text-text dark:text-text-dark">
        <label class="flex items-center gap-2 cursor-pointer">
          <input v-model="draft.kind" type="radio" value="http" class="accent-accent" />
          {{ t('syn.connector_kind_http') }}
        </label>
        <label class="flex items-center gap-2 cursor-pointer">
          <input v-model="draft.kind" type="radio" value="stdio" class="accent-accent" />
          {{ t('syn.connector_kind_stdio') }}
        </label>
      </div>

      <div v-if="draft.kind === 'http'">
        <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">{{ t('syn.connector_url') }}</label>
        <input
          v-model="draft.url"
          type="url"
          spellcheck="false"
          autocomplete="off"
          :class="field"
          placeholder="https://example.com/…"
        />
        <p class="mt-1.5 text-xs text-gray-500 dark:text-gray-400 leading-relaxed">{{ t('syn.connector_url_hint') }}</p>
      </div>

      <template v-else>
        <!-- Said up front on a phone, as a fact about the platform rather
             than as a connection that failed. -->
        <p v-if="isMobileOS" class="text-xs text-amber-600 dark:text-amber-400">{{ t('syn.connector_stdio_desktop_only') }}</p>
        <div>
          <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">{{ t('syn.connector_command') }}</label>
          <input
            v-model="draft.command"
            type="text"
            spellcheck="false"
            autocomplete="off"
            :class="[field, 'font-mono']"
            placeholder="/usr/local/bin/my-connector"
          />
          <p class="mt-1.5 text-xs text-gray-500 dark:text-gray-400 leading-relaxed">{{ t('syn.connector_command_hint') }}</p>
        </div>
        <div>
          <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">{{ t('syn.connector_args') }}</label>
          <textarea v-model="draft.args" rows="3" spellcheck="false" :class="[field, 'font-mono resize-y']" />
        </div>
      </template>

      <!-- Secrets: names are saved with the server, values go to the keychain. -->
      <div>
        <label class="block text-sm font-medium text-text dark:text-text-dark mb-1.5">
          {{ draft.kind === 'http' ? t('syn.connector_secret_headers') : t('syn.connector_secret_env') }}
        </label>
        <div v-for="(secret, i) in draft.secrets" :key="i" class="flex gap-2 mb-2">
          <input
            v-model="secret.name"
            type="text"
            spellcheck="false"
            autocomplete="off"
            :class="[field, 'w-2/5 font-mono']"
            :placeholder="draft.kind === 'http' ? 'Authorization' : 'API_TOKEN'"
            :aria-label="t('syn.connector_secret_name')"
          />
          <input
            v-model="secret.value"
            type="password"
            spellcheck="false"
            autocomplete="off"
            :class="[field, 'flex-1 min-w-0']"
            :placeholder="secret.stored ? t('syn.connector_secret_stored') : t('syn.connector_secret_value')"
            :aria-label="t('syn.connector_secret_value')"
          />
          <button type="button" class="p-1.5 text-gray-500 dark:text-gray-400 hover:text-red-500 cursor-pointer" :aria-label="t('syn.connector_remove')" @click="dropSecret(i)">
            <X class="w-4 h-4" />
          </button>
        </div>
        <button type="button" class="text-xs text-accent dark:text-accent-dark hover:underline cursor-pointer" @click="addSecret">
          + {{ t('syn.connector_add_secret') }}
        </button>
        <p class="mt-1.5 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.connector_secret_hint') }}</p>
      </div>

      <!-- Said where the server is set up, once, as information: choosing a
           server is the choice. -->
      <p class="text-xs text-gray-600 dark:text-gray-300 leading-relaxed">{{ t('syn.connector_honest') }}</p>

      <div v-if="tested" class="text-xs space-y-1">
        <p v-if="tested.desktop_only" class="text-gray-500 dark:text-gray-400">{{ t('syn.connector_status_desktop_only') }}</p>
        <p v-else-if="!tested.ok" class="text-red-600 dark:text-red-400 break-words">
          {{ t('syn.connector_status_failed', { reason: tested.error ?? '' }) }}
        </p>
        <template v-else>
          <p class="text-green-700 dark:text-green-400">
            {{ tested.tools.length ? t('syn.connector_test_ok', { n: tested.tools.length }) : t('syn.connector_test_none') }}
          </p>
          <ul class="space-y-0.5 max-h-40 overflow-y-auto">
            <li v-for="tool in tested.tools" :key="tool.name" class="flex items-baseline gap-2 min-w-0">
              <span class="font-mono text-text dark:text-text-dark truncate">{{ tool.name }}</span>
              <span
                class="shrink-0 text-xs px-1.5 rounded-full"
                :class="tool.read_only
                  ? 'bg-gray-100 dark:bg-white/10 text-gray-600 dark:text-gray-300'
                  : 'bg-amber-100 dark:bg-amber-500/15 text-amber-700 dark:text-amber-300'"
              >
                {{ tool.read_only ? t('syn.connector_reads') : t('syn.connector_sends') }}
              </span>
            </li>
          </ul>
        </template>
      </div>

      <div class="flex items-center gap-2">
        <button type="button" :class="primaryButton" :disabled="busy || !draft.name.trim()" @click="saveDraft">
          <Loader2 v-if="busy" class="w-3.5 h-3.5 animate-spin" />
          {{ t('syn.connector_save') }}
        </button>
        <button type="button" :class="plainButton" :disabled="busy" @click="runTest">{{ t('syn.connector_test') }}</button>
        <button type="button" :class="[plainButton, 'ml-auto']" :disabled="busy" @click="close">{{ t('syn.connector_cancel') }}</button>
      </div>
    </div>

    <p v-if="error" class="mt-2 text-xs text-red-600 dark:text-red-400 break-words">{{ error }}</p>
  </section>
</template>
