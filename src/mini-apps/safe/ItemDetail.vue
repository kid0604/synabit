<script setup lang="ts">
/**
 * One item, read-only, with the actions on it.
 *
 * A concealed field shows dots in steps of its rough length — never its real
 * length. The eye asks Rust for that one value and hides it again after thirty
 * seconds or when the item changes; Copy asks Rust to put it on the clipboard
 * and the value never comes here at all.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Copy, ExternalLink, Eye, EyeOff, Pencil, RotateCcw, Star, Trash2 } from 'lucide-vue-next';
import ConfirmModal from '../../shared/components/ConfirmModal.vue';
import type { FieldView, ItemView, SafeApi } from './api';
import { kindInfo } from './kinds';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi; item: ItemView }>();
const emit = defineEmits<{ (e: 'edit'): void; (e: 'changed'): void; (e: 'error', err: unknown): void }>();
const { t, locale } = useI18n();
const explain = useSafeError();

/** Values the user asked to see, by field id. Cleared on a timer and on leaving. */
const revealed = ref<Record<string, string>>({});
const REVEAL_FOR_MS = 30_000;
const timers = new Map<string, ReturnType<typeof setTimeout>>();

function hide(fieldId?: string) {
  if (fieldId) {
    clearTimeout(timers.get(fieldId));
    timers.delete(fieldId);
    const { [fieldId]: _, ...rest } = revealed.value;
    revealed.value = rest;
    return;
  }
  timers.forEach(clearTimeout);
  timers.clear();
  revealed.value = {};
}

async function toggle(f: FieldView) {
  if (revealed.value[f.id] !== undefined) return hide(f.id);
  try {
    const value = await props.api.reveal(props.item.id, f.id);
    revealed.value = { ...revealed.value, [f.id]: value };
    timers.set(f.id, setTimeout(() => hide(f.id), REVEAL_FOR_MS));
  } catch (e) {
    emit('error', e);
  }
}

const toast = ref('');
let toastTimer: ReturnType<typeof setTimeout> | undefined;
function say(text: string) {
  toast.value = text;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.value = ''), 2500);
}

async function copy(f: FieldView) {
  try {
    if (f.concealed) {
      const { clear_after_secs } = await props.api.copy(props.item.id, f.id);
      say(clear_after_secs ? t('safe.detail.copied', { n: clear_after_secs }) : t('safe.detail.copied_kept'));
    } else if (f.value) {
      await navigator.clipboard.writeText(f.value);
      say(t('safe.detail.copied_kept'));
    }
  } catch (e) {
    say(explain(e));
  }
}

watch(() => props.item.id, () => hide());
onBeforeUnmount(() => {
  hide();
  clearTimeout(toastTimer);
});

function dots(bucket: number | null) {
  return '•'.repeat(bucket ?? 8);
}

async function favorite() {
  try {
    await props.api.setFavorite(props.item.id, !props.item.favorite);
    emit('changed');
  } catch (e) {
    emit('error', e);
  }
}

async function trash(trashed: boolean) {
  try {
    await props.api.setTrashed(props.item.id, trashed);
    emit('changed');
  } catch (e) {
    emit('error', e);
  }
}

const confirmPurge = ref(false);
async function purge() {
  confirmPurge.value = false;
  try {
    await props.api.purge(props.item.id);
    emit('changed');
  } catch (e) {
    emit('error', e);
  }
}

function open(url: string) {
  const href = /^[a-z][a-z0-9+.-]*:/i.test(url) ? url : `https://${url}`;
  openUrl(href).catch((e) => emit('error', e));
}

const updated = computed(() =>
  t('safe.detail.updated', { date: new Date(props.item.updated_at * 1000).toLocaleString(locale.value, { dateStyle: 'medium', timeStyle: 'short' }) }),
);
const trashed = computed(() => props.item.trashed_at !== null);
</script>

<template>
  <div class="relative h-full flex flex-col">
    <header class="px-6 pt-6 pb-4 flex items-start gap-4">
      <div class="w-11 h-11 rounded-xl bg-accent/10 flex items-center justify-center flex-shrink-0">
        <component :is="kindInfo(item.kind).icon" class="w-5 h-5 text-accent" />
      </div>
      <div class="flex-1 min-w-0">
        <h2 class="text-xl font-semibold truncate">{{ item.title }}</h2>
        <p class="text-sm text-text-secondary dark:text-text-secondary-dark">{{ t(`safe.kind.${item.kind}`) }}</p>
      </div>
      <div class="flex items-center gap-1">
        <template v-if="!trashed">
          <button class="p-2 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark" :aria-label="item.favorite ? t('safe.detail.unfavorite') : t('safe.detail.favorite')" :title="item.favorite ? t('safe.detail.unfavorite') : t('safe.detail.favorite')" @click="favorite">
            <Star class="w-4 h-4" :class="item.favorite ? 'fill-warning text-warning' : ''" />
          </button>
          <button class="p-2 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark" :aria-label="t('safe.detail.trash')" :title="t('safe.detail.trash')" @click="trash(true)">
            <Trash2 class="w-4 h-4" />
          </button>
          <button class="ml-1 px-3 py-1.5 rounded-lg border border-border dark:border-border-dark text-sm inline-flex items-center gap-1.5 hover:bg-surface-hover dark:hover:bg-surface-hover-dark" @click="emit('edit')">
            <Pencil class="w-3.5 h-3.5" /> {{ t('safe.detail.edit') }}
          </button>
        </template>
        <template v-else>
          <button class="px-3 py-1.5 rounded-lg border border-border dark:border-border-dark text-sm inline-flex items-center gap-1.5 hover:bg-surface-hover dark:hover:bg-surface-hover-dark" @click="trash(false)">
            <RotateCcw class="w-3.5 h-3.5" /> {{ t('safe.detail.restore') }}
          </button>
          <button class="px-3 py-1.5 rounded-lg text-sm text-danger hover:bg-danger/10" @click="confirmPurge = true">
            {{ t('safe.detail.purge') }}
          </button>
        </template>
      </div>
    </header>

    <div class="flex-1 overflow-y-auto px-6 pb-8 space-y-6">
      <dl v-if="item.fields.length" class="rounded-xl border border-border dark:border-border-dark divide-y divide-border dark:divide-border-dark">
        <div v-for="f in item.fields" :key="f.id" class="group px-4 py-3 flex items-center gap-3">
          <div class="flex-1 min-w-0">
            <dt class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ f.label }}</dt>
            <dd v-if="f.empty" class="text-sm text-text-tertiary dark:text-text-tertiary-dark italic">{{ t('safe.detail.empty_value') }}</dd>
            <dd v-else-if="f.concealed" class="font-mono text-sm break-all" :class="revealed[f.id] === undefined ? 'tracking-widest' : 'select-text'">
              {{ revealed[f.id] ?? dots(f.length_bucket) }}
            </dd>
            <dd v-else class="text-sm break-words select-text" :class="{ 'whitespace-pre-wrap font-mono': f.kind === 'multiline' }">{{ f.value }}</dd>
          </div>
          <template v-if="!f.empty">
            <button v-if="f.concealed" class="p-1.5 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-text-secondary dark:text-text-secondary-dark" :aria-label="revealed[f.id] !== undefined ? t('safe.detail.hide') : t('safe.detail.reveal')" :title="revealed[f.id] !== undefined ? t('safe.detail.hide') : t('safe.detail.reveal')" @click="toggle(f)">
              <EyeOff v-if="revealed[f.id] !== undefined" class="w-4 h-4" /><Eye v-else class="w-4 h-4" />
            </button>
            <button class="p-1.5 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-text-secondary dark:text-text-secondary-dark" :aria-label="t('safe.detail.copy')" :title="t('safe.detail.copy')" @click="copy(f)">
              <Copy class="w-4 h-4" />
            </button>
          </template>
        </div>
      </dl>

      <section v-if="item.urls.length" class="space-y-2">
        <h3 class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.detail.websites') }}</h3>
        <button v-for="u in item.urls" :key="u.url" class="w-full flex items-center gap-2 text-left text-sm text-accent hover:underline" :title="t('safe.detail.open')" @click="open(u.url)">
          <ExternalLink class="w-3.5 h-3.5 flex-shrink-0" /><span class="truncate">{{ u.url }}</span>
        </button>
      </section>

      <section v-if="item.notes" class="space-y-2">
        <h3 class="text-xs font-medium text-text-secondary dark:text-text-secondary-dark">{{ t('safe.detail.notes') }}</h3>
        <p class="text-sm whitespace-pre-wrap select-text">{{ item.notes }}</p>
      </section>

      <section v-if="item.tags.length" class="flex flex-wrap gap-1.5">
        <span v-for="tag in item.tags" :key="tag" class="px-2 py-0.5 rounded-full bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-xs">#{{ tag }}</span>
      </section>

      <footer class="text-xs text-text-tertiary dark:text-text-tertiary-dark space-y-0.5">
        <p>{{ updated }}</p>
        <p v-if="item.history_count">{{ t('safe.detail.history', { n: item.history_count }) }}</p>
        <p>{{ t(`safe.detail.syn.${item.ai_level}`) }}</p>
      </footer>
    </div>

    <div v-if="toast" class="absolute bottom-6 left-1/2 -translate-x-1/2 px-4 py-2 rounded-full bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900 text-sm shadow-lg" role="status">
      {{ toast }}
    </div>

    <ConfirmModal
      :show="confirmPurge"
      :title="t('safe.detail.purge_title')"
      :message="t('safe.detail.purge_body', { title: item.title })"
      :confirm-text="t('safe.detail.purge')"
      :cancel-text="t('safe.editor.cancel')"
      is-destructive
      @confirm="purge"
      @cancel="confirmPurge = false"
    />
  </div>
</template>
