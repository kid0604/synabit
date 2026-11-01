import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import { i18n } from '../../../i18n';
import { useAppLockStore } from '../../../stores/useAppLockStore';
import { useSettings } from '../../../composables/useSettings';
import SettingsModal from '../SettingsModal.vue';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

const show = async (protectedApps: string[], allowed: string[]) => {
  const pinia = createPinia();
  setActivePinia(pinia);
  vi.mocked(invoke).mockResolvedValue(null);
  const store = useAppLockStore();
  store.isEnabled = true;
  store.protectedApps = protectedApps;
  store.synAllowedApps = allowed;
  const { showSettingsModal, settingsTab } = useSettings();
  showSettingsModal.value = true;
  settingsTab.value = 'security';
  const wrapper = mount(SettingsModal, {
    props: {
      initialTab: 'security',
      vaultPath: '/vault',
      vaultType: 'local',
      activeSyncProvider: 'none',
      syncing: false,
      syncError: '',
      lastSyncTime: '',
      autoSyncEnabled: false,
      autoSyncInterval: 5,
      syncServerAddr: '',
      syncServerIdHex: '',
    },
    global: { plugins: [i18n, pinia], stubs: { teleport: true } },
  });
  await flushPromises();
  return { wrapper, store };
};

const synSwitch = (wrapper: Awaited<ReturnType<typeof show>>['wrapper'], app: string) =>
  wrapper.find(`button[role="switch"][aria-label="Syn can use ${app}"]`);

describe('the switch that opens a locked app to Syn', () => {
  beforeEach(() => {
    i18n.global.locale.value = 'en';
    vi.mocked(invoke).mockReset();
  });

  it('is offered only on a protected app the lock keeps from Syn', async () => {
    const { wrapper } = await show(['finance', 'messages'], []);
    expect(synSwitch(wrapper, 'Finance').exists()).toBe(true);
    expect(synSwitch(wrapper, 'Finance').attributes('aria-checked')).toBe('false');
    expect(synSwitch(wrapper, 'People').exists()).toBe(false);
    expect(synSwitch(wrapper, 'Syn').exists()).toBe(false);
  });

  it('asks for the PIN to open an app, and saves nothing before it', async () => {
    const { wrapper, store } = await show(['finance'], []);
    await synSwitch(wrapper, 'Finance').trigger('click');
    await vi.dynamicImportSettled();
    await flushPromises();
    expect(wrapper.text()).toContain('Enter PIN to let Syn use Finance');
    expect(store.isSynAllowed('finance')).toBe(false);
    expect(invoke).not.toHaveBeenCalledWith('update_app_lock_config', expect.anything());
  });

  it('closes an app without asking', async () => {
    const { wrapper, store } = await show(['finance'], ['finance']);
    await synSwitch(wrapper, 'Finance').trigger('click');
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('update_app_lock_config', { config: { syn_allowed_apps: [] }, pin: null });
    expect(store.isSynAllowed('finance')).toBe(false);
  });
});
