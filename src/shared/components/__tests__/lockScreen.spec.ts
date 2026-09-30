import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import LockScreen from '../LockScreen.vue';
import { i18n } from '../../../i18n';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const lockConfig = (enabled: boolean) => ({
  is_enabled: enabled,
  app_lock_active: enabled,
  protected_apps: [],
  protected_notes: [],
  auto_lock_timeout_secs: 300,
});

const show = () => {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(LockScreen, {
    props: { title: 'Locked' },
    global: { plugins: [i18n, pinia], stubs: { teleport: true } },
  });
};

const typePin = async (wrapper: ReturnType<typeof show>, pin: string) => {
  for (const digit of pin) {
    const key = wrapper.findAll('button.numpad-digit').find(b => b.text() === digit);
    await key!.trigger('click');
  }
  await flushPromises();
};

describe('LockScreen', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  // The PIN is handed on so a caller can give it to a command the backend
  // guards too (`remove_app_lock`, `set_family_safe`).
  it('hands on the PIN it checked', async () => {
    vi.mocked(invoke).mockResolvedValue({ success: true, remaining_attempts: 5, locked_until: null });
    const wrapper = show();
    await typePin(wrapper, '123456');
    expect(invoke).toHaveBeenCalledWith('verify_app_lock', { pin: '123456' });
    expect(wrapper.emitted('unlocked')).toEqual([['123456']]);
  });

  it('explains a forgotten PIN and resets it only through the backend', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'app_lock_reset_begin') return { folder: '/data/synabit', name: 'synabit-reset-042042' };
      if (command === 'get_app_lock_config') return lockConfig(false);
      return null;
    });
    const wrapper = show();
    await wrapper.findAll('button').find(b => b.text() === i18n.global.t('shell.lock.forgot'))!.trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain(i18n.global.t('shell.lock.forgot_hint'));
    expect(wrapper.find('[data-reset-name]').text()).toBe('synabit-reset-042042');
    expect(wrapper.find('[data-reset-folder]').text()).toBe('/data/synabit');

    await wrapper.findAll('button').find(b => b.text() === i18n.global.t('shell.lock.reset_pin'))!.trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('app_lock_reset_finish');
    // Not "unlocked": whatever the prompt was guarding is not done on the
    // back of a reset.
    expect(wrapper.emitted('unlocked')).toBeUndefined();
    expect(wrapper.emitted('cancelled')).toHaveLength(1);
  });

  it('says so when the folder has not been made', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'app_lock_reset_begin') return { folder: '/data/synabit', name: 'synabit-reset-042042' };
      if (command === 'app_lock_reset_finish') throw 'RESET_NOT_PROVEN';
      return null;
    });
    const wrapper = show();
    await wrapper.findAll('button').find(b => b.text() === i18n.global.t('shell.lock.forgot'))!.trigger('click');
    await flushPromises();
    await wrapper.findAll('button').find(b => b.text() === i18n.global.t('shell.lock.reset_pin'))!.trigger('click');
    await flushPromises();
    expect(wrapper.find('[role="alert"]').text()).toBe(i18n.global.t('shell.lock.reset_not_found'));
    expect(wrapper.emitted('cancelled')).toBeUndefined();
  });

  it('offers no reset where the backend refuses one, and still explains', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === 'app_lock_reset_begin') throw 'RESET_NOT_ON_THIS_DEVICE';
      return null;
    });
    const wrapper = show();
    await wrapper.findAll('button').find(b => b.text() === i18n.global.t('shell.lock.forgot'))!.trigger('click');
    await flushPromises();
    expect(wrapper.text()).toContain(i18n.global.t('shell.lock.reset_not_here'));
    expect(wrapper.findAll('button').some(b => b.text() === i18n.global.t('shell.lock.reset_pin'))).toBe(false);
  });
});
