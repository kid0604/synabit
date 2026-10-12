import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import HowWorkedOut from '../HowWorkedOut.vue';
import { i18n } from '../../../../i18n';
import { howWorkedOut } from '../../declared';
import type { SynToolCallEvent } from '../../types';

describe('How a figure was worked out', () => {
  const call = (tool_name: string, tool_args: Record<string, unknown>, result_preview: string): SynToolCallEvent => ({
    conversation_id: 'c',
    tool_name,
    tool_args,
    result_preview,
    iteration: 1,
  });

  it('lists the tables and the pipelines, only when something was computed', () => {
    const log = [
      call('connector__expenses__list_expenses', {}, '=== RESULT … {"handle":"r1","rows":3000,"columns":[…'),
      call('crunch', { handle: 'r1', pipeline: 'stats sum(amount) by category' }, '{"handle":"r2","rows":[{"category":"Shopping"'),
    ];
    const steps = howWorkedOut(log);
    expect(steps).toEqual([
      { kind: 'fetched', from: 'expenses · list_expenses', asked: '{}', handle: 'r1', rows: 3000 },
      { kind: 'crunched', from: 'r1', asked: 'stats sum(amount) by category', handle: 'r2', rows: null },
    ]);
    expect(howWorkedOut([log[0]])).toEqual([]);
    expect(howWorkedOut(undefined)).toEqual([]);
  });

  it('shows nothing for an answer that computed nothing', () => {
    const w = mount(HowWorkedOut, { props: { toolCalls: [call('query_nodes', { query: 'x' }, '{}')] }, global: { plugins: [i18n] } });
    expect(w.find('[data-test="how-worked-out"]').exists()).toBe(false);
  });
});

