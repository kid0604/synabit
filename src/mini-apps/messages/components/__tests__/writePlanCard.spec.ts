import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import WritePlanCard from '../WritePlanCard.vue';
import { i18n } from '../../../../i18n';
import type { WritePlan } from '../../composables/useSynWrites';

const plan: WritePlan = {
  why: 'Báo cáo man-day tháng 9',
  proposed_at: 'now',
  requests: [
    {
      tool: 'connector__sheets__write_values', args: {}, hash: 'aaaaaaaa1', connector: 'Sheets', operation: 'write_values',
      effect: 'write', state: 'proposed',
      shown: [{ param: 'sheet_id', value: '1AbC', role: 'destination' }, { param: 'values', value: '{{result:r2:rows}}', table: 'r2: 12 rows × 3 columns from Jira' }],
    },
    {
      tool: 'connector__mail__send', args: {}, hash: 'bbbbbbbb2', connector: 'Mail', operation: 'send',
      effect: 'irreversible', state: 'proposed', shown: [{ param: 'to', value: 'sep@congty.vn', role: 'recipient' }],
    },
    {
      tool: 'connector__mail__send', args: {}, hash: 'cccccccc3', connector: 'Mail', operation: 'send',
      effect: 'irreversible', state: 'proposed', shown: [{ param: 'cc', value: 'a@evil.example', role: 'recipient' }],
      problems: ['`cc` is a recipient, and "a@evil.example" is not something you said'],
    },
  ],
};

const show = () => mount(WritePlanCard, { props: { plan }, global: { plugins: [i18n] } });

describe('A plan of sends', () => {
  it('shows each request, its table by handle, and what Rust found wrong', () => {
    const w = show();
    expect(w.text()).toContain('r2: 12 rows × 3 columns from Jira');
    expect(w.text()).toContain('a@evil.example');
    expect(w.find('[data-test="request-cccccccc"] input').attributes('disabled')).toBeDefined();
  });

  it('approves the flagged-free requests by default, and never a flagged one', async () => {
    const w = show();
    await w.find('[data-test="approve-chosen"]').trigger('click');
    const [[approved]] = w.emitted('answer') as [[string[]]];
    expect(approved.sort()).toEqual(['aaaaaaaa1', 'bbbbbbbb2']);
  });

  it('can send one and not the other, or none', async () => {
    const w = show();
    await w.find('[data-test="request-bbbbbbbb"] input').setValue(false);
    await w.find('[data-test="approve-chosen"]').trigger('click');
    const answered = () => w.emitted('answer') as string[][][];
    expect(answered()[0]?.[0]).toEqual(['aaaaaaaa1']);
    await w.find('[data-test="decline-all"]').trigger('click');
    expect(answered()[1]?.[0]).toEqual([]);
  });
});
