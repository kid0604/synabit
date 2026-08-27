import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { ref, nextTick, defineComponent, h } from 'vue';
import { mount, type VueWrapper } from '@vue/test-utils';
import { useBackGuard, backGuardDepth } from '../useBackGuard';

/**
 * The back gesture on Android is handled by consuming history entries, so these
 * drive the real `window.history` that jsdom provides and assert on what the
 * user would see: which layer closed, and whether history was left tidy.
 */

/**
 * The layer stack lives at module scope, which is what lets one back press
 * reach a layer registered by a different component. Tests therefore have to
 * clean up after themselves or they leak into each other.
 */
const mounted: VueWrapper[] = [];

/** Mount a component that registers `isOpen` as a back-dismissible layer. */
function mountGuard(isOpen: ReturnType<typeof ref<boolean>>) {
  const closedBy: string[] = [];
  let detach = () => {};
  const wrapper = mount(
    defineComponent({
      setup() {
        detach = useBackGuard(isOpen as any, () => {
          closedBy.push('back');
          isOpen.value = false;
        }).detach;
        return () => h('div');
      },
    }),
  );
  mounted.push(wrapper);
  return { wrapper, closedBy, detach: () => detach() };
}

/** jsdom fires popstate asynchronously, as a real browser does. */
function pressBack(): Promise<void> {
  return new Promise((resolve) => {
    window.addEventListener('popstate', () => resolve(), { once: true });
    window.history.back();
  });
}

describe('useBackGuard', () => {
  beforeEach(() => {
    window.history.replaceState(null, '', '/');
    expect(backGuardDepth()).toBe(0);
  });

  afterEach(async () => {
    // Unmounting is also how the stack is released in production, so this
    // exercises that path on every test rather than only the one that asserts
    // it directly.
    while (mounted.length) mounted.pop()!.unmount();
    expect(backGuardDepth()).toBe(0);
    // Let any `popstate` a test set in motion arrive before the next one runs.
    await new Promise((r) => setTimeout(r, 0));
  });

  it('closes an open layer instead of letting the press through', async () => {
    const isOpen = ref(true);
    const { closedBy } = mountGuard(isOpen);
    await nextTick();

    expect(backGuardDepth()).toBe(1);

    await pressBack();

    expect(isOpen.value).toBe(false);
    expect(closedBy).toEqual(['back']);
    expect(backGuardDepth()).toBe(0);
  });

  it('does nothing while the layer is closed', async () => {
    const isOpen = ref(false);
    mountGuard(isOpen);
    await nextTick();

    expect(backGuardDepth()).toBe(0);
  });

  /**
   * The entry pushed when the layer opened has to come back off when it closes
   * by its own button, or history fills with steps that appear to do nothing.
   */
  it('gives back the history entry when the layer closes on its own', async () => {
    const isOpen = ref(false);
    mountGuard(isOpen);
    await nextTick();

    const before = window.history.length;
    isOpen.value = true;
    await nextTick();
    expect(backGuardDepth()).toBe(1);

    isOpen.value = false;
    await nextTick();
    await new Promise((r) => setTimeout(r, 0));

    expect(backGuardDepth()).toBe(0);
    expect(window.history.length).toBeLessThanOrEqual(before + 1);
  });

  /**
   * The reason `selfInflictedPops` exists. Closing one layer by button must not
   * dismiss the layer underneath it as collateral.
   */
  it('closing the top layer by button leaves the one beneath it open', async () => {
    const outer = ref(true);
    const inner = ref(false);
    const outerGuard = mountGuard(outer);
    mountGuard(inner);
    await nextTick();

    inner.value = true;
    await nextTick();
    expect(backGuardDepth()).toBe(2);

    inner.value = false;
    await nextTick();
    await new Promise((r) => setTimeout(r, 0));

    expect(outer.value).toBe(true);
    expect(outerGuard.closedBy).toEqual([]);
    expect(backGuardDepth()).toBe(1);
  });

  it('unwinds nested layers one press at a time, innermost first', async () => {
    const outer = ref(true);
    const inner = ref(false);
    const outerGuard = mountGuard(outer);
    const innerGuard = mountGuard(inner);
    await nextTick();

    inner.value = true;
    await nextTick();

    await pressBack();
    expect(inner.value).toBe(false);
    expect(innerGuard.closedBy).toEqual(['back']);
    expect(outer.value).toBe(true);

    await pressBack();
    expect(outer.value).toBe(false);
    expect(outerGuard.closedBy).toEqual(['back']);
    expect(backGuardDepth()).toBe(0);
  });

  /**
   * The sidebar's More menu closes and navigates on one click. Removing the
   * menu's history entry the ordinary way schedules a `history.back()`, and
   * the router pushes the new route a few microtasks later — so the press
   * lands on the navigation and undoes it, and the click looks ignored. That
   * is what made the hidden apps, People and Finance, impossible to open.
   *
   * jsdom traverses history too leniently to show the undo itself, so this
   * asserts the step that causes it: after detaching, closing the layer must
   * not reach for `history.back()` at all.
   */
  it('detaching stops the close from spending a history press', async () => {
    const isOpen = ref(false);
    const guard = mountGuard(isOpen);
    await nextTick();

    isOpen.value = true;
    await nextTick();
    expect(backGuardDepth()).toBe(1);

    const back = vi.spyOn(window.history, 'back');
    try {
      // One click: give up the entry, close the menu, and route somewhere new.
      guard.detach();
      isOpen.value = false;
      await nextTick();
      await new Promise((r) => setTimeout(r, 0));

      expect(back).not.toHaveBeenCalled();
      expect(backGuardDepth()).toBe(0);
    } finally {
      back.mockRestore();
    }
  });

  /** Closing without detaching still tidies its own entry away. */
  it('still spends the press when the layer closes on its own', async () => {
    const isOpen = ref(false);
    mountGuard(isOpen);
    await nextTick();

    isOpen.value = true;
    await nextTick();

    const back = vi.spyOn(window.history, 'back');
    try {
      isOpen.value = false;
      await nextTick();
      await new Promise((r) => setTimeout(r, 0));

      expect(back).toHaveBeenCalledTimes(1);
    } finally {
      back.mockRestore();
    }
  });

  /** A layer that unmounts while open must not stay on the stack. */
  it('forgets a layer whose component goes away', async () => {
    const isOpen = ref(true);
    const { wrapper } = mountGuard(isOpen);
    await nextTick();
    expect(backGuardDepth()).toBe(1);

    wrapper.unmount();

    expect(backGuardDepth()).toBe(0);
  });
});
