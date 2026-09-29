<script setup lang="ts">
/**
 * Above the list when "Health" is chosen: what is wrong, how often, and the
 * breach check. Section 9.8 of `docs/safe-2026-09-28.md`.
 *
 * The breach check reaches the internet, so it only runs when the user turned
 * it on in settings and pressed the button — and even then five hex
 * characters of each hash are all that leave.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { ShieldAlert } from 'lucide-vue-next';
import type { HealthFlag, Overview, SafeApi } from './api';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi; overview: Overview | null; flag: HealthFlag | null; breachCheck: boolean }>();
const emit = defineEmits<{ (e: 'flag', flag: HealthFlag | null): void; (e: 'checked'): void }>();
const { t, locale } = useI18n();
const explain = useSafeError();

const ORDER: HealthFlag[] = ['breached', 'reused', 'weak', 'expired', 'expiring', 'old'];
const counts = computed(() => new Map(props.overview?.health ?? []));

const checking = ref(false);
const result = ref('');
async function check() {
  checking.value = true;
  result.value = '';
  try {
    const r = await props.api.checkBreaches();
    result.value = t('safe.health.checked', { checked: r.checked, breached: r.breached }) + (r.failed ? ' ' + t('safe.health.check_failed', { n: r.failed }) : '');
    emit('checked');
  } catch (e) {
    result.value = explain(e);
  } finally {
    checking.value = false;
  }
}

const lastChecked = computed(() =>
  props.overview?.breach_checked_at
    ? new Date(props.overview.breach_checked_at * 1000).toLocaleTimeString(locale.value, { hour: '2-digit', minute: '2-digit' })
    : null,
);
</script>

<template>
  <div class="p-3 border-b border-border dark:border-border-dark space-y-2.5">
    <div class="flex flex-wrap gap-1.5">
      <button
        class="px-2.5 py-1 rounded-full text-xs border"
        :class="flag === null ? 'bg-accent text-white border-accent' : 'border-border dark:border-border-dark'"
        @click="emit('flag', null)"
      >
        {{ t('safe.health.all', { n: overview?.unhealthy ?? 0 }) }}
      </button>
      <template v-for="f in ORDER" :key="f">
        <button
          v-if="counts.get(f)"
          class="px-2.5 py-1 rounded-full text-xs border"
          :class="flag === f ? 'bg-accent text-white border-accent' : 'border-border dark:border-border-dark'"
          @click="emit('flag', f)"
        >
          {{ t(`safe.health.flag.${f}`) }} · {{ counts.get(f) }}
        </button>
      </template>
    </div>
    <div class="flex items-center gap-2 text-xs text-text-secondary dark:text-text-secondary-dark">
      <ShieldAlert class="w-3.5 h-3.5 flex-shrink-0" />
      <template v-if="breachCheck">
        <button :disabled="checking" class="text-accent hover:underline disabled:opacity-40" @click="check">
          {{ checking ? t('safe.health.checking') : t('safe.health.check') }}
        </button>
        <span v-if="lastChecked && !result">· {{ t('safe.health.last_checked', { at: lastChecked }) }}</span>
      </template>
      <span v-else>{{ t('safe.health.check_off') }}</span>
    </div>
    <p v-if="result" class="text-xs" role="status">{{ result }}</p>
  </div>
</template>
