import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createTestingPinia } from '@pinia/testing';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => p }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: vi.fn(async () => true), message: vi.fn(async () => undefined) }));

import TiptapEditor from '../TiptapEditor.vue';

/**
 * Typing saved a fifth of a second later; closing the note in between had to
 * save it at once. The close hook ran after `useEditor` had destroyed the
 * editor, threw, and the typing was lost.
 */
describe('closing a note right after typing', () => {
  it('saves what was typed', async () => {
    const wrapper = mount(TiptapEditor, {
      props: { modelValue: '', vaultPath: '/vault' },
      global: { plugins: [i18n, createTestingPinia({ createSpy: vi.fn })], config: { errorHandler: (e) => { throw e; } } },
      attachTo: document.body,
    });
    await new Promise((r) => setTimeout(r, 20));
    const editor = (wrapper.find('.ProseMirror').element as any).editor;
    editor.commands.insertContent('Gọi CMC trước 5 giờ');

    wrapper.unmount();

    const emitted = wrapper.emitted('update:modelValue') ?? [];
    const saved = emitted[emitted.length - 1]?.[0];
    expect(saved).toContain('Gọi CMC trước 5 giờ');
  });
});
