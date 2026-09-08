import { describe, it, expect, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

import ConfirmModal from '../ConfirmModal.vue';

/**
 * The dialog ten screens ask their last question through.
 *
 * It is worth its own tests precisely because nothing owns it: a change here
 * lands on deleting a highlight, disconnecting a device, clearing a PDF and
 * removing a conversation at once, and none of those screens would notice.
 */
const open = async (props: Record<string, unknown> = {}) => {
  const wrapper = mount(ConfirmModal, {
    props: {
      show: true,
      title: 'Delete this thread?',
      message: 'It moves to the Trash.',
      confirmText: 'Delete',
      cancelText: 'Cancel',
      ...props,
    },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
};

const dialog = () => document.body.querySelector('[role="dialog"]') as HTMLElement;
const button = (label: string) =>
  Array.from(document.body.querySelectorAll('button')).find(
    b => b.textContent?.trim() === label
  ) as HTMLButtonElement;

beforeEach(() => {
  document.body.innerHTML = '';
});

describe('the shape of the question', () => {
  it('announces itself as a dialog and says which text is its name', async () => {
    await open();
    const el = dialog();
    expect(el.getAttribute('aria-modal')).toBe('true');

    const labelledBy = el.getAttribute('aria-labelledby');
    expect(document.getElementById(labelledBy!)?.textContent).toContain('Delete this thread?');

    const describedBy = el.getAttribute('aria-describedby');
    expect(document.getElementById(describedBy!)?.textContent).toContain('moves to the Trash');
  });

  /**
   * Three ways to say no — a ✕, a Cancel and the scrim — is two ways to wonder
   * whether they mean the same thing. Material 3 basic dialogs have no ✕.
   */
  it('offers exactly the answers it named', async () => {
    await open();
    const labels = Array.from(document.body.querySelectorAll('button')).map(b =>
      b.textContent?.trim()
    );
    expect(labels).toEqual(['Cancel', 'Delete']);
  });

  it('adds the third answer only when there is one', async () => {
    await open({ secondaryText: 'Keep subtasks' });
    const labels = Array.from(document.body.querySelectorAll('button')).map(b =>
      b.textContent?.trim()
    );
    expect(labels).toEqual(['Cancel', 'Keep subtasks', 'Delete']);
  });

  /**
   * M3's own rule, and the reason to follow it here: with an icon the headline
   * centres under it, so the *shape* of the dialog says what kind of question
   * this is before any of the words are read.
   */
  it('wears the icon and the centred headline only when it is destructive', async () => {
    await open({ isDestructive: true });
    expect(dialog().querySelector('svg[aria-hidden="true"]')).not.toBeNull();
    expect(dialog().querySelector('h2')?.className).toContain('text-center');

    document.body.innerHTML = '';
    await open({ isDestructive: false });
    expect(dialog().querySelector('svg[aria-hidden="true"]')).toBeNull();
    expect(dialog().querySelector('h2')?.className).toContain('text-left');
  });
});

describe('answering it', () => {
  it('reports each answer separately', async () => {
    const wrapper = await open({ secondaryText: 'Keep' });

    button('Delete').click();
    button('Keep').click();
    button('Cancel').click();

    expect(wrapper.emitted('confirm')).toHaveLength(1);
    expect(wrapper.emitted('secondary')).toHaveLength(1);
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });

  /** A modal that cannot be dismissed from the keyboard is one a keyboard
   *  user is stuck inside. */
  it('takes Escape as no', async () => {
    const wrapper = await open();
    dialog().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flushPromises();
    expect(wrapper.emitted('cancel')).toHaveLength(1);
  });

  it('ignores other keys', async () => {
    const wrapper = await open();
    dialog().dispatchEvent(new KeyboardEvent('keydown', { key: 'a', bubbles: true }));
    await flushPromises();
    expect(wrapper.emitted('cancel')).toBeUndefined();
  });

  /**
   * Enter is the key people press to make a dialog go away. On a destructive
   * question it must not be the key that deletes their work — so focus lands
   * on the safe answer, and only there.
   */
  it('lands focus on Cancel when the answer destroys something', async () => {
    await open({ isDestructive: true });
    expect(document.activeElement?.textContent?.trim()).toBe('Cancel');
  });

  it('lands focus on the confirm when nothing is destroyed', async () => {
    await open({ isDestructive: false, confirmText: 'Disconnect' });
    expect(document.activeElement?.textContent?.trim()).toBe('Disconnect');
  });

  /** Focus goes back where it came from, rather than to the document body. */
  it('gives focus back to whatever opened it', async () => {
    const opener = document.createElement('button');
    opener.textContent = 'Open';
    document.body.appendChild(opener);
    opener.focus();

    const wrapper = mount(ConfirmModal, {
      props: { show: false, title: 'T', message: 'M' },
      attachTo: document.body,
    });

    await wrapper.setProps({ show: true });
    await flushPromises();
    expect(document.activeElement).not.toBe(opener);

    await wrapper.setProps({ show: false });
    await flushPromises();
    expect(document.activeElement).toBe(opener);
  });
});
