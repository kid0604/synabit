import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import MomentSheet from '../MomentSheet.vue';
import type { MomentDetail } from '../MomentSheet.vue';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const { invoke } = await import('@tauri-apps/api/core');

const kept: MomentDetail = {
  path: 'Moments/6f3c.md',
  title: 'Ăn trưa',
  happened_from: '2026-07-21',
  happened_to: '2026-07-21',
  precision: 'day',
  time: null,
  place: null,
  category: 'meal',
  amount: null,
  about: [],
  people: [{ id: 'People/nga.md', title: 'Nga' }],
  names: ['Đức'],
  hand: [],
  source_node: 'Notes/2026-07-21.md',
  quote: 'Trưa ăn bún chả với Nga',
  origin: 'extract',
  categories: ['meal', 'work', 'other'],
  known_people: [{ id: 'People/duc.md', title: 'Đức Trần' }],
};

const open = async (moment: MomentDetail = kept) => {
  vi.mocked(invoke).mockImplementation(async (command: string) => (command === 'timeline_moment' ? moment : null));
  const wrapper = mount(MomentSheet, {
    props: { vaultPath: '/vault', path: moment.path },
    global: { plugins: [i18n] },
  });
  await flushPromises();
  return wrapper;
};

const lastCall = (command: string) => vi.mocked(invoke).mock.calls.filter(c => c[0] === command).pop()?.[1];

describe('A moment already kept', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('opens with everything it holds, and the evidence read-only', async () => {
    const wrapper = await open();
    expect((wrapper.find('[data-moment-title]').element as HTMLTextAreaElement).value).toBe('Ăn trưa');
    expect((wrapper.find('[data-moment-from]').element as HTMLInputElement).value).toBe('2026-07-21');
    expect(wrapper.find('[data-moment-people]').text()).toContain('Nga');
    expect(wrapper.find('[data-moment-people]').text()).toContain('Đức');
    expect(wrapper.find('[data-moment-quote]').text()).toContain('Trưa ăn bún chả với Nga');
    // The quote is shown, and there is no box to rewrite it in.
    expect(wrapper.findAll('textarea')).toHaveLength(1);
    // The kinds are the vault's.
    expect(wrapper.find('[data-moment-category]').findAll('option').map(o => o.text())).toEqual(['Meal', 'Work', 'Other']);
  });

  it('writes back every field the person changed', async () => {
    const wrapper = await open();
    await wrapper.find('[data-moment-title]').setValue('Ăn trưa bún chả với Nga');
    await wrapper.find('[data-moment-time]').setValue('12:15');
    await wrapper.find('[data-moment-where]').setValue('Hàng Mành');
    await wrapper.find('[data-moment-amount]').setValue('50000');
    await wrapper.find('[data-moment-save]').trigger('click');
    await flushPromises();

    expect(lastCall('timeline_moment_write')).toMatchObject({
      path: 'Moments/6f3c.md',
      edits: {
        title: 'Ăn trưa bún chả với Nga',
        time: '12:15',
        place: 'Hàng Mành',
        amount: 50000,
        people: ['People/nga.md', 'Đức'],
      },
    });
    expect(wrapper.emitted('changed')).toHaveLength(1);
    expect(wrapper.emitted('close')).toHaveLength(1);
  });

  it('can say who a name nobody matched belongs to', async () => {
    const wrapper = await open();
    await wrapper.find('[data-moment-assign]').setValue('People/duc.md');
    await wrapper.find('[data-moment-save]').trigger('click');
    await flushPromises();
    expect(lastCall('timeline_moment_write')).toMatchObject({ edits: { people: ['People/nga.md', 'People/duc.md'] } });
  });

  /// Letting a moment go undoes a decision, so it takes two presses — and the
  /// file goes to the trash, which the Rust side does.
  it('asks before letting a moment go', async () => {
    const wrapper = await open();
    await wrapper.find('[data-moment-delete]').trigger('click');
    expect(vi.mocked(invoke).mock.calls.some(c => c[0] === 'timeline_moment_delete')).toBe(false);
    await wrapper.find('[data-moment-delete-yes]').trigger('click');
    await flushPromises();
    expect(lastCall('timeline_moment_delete')).toMatchObject({ path: 'Moments/6f3c.md' });
    expect(wrapper.emitted('changed')).toHaveLength(1);
  });

  it('offers the note it was read from, and says when there is none', async () => {
    const wrapper = await open();
    await wrapper.find('[data-moment-source]').trigger('click');
    expect(wrapper.emitted('open')?.[0]).toEqual(['Notes/2026-07-21.md', 'Trưa ăn bún chả với Nga']);

    const byHand = await open({ ...kept, source_node: null, quote: null, origin: 'manual' });
    expect(byHand.find('[data-moment-source]').exists()).toBe(false);
    expect(byHand.text()).toContain('Written by hand');
  });

  it('will not save a moment with no title', async () => {
    const wrapper = await open();
    await wrapper.find('[data-moment-title]').setValue('   ');
    expect(wrapper.find('[data-moment-save]').attributes('disabled')).toBeDefined();
  });
});
