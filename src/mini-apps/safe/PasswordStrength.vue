<script setup lang="ts">
/**
 * A strength bar under a master-password field.
 *
 * The estimate is Rust's (`generator::estimate_bits`), which is pessimistic on
 * purpose: it counts a run like `aaaa` or `1234` as one character. It still
 * over-rates clever substitutions; zxcvbn replaces it later. `bits` is emitted
 * so the form can refuse a password below the floor.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import type { SafeApi } from './api';

const props = defineProps<{ api: SafeApi; password: string }>();
const emit = defineEmits<{ (e: 'bits', bits: number): void }>();
const { t } = useI18n();

const bits = ref(0);
let timer: ReturnType<typeof setTimeout> | undefined;
let asked = 0;

watch(
  () => props.password,
  (pw) => {
    clearTimeout(timer);
    if (!pw) {
      bits.value = 0;
      emit('bits', 0);
      return;
    }
    const mine = ++asked;
    timer = setTimeout(async () => {
      const b = await props.api.estimate(pw).catch(() => 0);
      if (mine !== asked) return;
      bits.value = b;
      emit('bits', b);
    }, 120);
  },
  { immediate: true },
);

const level = computed(() => (bits.value < 40 ? 0 : bits.value < 60 ? 1 : bits.value < 80 ? 2 : 3));
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
