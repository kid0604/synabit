<script setup lang="ts">
/**
 * Creating a Safe: why, a master password, the Secret Key, and a check that
 * the Secret Key was really written down.
 *
 * The check is the step people skip in other password managers and regret a
 * year later. Asking for three words from the twelve costs ten seconds and is
 * the only proof, short of losing the device, that the kit exists.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { save } from '@tauri-apps/plugin-dialog';
import { AlertTriangle, Check, Dices, Eye, EyeOff, KeyRound, Loader2, ShieldCheck } from 'lucide-vue-next';
import type { SafeApi } from './api';
import PasswordStrength from './PasswordStrength.vue';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi }>();
const emit = defineEmits<{ (e: 'done'): void }>();
const { t, tm, rt } = useI18n();
const explain = useSafeError();

type Step = 'intro' | 'password' | 'creating' | 'kit' | 'verify';
const step = ref<Step>('intro');
const error = ref('');

const password = ref('');
const confirm = ref('');
const shown = ref(false);
const bits = ref(0);
/** Below this, the form refuses. The meter asks for more. */
const MIN_BITS = 50;

const canCreate = computed(
  () => password.value.length >= 10 && password.value === confirm.value && bits.value >= MIN_BITS,
);

async function suggest() {
  const { value } = await props.api.generate({ mode: 'passphrase', words: 6, separator: '-', capitalise: false, digit: false });
  password.value = value;
  confirm.value = value;
  shown.value = true;
}

const words = ref<string[]>([]);
const storedOnDevice = ref(true);

async function create() {
  if (!canCreate.value) return;
  error.value = '';
  step.value = 'creating';
  try {
    const created = await props.api.create(password.value);
    words.value = created.secret_key.split(' ');
    storedOnDevice.value = created.stored_on_device;
    step.value = 'kit';
  } catch (e) {
    error.value = explain(e);
    step.value = 'password';
  } finally {
    password.value = '';
    confirm.value = '';
  }
}

const kitSaved = ref(false);
const acknowledged = ref(false);

async function saveKit() {
  error.value = '';
  const path = await save({
    defaultPath: 'Synabit Safe Emergency Kit.html',
    filters: [{ name: 'HTML', extensions: ['html'] }],
  });
  if (!path) return;
  try {
    await props.api.saveEmergencyKit(path);
    kitSaved.value = true;
  } catch (e) {
    error.value = explain(e);
  }
}

/** Three distinct positions, 1-based, in order. */
const asked = ref<number[]>([]);
const answers = ref<string[]>(['', '', '']);
const wrong = ref(false);

function toVerify() {
  const picks = new Set<number>();
  const random = new Uint32Array(8);
  crypto.getRandomValues(random);
  for (const r of random) {
    if (picks.size === 3) break;
    picks.add((r % 12) + 1);
  }
  let n = 1;
  while (picks.size < 3) picks.add(n++);
  asked.value = [...picks].sort((a, b) => a - b);
  answers.value = ['', '', ''];
  wrong.value = false;
  step.value = 'verify';
}

function finish() {
  const ok = asked.value.every((pos, i) => answers.value[i].trim().toLowerCase() === words.value[pos - 1]);
  if (!ok) {
    wrong.value = true;
    return;
  }
  words.value = [];
  answers.value = ['', '', ''];
  emit('done');
}

const points = computed(() => (tm('safe.intro.points') as unknown[]).map((p) => rt(p as never)));
</script>

<template>
  <div class="h-full overflow-y-auto flex items-start justify-center px-4 py-10" data-tauri-drag-region>
    <div class="w-full max-w-md">
      <!-- Intro -->
      <section v-if="step === 'intro'" class="space-y-6">
        <div class="w-14 h-14 rounded-2xl bg-accent/10 flex items-center justify-center">
          <KeyRound class="w-7 h-7 text-accent" />
        </div>
        <div class="space-y-2">
          <h1 class="text-2xl font-semibold">{{ t('safe.intro.title') }}</h1>
          <p class="text-text-secondary dark:text-text-secondary-dark">{{ t('safe.intro.body') }}</p>
        </div>
        <ul class="space-y-3">
          <li v-for="(p, i) in points" :key="i" class="flex gap-3 text-sm">
            <ShieldCheck class="w-4 h-4 mt-0.5 flex-shrink-0 text-success" />
            <span>{{ p }}</span>
          </li>
        </ul>
        <button class="w-full py-2.5 rounded-xl bg-accent text-white font-medium hover:opacity-90" @click="step = 'password'">
          {{ t('safe.intro.start') }}
        </button>
      </section>

      <!-- Master password -->
      <form v-else-if="step === 'password'" class="space-y-5" @submit.prevent="create">
        <div class="space-y-1">
          <h1 class="text-xl font-semibold">{{ t('safe.setup.password_title') }}</h1>
          <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.setup.password_body') }}</p>
        </div>
        <label class="block space-y-1.5">
          <span class="text-sm font-medium">{{ t('safe.setup.password') }}</span>
          <div class="relative">
            <input
              v-model="password"
              :type="shown ? 'text' : 'password'"
              autocomplete="off" spellcheck="false" autocapitalize="off"
              class="w-full pl-3 pr-10 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark font-mono focus:outline-none focus:ring-2 focus:ring-accent"
              autofocus
            />
            <button type="button" class="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-text-tertiary dark:text-text-tertiary-dark" :aria-label="shown ? t('safe.detail.hide') : t('safe.detail.reveal')" @click="shown = !shown">
              <EyeOff v-if="shown" class="w-4 h-4" /><Eye v-else class="w-4 h-4" />
            </button>
          </div>
        </label>
        <PasswordStrength :api="api" :password="password" @bits="bits = $event" />
        <label class="block space-y-1.5">
          <span class="text-sm font-medium">{{ t('safe.setup.confirm') }}</span>
          <input
            v-model="confirm"
            :type="shown ? 'text' : 'password'"
            autocomplete="off" spellcheck="false" autocapitalize="off"
            class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark font-mono focus:outline-none focus:ring-2 focus:ring-accent"
          />
        </label>
        <p v-if="confirm && confirm !== password" class="text-sm text-danger">{{ t('safe.setup.mismatch') }}</p>
        <button type="button" class="inline-flex items-center gap-1.5 text-sm text-accent hover:underline" @click="suggest">
          <Dices class="w-4 h-4" /> {{ t('safe.setup.suggest') }}
        </button>
        <div class="flex gap-2.5 p-3 rounded-lg bg-warning/10 text-sm">
          <AlertTriangle class="w-4 h-4 mt-0.5 flex-shrink-0 text-warning" />
          <span>{{ t('safe.setup.warning') }}</span>
        </div>
        <p v-if="error" class="text-sm text-danger" role="alert">{{ error }}</p>
        <button type="submit" :disabled="!canCreate" class="w-full py-2.5 rounded-xl bg-accent text-white font-medium hover:opacity-90 disabled:opacity-40">
          {{ t('safe.setup.create') }}
        </button>
      </form>

      <!-- Creating -->
      <section v-else-if="step === 'creating'" class="py-20 flex flex-col items-center gap-3 text-center" aria-live="polite">
        <Loader2 class="w-6 h-6 animate-spin text-accent" />
        <p class="font-medium">{{ t('safe.setup.creating') }}</p>
        <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.setup.creating_hint') }}</p>
      </section>

      <!-- Secret Key -->
      <section v-else-if="step === 'kit'" class="space-y-5">
        <div class="space-y-1">
          <h1 class="text-xl font-semibold">{{ t('safe.kit.title') }}</h1>
          <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.kit.body') }}</p>
        </div>
        <ol class="grid grid-cols-3 gap-2 select-text">
          <li v-for="(w, i) in words" :key="i" class="px-2.5 py-2 rounded-lg border border-border dark:border-border-dark bg-surface dark:bg-surface-dark font-mono text-sm">
            <span class="text-text-tertiary dark:text-text-tertiary-dark mr-1.5">{{ i + 1 }}</span>{{ w }}
          </li>
        </ol>
        <div v-if="!storedOnDevice" class="flex gap-2.5 p-3 rounded-lg bg-warning/10 text-sm">
          <AlertTriangle class="w-4 h-4 mt-0.5 flex-shrink-0 text-warning" />
          <span>{{ t('safe.kit.not_stored') }}</span>
        </div>
        <button v-if="storedOnDevice" class="w-full py-2 rounded-xl border border-border dark:border-border-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-sm font-medium inline-flex items-center justify-center gap-2" @click="saveKit">
          <Check v-if="kitSaved" class="w-4 h-4 text-success" />
          {{ kitSaved ? t('safe.kit.saved') : t('safe.kit.save') }}
        </button>
        <p v-if="error" class="text-sm text-danger" role="alert">{{ error }}</p>
        <label class="flex gap-2.5 items-start text-sm">
          <input v-model="acknowledged" type="checkbox" class="mt-0.5" />
          <span>{{ t('safe.kit.confirm') }}</span>
        </label>
        <button :disabled="!acknowledged" class="w-full py-2.5 rounded-xl bg-accent text-white font-medium hover:opacity-90 disabled:opacity-40" @click="toVerify">
          {{ t('safe.kit.next') }}
        </button>
      </section>

      <!-- Verify -->
      <form v-else-if="step === 'verify'" class="space-y-5" @submit.prevent="finish">
        <div class="space-y-1">
          <h1 class="text-xl font-semibold">{{ t('safe.kit.verify_title') }}</h1>
          <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t('safe.kit.verify_body') }}</p>
        </div>
        <label v-for="(pos, i) in asked" :key="pos" class="block space-y-1.5">
          <span class="text-sm font-medium">{{ t('safe.kit.word', { n: pos }) }}</span>
          <input
            v-model="answers[i]"
            autocomplete="off" spellcheck="false" autocapitalize="off"
            class="w-full px-3 py-2 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark font-mono focus:outline-none focus:ring-2 focus:ring-accent"
          />
        </label>
        <p v-if="wrong" class="text-sm text-danger" role="alert">{{ t('safe.kit.verify_wrong') }}</p>
        <div class="flex gap-2">
          <button type="button" class="flex-1 py-2.5 rounded-xl border border-border dark:border-border-dark text-sm" @click="step = 'kit'">
            {{ t('safe.kit.back') }}
          </button>
          <button type="submit" class="flex-1 py-2.5 rounded-xl bg-accent text-white font-medium hover:opacity-90">
            {{ t('safe.kit.done') }}
          </button>
        </div>
      </form>
    </div>
  </div>
</template>
