import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import { nextTick } from 'vue';
import BoardSearch from '../components/BoardSearch.vue';
import type { WBNode } from '../boardFile';

const i18n = () => createI18n({ legacy: false, locale: 'en', missingWarn: false, fallbackWarn: false, messages: { en: {} } });
const nodes: WBNode[] = [{ id: 'a b', type: 'sticky', position: { x: 0, y: 0 }, data: { label: 'Budget' } }];

describe('closing the find bar', () => {
  it('gives focus back to where it was when nothing was found', async () => {
    const opener = document.createElement('button');
    document.body.appendChild(opener);
    opener.focus();
    const wrapper = mount(BoardSearch, { props: { nodes, hidden: new Set<string>() }, global: { plugins: [i18n()] }, attachTo: document.body });
    await nextTick(); await nextTick();
    expect(document.activeElement?.tagName).toBe('INPUT');
    wrapper.unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it('puts focus on the match it showed', async () => {
    const node = document.createElement('div');
    node.className = 'vue-flow__node';
    node.dataset.id = 'a b';
    node.tabIndex = 0;
    document.body.appendChild(node);
    const wrapper = mount(BoardSearch, { props: { nodes, hidden: new Set<string>() }, global: { plugins: [i18n()] }, attachTo: document.body });
    await nextTick(); await nextTick();
    const input = wrapper.find('input');
    await input.setValue('budget');
    await input.trigger('keydown', { key: 'Enter' });
    expect(wrapper.emitted('show')?.slice(-1)[0]).toEqual(['a b']);
    wrapper.unmount();
    expect(document.activeElement).toBe(node);
    node.remove();
  });
});
