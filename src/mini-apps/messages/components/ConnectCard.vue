<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { CircleCheck, HelpCircle, Lock, Plug } from 'lucide-vue-next';
import { useSynDeclared } from '../composables/useSynDeclared';
import {
  bridgesOf,
  commandText,
  hostOf,
  inLang,
  inputsOf,
  notificationPackages,
  sends,
  type Connected,
  type DeclaredInput,
} from '../declared';

/**
 * Connecting Syn to a service, in one card.
 *
 * The card asks only what the person alone knows — a key, their account,
 * their own server's address — each with where to find it, and one press of
 * Connect keeps the key, approves the connector on this device and tries it.
 * What it tried is said in plain words; what failed points at the field to
 * fix, and the card stays. Everything technical is decided, not asked. See
 * `docs/syn-connectors-simple-2026-10-12.md`.
 *
 * Every word comes from the declaration as Rust read it from the vault, or
 * from the locale — never from a model's message.
 */
const props = defineProps<{ vaultPath: string; connectorId: string }>();
const emit = defineEmits<{ done: [connected: boolean] }>();
const { t, te, locale } = useI18n();

const declared = useSynDeclared(() => props.vaultPath);
onMounted(async () => {
  await declared.load();
  const inst = view.value?.instance;
  for (const [k, v] of Object.entries(inst?.inputs ?? {})) values.value[k] = v;
  folder.value = inst?.folder ?? null;
  if (packages.value.length) access.value = await declared.device.access();
});

const view = computed(() => declared.declared.value.find((v) => v.manifest?.id === props.connectorId) ?? null);
const m = computed(() => view.value?.manifest ?? null);
const lang = computed(() => String(locale.value).slice(0, 2));

const title = computed(() => m.value?.name ?? props.connectorId);

const inputs = computed<DeclaredInput[]>(() => (m.value ? inputsOf(m.value) : []));
const values = ref<Record<string, string>>({});
const secrets = ref<Record<string, string>>({});
const open = ref<string | null>(null);

const isDevice = computed(() => m.value?.transport === 'device');
const isSsh = computed(() => m.value?.transport === 'ssh');
const bridges = computed(() => (m.value ? bridgesOf(m.value) : []));
const packages = computed(() => (m.value ? notificationPackages(m.value) : []));
const folder = ref<string | null>(null);
const agreed = ref(false);
const access = ref(false);

/** Where it reads: the declared host, the address being typed, or this device. */
const where = computed(() => {
  if (!m.value) return '';
  if (isDevice.value) return t('syn.connect_this_device');
  const address = inputs.value.find((i) => i.kind === 'address');
  if (address) {
    try {
      return new URL(values.value[address.name] ?? '').host || t('syn.connect_your_server');
    } catch {
      return t('syn.connect_your_server');
    }
  }
  return hostOf(m.value);
});

const busy = ref(false);
const result = ref<Connected | null>(null);

/** What a failure means, in the person's words. */
const problem = computed(() => {
  const r = result.value;
  if (!r || r.ok) return null;
  const field = inputs.value.find((i) => i.name === r.field);
  const label = field ? inLang(field.label, lang.value) : '';
  const key = `syn.connect_error_${r.code ?? 'server_error'}`;
  return te(key) ? t(key, { field: label, host: where.value }) : t('syn.connect_error_server_error', { host: where.value });
});

/** What trusting what the server showed is called: a certificate, or the
 * machine itself, first met or met again with a new key. */
const TRUST: Record<string, string> = {
  self_signed: 'syn.connect_trust',
  host_key_unknown: 'syn.connect_trust_host',
  host_key_changed: 'syn.connect_trust_new_key',
};
const trustLabel = computed(() => {
  const code = result.value?.code;
  return code && result.value?.cert && TRUST[code] ? t(TRUST[code]) : null;
});
const hostKey = computed(() => (result.value?.code?.startsWith('host_key_') ? (result.value.cert ?? null) : null));

const connect = async (trustCert: string | null = null) => {
  if (!m.value || busy.value) return;
  busy.value = true;
  result.value = await declared.connect(m.value.id, { ...values.value }, { ...secrets.value }, trustCert, folder.value);
  busy.value = false;
  secrets.value = {};
  if (result.value?.ok) emit('done', true);
};

const chooseFolder = async () => {
  const picked = await declared.device.chooseFolder();
  if (picked) folder.value = picked;
};

const openAccess = async () => {
  await declared.device.openAccess();
};

const recheckAccess = async () => {
  access.value = await declared.device.access();
};

/**
 * Approved here before, and changed since — by Syn adding a command, most
 * often. The address and the key are already known, so the card is about what
 * changed: the new commands first, the fields folded away. Shown open, they
 * read as being asked for the password again, which nobody was (2026-10-13).
 */
const again = computed(() => view.value?.standing.state === 'changed' && !!view.value.instance);
const editing = ref(false);
const kept = computed(
  () =>
    again.value &&
    !editing.value &&
    !result.value?.field &&
    inputs.value.every((i) => (i.kind === 'secret' ? !!view.value?.secret_here : !!values.value[i.name])),
);
/** The operations not approved as they now are, and the rest. */
const fresh = computed(() => new Set(again.value ? (view.value?.not_yet_approved ?? []) : []));
const newOps = computed(() => (m.value?.operations ?? []).filter((op) => fresh.value.size === 0 || fresh.value.has(op.name)));
const oldOps = computed(() => (fresh.value.size ? (m.value?.operations ?? []).filter((op) => !fresh.value.has(op.name)) : []));

const ready = computed(
  () =>
    !bridges.value.includes('folder.watch') || !!folder.value,
);
</script>

<template>
  <div class="rounded-xl border border-gray-200 dark:border-gray-700/60 bg-white dark:bg-white/5 p-4 space-y-3" data-test="connect-card">
    <div class="flex items-center gap-2">
      <Plug class="w-4 h-4 text-accent dark:text-accent-dark" aria-hidden="true" />
      <p class="text-sm font-medium text-text dark:text-text-dark">{{ title }}</p>
    </div>

    <template v-if="m">
      <p v-if="again" class="text-xs font-medium text-text dark:text-text-dark" data-test="connect-again">
        {{ fresh.size ? t('syn.connect_again_add', { where }) : t('syn.connect_again_intro', { where }) }}
      </p>
      <p v-else-if="isSsh" class="text-xs text-gray-600 dark:text-gray-300" data-test="connect-what">
        {{ t('syn.connect_runs_at', { where }) }} {{ t('syn.connect_runs_ask') }}
      </p>
      <p v-else class="text-xs text-gray-600 dark:text-gray-300" data-test="connect-what">
        {{ t('syn.connect_reads_at', { where }) }}
        {{ sends(m) ? t('syn.connect_sends_ask') : t('syn.connect_sends_nothing') }}
      </p>

      <div v-if="isSsh" class="space-y-1 text-xs" data-test="connect-commands">
        <p v-if="!again" class="text-gray-600 dark:text-gray-300">{{ t('syn.connect_commands') }}</p>
        <ul class="rounded-lg bg-gray-50 dark:bg-white/5 px-3 py-2 space-y-1.5">
          <li v-for="op in newOps" :key="op.name" :data-test="`command-${op.name}`">
            <p class="flex items-center gap-1.5 text-text dark:text-text-dark">
              <span
                v-if="fresh.has(op.name)"
                class="rounded px-1.5 py-px text-[10px] font-medium bg-accent/15 text-accent dark:text-accent-dark"
                data-test="command-new"
              >{{ t('syn.connect_new_badge') }}</span>
              {{ op.description }}
            </p>
            <code class="block font-mono text-gray-600 dark:text-gray-300 break-all">{{ commandText(op.command) }}</code>
          </li>
        </ul>
        <div v-if="oldOps.length" class="space-y-1 pt-1 text-gray-500 dark:text-gray-400" data-test="commands-allowed">
          <p>{{ t('syn.connect_already_allowed') }}</p>
          <ul class="space-y-0.5 px-3">
            <li v-for="op in oldOps" :key="op.name"><code class="font-mono break-all">{{ commandText(op.command) }}</code></li>
          </ul>
        </div>
      </div>

      <div v-if="kept" class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-gray-600 dark:text-gray-300" data-test="connect-kept">
        <Lock class="w-3 h-3" aria-hidden="true" />
        <span v-if="inputs.some((i) => i.kind === 'secret')">{{ t('syn.connect_kept_summary') }}</span>
        <button type="button" class="text-accent dark:text-accent-dark cursor-pointer" data-test="connect-change" @click="editing = true">
          {{ t('syn.connect_change_details') }}
        </button>
      </div>

      <div v-for="input in kept ? [] : inputs" :key="input.name" class="space-y-1" :data-test="`input-${input.name}`">
        <label class="text-xs font-medium text-gray-600 dark:text-gray-300" :for="`connect-${input.name}`">{{ inLang(input.label, lang) }}</label>
        <input
          v-if="input.kind === 'secret'"
          :id="`connect-${input.name}`"
          v-model="secrets[input.name]"
          type="password"
          autocomplete="off"
          class="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border text-sm text-text dark:text-text-dark outline-none focus:border-accent"
          :class="result?.field === input.name ? 'border-red-400' : 'border-gray-200 dark:border-gray-700/50'"
          :placeholder="view?.secret_here ? t('syn.connect_secret_kept') : (input.example ?? '')"
        />
        <input
          v-else
          :id="`connect-${input.name}`"
          v-model="values[input.name]"
          type="text"
          autocomplete="off"
          class="w-full px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border text-sm text-text dark:text-text-dark outline-none focus:border-accent"
          :class="result?.field === input.name ? 'border-red-400' : 'border-gray-200 dark:border-gray-700/50'"
          :placeholder="input.example ?? ''"
        />
        <button
          v-if="input.guide?.steps || input.guide?.link"
          type="button"
          class="flex items-center gap-1 text-xs text-accent dark:text-accent-dark cursor-pointer"
          :data-test="`guide-${input.name}`"
          @click="open = open === input.name ? null : input.name"
        >
          <HelpCircle class="w-3.5 h-3.5" aria-hidden="true" />{{ t('syn.connect_where_to_find') }}
        </button>
        <div v-if="open === input.name" class="rounded-lg bg-gray-50 dark:bg-white/5 px-3 py-2 text-xs text-gray-600 dark:text-gray-300 space-y-1">
          <ol class="list-decimal list-inside space-y-0.5">
            <li v-for="(step, i) in input.guide?.steps?.[lang] ?? input.guide?.steps?.en ?? []" :key="i">{{ step }}</li>
          </ol>
          <a v-if="input.guide?.link" :href="input.guide.link" target="_blank" rel="noopener" class="text-accent dark:text-accent-dark break-all">{{ input.guide.link }}</a>
          <p v-if="m.provenance === 'syn'" class="italic">{{ t('syn.connect_guide_by_syn', { host: where }) }}</p>
        </div>
      </div>

      <div v-if="isDevice && bridges.includes('folder.watch')" class="flex flex-wrap items-center gap-2 text-xs">
        <button type="button" class="btn-secondary" data-test="choose-folder" @click="chooseFolder">{{ t('syn.connect_choose_folder') }}</button>
        <span class="font-mono text-gray-600 dark:text-gray-300 break-all" data-test="folder">{{ folder ?? t('syn.connect_no_folder') }}</span>
      </div>

      <div v-if="packages.length" class="space-y-2 rounded-lg border border-amber-300/70 dark:border-amber-500/30 bg-amber-50/60 dark:bg-amber-500/5 p-3 text-xs" data-test="notification-disclosure">
        <p class="text-text dark:text-text-dark">{{ t('syn.connect_notifications_disclosure', { apps: packages.join(', ') }) }}</p>
        <label class="flex items-start gap-1.5 cursor-pointer text-text dark:text-text-dark">
          <input v-model="agreed" type="checkbox" class="mt-0.5 accent-accent" data-test="notifications-agree" />
          {{ t('syn.connect_notifications_agree') }}
        </label>
        <ol v-if="agreed && !access" class="list-decimal list-inside text-gray-600 dark:text-gray-300 space-y-0.5">
          <li>{{ t('syn.connect_notifications_step1') }}</li>
          <li>{{ t('syn.connect_notifications_step2') }}</li>
        </ol>
        <div v-if="agreed" class="flex flex-wrap gap-2">
          <button v-if="!access" type="button" class="btn-secondary" data-test="open-access" @click="openAccess">{{ t('syn.connect_notifications_open') }}</button>
          <button v-if="!access" type="button" class="btn-secondary" @click="recheckAccess">{{ t('syn.connect_notifications_done') }}</button>
          <span v-else class="text-green-700 dark:text-green-400">{{ t('syn.connect_notifications_on') }}</span>
        </div>
      </div>

      <p v-if="!kept && inputs.some((i) => i.kind === 'secret')" class="flex items-center gap-1 text-xs text-gray-500 dark:text-gray-400">
        <Lock class="w-3 h-3" aria-hidden="true" />{{ t('syn.connect_secret_note') }}
      </p>

      <p v-if="result?.ok" class="flex items-center gap-1.5 text-xs text-green-700 dark:text-green-400" data-test="connect-ok">
        <CircleCheck class="w-4 h-4" aria-hidden="true" />
        {{ result.rows != null ? t('syn.connect_ok_rows', { n: result.rows }, result.rows) : t('syn.connect_ok') }}
      </p>
      <div v-else-if="problem" class="space-y-1.5 text-xs" data-test="connect-problem">
        <p class="text-red-600 dark:text-red-400">{{ problem }}</p>
        <code v-if="hostKey" class="block font-mono text-text dark:text-text-dark break-all" data-test="host-key">{{ hostKey }}</code>
        <button v-if="trustLabel && result?.cert" type="button" class="btn-secondary" data-test="trust" @click="connect(result.cert)">
          {{ trustLabel }}
        </button>
        <details v-if="result?.said" class="text-gray-500 dark:text-gray-400">
          <summary class="cursor-pointer">{{ t('syn.connect_details') }}</summary>
          <p class="break-all">{{ result.said }}</p>
        </details>
      </div>

      <div v-if="!result?.ok" class="flex items-center justify-end gap-2">
        <button type="button" class="btn-secondary" :disabled="busy" @click="emit('done', false)">{{ t('syn.connect_not_now') }}</button>
        <button
          type="button"
          class="btn-primary"
          data-test="connect"
          :disabled="busy || !ready || (packages.length > 0 && !agreed)"
          @click="connect()"
        >
          {{ busy ? t('syn.connect_trying') : again ? t('syn.connect_allow') : t('syn.connect_go') }}
        </button>
      </div>
    </template>
    <!-- Gone from the vault — deleted, or never filed — and still asked about.
         Without a button the card could not be closed, and the run behind it
         waited for an answer nobody could give (2026-10-15). -->
    <div v-else-if="!declared.busy.value" class="space-y-3">
      <p class="text-xs text-red-600 dark:text-red-400" data-test="connect-missing">{{ t('syn.connect_missing') }}</p>
      <div class="flex justify-end">
        <button type="button" class="btn-secondary" data-test="connect-dismiss" @click="emit('done', false)">{{ t('syn.connect_not_now') }}</button>
      </div>
    </div>
  </div>
</template>
