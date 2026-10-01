import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(0)) }));

import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../../stores/useAppStore';
import { followLink } from '../pane';

beforeEach(() => {
  setActivePinia(createPinia());
  vi.mocked(invoke).mockClear();
});

describe('following a clicked link', () => {
  it("opens in the computer's own browser by default", async () => {
    expect(useAppStore().linkOpenIn).toBe('system');
    await followLink('https://www.themarginalian.org/');
    expect(invoke).toHaveBeenCalledWith('open_link_in_system_browser', { url: 'https://www.themarginalian.org/' });
    expect(invoke).not.toHaveBeenCalledWith('syn_open_page', expect.anything());
  });

  it("opens in Synabit's pane when that is the setting", async () => {
    useAppStore().linkOpenIn = 'pane';
    await followLink('https://aeon.co/');
    expect(invoke).toHaveBeenCalledWith('syn_open_page', { url: 'https://aeon.co/' });
    expect(invoke).not.toHaveBeenCalledWith('open_link_in_system_browser', expect.anything());
  });
});
