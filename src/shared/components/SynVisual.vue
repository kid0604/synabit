<script setup lang="ts">
/**
 * One visual Syn drew, in its frame: under an answer in the chat, or in a
 * note it was kept in.
 *
 * The frame is `synvisual://…/v/<id>`, sandboxed to `allow-scripts` — no
 * `allow-same-origin`, forms, popups or top navigation — and the bridge to it
 * is `visualHost`. Everything around the frame is the app's: the label that
 * says Syn drew it, the buttons, the error line. Nothing the model wrote can
 * add to them (`docs/syn-the-conversation-2026-09-10.md` §2).
 *
 * The frame is put in the page only once it scrolls near. A phone, or a
 * vault with the `visuals` setting off, shows what the visual is — its title
 * and kind — rather than running it: visuals are a desktop thing (plan §7),
 * and the setting is the person saying no model-written code runs here, in a
 * note kept before as much as in a new answer.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { diagramTheme } from '../mermaid';
import { usePlatform } from '../../composables/usePlatform';
import { useNodeService } from '../../composables/useNodeService';
import { logger } from '../../utils/logger';
import {
  answer,
  contextChanged,
  frameHeight,
  visualLocale,
  visualOrigin,
  visualSrc,
  type HostContext,
  type VisualTheme,
} from '../syn/visualHost';
import { KEPT_IN, bodyForVisual } from '../../mini-apps/messages/keepAsNote';

const props = withDefaults(
  defineProps<{
    visualId: string;
    vaultPath: string;
    /** Offer to keep it as a note — not when it is already in one. */
    keepable?: boolean;
  }>(),
  { keepable: true },
);

const emit = defineEmits<{
  /** A question the visual asks back, for the chat box. Never sent from here. */
  ask: [text: string];
  /** The note it was kept in, to open. */
  open: [note: { id: string; title: string; node_type: string }];
}>();

const { t, locale } = useI18n();
const { isMobileOS } = usePlatform();
const nodes = useNodeService();

interface Meta {
  id: string;
  title: string;
  kind: string;
  question: string;
}

const meta = ref<Meta | null>(null);
const missing = ref(false);
const holder = ref<HTMLElement | null>(null);
const frame = ref<HTMLIFrameElement | null>(null);
const near = ref(false);
const reported = ref(0);
const expanded = ref(false);
const errors = ref<{ kind: string; message: string }[]>([]);
const showErrors = ref(false);
const kept = ref<{ id: string; title: string; node_type: string } | null>(null);
const busy = ref(false);
/** The `visuals` setting is off: show what the visual is, run nothing. */
const off = ref(false);
/** The setting has been read: until then nothing runs, so an off switch is never late. */
const settled = ref(false);
const held = computed(() => isMobileOS.value || off.value);
const KINDS = ['diagram', 'chart', 'explainer', 'simulation'];
const kindLabel = computed(() => (meta.value && KINDS.includes(meta.value.kind) ? t(`syn.visual_kind_${meta.value.kind}`) : t('syn.visual_untitled')));

const theme = computed<VisualTheme>(() => (diagramTheme.value === 'dark' ? 'dark' : 'light'));
const context = computed<HostContext>(() => ({ theme: theme.value, locale: visualLocale(String(locale.value)) }));
// The theme the frame first loads with; later changes are sent, not reloaded.
const src = ref(visualSrc(visualOrigin(navigator.userAgent), props.visualId, theme.value));
const height = computed(() => frameHeight(reported.value || 160, expanded.value));

const onMessage = (event: MessageEvent) => {
  // A sandboxed frame's origin is "null"; its window is what identifies it.
  if (!frame.value || event.source !== frame.value.contentWindow) return;
  const reply = answer(event.data, context.value, {
    height: (h) => {
      reported.value = h;
    },
    ask: (text) => emit('ask', text),
    error: (e) => {
      if (errors.value.length < 20) errors.value.push(e);
    },
  });
  if (reply) frame.value.contentWindow?.postMessage(reply, '*');
};

watch(context, (now) => frame.value?.contentWindow?.postMessage(contextChanged(now), '*'));

let observer: IntersectionObserver | null = null;
onMounted(async () => {
  window.addEventListener('message', onMessage);
  if (typeof IntersectionObserver === 'undefined') {
    near.value = true;
  } else {
    observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        near.value = true;
        observer?.disconnect();
      }
    }, { rootMargin: '400px' });
    if (holder.value) observer.observe(holder.value);
  }
  try {
    const settings = await invoke<{ visuals?: boolean } | null>('syn_get_settings', { vaultPath: props.vaultPath });
    off.value = settings?.visuals === false;
  } catch {
    // Unread settings are the default ones, and visuals are on by default.
  }
  settled.value = true;
  try {
    meta.value = await invoke<Meta>('syn_visual_meta', { vaultPath: props.vaultPath, id: props.visualId });
  } catch {
    missing.value = true;
  }
});

onBeforeUnmount(() => {
  window.removeEventListener('message', onMessage);
  observer?.disconnect();
});

async function keep() {
  if (!meta.value || busy.value) return;
  if (kept.value) {
    emit('open', kept.value);
    return;
  }
  busy.value = true;
  try {
    const title = meta.value.title || t('syn.visual_untitled');
    const relPath = await nodes.createNode({ directory: KEPT_IN, nodeType: 'note', silent: true });
    await nodes.writeNode({
      relPath,
      nodeType: 'note',
      title,
      properties: {},
      content: bodyForVisual(props.visualId, meta.value.question),
      eventType: 'created',
    });
    kept.value = { id: relPath, title, node_type: 'note' };
  } catch (e) {
    logger.error('[Syn] Could not keep the visual', e);
  } finally {
    busy.value = false;
  }
}

async function download() {
  if (!meta.value || busy.value) return;
  busy.value = true;
  try {
    const name = (meta.value.title || 'visual').replace(/[\\/:*?"<>|]/g, '_').slice(0, 80);
    const path = await save({ defaultPath: `${name}.html`, filters: [{ name: 'HTML', extensions: ['html'] }] });
    if (!path) return;
    const html = await invoke<string>('syn_visual_export', { vaultPath: props.vaultPath, id: props.visualId, theme: theme.value });
    await writeTextFile(path, html);
  } catch (e) {
    logger.error('[Syn] Could not save the visual', e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <figure
    ref="holder"
    class="syn-visual mt-3 rounded-xl border border-gray-200 dark:border-gray-700/50 overflow-hidden"
    :data-visual="visualId"
  >
    <figcaption class="flex items-center gap-2 px-3 py-1.5 text-xs text-gray-500 dark:text-gray-400 border-b border-gray-100 dark:border-gray-800/50">
      <span class="shrink-0">{{ t('syn.visual_by_syn') }}</span>
      <span v-if="meta?.title" class="truncate text-text dark:text-text-dark">· {{ meta.title }}</span>
    </figcaption>

    <p v-if="missing" class="px-3 py-3 text-xs text-gray-500 dark:text-gray-400">{{ t('syn.visual_missing') }}</p>
    <div v-else-if="held" class="syn-visual-held px-3 py-3 text-xs text-gray-500 dark:text-gray-400">
      <p class="text-sm text-text dark:text-text-dark">{{ meta?.title || t('syn.visual_untitled') }}</p>
      <p class="mt-0.5">{{ kindLabel }} · {{ off && !isMobileOS ? t('syn.visual_off') : t('syn.visual_desktop_only') }}</p>
    </div>
    <iframe
      v-else-if="near && settled"
      ref="frame"
      :src="src"
      sandbox="allow-scripts"
      referrerpolicy="no-referrer"
      loading="lazy"
      :title="meta?.title || t('syn.visual_by_syn')"
      class="block w-full border-0 bg-transparent"
      :style="{ height: `${height}px` }"
    />
    <div v-else class="h-40" />

    <div v-if="errors.length" class="px-3 py-1.5 text-xs text-danger border-t border-gray-100 dark:border-gray-800/50">
      <button type="button" class="underline-offset-2 hover:underline" @click="showErrors = !showErrors">
        {{ t('syn.visual_broke', { count: errors.length }, errors.length) }}
      </button>
      <ul v-if="showErrors" class="mt-1 space-y-0.5 font-mono break-all text-gray-500 dark:text-gray-400">
        <li v-for="(e, i) in errors" :key="i">{{ e.kind }}: {{ e.message }}</li>
      </ul>
    </div>

    <div v-if="!missing && !held" class="flex flex-wrap gap-1.5 px-3 py-1.5 border-t border-gray-100 dark:border-gray-800/50">
      <button v-if="keepable" type="button" class="visual-action" :disabled="busy || !meta" @click="keep">
        {{ kept ? t('syn.keep_open_it', { title: kept.title }) : t('syn.keep_as_note') }}
      </button>
      <button type="button" class="visual-action" @click="expanded = !expanded">
        {{ expanded ? t('syn.visual_fold') : t('syn.visual_expand') }}
      </button>
      <button type="button" class="visual-action" :disabled="busy || !meta" @click="download">
        {{ t('syn.visual_download') }}
      </button>
    </div>
  </figure>
</template>

<style scoped>
.visual-action {
  font-size: 12px;
  padding: 2px 10px;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  color: var(--color-text-secondary);
}
:global(.dark) .visual-action {
  border-color: var(--color-border-dark);
  color: var(--color-text-secondary-dark);
}
.visual-action:hover:not(:disabled) {
  background: var(--color-surface-hover);
}
:global(.dark) .visual-action:hover:not(:disabled) {
  background: var(--color-surface-hover-dark);
}
.visual-action:disabled {
  opacity: 0.5;
}
</style>
