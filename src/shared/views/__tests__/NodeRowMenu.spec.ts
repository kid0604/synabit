import { describe, it, expect, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import NodeRowMenu from '../NodeRowMenu.vue';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      things: {
        open_in: 'Open in {app}',
        rename: 'Rename',
        duplicate: 'Duplicate',
        copy_path: 'Copy path',
        delete: 'Delete', pin: 'Pin', unpin: 'Unpin',
      },
    },
  },
});

/**
 * Mount and read from the document, not from the wrapper.
 *
 * The menu teleports itself to `<body>`, so `wrapper.text()` is empty by
 * design — which is the same reason it is visible at all, since the row it was
 * clicked in clips everything that crosses its edge.
 */
const open = (nodeType: string, at = { x: 300, y: 100 }, pinned = false) => {
  mount(NodeRowMenu, {
    props: { nodeId: 'X/y.md', nodeType, at, pinned },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });
  const panel = document.body.querySelector('.fixed') as HTMLElement;
  return { panel, text: () => panel.textContent ?? '' };
};

afterEach(() => {
  document.body.innerHTML = '';
});

describe('the menu on a row', () => {
  /**
   * The point of the whole component. `routeForNodeType` answers `null` for a
   * type no app owns, and the entry is hidden on that answer — so a menu never
   * offers to hand a book to a screen that has never heard of books, and no
   * list in here says which types those are.
   */
  it('offers to open a node in the app that owns its type', () => {
    expect(open('task').text()).toContain('Open in Tasks');
    document.body.innerHTML = '';
    expect(open('note').text()).toContain('Open in Notes');
  });

  it('offers nothing of the kind for a type nobody owns', () => {
    const menu = open('animal');
    expect(menu.text()).not.toContain('Open in');
    // Everything that works on any node is still there.
    expect(menu.text()).toContain('Rename');
    expect(menu.text()).toContain('Duplicate');
    expect(menu.text()).toContain('Delete');
  });

  /**
   * Drawn over the page rather than inside the row.
   *
   * Two things clipped it there and both are still true of the list: it
   * scrolls, and its rows carry `content-visibility: auto`, which brings paint
   * containment and cuts off anything crossing a row's edge. The menu appeared
   * as a two-pixel sliver.
   */
  it('renders outside the list, in a layer over the page', () => {
    const { panel } = open('note');
    expect(panel, 'the menu is teleported to the body').not.toBeNull();
    expect(panel.parentElement).toBe(document.body);
    expect(panel.className).toContain('fixed');
  });

  /** A row at the right edge would otherwise open a menu past the window. */
  it('stays inside the window horizontally', () => {
    expect(parseInt(open('note', { x: 20, y: 100 }).panel.style.left, 10)).toBeGreaterThanOrEqual(8);
    document.body.innerHTML = '';

    const far = open('note', { x: window.innerWidth + 200, y: 100 });
    expect(parseInt(far.panel.style.left, 10)).toBeLessThanOrEqual(window.innerWidth - 8);
  });

  /**
   * A row near the bottom of a long list is exactly where a menu is most
   * likely to be used, and the place it has least room to open downwards.
   */
  it('opens upwards when there is no room below', () => {
    const near = parseInt(open('note', { x: 300, y: 40 }).panel.style.top, 10);
    expect(near).toBeGreaterThan(40);
    document.body.innerHTML = '';

    const bottom = parseInt(open('note', { x: 300, y: window.innerHeight - 4 }).panel.style.top, 10);
    expect(bottom, 'a menu at the bottom edge opens above the button').toBeLessThan(
      window.innerHeight - 4,
    );
    expect(bottom).toBeGreaterThanOrEqual(8);
  });

  /**
   * Pinning, on a node of any kind.
   *
   * It lived only in the Notes menu while only notes could be pinned. Things
   * pins whatever it shows, which is why `pinned` stopped being a note's key
   * and became the app's on every kind.
   */
  it('offers to pin whatever kind of thing this is', () => {
    expect(open('animal').text()).toContain('Pin');
    document.body.innerHTML = '';
    expect(open('book').text()).toContain('Pin');
  });

  /** And offers the other one when it is already pinned. */
  it('offers to unpin one that is pinned', () => {
    const menu = open('animal', { x: 300, y: 100 }, true);

    expect(menu.text()).toContain('Unpin');
    expect(menu.text()).not.toContain('Pin\u00a0');
  });
});
