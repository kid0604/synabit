<script setup lang="ts">
/**
 * The generator, inside the editor.
 *
 * The value it shows is generated in Rust and sent here, because the editor is
 * the one screen where a value is in the WebView by necessity — the user is
 * choosing it. The strength shown is the recipe's exact entropy, not a guess.
 */
import { computed, onMounted, reactive, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { RotateCcw } from 'lucide-vue-next';
import type { Recipe, SafeApi } from './api';

const props = defineProps<{ api: SafeApi }>();
const emit = defineEmits<{ (e: 'use', value: string): void }>();
const { t } = useI18n();

const mode = ref<Recipe['mode']>('random');
const random = reactive({ length: 20, lower: true, upper: true, digits: true, symbols: true, avoid_ambiguous: false });
const phrase = reactive({ words: 6, separator: '-', capitalise: false, digit: false });
const pin = reactive({ length: 6 });

const recipe = computed<Recipe>(() =>
  mode.value === 'random' ? { mode: 'random', ...random } : mode.value === 'passphrase' ? { mode: 'passphrase', ...phrase } : { mode: 'pin', ...pin },
);

const value = ref('');
const bits = ref(0);
const failed = ref(false);

async function again() {
  try {
    const g = await props.api.generate(recipe.value);
    value.value = g.value;
    bits.value = g.bits;
    failed.value = false;
  } catch {
    failed.value = true;
  }
}

watch(recipe, again, { deep: true });
onMounted(again);
</script>

<template>
  <div class="space-y-3 p-3 rounded-xl border border-border dark:border-border-dark bg-base dark:bg-base-dark">
    <div class="flex gap-1 p-0.5 rounded-lg bg-surface dark:bg-surface-dark text-xs" role="tablist">
      <button
        v-for="m in (['random', 'passphrase', 'pin'] as const)"
        :key="m"
        type="button"
        role="tab"
        :aria-selected="mode === m"
        class="flex-1 py-1 rounded-md"
        :class="mode === m ? 'bg-base dark:bg-base-dark shadow-sm font-medium' : 'text-text-secondary dark:text-text-secondary-dark'"
        @click="mode = m"
      >
        {{ t(`safe.generator.${m}`) }}
      </button>
    </div>

    <div class="flex items-center gap-2">
      <output class="flex-1 min-w-0 px-2.5 py-2 rounded-lg bg-surface dark:bg-surface-dark font-mono text-sm break-all">{{ failed ? '—' : value }}</output>
      <button type="button" class="p-2 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark" :aria-label="t('safe.generator.again')" :title="t('safe.generator.again')" @click="again">
        <RotateCcw class="w-4 h-4" />
      </button>
    </div>
    <p class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.generator.bits', { n: Math.round(bits) }) }}</p>

    <div v-if="mode === 'random'" class="space-y-2 text-sm">
      <label class="flex items-center gap-3">
        <span class="w-20">{{ t('safe.generator.length') }}</span>
        <input v-model.number="random.length" type="range" min="8" max="64" class="flex-1" />
        <span class="w-8 text-right tabular-nums">{{ random.length }}</span>
      </label>
      <div class="flex flex-wrap gap-x-4 gap-y-1">
        <label v-for="k in (['lower', 'upper', 'digits', 'symbols'] as const)" :key="k" class="inline-flex items-center gap-1.5">
          <input v-model="random[k]" type="checkbox" /> {{ t(`safe.generator.${k}`) }}
        </label>
      </div>
      <label class="inline-flex items-center gap-1.5">
        <input v-model="random.avoid_ambiguous" type="checkbox" /> {{ t('safe.generator.avoid_ambiguous') }}
      </label>
    </div>

    <div v-else-if="mode === 'passphrase'" class="space-y-2 text-sm">
      <label class="flex items-center gap-3">
        <span class="w-20">{{ t('safe.generator.words') }}</span>
        <input v-model.number="phrase.words" type="range" min="4" max="10" class="flex-1" />
        <span class="w-8 text-right tabular-nums">{{ phrase.words }}</span>
      </label>
      <div class="flex flex-wrap gap-x-4 gap-y-1">
        <label class="inline-flex items-center gap-1.5"><input v-model="phrase.capitalise" type="checkbox" /> {{ t('safe.generator.capitalise') }}</label>
        <label class="inline-flex items-center gap-1.5"><input v-model="phrase.digit" type="checkbox" /> {{ t('safe.generator.add_digit') }}</label>
      </div>
    </div>

    <div v-else class="text-sm">
      <label class="flex items-center gap-3">
        <span class="w-20">{{ t('safe.generator.length') }}</span>
        <input v-model.number="pin.length" type="range" min="4" max="12" class="flex-1" />
        <span class="w-8 text-right tabular-nums">{{ pin.length }}</span>
      </label>
    </div>

    <button type="button" :disabled="failed || !value" class="w-full py-1.5 rounded-lg bg-accent text-white text-sm font-medium hover:opacity-90 disabled:opacity-40" @click="emit('use', value)">
      {{ t('safe.generator.use') }}
    </button>
  </div>
</template>
