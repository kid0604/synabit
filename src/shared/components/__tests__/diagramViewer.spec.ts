import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import DiagramViewer from '../DiagramViewer.vue';
import source from '../DiagramViewer.vue?raw';
import bubble from '../../../mini-apps/messages/components/MessageBubble.vue?raw';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';

/** Enough of an SVG to be one. jsdom measures everything as zero, which is
 *  exactly why `fit` has to survive a diagram it cannot measure. */
const SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 400" style="max-width: 800px"><rect/></svg>';

const open = (svg: string | null = SVG) =>
  mount(DiagramViewer, {
    props: { svg },
    attachTo: document.body,
    global: { mocks: { $t: (key: string) => key } },
  });

/**
 * A Mermaid flowchart drawn for "the architecture of Splunk" comes back a
 * thousand pixels wide. A chat bubble is four hundred, and `max-width: 100%`
 * scales the whole thing down until the labels are two pixels tall — legible
 * the way a map folded to the size of a stamp is legible.
 *
 * `overflow-x: auto` never helped, because the SVG shrank rather than
 * overflowed. This is where the diagram is actually looked at.
 */
describe('the diagram viewer', () => {
  // The panel is teleported to `body`, so it is the document that has to be
  // asked about it, not the wrapper.
  const shown = () => document.body.textContent ?? '';
  const stage = () => document.querySelector('.flex-1') as HTMLElement | null;
  const percent = () => Number(shown().match(/(\d+)%/)?.[1]);

  const wheel = async (deltaY: number) => {
    stage()?.dispatchEvent(
      new WheelEvent('wheel', { deltaY, clientX: 10, clientY: 10, bubbles: true, cancelable: true }),
    );
  };

  it('shows nothing at all until there is something to show', () => {
    const w = open(null);
    expect(document.querySelector('svg')).toBeNull();
    w.unmount();
  });

  it('closes on Escape, and says so through the event rather than by hiding itself', async () => {
    const w = open();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await w.vm.$nextTick();
    expect(w.emitted('close')).toBeTruthy();
    w.unmount();
  });

  /**
   * The wheel is the whole point: there is no one right size for a diagram, so
   * the viewer opens at "whole" and the wheel covers the rest.
   */
  it('zooms on the wheel, and stops at both ends', async () => {
    const w = open();
    await w.vm.$nextTick();
    const before = percent();

    await wheel(-100);
    await w.vm.$nextTick();
    expect(percent()).toBeGreaterThan(before);

    // Far past the ceiling, and it stops rather than running away.
    for (let i = 0; i < 80; i++) await wheel(-100);
    await w.vm.$nextTick();
    expect(percent()).toBe(800);

    for (let i = 0; i < 200; i++) await wheel(100);
    await w.vm.$nextTick();
    expect(percent()).toBe(20);
    w.unmount();
  });

  /**
   * jsdom reports every box as zero, which is the case `fit` has to survive:
   * dividing by it would give `Infinity` and the diagram would vanish.
   */
  it('leaves the size alone when nothing can be measured', async () => {
    const w = open();
    await w.vm.$nextTick();
    await w.vm.$nextTick();
    expect(shown()).toContain('100%');
    w.unmount();
  });

  /**
   * Mermaid writes `max-width` into the SVG's own style attribute — the thing
   * that keeps it inside a bubble, and the one thing not wanted here. Left in,
   * the drawing is scaled down and then scaled back up, which is soft edges for
   * no reason.
   */
  it('takes off the max-width Mermaid wrote into the picture', () => {
    expect(source).toContain('max-width: none !important');
  });

  it('has a label in both languages for everything it can do', () => {
    for (const locale of [en, vi]) {
      for (const key of [
        'diagram_open', 'diagram_zoom_in', 'diagram_zoom_out',
        'diagram_fit', 'diagram_close', 'diagram_hint',
      ]) {
        expect(locale.syn, key).toHaveProperty(key);
      }
    }
  });
});

/**
 * A picture that does something has to say so.
 *
 * The diagram in the bubble was a dead end: fitted, unreadable, and with no
 * hint that there was anything more to it.
 */
describe('opening a diagram from the conversation', () => {
  it('marks the rendered diagram so a click can find it', () => {
    expect(bubble).toContain('data-diagram=');
    expect(bubble).toContain("target.closest('[data-diagram]')");
    expect(bubble).toContain('DiagramViewer');
  });

  it('says it opens, by cursor and by label, before anything is clicked', () => {
    expect(bubble).toContain('cursor: zoom-in');
    expect(bubble).toContain("t('syn.diagram_open')");
    expect(bubble, 'and to somebody who cannot see a cursor').toContain('role="button"');
  });

  /** A button that only answers a mouse is a lie told to whoever is reading
   *  this with a keyboard. */
  it('opens from the keyboard as well as the mouse', () => {
    expect(bubble).toContain('handleContentKey');
    expect(bubble).toContain("e.key !== 'Enter' && e.key !== ' '");
    expect(bubble).toContain('tabindex="0"');
  });

  /**
   * The SVG is kept rather than read back out of the DOM it was written into.
   * Reading it back would work and would be fetching a copy of something
   * already held.
   */
  it('keeps what it rendered', () => {
    expect(bubble).toContain('diagrams.set(id, svg)');
  });
});
