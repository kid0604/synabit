<script setup lang="ts">
/**
 * A strength bar under a master-password field.
 *
 * The estimate is Rust's (`health::strength`): zxcvbn on a computer, which
 * knows dictionaries, names, dates, keyboard walks and clever substitutions,
 * and a coarser count on a phone. Its 0–4 score is emitted so a form can
 * refuse anything below 3 — "guessable in hours offline" is not a master
 * password.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import type { SafeApi } from './api';

const props = defineProps<{ api: SafeApi; password: string }>();
const emit = defineEmits<{ (e: 'score', score: number): void }>();
const { t } = useI18n();

const score = ref(0);
const bits = ref(0);
let timer: ReturnType<typeof setTimeout> | undefined;
let asked = 0;

watch(
  () => props.password,
  (pw) => {
    clearTimeout(timer);
    if (!pw) {
      score.value = 0;
      bits.value = 0;
      emit('score', 0);
      return;
    }
    const mine = ++asked;
    // Until this password is judged, it is not strong enough: a form must not
    // pass on the score of the one typed before it.
    emit('score', 0);
    timer = setTimeout(async () => {
      const s = await props.api.estimate(pw).catch(() => ({ score: 0, bits: 0 }));
      if (mine !== asked) return;
      score.value = s.score;
      bits.value = s.bits;
      emit('score', s.score);
    }, 120);
  },
  { immediate: true },
);

/** Four steps on screen for five scores: 0 and 1 are both weak. */
const level = computed(() => Math.max(0, score.value - 1));
const label = computed(() => [t('safe.strength.weak'), t('safe.strength.fair'), t('safe.strength.strong'), t('safe.strength.very_strong')][level.value]);
const colour = computed(() => ['bg-danger', 'bg-warning', 'bg-success', 'bg-success'][level.value]);
</script>

<template>
  <div v-if="password" class="space-y-1" aria-live="polite">
    <div class="flex gap-1">
      <div v-for="i in 4" :key="i" class="h-1 flex-1 rounded-full" :class="i - 1 <= level ? colour : 'bg-border dark:bg-border-dark'" />
    </div>
    <p class="text-xs text-text-secondary dark:text-text-secondary-dark">
      {{ label }} · {{ t('safe.strength.bits', { n: Math.round(bits) }) }}
    </p>
  </div>
</template>
