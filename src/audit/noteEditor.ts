/**
 * The note editor as the app mounts it — `TiptapEditor`, every extension it
 * has — for checks that need the real thing rather than a copy of its list.
 * Used by `npm run audit` and the note property tests.
 *
 * The spec that calls this mocks Tauri (`@tauri-apps/api/core`, `/event`,
 * `@tauri-apps/plugin-dialog`); `vi.mock` only works in the spec file itself.
 */
import { mount, type VueWrapper } from '@vue/test-utils';
import { createTestingPinia } from '@pinia/testing';
import { vi } from 'vitest';
import type { Editor } from '@tiptap/core';
import { i18n } from '../i18n';
import TiptapEditor from '../mini-apps/note/TiptapEditor.vue';
import { savedMarkdown } from '../mini-apps/note/editor/roundTrip';
import { useAssetPaths } from '../mini-apps/note/editor/composables/useAssetPaths';

// What a WebView has and jsdom does not, for node views that measure or watch
// themselves. Nothing here is drawn.
class Inert {
  observe() {}
  unobserve() {}
  disconnect() {}
  takeRecords() {
    return [];
  }
}
for (const name of ['ResizeObserver', 'IntersectionObserver'] as const) {
  if (!(name in globalThis)) (globalThis as any)[name] = Inert;
}
if (typeof window !== 'undefined' && !window.matchMedia) {
  (window as any).matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} });
}

export interface NoteEditor {
  wrapper: VueWrapper<any>;
  editor: Editor;
  /** Markdown into the editor the way opening a note puts it there. */
  open: (markdown: string) => void;
  /** The markdown saving writes to the file, asset paths made relative again. */
  save: () => string;
}

export async function mountNoteEditor(vaultPath = '/vault'): Promise<NoteEditor> {
  const wrapper = mount(TiptapEditor, {
    props: { modelValue: '', vaultPath },
    global: { plugins: [i18n, createTestingPinia({ createSpy: vi.fn })] },
    attachTo: document.body,
  });
  await new Promise((r) => setTimeout(r, 50));
  const editor = (wrapper.find('.ProseMirror').element as any).editor as Editor | undefined;
  if (!editor) throw new Error('The editor did not mount');
  const { stripLocalAssets } = useAssetPaths(vaultPath);
  return { wrapper, editor, open: (md: string) => (wrapper.vm as any).loadContent(md), save: () => stripLocalAssets(savedMarkdown(editor)) };
}
