<script setup lang="ts">
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Loader2 } from 'lucide-vue-next';
import { openUrl } from '@tauri-apps/plugin-opener';
import { logger } from '../../../utils/logger';
import { useTelegram } from '../composables/useTelegram';

const { t } = useI18n();
const { status, tokenDraft, busy, error, pairing, connect, disconnect, startPairing, unpair, retry, setReminders } =
  useTelegram();

const copied = ref(false);

/** One dot and one sentence for how the bot is, once it has a token. */
const state = computed(() => {
  const s = status.value;
  if (!s?.has_token) return null;
  if (s.problem === 'conflict') return { dot: 'bg-amber-500', text: t('syn.telegram_conflict') };
  if (s.problem === 'unauthorized') return { dot: 'bg-red-500', text: t('syn.telegram_unauthorized') };
  if (s.problem === 'network') return { dot: 'bg-amber-500', text: t('syn.telegram_network') };
  if (s.problem === 'keychain') return { dot: 'bg-amber-500', text: t('syn.telegram_keychain') };
  if (s.running) return { dot: 'bg-green-500', text: t('syn.telegram_running') };
  return { dot: 'bg-gray-300 dark:bg-gray-600', text: t('syn.telegram_starting') };
});

/** The two problems that wait for a person rather than retrying on their own. */
const waitsForYou = computed(
  () => status.value?.problem === 'conflict' || status.value?.problem === 'unauthorized',
);

const openLink = () => {
  if (!pairing.value) return;
  openUrl(pairing.value.link).catch(e => logger.error('[Telegram] Could not open the pairing link', e));
};

const copyLink = async () => {
  if (!pairing.value) return;
  try {
    await navigator.clipboard.writeText(pairing.value.link);
    copied.value = true;
    setTimeout(() => {
      copied.value = false;
    }, 2000);
  } catch (e) {
    logger.error('[Telegram] Could not copy the pairing link', e);
  }
};

const subtleDanger =
  'shrink-0 px-3 py-1.5 rounded-lg text-sm font-medium cursor-pointer text-red-600 dark:text-red-400 ' +
  'hover:bg-red-50 dark:hover:bg-red-500/10 border border-gray-200 dark:border-gray-700/50 transition-colors ' +
  'disabled:opacity-40 disabled:cursor-not-allowed';
const plainButton =
  'px-3 py-2 rounded-lg text-sm font-medium cursor-pointer border border-gray-200 dark:border-gray-700/50 ' +
  'text-text dark:text-text-dark hover:bg-gray-50 dark:hover:bg-white/5 transition-colors ' +
  'disabled:opacity-40 disabled:cursor-not-allowed';
const primaryButton =
  'flex items-center gap-1.5 px-3 py-2 rounded-lg text-sm font-medium cursor-pointer bg-violet-500 text-white ' +
  'hover:bg-violet-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed';
</script>

<template>
  <!-- TELEGRAM
       A second way to reach the same Syn, from a phone. Its own section rather
       than part of the form above: nothing here waits for Save. A token is
       checked with Telegram the moment it is given, and a pairing happens on
       the phone, so every control acts at once. -->
  <section>
    <h3 class="text-[11px] font-bold uppercase tracking-wider text-gray-400 dark:text-gray-500 mb-3">
      {{ t('syn.telegram_title') }}
    </h3>
    <p class="text-xs text-gray-500 dark:text-gray-400 leading-relaxed mb-3">
      {{ t('syn.telegram_intro') }}
    </p>

    <p v-if="status && !status.supported" class="text-xs text-gray-500 dark:text-gray-400">
      {{ t('syn.telegram_desktop_only') }}
    </p>

    <!-- No token on this computer: the one thing to do is give it one. -->
    <div v-else-if="status && !status.has_token" class="space-y-2">
      <label class="block text-sm font-medium text-text dark:text-text-dark">
        {{ t('syn.telegram_token') }}
      </label>
      <div class="flex gap-2">
        <input
          v-model="tokenDraft"
          type="password"
          spellcheck="false"
          autocomplete="off"
          class="flex-1 min-w-0 px-3 py-2 rounded-lg bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700/50
                 text-sm text-text dark:text-text-dark placeholder-gray-400 outline-none
                 focus:border-violet-400 dark:focus:border-violet-500/50 focus:ring-1 focus:ring-violet-400/20
                 transition-all"
          :placeholder="t('syn.telegram_token_placeholder')"
          @keydown.enter="connect"
        />
        <button type="button" :class="primaryButton" :disabled="busy || !tokenDraft.trim()" @click="connect">
          <Loader2 v-if="busy" class="w-3.5 h-3.5 animate-spin" />
          {{ t('syn.telegram_connect') }}
        </button>
      </div>
      <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('syn.telegram_token_desc') }}</p>
      <!-- Said where the token is typed, once, and as information rather than
           a box to tick: typing the token is the choice. -->
      <p class="text-[11px] text-gray-500 dark:text-gray-400 leading-relaxed">
        {{ t('syn.telegram_privacy') }}
      </p>
    </div>

    <div v-else-if="status && state" class="space-y-3">
      <div class="flex items-center gap-2 min-w-0">
        <span class="w-2 h-2 rounded-full shrink-0" :class="state.dot" />
        <span class="text-sm text-text dark:text-text-dark truncate">{{ state.text }}</span>
        <span v-if="status.bot_username" class="text-xs text-gray-500 dark:text-gray-400 truncate">
          @{{ status.bot_username }}
        </span>
        <button type="button" :class="['ml-auto', subtleDanger]" :disabled="busy" @click="disconnect">
          {{ t('syn.telegram_remove_token') }}
        </button>
      </div>

      <button v-if="waitsForYou" type="button" :class="plainButton" :disabled="busy" @click="retry">
        {{ t('syn.telegram_retry') }}
      </button>

      <template v-if="status.paired">
        <div class="flex items-center gap-2 min-w-0">
          <span class="text-sm text-text dark:text-text-dark truncate">
            {{ t('syn.telegram_paired_with', { name: status.paired.name }) }}
          </span>
          <button type="button" :class="['ml-auto', subtleDanger]" :disabled="busy" @click="unpair">
            {{ t('syn.telegram_unpair') }}
          </button>
        </div>
        <!-- Only once paired: before that there is nowhere to send them. -->
        <label class="flex items-start gap-3 cursor-pointer">
          <input
            type="checkbox"
            :checked="status.reminders"
            :disabled="busy"
            class="mt-0.5 w-4 h-4 accent-violet-500 cursor-pointer"
            @change="setReminders(($event.target as HTMLInputElement).checked)"
          />
          <span class="min-w-0">
            <span class="block text-sm text-text dark:text-text-dark">{{ t('syn.telegram_reminders') }}</span>
            <span class="block text-[11px] text-gray-500 dark:text-gray-400 mt-0.5 leading-relaxed">
              {{ t('syn.telegram_reminders_hint') }}
            </span>
          </span>
        </label>
      </template>
      <template v-else>
        <button v-if="!pairing" type="button" :class="plainButton" :disabled="busy" @click="startPairing">
          {{ t('syn.telegram_pair') }}
        </button>
        <div v-else class="p-3 rounded-xl border border-gray-200 dark:border-gray-700/50 space-y-2">
          <p class="text-xs text-gray-600 dark:text-gray-300 leading-relaxed">{{ t('syn.telegram_pair_hint') }}</p>
          <p class="text-[11px] font-mono break-all text-gray-500 dark:text-gray-400 select-all">{{ pairing.link }}</p>
          <div class="flex gap-2">
            <button type="button" :class="primaryButton" @click="openLink">{{ t('syn.telegram_pair_open') }}</button>
            <button type="button" :class="plainButton" @click="copyLink">
              {{ copied ? t('syn.telegram_copied') : t('syn.telegram_copy') }}
            </button>
          </div>
          <p class="text-[11px] text-gray-500 dark:text-gray-400">{{ t('syn.telegram_pair_expires') }}</p>
        </div>
      </template>

      <p v-if="status.pending > 0" class="text-xs text-gray-500 dark:text-gray-400">
        {{ t('syn.telegram_pending', { n: status.pending }) }}
      </p>
    </div>

    <p v-if="error" class="mt-2 text-xs text-red-600 dark:text-red-400">{{ error }}</p>
  </section>
</template>
