import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import KeepAsLens from '../KeepAsLens.vue';
import { i18n } from '../../../../i18n';
import type { SynToolCallEvent } from '../../types';

const writeNode = vi.fn().mockResolvedValue(undefined);
vi.mock('../../../../composables/useNodeService', () => ({
  useNodeService: () => ({ writeNode }),
}));
vi.mock('../../../../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn() },
}));

const called = (query: string, n = 1): SynToolCallEvent => ({
  conversation_id: 'c',
  tool_name: 'query_nodes',
  tool_args: { query },
  result_preview: '',
  iteration: n,
});

const show = (toolCalls: SynToolCallEvent[]) =>
  mount(KeepAsLens, {
    props: { vaultPath: '/vault', toolCalls },
    global: { plugins: [i18n] },
  });

beforeEach(() => writeNode.mockClear());

describe('The question the assistant wrote', () => {
  /// §6.2: the assistant writes a query, it does not replace one. Somebody who
  /// cannot write `events | seq gaps by who` asks once, and keeps it.
  it('shows the query itself, not a description of it', () => {
    const wrapper = show([called('events | seq gaps by who')]);
    expect(wrapper.find('[data-kept-question]').text()).toContain('seq gaps by who');
  });

  /// A turn often asks the same thing twice — narrow, then wider when the
  /// first found nothing. Offering it twice would read as two findings.
  it('offers the same question once however often it was asked', () => {
    const wrapper = show([called('is:note pricing'), called('is:note pricing', 2)]);
    expect(wrapper.findAll('[data-kept-question]')).toHaveLength(1);
  });

  it('keeps it as an ordinary lens node, named after itself', async () => {
    const wrapper = show([called('events when:2019')]);
    await wrapper.find('[data-keep-question]').trigger('click');
    await flushPromises();

    expect(writeNode).toHaveBeenCalledWith(
      expect.objectContaining({
        nodeType: 'lens',
        title: 'events when:2019',
        properties: { query: 'events when:2019' },
      }),
    );
    expect(wrapper.find('[data-question-kept]').exists()).toBe(true);
    expect(wrapper.find('[data-keep-question]').exists()).toBe(false);
  });

  it('says nothing when the turn asked nothing', () => {
    expect(show([]).find('[data-kept-questions]').exists()).toBe(false);
    const other: SynToolCallEvent = { ...called('x'), tool_name: 'get_node' };
    expect(show([other]).find('[data-kept-questions]').exists()).toBe(false);
  });
});
