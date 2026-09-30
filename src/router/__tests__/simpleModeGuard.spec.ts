import { describe, it, expect, beforeEach, vi } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';

vi.mock('@tauri-apps/plugin-store', () => ({ load: vi.fn() }));
vi.mock('@tauri-apps/plugin-os', () => ({ type: () => 'macos' }));

import router from '../index';
import { useAppStore } from '../../stores/useAppStore';
import { simpleModePass } from '../../shared/simpleMode';

/**
 * The real guard, wired to the real store: `appAccess.spec.ts` covers the rule,
 * this covers that the router actually asks it — and asks the store at
 * navigation time, so flipping the setting takes effect without a reload.
 */
describe('router guard in simple mode', () => {
  beforeEach(async () => {
    setActivePinia(createPinia());
    await router.push('/note');
  });

  it('lets every app through when simple mode is off', async () => {
    await router.push('/whiteboard');
    expect(router.currentRoute.value.name).toBe('whiteboard');
  });

  it('redirects a hidden app to Notes when simple mode is on', async () => {
    useAppStore().simpleMode = true;
    await router.push('/whiteboard');
    expect(router.currentRoute.value.name).toBe('note');
    await router.push('/calendar');
    expect(router.currentRoute.value.name).toBe('calendar');
  });

  it('follows the old /syn redirect into the guard', async () => {
    useAppStore().simpleMode = true;
    await router.push('/task');
    await router.push('/syn');
    expect(router.currentRoute.value.name).toBe('note');
  });

  it('lets a hidden app through once when asked, and closes it again on leaving', async () => {
    useAppStore().simpleMode = true;
    simpleModePass.value = 'whiteboard';
    await router.push('/whiteboard');
    expect(router.currentRoute.value.name).toBe('whiteboard');
    await router.push('/task');
    expect(simpleModePass.value).toBeNull();
    await router.push('/whiteboard');
    expect(router.currentRoute.value.name).toBe('note');
  });
});
