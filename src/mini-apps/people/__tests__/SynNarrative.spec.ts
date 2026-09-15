import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { invoke } from '@tauri-apps/api/core';
import SynNarrative from '../SynNarrative.vue';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const person = { id: 'People/tuan.md', title: 'Tuấn', properties: {} };

const mountCard = () =>
  mount(SynNarrative, { props: { person, vaultPath: '/vault' }, global: { plugins: [i18n] } });

describe('SynNarrative', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('asks for nothing until it is asked', () => {
    mountCard();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('shows each sentence with numbers that open the records it rests on', async () => {
    vi.mocked(invoke).mockResolvedValue({
      sentences: [
        { text: 'They met in 2013.', sources: [1] },
        { text: 'Tuấn came to the wedding.', sources: [2, 3] },
      ],
      sources: [
        { n: 1, node_id: 'People/Interactions/c.md', node_type: 'interaction', title: 'Coffee', date: '2013-10-19', what: 'coffee' },
        { n: 2, node_id: 'Notes/wedding.md', node_type: 'note', title: 'Wedding', date: '2016-05-14', what: 'note' },
        { n: 3, node_id: 'Events/wedding.md', node_type: 'event', title: 'Wedding', date: '2016-05-14', what: 'event' },
      ],
      dropped: 1,
      withheld: false,
    });

    const wrapper = mountCard();
    await wrapper.find('button').trigger('click');
    await flushPromises();

    expect(invoke).toHaveBeenCalledWith('syn_narrate_person', expect.objectContaining({ personId: 'People/tuan.md' }));
    expect(wrapper.text()).toContain('They met in 2013.');
    expect(wrapper.findAll('[data-citation]')).toHaveLength(3);
    expect(wrapper.text()).toContain('1 sentences without a source were left out.');

    await wrapper.findAll('[data-citation]')[1].trigger('click');
    expect(wrapper.emitted('open-node')?.[0]).toEqual(['Notes/wedding.md', 'note']);
  });

  it('says so when nothing the model wrote had a source', async () => {
    vi.mocked(invoke).mockResolvedValue({
      sentences: [],
      sources: [{ n: 1, node_id: 'x', node_type: 'note', title: 'x', date: '2020-01-01', what: 'note' }],
      dropped: 3,
      withheld: false,
    });
    const wrapper = mountCard();
    await wrapper.find('button').trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain('Syn wrote nothing that rests on a record');
    expect(wrapper.findAll('[data-citation]')).toHaveLength(0);
  });
});
