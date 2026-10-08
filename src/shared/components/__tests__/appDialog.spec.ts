import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import AppDialog from '../AppDialog.vue';
import { backGuardDepth } from '../../../composables/useBackGuard';

/**
 * Every dialog mounted here is unmounted after its test: an open dialog holds
 * a place on the back guard's stack, which is module state and would
 * otherwise carry into the next test.
 */
const mounted: VueWrapper[] = [];

const open = async (props: Record<string, unknown> = {}) => {
  const wrapper = mount(AppDialog, {
    props: { show: true, ariaLabel: 'Edit thing', ...props },
    slots: { default: '<input id="first" /><button id="second">Save</button>' },
    attachTo: document.body,
  });
  mounted.push(wrapper);
  await flushPromises();
  return wrapper;
};

const scrim = () => document.body.querySelector('.app-dialog-scrim') as HTMLElement;

/** A real click: the press lands on one element, the click on their common ancestor. */
const pressAndRelease = (down: HTMLElement, clickTarget: HTMLElement) => {
  down.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
  clickTarget.dispatchEvent(new MouseEvent('click', { bubbles: true }));
};

beforeEach(() => {
  document.body.innerHTML = '';
  window.history.replaceState(null, '', '/');
});

afterEach(async () => {
  while (mounted.length) {
    const w = mounted.pop()!;
    // Some tests unmount their own; doing it twice throws.
    try { w.unmount(); } catch { /* already gone */ }
  }
  expect(backGuardDepth()).toBe(0);
  // Let any history step a closing dialog set in motion land before the next test.
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
});

/** Android's Back, as the WebView delivers it: a step back through history. */
function pressBack(): Promise<void> {
  return new Promise((resolve) => {
    window.addEventListener('popstate', () => resolve(), { once: true });
    window.history.back();
  });
}

describe('the dialog shell', () => {
  it('announces itself as a named modal dialog', async () => {
    await open();
    const el = document.body.querySelector('[role="dialog"]')!;
    expect(el.getAttribute('aria-modal')).toBe('true');
    expect(el.getAttribute('aria-label')).toBe('Edit thing');
  });

  it('moves focus to the first thing that can take it', async () => {
    await open();
    expect(document.activeElement?.id).toBe('first');
  });

  it('closes on Escape and on the scrim, but not on a click inside', async () => {
    const w = await open();
    document.body.querySelector('#second')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(w.emitted('close')).toBeUndefined();
    document.body.querySelector('[role="dialog"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(w.emitted('close')).toHaveLength(1);
    pressAndRelease(scrim(), scrim());
    expect(w.emitted('close')).toHaveLength(2);
  });

  /**
   * Selecting text in a field and letting go past the panel's edge sends the
   * click to the scrim. That used to close the dialog and lose the typing.
   */
  it('stays open when a press that began inside ends on the scrim', async () => {
    const w = await open();
    pressAndRelease(document.body.querySelector('#first') as HTMLElement, scrim());
    expect(w.emitted('close')).toBeUndefined();
  });

  it('keeps Tab inside', async () => {
    await open();
    const first = document.body.querySelector('#first') as HTMLElement;
    const last = document.body.querySelector('#second') as HTMLElement;
    // jsdom lays nothing out, so every element reports a null offsetParent.
    for (const el of [first, last]) Object.defineProperty(el, 'offsetParent', { get: () => document.body });
    last.focus();
    const tab = new KeyboardEvent('keydown', { key: 'Tab', bubbles: true, cancelable: true });
    last.dispatchEvent(tab);
    expect(tab.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(first);
  });

  it('gives focus back when it is removed while still open', async () => {
    const opener = document.createElement('button');
    document.body.appendChild(opener);
    opener.focus();
    const w = await open();
    w.unmount();
    expect(document.activeElement).toBe(opener);
  });

  it('cannot be dismissed when it must be answered', async () => {
    const w = await open({ dismissible: false });
    document.body.querySelector('[role="dialog"]')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    pressAndRelease(scrim(), scrim());
    expect(w.emitted('close')).toBeUndefined();
  });

  it('gives focus back when it closes', async () => {
    const opener = document.createElement('button');
    document.body.appendChild(opener);
    opener.focus();
    const w = await open();
    await w.setProps({ show: false });
    await flushPromises();
    expect(document.activeElement).toBe(opener);
  });
});

describe('the dialog and the back gesture', () => {
  it('closes on Back, as it does on Escape', async () => {
    const w = await open();
    expect(backGuardDepth()).toBe(1);
    await pressBack();
    expect(w.emitted('close')).toHaveLength(1);
    expect(backGuardDepth()).toBe(0);
  });

  it('does not claim Back when it must be answered', async () => {
    await open({ dismissible: false });
    expect(backGuardDepth()).toBe(0);
  });

  it('does not claim Back while it is closed', async () => {
    const w = await open({ show: false });
    expect(backGuardDepth()).toBe(0);
    await w.setProps({ show: true });
    expect(backGuardDepth()).toBe(1);
    await w.setProps({ show: false });
    expect(backGuardDepth()).toBe(0);
  });

  /**
   * A question asked from inside Settings is a dialog over a dialog. Back must
   * take the question away and leave Settings where it was.
   */
  it('closes the one on top first', async () => {
    const under = await open({ ariaLabel: 'Settings' });
    const over = await open({ ariaLabel: 'Are you sure?', elevated: true });
    expect(backGuardDepth()).toBe(2);

    await pressBack();
    expect(over.emitted('close')).toHaveLength(1);
    expect(under.emitted('close')).toBeUndefined();
    // The caller closes it by dropping it, as most callers do with `v-if`.
    over.unmount();
    expect(backGuardDepth()).toBe(1);

    await pressBack();
    expect(under.emitted('close')).toHaveLength(1);
  });

  /**
   * Most callers mount the dialog with `v-if` and `:show="true"`, so closing it
   * by its button unmounts it while `show` is still true. Its history entry has
   * to go with it, or the next Back is spent on nothing.
   */
  it('takes its history entry away when it is dropped while open', async () => {
    const w = await open();
    const back = vi.spyOn(window.history, 'back');
    try {
      w.unmount();
      await new Promise((r) => setTimeout(r, 0));
      expect(back).toHaveBeenCalledTimes(1);
    } finally {
      back.mockRestore();
    }
  });
});
