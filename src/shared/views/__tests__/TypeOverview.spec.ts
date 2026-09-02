import { describe, it, expect, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import TypeOverview from '../TypeOverview.vue';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      things: {
        n_things: '{count} things',
        usual_fields: 'Usually has',
        also_seen: 'Also seen in the vault',
        share_of_kind: 'on {percent}% of them',
        app_fields: '{count} app fields',
        type_overview_note: 'Worked out from the files themselves.',
        back_to_kinds: 'All kinds', browse_hint: 'Show every {type} in a table',
        move_up: 'Up', move_down: 'Down',
        drop_from_shape: 'Remove from “Usually has” — nothing is deleted from any file',
        adopt_into_shape: 'Add to “Usually has” — new nodes will arrive with this field',
        rename_field_hint: 'Rename this field across every node of this kind',
        adopt_short: 'Add', drop_short: 'Remove', rename_short: 'Rename…',
        erase_short: 'Delete…', delete_field_hint: 'Delete this field and its value from every node',
        declare_field: 'Declare a field',
        add: 'Add', field_name: 'Field name', no_shape_yet: 'No shape yet',
        kind_text: 'Text', kind_number: 'Number', kind_boolean: 'Yes / no',
        kind_date: 'Date', kind_list: 'List',
        kind_seen_as: 'files say {seen}', kind_change: 'Change the kind',
        row_actions: 'More', kind_from_files: 'What the files hold.',
        remove_kind_short: 'Remove…', rename_kind_short: 'Rename…', kind_json: 'Structured',
        kind_disagrees: 'Declared {declared}; the files hold {seen}.',
      },
    },
  },
});

/**
 * The vault as it is, not as a schema wishes it were.
 *
 * The kind shown here is the real one: four animals, `colour` on two of them
 * and `màu` on a third — one idea under two words, because nothing put the
 * first word on screen when the second was typed.
 */
afterEach(() => {
  document.body.innerHTML = '';
});

/**
 * Open a shape row's menu, which is where its verbs live now.
 *
 * They used to be printed on the row — up, down, Remove, Rename, Delete beside
 * a count, a kind and a warning. Eleven things on one line, and no two rows
 * aligned because the warning is only on some of them.
 */
const openRowMenu = async (wrapper: ReturnType<typeof mount>) => {
  const more = wrapper.findAll('button').find(b => b.attributes('title') === 'More');
  await more?.trigger('click');
  return {
    text: () => document.body.textContent ?? '',
    click: (label: string) =>
      Array.from(document.body.querySelectorAll('button'))
        .find(b => b.textContent?.includes(label))
        ?.click(),
  };
};

const animals = () =>
  mount(TypeOverview, {
    props: {
      nodeType: 'animal',
      count: 4,
      fields: [
        { key: 'type', count: 4 },
        { key: 'node_id', count: 4 },
        { key: 'species', count: 2 },
        { key: 'colour', count: 2 },
        { key: 'màu', count: 1 },
      ],
      usual: ['species', 'colour'],
    },
    global: { plugins: [i18n] },
  });

describe('a kind, described by its own files', () => {
  it('says what a new one of these usually holds', () => {
    const text = animals().text();
    expect(text).toContain('Species');
    expect(text).toContain('Colour');
  });

  /**
   * The point of the screen. A key too rare to be part of the kind is not
   * hidden — hiding it is how a second word for one idea stays invisible, and
   * it would also make this screen a liar about what is in the files.
   */
  it('shows the rare key rather than tidying it away', () => {
    const wrapper = animals();
    expect(wrapper.text()).toContain('màu');
    expect(wrapper.text()).toContain('Also seen in the vault');
  });

  /**
   * No heuristic pairs `colour` with `màu` — they are translations, not
   * similar strings. The counts do the work instead, and they only do it if
   * both are on screen with their numbers.
   */
  it('puts the counts beside both, which is the whole argument', () => {
    const text = animals().text();
    expect(text).toContain('2 / 4');
    expect(text).toContain('1 / 4');
  });

  it('keeps the raw key visible, because queries are written against it', () => {
    expect(animals().text()).toContain('colour');
  });

  /** Machinery is listed, not mixed in with what a person filled out. */
  it('separates the keys the app writes for itself', () => {
    const wrapper = mount(TypeOverview, {
      props: {
        nodeType: 'note',
        count: 10,
        fields: [
          { key: 'tags', count: 9 },
          { key: 'pinned', count: 10 },
          { key: 'node_id', count: 10 },
        ],
        usual: ['tags'],
      },
      global: { plugins: [i18n] },
    });

    expect(wrapper.text()).toContain('2 app fields');
    // `pinned` is the app's; it must not be offered as part of the shape.
    const shape = wrapper.text().split('Also seen')[0].split('app fields')[0];
    expect(shape).not.toContain('Pinned');
  });

  it('survives a kind with no nodes without dividing by zero', () => {
    const wrapper = mount(TypeOverview, {
      props: { nodeType: 'ghost', count: 0, fields: [], usual: [] },
      global: { plugins: [i18n] },
    });

    expect(wrapper.text()).toContain('ghost');
    expect(wrapper.text()).not.toContain('NaN');
  });

  /**
   * Arriving here from the manager used to be a one-way trip: the pane simply
   * became a kind, with no title bar and nothing to click. The way up is the
   * first control on the page, where the manager's own back arrow is.
   */
  it('offers a way back up to the list of kinds', async () => {
    const wrapper = animals();
    await wrapper.findAll('button')[0].trigger('click');

    expect(wrapper.emitted('back')).toBeTruthy();
  });

  it('says which kind you are looking at, and how many there are', () => {
    const text = animals().text();
    expect(text).toContain('animal');
    expect(text).toContain('4');
  });

  /**
   * A field declared and not yet used has to appear, or declaring it does
   * nothing you can see.
   *
   * The shape used to be built by filtering the observed keys, so a field
   * nothing carried had no row to survive the filter: you could add `isbn` to
   * `book` and watch the screen ignore you.
   */
  it('shows a field that was declared before anything had one', () => {
    const wrapper = mount(TypeOverview, {
      props: {
        nodeType: 'book',
        count: 3,
        fields: [{ key: 'author', count: 3 }],
        usual: ['author', 'isbn'],
        declared: true,
      },
      global: { plugins: [i18n] },
    });

    expect(wrapper.text()).toContain('Isbn');
    expect(wrapper.text(), 'and says plainly that nothing carries it yet').toContain('0 / 3');
  });

  /**
   * The shape's order is the shape's own. Filtering the observed list gave
   * back the vault's order every time, which made the up and down arrows look
   * like they did nothing.
   */
  it('lays the shape out in the order it was given, not the vault’s', () => {
    const wrapper = mount(TypeOverview, {
      props: {
        nodeType: 'animal',
        count: 4,
        // The vault reports `colour` first, having more of it.
        fields: [{ key: 'colour', count: 4 }, { key: 'species', count: 1 }],
        usual: ['species', 'colour'],
        declared: true,
      },
      global: { plugins: [i18n] },
    });

    const text = wrapper.text();
    expect(text.indexOf('Species')).toBeLessThan(text.indexOf('Colour'));
  });

  it('declares a new field with the kind chosen for it', async () => {
    const wrapper = animals();
    await wrapper.findAll('button').find(b => b.text().includes('Declare a field'))?.trigger('click');
    await wrapper.find('input').setValue('weight');
    // The kind is a row of pills, not a `<select>`: WKWebView draws native
    // form controls with its own chrome and ignores the stylesheet, so the one
    // on this page arrived wearing macOS's double chevron.
    //
    // The last one, because every shape row carries a picker too — that is how
    // an already-declared field's kind is changed.
    const numbers = wrapper.findAll('button').filter(b => b.text() === 'Number');
    await numbers[numbers.length - 1].trigger('click');
    await wrapper.findAll('button').find(b => b.text() === 'Add')?.trigger('click');

    expect(wrapper.emitted('addField')?.[0]).toEqual(['weight', 'number']);
  });

  /**
   * Two controls sat here as matching grey icons: one changed what a new node
   * is offered, the other rewrote every file of the kind. Words, because the
   * difference is the whole thing a person needs to see.
   */
  /**
   * A stray key keeps its verbs on the row: there are two of them and the row
   * is nearly empty. A shape row has five and a count and a kind, which is how
   * it ended up holding eleven things — those moved into the menu.
   */
  it('says in words what a stray key can be asked to do', () => {
    const text = animals().text();
    expect(text).toContain('Rename…');
    expect(text).toContain('Add');
    // On the button's exact text, not on the page's: the header carries
    // "Remove…" for the kind itself, and a substring check would catch that
    // instead of the shape row's own verb.
    const verbs = animals().findAll('button').map(b => b.text());
    expect(verbs, 'a shape row’s Remove lives in its menu now').not.toContain('Remove');
  });

  /**
   * The verb is on the button; the consequence is on hover.
   *
   * Printing the whole sentence on every row put six copies of "Remove from
   * the shape (keeps the data)" down one column, which reads as a wall and
   * stops being read at all. The consequence is a thing you need once.
   */
  /**
   * The verb is on the button; the consequence is on hover. Printing the whole
   * sentence on every row put six copies of "Remove from the shape (keeps the
   * data)" down one column, which reads as a wall and stops being read.
   */
  it('keeps the full explanation available without printing it six times', async () => {
    const wrapper = animals();
    const onRow = wrapper.findAll('button').map(b => b.attributes('title') ?? '');
    expect(onRow.some(x => x.includes('new nodes will arrive with this field'))).toBe(true);

    await openRowMenu(wrapper);
    const inMenu = Array.from(document.body.querySelectorAll('button'))
      .map(b => b.getAttribute('title') ?? '');
    expect(inMenu.some(x => x.includes('nothing is deleted from any file'))).toBe(true);

    // And the long form is nowhere in the visible text.
    expect(wrapper.text()).not.toContain('nothing is deleted from any file');
  });

  /**
   * The controls do not hide.
   *
   * They used to appear on hover and then stay — a row lit as the cursor
   * passed and never dimmed, and a row whose button had been clicked held
   * focus inside it, so `focus-within` pinned it on for good. Rather than
   * chase that, they stopped hiding: this is a page whose whole purpose is
   * acting on these rows, and controls you cannot see until the cursor finds
   * them are controls you cannot scan.
   */
  it('shows every row’s controls without needing the cursor', () => {
    const html = animals().html();

    expect(html, 'nothing is hidden behind hover').not.toContain('opacity-0');
    // The one that pinned rows open after a click.
    expect(html).not.toContain('focus-within');
  });

  /**
   * Renaming was offered on the loose keys and not on the kind's own, which was
   * an oversight rather than a rule: nothing about the operation cares which
   * section a key sits in. Two words for one idea are as likely between two
   * fields the kind is built on, and `start_date` becoming `due_date` is an
   * ordinary thing to want.
   */
  it('offers renaming on a field the kind is built on, not only on the strays', async () => {
    const wrapper = mount(TypeOverview, {
      props: {
        nodeType: 'animal',
        count: 4,
        fields: [{ key: 'species', count: 4 }],
        usual: ['species'],
        declared: true,
      },
      global: { plugins: [i18n] },
    });

    const menu = await openRowMenu(wrapper);
    menu.click('Rename…');
    await wrapper.vm.$nextTick();

    expect(wrapper.emitted('rename')?.[0]?.[0]).toBe('species');
  });

  /**
   * "So show me them" is the obvious next question on a page describing a
   * kind, and it was answerable only by finding a small layout icon back in
   * the rail. The count somebody is already looking at is the thing to click.
   */
  it('opens the things themselves from the count', async () => {
    const wrapper = animals();
    const pill = wrapper.findAll('button').find(b => b.text().trim().startsWith('4'));

    await pill?.trigger('click');

    expect(wrapper.emitted('browse')).toBeTruthy();
  });

  /** A kind with nothing in it has nothing to open, and does not pretend to. */
  it('is not a button when there is nothing to show', () => {
    const wrapper = mount(TypeOverview, {
      props: { nodeType: 'ghost', count: 0, fields: [], usual: [] },
      global: { plugins: [i18n] },
    });

    expect(wrapper.findAll('button').some(b => b.text().trim() === '0')).toBe(false);
  });

  /**
   * Changing an already-declared field's kind.
   *
   * Nothing is converted by it: every node holding a value keeps that value,
   * and takes its kind from the value. The declaration reaches exactly one
   * place — the empty box on a node that has not filled this in.
   */
  it('changes how an empty one of a declared field is drawn', async () => {
    const wrapper = mount(TypeOverview, {
      props: {
        nodeType: 'task',
        count: 5,
        fields: [{ key: 'due_date', count: 5 }],
        usual: ['due_date'],
        kinds: { due_date: 'date' },
        declared: true,
      },
      global: { plugins: [i18n] },
    });

    // The kind is a word until it is clicked: five pills on every row is two
    // hundred pixels of choice nobody is making right now.
    await wrapper.findAll('button').find(b => b.text().includes('Date'))?.trigger('click');
    await wrapper.findAll('button').find(b => b.text() === 'Text')?.trigger('click');

    expect(wrapper.emitted('setKind')?.[0]).toEqual(['due_date', 'text']);
  });

  /**
   * A declaration the files disagree with.
   *
   * Nothing is broken by it — a value is drawn by what it is, and saving
   * converts nothing — so this is a remark and not an error. What it costs is
   * the empty box, which is exactly what the tooltip says.
   */
  it('says so when the files hold something else', () => {
    const wrapper = mount(TypeOverview, {
      props: {
        nodeType: 'task',
        count: 5,
        fields: [{ key: 'due_date', count: 5 }],
        usual: ['due_date'],
        kinds: { due_date: 'text' },
        observedKinds: { due_date: 'date' },
        declared: true,
      },
      global: { plugins: [i18n] },
    });

    // Shown on the kind itself — declared, then what the files hold.
    expect(wrapper.text()).toContain('Text');
    expect(wrapper.text()).toContain('≠ Date');
  });

  /**
   * The trap. A key that is an empty string on every node is the absence of
   * evidence, so it arrives without an observed kind at all — and a warning
   * that fires where there is nothing to disagree with is one nobody reads.
   */
  it('says nothing when the files hold no evidence either way', () => {
    const wrapper = mount(TypeOverview, {
      props: {
        nodeType: 'task',
        count: 5,
        fields: [{ key: 'priority', count: 5 }],
        usual: ['priority'],
        kinds: { priority: 'number' },
        observedKinds: {},
        declared: true,
      },
      global: { plugins: [i18n] },
    });

    expect(wrapper.text()).not.toContain('≠');
  });

  it('says nothing when the declaration and the files agree', () => {
    const wrapper = mount(TypeOverview, {
      props: {
        nodeType: 'task',
        count: 5,
        fields: [{ key: 'due_date', count: 5 }],
        usual: ['due_date'],
        kinds: { due_date: 'date' },
        observedKinds: { due_date: 'date' },
        declared: true,
      },
      global: { plugins: [i18n] },
    });

    expect(wrapper.text()).not.toContain('≠');
  });
});

/**
 * Why only a shape field's kind can be set.
 *
 * A kind is part of a declaration, and `fields:` in the schema *is* the shape
 * — there is nowhere to hang a kind for a key outside it. So a stray key shows
 * what the files hold instead: a fact rather than a choice, and the same fact
 * that draws those values anyway.
 */
describe('the kind of a key that is not part of the shape', () => {
  const withStray = () =>
    mount(TypeOverview, {
      props: {
        nodeType: 'task',
        count: 127,
        fields: [{ key: 'status', count: 126 }, { key: 'checklist', count: 26 }],
        usual: ['status'],
        kinds: { status: 'text' },
        observedKinds: { status: 'text', checklist: 'json' },
        declared: true,
      },
      global: { plugins: [i18n] },
    });

  it('shows what the files hold', () => {
    expect(withStray().text()).toContain('Structured');
  });

  /** Shown, and not a control: there is no declaration to change. */
  it('is not something to click', () => {
    const wrapper = withStray();
    const asButton = wrapper.findAll('button').some(b => b.text() === 'Structured');

    expect(asButton, 'a fact, not a choice').toBe(false);
  });

  /** And the way to make it a choice is named, where somebody will look. */
  it('says how to make it one', () => {
    const titles = withStray().findAll('span').map(s => s.attributes('title') ?? '');
    expect(titles.some(x => x.includes('What the files hold'))).toBe(true);
  });
});

/**
 * The two verbs a kind answers to, both on the page about it.
 *
 * There were three: a middle one that discarded only the declaration. It went
 * because on a kind with no files it did exactly what Remove does, and on one
 * with files it drew a distinction almost nobody wanted.
 */
describe('what a kind can be asked to do', () => {
  const page = (count: number, declared: boolean) =>
    mount(TypeOverview, {
      props: {
        nodeType: 'abc',
        count,
        fields: [{ key: 'note', count }],
        usual: [],
        declared,
      },
      global: { plugins: [i18n] },
    });

  it('offers renaming and removing, and nothing between them', async () => {
    const wrapper = page(1, true);
    const text = wrapper.text();

    expect(text).toContain('Rename…');
    expect(text).toContain('Remove…');
    expect(text).not.toContain('Discard');
  });

  it('offers both whether or not a structure was declared', () => {
    const text = page(1, false).text();

    expect(text).toContain('Rename…');
    expect(text).toContain('Remove…');
  });

  it('emits the removal rather than doing it', async () => {
    const wrapper = page(1, true);
    await wrapper.findAll('button').find(b => b.text() === 'Remove…')?.trigger('click');

    expect(wrapper.emitted('removeKind')).toBeTruthy();
  });
});
