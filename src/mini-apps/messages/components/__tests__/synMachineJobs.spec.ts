import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

/**
 * Long work in a room, on the screen (plan M7): each job with how long it has
 * run, how far its own output says it is, whether it is paused for a room with
 * a network (I8), and a stop button that goes straight to Rust.
 */

const jobs = [
  { id: 'j00000001', command: 'ffmpeg -i in.mov out.mp4', seconds: 3725, paused: false, progress: { time: '00:41:02.10' } },
  { id: 'j00000002', command: 'python3 train.py', seconds: 65, paused: true, progress: { percent: 40 } },
];
const invoke = vi.fn(async (name: string, ..._args: unknown[]): Promise<unknown> => {
  if (name === 'syn_machine_status') return { sealed: true, broken: null, running: 3, jobs };
  if (name === 'syn_machine_stop_job') return true;
  return [];
});
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const { i18n } = await import('../../../../i18n');
const SynMachine = (await import('../SynMachine.vue')).default;

describe('long work on the machine screen', () => {
  beforeEach(() => {
    invoke.mockClear();
    i18n.global.locale.value = 'en';
  });

  it('lists each job with its time, progress and pause', async () => {
    const wrapper = mount(SynMachine, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
    await flushPromises();
    const items = wrapper.findAll('[data-test="jobs"] li');
    expect(items).toHaveLength(2);
    expect(items[0].text()).toContain('ffmpeg -i in.mov out.mp4');
    expect(items[0].text()).toContain('1:02:05 so far');
    expect(items[0].text()).toContain('00:41:02.10');
    expect(items[0].text()).not.toContain('Paused');
    expect(items[1].text()).toContain('1:05 so far');
    expect(items[1].text()).toContain('40%');
    expect(items[1].text()).toContain('Paused');
    wrapper.unmount();
  });

  it('stops one job by its id', async () => {
    const wrapper = mount(SynMachine, { props: { vaultPath: '/vault' }, global: { plugins: [i18n] } });
    await flushPromises();
    await wrapper.find('[data-test="job-stop-j00000002"]').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('syn_machine_stop_job', { id: 'j00000002' });
    expect(invoke.mock.calls.filter(([name]) => name === 'syn_machine_stop_all')).toHaveLength(0);
    wrapper.unmount();
  });
});
