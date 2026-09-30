import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { defineComponent, KeepAlive, h, ref } from 'vue';
import { mount } from '@vue/test-utils';
import { useUndoableAction } from '../useUndoableAction';

/** The composable registers an unmount hook, so it lives inside a component. */
const host = (options?: Parameters<typeof useUndoableAction>[0]) => {
  let api!: ReturnType<typeof useUndoableAction>;
  const wrapper = mount(defineComponent({
    setup() {
      api = useUndoableAction(options);
      return () => null;
    },
  }));
  return { api, wrapper };
};

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('an undoable action', () => {
  it('does nothing real until the window closes', async () => {
    const { api } = host({ windowMs: 1000 });
    const commit = vi.fn();
    await api.run('Deleted A', commit, vi.fn());
    expect(api.show.value).toBe(true);
    expect(commit).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1000);
    expect(commit).toHaveBeenCalledOnce();
    expect(api.show.value).toBe(false);
  });

  it('puts the screen back and never commits when undone', async () => {
    const { api } = host({ windowMs: 1000 });
    const commit = vi.fn();
    const revert = vi.fn();
    await api.run('Deleted A', commit, revert);
    api.undo();
    await vi.advanceTimersByTimeAsync(2000);
    expect(revert).toHaveBeenCalledOnce();
    expect(commit).not.toHaveBeenCalled();
  });

  it('commits the waiting action when another one starts', async () => {
    const { api } = host({ windowMs: 1000 });
    const first = vi.fn();
    await api.run('Deleted A', first, vi.fn());
    await api.run('Deleted B', vi.fn(), vi.fn());
    expect(first).toHaveBeenCalledOnce();
    expect(api.message.value).toBe('Deleted B');
  });

  it('commits when the screen goes away', async () => {
    const { api, wrapper } = host({ windowMs: 1000 });
    const commit = vi.fn();
    await api.run('Deleted A', commit, vi.fn());
    wrapper.unmount();
    await vi.runAllTimersAsync();
    expect(commit).toHaveBeenCalledOnce();
  });

  it('reverts and reports when the real work fails', async () => {
    const onError = vi.fn();
    const { api } = host({ windowMs: 1000, onError });
    const revert = vi.fn();
    await api.run('Deleted A', () => Promise.reject(new Error('disk')), revert);
    await vi.advanceTimersByTimeAsync(1000);
    expect(revert).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledOnce();
  });

  /**
   * The race the first version had: a third action started while the first was
   * still being written was overwritten by the second, never committed, and
   * left hidden for the rest of the session.
   */
  it('never loses an action started while an earlier one is still being written', async () => {
    const { api } = host({ windowMs: 1000 });
    let finishA!: () => void;
    const a = vi.fn(() => new Promise<void>(r => { finishA = r; }));
    const b = vi.fn();
    const c = vi.fn();
    await api.run('A', a, vi.fn());
    const runB = api.run('B', b, vi.fn());   // starts committing A, which hangs
    void api.run('C', c, vi.fn());           // B is committed at once, C waits
    finishA();
    await runB;
    expect(b).toHaveBeenCalledOnce();
    expect(api.message.value).toBe('C');
    await vi.advanceTimersByTimeAsync(1000);
    expect(c).toHaveBeenCalledOnce();
  });

  it('holds the countdown while paused', async () => {
    const { api } = host({ windowMs: 1000 });
    const commit = vi.fn();
    await api.run('A', commit, vi.fn());
    await vi.advanceTimersByTimeAsync(600);
    api.pause();
    await vi.advanceTimersByTimeAsync(5000);
    expect(commit).not.toHaveBeenCalled();
    api.resume();
    await vi.advanceTimersByTimeAsync(399);
    expect(commit).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(commit).toHaveBeenCalledOnce();
  });

  it('commits when the window is hidden', async () => {
    const { api } = host({ windowMs: 1000 });
    const commit = vi.fn();
    await api.run('A', commit, vi.fn());
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' });
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.advanceTimersByTimeAsync(0);
    expect(commit).toHaveBeenCalledOnce();
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'visible' });
  });

  it('commits when its app is switched away from inside <keep-alive>', async () => {
    let api!: ReturnType<typeof useUndoableAction>;
    const App = defineComponent({ setup() { api = useUndoableAction({ windowMs: 1000 }); return () => null; } });
    const Other = defineComponent({ render: () => null });
    const which = ref<'app' | 'other'>('app');
    const wrapper = mount(defineComponent({
      setup: () => () => h(KeepAlive, null, [which.value === 'app' ? h(App) : h(Other)]),
    }));
    const commit = vi.fn();
    await api.run('A', commit, vi.fn());
    which.value = 'other';
    await wrapper.vm.$nextTick();
    await vi.advanceTimersByTimeAsync(0);
    expect(commit).toHaveBeenCalledOnce();
  });
});
