<script setup lang="ts">
/**
 * Draws a visual Syn is about to keep where nobody sees it, so what its lint
 * finds reaches the model while it can still fix it (`syn::visual::look`).
 *
 * The engine emits `syn-visual-look` with a token; the frame is
 * `synvisual://…/draft/<token>`, sandboxed like every visual's frame, and the
 * draft page lints itself once drawn. This answers the frame as `SynVisual`
 * does — the theme and the locale, nothing else — and sends the lint back
 * with `syn_visual_looked`. The engine stops waiting after a few seconds,
 * whatever happens here.
 */
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logger } from '../../utils/logger';
import { LOOK_FRAME_STYLE, answer, draftSrc, lintFrom, visualLocale, visualOrigin, type HostContext } from '../syn/visualHost';

const { locale } = useI18n();
const drafts = ref<{ token: string; src: string }[]>([]);
const frames = new Map<string, HTMLIFrameElement>();
/** No draft is held longer than the engine waits for it, and a little more. */
const DROP_AFTER = 6000;

const done = (token: string) => {
  frames.delete(token);
  drafts.value = drafts.value.filter((d) => d.token !== token);
};

const onMessage = (event: MessageEvent) => {
  for (const [token, frame] of frames) {
    if (event.source !== frame.contentWindow) continue;
    const lint = lintFrom(event.data);
    if (lint) {
      invoke('syn_visual_looked', { token, lint }).catch((e) => logger.warn('[syn-visual] the lint was not taken:', e));
      done(token);
      return;
    }
    // The theme the draft was asked in; the locale the person reads.
    const context: HostContext = { theme: 'light', locale: visualLocale(String(locale.value)) };
    const reply = answer(event.data, context, { height: () => {}, ask: () => {}, error: () => {} });
    if (reply) frame.contentWindow?.postMessage(reply, '*');
    return;
  }
};

const setFrame = (token: string) => (el: unknown) => {
  if (el instanceof HTMLIFrameElement) frames.set(token, el);
};

let stop: UnlistenFn | null = null;
onMounted(async () => {
  window.addEventListener('message', onMessage);
  stop = await listen<{ token?: unknown }>('syn-visual-look', (event) => {
    const token = event.payload?.token;
    if (typeof token !== 'string' || !/^[0-9a-f]{32}$/.test(token)) return;
    drafts.value = [...drafts.value, { token, src: draftSrc(visualOrigin(navigator.userAgent), token) }];
    setTimeout(() => done(token), DROP_AFTER);
  });
});

onBeforeUnmount(() => {
  window.removeEventListener('message', onMessage);
  stop?.();
});
</script>

<template>
  <iframe
    v-for="draft in drafts"
    :key="draft.token"
    :ref="setFrame(draft.token)"
    :src="draft.src"
    sandbox="allow-scripts"
    referrerpolicy="no-referrer"
    aria-hidden="true"
    tabindex="-1"
    :style="LOOK_FRAME_STYLE"
  />
</template>
