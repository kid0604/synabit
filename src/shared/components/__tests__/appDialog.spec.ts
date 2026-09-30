import { describe, it, expect, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import AppDialog from '../AppDialog.vue';

const open = async (props: Record<string, unknown> = {}) => {
  const wrapper = mount(AppDialog, {
    props: { show: true, ariaLabel: 'Edit thing', ...props },
    slots: { default: '<input id="first" /><button id="second">Save</button>' },
    attachTo: document.body,
  });
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
});

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
