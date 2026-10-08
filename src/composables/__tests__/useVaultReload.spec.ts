import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { defineComponent, h, KeepAlive, nextTick, ref } from 'vue';
import { mount } from '@vue/test-utils';
import { useVaultReload, type VaultReload } from '../useVaultReload';

/**
 * A screen hidden by the keep-alive must not re-read the vault on every
 * change behind another screen — and must not come back showing what it
 * read before the change either.
 */
describe('useVaultReload', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const setup = () => {
    const reload = vi.fn();
    let api!: VaultReload;
    const Screen = defineComponent({
      setup() {
        api = useVaultReload(reload);
        return () => h('div', 'screen');
      },
    });
    const Other = defineComponent({ render: () => h('div', 'other') });
    const showing = ref<'screen' | 'other'>('screen');
    const wrapper = mount(defineComponent({
      render: () => h(KeepAlive, null, [showing.value === 'screen' ? h(Screen) : h(Other)]),
    }));
    return { reload, api: () => api, showing, wrapper };
  };

  it('coalesces a burst into one reload after the pause', () => {
    const { reload, api } = setup();
    api().schedule();
    api().schedule();
    api().schedule();
    expect(reload).not.toHaveBeenCalled();
    vi.advanceTimersByTime(300);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('marks a hidden screen stale and reloads it once when shown', async () => {
    const { reload, api, showing } = setup();
    showing.value = 'other';
    await nextTick();

    api().schedule();
    api().schedule();
    api().whenShown();
    vi.advanceTimersByTime(1000);
    expect(reload).not.toHaveBeenCalled();

    showing.value = 'screen';
    await nextTick();
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('does not run a reload that was due when the screen went away', async () => {
    const { reload, api, showing } = setup();
    api().schedule();
    showing.value = 'other';
    await nextTick();
    vi.advanceTimersByTime(1000);
    expect(reload).not.toHaveBeenCalled();

    showing.value = 'screen';
    await nextTick();
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('reloads nothing on the way back when nothing changed', async () => {
    const { reload, showing } = setup();
    showing.value = 'other';
    await nextTick();
    showing.value = 'screen';
    await nextTick();
    expect(reload).not.toHaveBeenCalled();
  });
});
