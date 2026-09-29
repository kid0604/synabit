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
import { Copy, ExternalLink, Eye, EyeOff, Link, ListTodo, Pencil, RotateCcw, ShieldAlert, Star, Trash2 } from 'lucide-vue-next';
import { useNodeService } from '../../composables/useNodeService';
import { taskProperties } from '../task/types';
import ConfirmModal from '../../shared/components/ConfirmModal.vue';
import SynAccess from './SynAccess.vue';
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

/*
 * The current code, counted down here and asked for again when it runs out.
 * One call a period rather than one a second: the countdown is arithmetic,
 * only the code needs the secret.
 */
const code = ref<{ code: string; remaining: number; period: number } | null>(null);
let tick: ReturnType<typeof setInterval> | undefined;
async function fetchCode() {
  if (!props.item.totp) {
    code.value = null;
    return;
  }
  try {
    code.value = await props.api.totp(props.item.id);
  } catch (e) {
    code.value = null;
    emit('error', e);
  }
}
function startCodes() {
  clearInterval(tick);
  code.value = null;
  if (!props.item.totp) return;
  void fetchCode();
  tick = setInterval(() => {
    if (!code.value) return;
    if (code.value.remaining <= 1) void fetchCode();
    else code.value = { ...code.value, remaining: code.value.remaining - 1 };
  }, 1000);
}
async function copyCode() {
  try {
    const { clear_after_secs } = await props.api.copyTotp(props.item.id);
    say(clear_after_secs ? t('safe.detail.copied', { n: clear_after_secs }) : t('safe.detail.copied_kept'));
  } catch (e) {
    say(explain(e));
  }
}
const spaced = (c: string) => (c.length === 6 ? `${c.slice(0, 3)} ${c.slice(3)}` : c);
/** Both factors of one account in one place — worth a line, not a refusal. */
const sharesAPassword = computed(() => props.item.fields.some((f) => f.kind === 'password' && !f.empty));

watch(() => props.item.id, () => {
  hide();
  startCodes();
});
watch(() => props.item.totp, startCodes);
startCodes();
onBeforeUnmount(() => {
  hide();
  clearTimeout(toastTimer);
  clearInterval(tick);
});

function dots(bucket: number | null) {
  return '•'.repeat(bucket ?? 8);
}

/**
 * A link to paste into a note. The id and nothing else: a note is a plain
 * file that syncs and may sit in git, and the title of what is in the Safe
 * is not the note's to carry. The id is not a secret, so the ordinary
 * clipboard is fine.
 */
async function copyLink() {
  try {
    await navigator.clipboard.writeText(`synabit://safe/${props.item.id}`);
    say(t('safe.detail.link_copied'));
  } catch (e) {
    say(explain(e));
  }
}

/**
 * A task to change this password, in Tasks, due in a week, linking back here.
 * The task is named after the item — the user asked for it by pressing this,
 * and a task that does not say what to change is no task.
 */
const ns = useNodeService();
async function createTask() {
  const due = new Date(Date.now() + 7 * 86_400_000).toISOString().slice(0, 10);
  try {
    await ns.writeNode({
      relPath: `Tasks/${crypto.randomUUID()}.md`,
      nodeType: 'task',
      title: t('safe.health.task_title', { title: props.item.title }),
      properties: taskProperties({ status: 'todo', priority: 'high', due_date: due, tags: ['safe'] } as never),
      content: `[${t('safe.health.open_in_safe')}](synabit://safe/${props.item.id})`,
      eventType: 'created',
    });
    say(t('safe.health.task_created'));
  } catch (e) {
    say(explain(e));
  }
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
          <button class="p-2 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark" :aria-label="t('safe.detail.copy_link')" :title="t('safe.detail.copy_link')" @click="copyLink">
            <Link class="w-4 h-4" />
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
      <section v-if="item.health.length && !trashed" class="rounded-xl border border-warning/40 bg-warning/5 p-4 space-y-2">
        <p v-for="f in item.health" :key="f" class="flex gap-2 text-sm">
          <ShieldAlert class="w-4 h-4 mt-0.5 flex-shrink-0 text-warning" />
          <span><strong class="font-medium">{{ t(`safe.health.flag.${f}`) }}.</strong> {{ t(`safe.health.explain.${f}`) }}</span>
        </p>
        <button class="inline-flex items-center gap-1.5 text-sm text-accent hover:underline" :title="t('safe.health.task_hint')" @click="createTask">
          <ListTodo class="w-4 h-4" /> {{ t('safe.health.task') }}
        </button>
      </section>

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

      <section v-if="item.totp" class="rounded-xl border border-border dark:border-border-dark px-4 py-3 flex items-center gap-3">
        <div class="flex-1 min-w-0">
          <p class="text-xs text-text-secondary dark:text-text-secondary-dark">{{ t('safe.totp.label') }}</p>
          <p class="font-mono text-lg tracking-wider tabular-nums">{{ code ? spaced(code.code) : '— — —' }}</p>
        </div>
        <svg v-if="code" class="w-6 h-6 -rotate-90" viewBox="0 0 24 24" :aria-label="t('safe.totp.remaining', { n: code.remaining })" role="img">
          <circle cx="12" cy="12" r="10" fill="none" stroke="currentColor" stroke-width="2.5" class="text-border dark:text-border-dark" />
          <circle cx="12" cy="12" r="10" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" class="text-accent"
            :stroke-dasharray="62.83" :stroke-dashoffset="62.83 * (1 - code.remaining / code.period)" />
        </svg>
        <button class="p-1.5 rounded-lg hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-text-secondary dark:text-text-secondary-dark" :aria-label="t('safe.detail.copy')" :title="t('safe.detail.copy')" @click="copyCode">
          <Copy class="w-4 h-4" />
        </button>
      </section>
      <p v-if="item.totp && sharesAPassword" class="-mt-4 text-xs text-warning">{{ t('safe.totp.same_place') }}</p>

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

      <SynAccess v-if="!trashed" :api="api" :item="item" @saved="emit('changed')" />

      <footer class="text-xs text-text-tertiary dark:text-text-tertiary-dark space-y-0.5">
        <p>{{ updated }}</p>
        <p v-if="item.history_count">{{ t('safe.detail.history', { n: item.history_count }) }}</p>
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
