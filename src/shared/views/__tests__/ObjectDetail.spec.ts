import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import ObjectDetail from '../ObjectDetail.vue';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      things: {
        remove_field: 'Remove', properties: 'Properties',
        show_more_fields: '{count} more', show_fewer_fields: 'Show fewer',
        add_field: 'Add field',
      },
    },
  },
});

/**
 * The width toggle shipped broken, and nothing noticed.
 *
 * The prop was declared, the button was wired, the value arrived — and the div
 * still carried a hard-coded `max-w-3xl`, because an edit meant to make that
 * class conditional silently matched nothing. Type-check, lint and every unit
 * test passed, since none of them can see a CSS class that failed to change.
 *
 * So the assertion is on the class itself. It is the only gate that can fail
 * for this.
 */
const mountWith = (props: Record<string, unknown>, options: Record<string, unknown> = {}) =>
  mount(ObjectDetail, {
    ...options,
    props: {
      nodeType: 'book',
      readOnlyRows: [],
      appFields: [],
      title: 'Sapiens',
      body: '',
      fields: [],
      ...props,
    },
    global: {
      plugins: [i18n],
      stubs: { TiptapEditor: true },
    },
  });

const field = (key: string, value: string) =>
  ({ key, value, kind: 'text' as const, original: value });

const render = (fullWidth: boolean) => mountWith({ fullWidth });

describe('how wide the body runs', () => {
  it('keeps a reading column by default', () => {
    expect(render(false).html()).toContain('max-w-3xl');
  });

  it('lets the body fill the pane when asked', () => {
    const html = render(true).html();
    expect(html).toContain('max-w-none');
    expect(html, 'the capped width must be gone, not merely joined').not.toContain('max-w-3xl');
  });

  /**
   * `w-full` is half the fix. Lifting the cap on a div that has nothing to
   * fill leaves it exactly where it was, which is what "it does not work"
   * looks like from the outside.
   */
  it('gives the column something to fill', () => {
    expect(render(true).html()).toContain('w-full');
  });
});

/**
 * Enter did nothing in a property's value.
 *
 * A field was added, a value typed, Enter pressed — and the only thing that
 * committed it was clicking somewhere else. The value sat on screen looking
 * saved and was not, which is the worst version of this bug: the screen and
 * the file disagreed and nothing said so.
 */
describe('finishing an edit with the keyboard', () => {
  const oneField = () =>
    mountWith(
      { fields: [{ key: 'colour', value: 'vàng', kind: 'text', original: 'vàng' }] },
      { attachTo: document.body },
    );

  /**
   * Enter alone, with no blur triggered by hand.
   *
   * Asserting on both is how this test would have passed while the bug was
   * live: a blur emits `save` on its own, so it hides whether Enter did
   * anything. The field is focused for real, and Enter has to be what takes
   * the focus away.
   */
  it('saves when Enter is pressed in a value', async () => {
    const wrapper = oneField();
    const value = wrapper.findAll('input')[1];
    (value.element as HTMLInputElement).focus();
    expect(document.activeElement, 'the field is focused to begin with').toBe(value.element);

    await value.trigger('keydown.enter');

    expect(document.activeElement, 'Enter leaves the field').not.toBe(value.element);
    expect(wrapper.emitted('save'), 'Enter has to commit like leaving does').toBeTruthy();
  });

  it('saves when the value is left by clicking away', async () => {
    const wrapper = oneField();
    await wrapper.findAll('input')[1].trigger('blur');
    expect(wrapper.emitted('save')).toBeTruthy();
  });

  /**
   * Naming a field and saying what it holds is one thought. Enter used to end
   * it halfway: the name stuck, the cursor was dropped, and reaching the value
   * needed the mouse.
   */
  it('moves from a field’s name to its value on Enter', async () => {
    const wrapper = mountWith({
      fields: [{ key: '', value: '', kind: 'text', original: undefined }],
    });
    // An unnamed field shows its name box, which is where the cursor starts.
    const name = wrapper.findAll('input')[1];

    await name.trigger('keydown.enter');

    expect(wrapper.emitted('save'), 'the name is kept on the way past').toBeTruthy();
  });

  /**
   * Removing a field is the one control here that deletes a value from a file,
   * so somebody has to be asked — and the write cannot happen on the same
   * click. It used to emit `save` alongside `removeField`, which meant the
   * file was already rewritten by the time any confirmation appeared.
   */
  it('does not save on the click that asks to remove a field', async () => {
    const wrapper = mountWith({
      fields: [{ key: 'colour', value: 'vàng', kind: 'text', original: 'vàng' }],
    });
    const remove = wrapper.findAll('button').find(b => b.attributes('aria-label') === 'Remove');
    await remove?.trigger('click');

    expect(wrapper.emitted('removeField')?.[0]).toEqual([0]);
    expect(wrapper.emitted('save'), 'the listener saves, after the answer').toBeFalsy();
  });
});

/**
 * Ten properties on a task and fifty on a kind somebody has been refining.
 *
 * Past a point the list stops being information about this node and becomes
 * the shape of its kind, repeated on every node of it.
 */
describe('a node with more properties than fit', () => {
  const many = () =>
    mountWith({
      fields: [
        field('status', 'done'),
        field('due_date', '2026-07-09'),
        field('comment', ''),
        field('priority', ''),
        field('tags', ''),
      ],
    });

  /** An answer is worth more than an offer, so the answers come first. */
  it('leads with the fields that say something', () => {
    const text = many().text();
    expect(text).toContain('Status');
    expect(text).toContain('Due date');
    expect(text).not.toContain('Priority');
  });

  it('counts what it is holding back', () => {
    expect(many().text()).toContain('3 more');
  });

  it('shows the rest when asked', async () => {
    const wrapper = many();
    await wrapper.findAll('button').find(b => b.text().includes('3 more'))?.trigger('click');

    expect(wrapper.text()).toContain('Priority');
    expect(wrapper.text()).toContain('Comment');
  });

  /**
   * A node just created has nothing but offers. Hiding them because none is
   * filled in would hand somebody a blank page where the whole point was to
   * put the kind's shape in front of them.
   */
  it('shows the empty ones when there is nothing else to show', () => {
    const wrapper = mountWith({
      fields: [field('species', ''), field('colour', '')],
    });

    expect(wrapper.text()).toContain('Species');
    expect(wrapper.text()).toContain('Colour');
  });

  /**
   * Rows are edited and removed by position, so a filtered list has to carry
   * the original index. Getting this wrong deletes a field somebody can see by
   * clicking the X beside a different one.
   */
  it('removes the row that was clicked, not the one in that position', async () => {
    const wrapper = many();
    const removes = wrapper.findAll('button').filter(b => b.attributes('aria-label') === 'Remove');

    // The second visible row is `due_date`, which is index 1 in the full list.
    await removes[1].trigger('click');
    expect(wrapper.emitted('removeField')?.[0]).toEqual([1]);
  });

  /**
   * Collapsing uses `v-show`, so the rows stay in the document and keep their
   * state — a half-typed value survives a fold.
   *
   * Which is why this asserts on the style rather than on `wrapper.text()`,
   * which reads hidden nodes too and would pass whether the fold worked or
   * not. `isVisible()` is no good here either: it needs the tree in the
   * document, and these mount detached.
   */
  it('puts the whole lot away when the heading is clicked', async () => {
    const wrapper = many();
    const rows = () => wrapper.find('.space-y-1\\.5');
    expect(rows().attributes('style')).toBeUndefined();

    await wrapper.findAll('button').find(b => b.text().includes('Properties'))?.trigger('click');

    expect(rows().attributes('style')).toContain('display: none');
    expect(wrapper.text(), 'and the heading says how many are folded away').toContain('5');
  });
});

/**
 * Asserted on the class, because nothing else can see it.
 *
 * This is the same gate the width toggle needed: type-check, lint and every
 * behavioural test pass over a missing padding class without noticing, and the
 * only report is somebody looking at the screen and saying it looks wrong.
 */
describe('the space under a body', () => {
  it('leaves room below the last line', () => {
    const wrapper = mountWith({ body: 'Mất gói ở chặng hai.' });

    // Read from the class lists, not from `html()`. Vue renders comments into
    // the output, and the comment above this padding explains it by name — so
    // asserting on the HTML string passed with the padding deleted. A test
    // that cannot fail is not a test.
    const roomy = wrapper.findAll('div')
      .some(d => d.classes('pt-6') && d.classes('pb-20'));

    // `pb-20` is what the Notes editor carries. A body that ends on the pane's
    // edge cannot be scrolled anywhere comfortable to read, and clicking below
    // the text has almost nothing to hit.
    expect(roomy).toBe(true);
  });
});
