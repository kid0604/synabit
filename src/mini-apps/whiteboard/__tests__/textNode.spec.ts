import { describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';

vi.mock('@vue-flow/node-resizer', () => ({ NodeResizer: { render: () => null } }));
vi.mock('../../../shared/syn/pane', () => ({ followLink: vi.fn() }));
import TextNode from '../nodes/TextNode.vue';

describe('a text box just put down', () => {
  it('is open for typing, with the caret in it', async () => {
    const i18n = createI18n({ legacy: false, locale: 'en', missingWarn: false, fallbackWarn: false, messages: { en: {} } });
    const wrapper = mount(TextNode, { props: { id: 't', data: { label: '', editing: true } }, global: { plugins: [i18n] }, attachTo: document.body });
    await new Promise((r) => setTimeout(r, 50));
    const editor = wrapper.element.querySelector('.ProseMirror');
    expect(editor).not.toBeNull();
    expect(editor!.contains(document.activeElement)).toBe(true);
    wrapper.unmount();
  });
});

describe('a link written in a text box', () => {
  it('goes the board\'s way, vault and mail links included', async () => {
    const follow = vi.fn();
    const i18n = createI18n({ legacy: false, locale: 'en', missingWarn: false, fallbackWarn: false, messages: { en: {} } });
    const wrapper = mount(TextNode, {
      props: { id: 't', data: { label: '[spec](synabit://note/abc) and [mail](mailto:a@b.c)' } },
      global: { plugins: [i18n], provide: { wbFollowLink: follow } },
    });
    const links = wrapper.findAll('a');
    await links[0].trigger('click');
    await links[1].trigger('click');
    expect(follow.mock.calls.map((c) => c[0])).toEqual(['synabit://note/abc', 'mailto:a@b.c']);
    wrapper.unmount();
  });
});
