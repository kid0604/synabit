import { describe, it, expect, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import SchemaManager from '../SchemaManager.vue';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      things: {
        manager_title: 'Kinds', n_kinds: '{count} kinds', manager_search: 'Search…',
        sort_count: 'Most things', sort_loose: 'Most loose', sort_name: 'By name',
        col_kind: 'Kind', col_things: 'Things', col_fields: 'Fields', col_loose: 'Loose',
        manager_nothing: 'Nothing matches that.', row_actions: 'More',
        rename_kind_short: 'Rename…', remove_kind_short: 'Remove…',
        delete_kind_hint: 'Remove {type} from the vault',
        manager_note: 'Loose means keys the shape does not account for.',
        new_kind: 'New kind', cancel: 'Cancel', back: 'Back',
        manager_summary: '{things} things, {loose} loose.',
        new_kind_title: 'Design a kind', new_kind_save: 'Save', new_kind_note: 'Shape only.',
        kind_name: 'Name', kind_name_hint: 'book…', kind_fields: 'Fields',
        field_name: 'Field name', remove_field: 'Remove', add_field: 'Add field',
        kind_text: 'Text', kind_number: 'Number', kind_boolean: 'Yes / no', kind_date: 'Date', kind_list: 'List',
      },
    },
  },
});

/**
 * The real vault: animals that drifted, tasks that did not, and a kind that
 * was designed and never used.
 */
const kinds = [
  {
    nodeType: 'animal',
    count: 4,
    observed: [
      { key: 'species', count: 2 },
      { key: 'colour', count: 2 },
      { key: 'màu', count: 1 },
      { key: 'node_id', count: 4 },
    ],
    shape: ['species', 'colour'],
    declared: false,
  },
  {
    nodeType: 'task',
    count: 127,
    observed: [{ key: 'status', count: 126 }, { key: 'order', count: 13 }],
    shape: ['status'],
    declared: true,
  },
  { nodeType: 'book', count: 0, observed: [], shape: ['author'], declared: true },
];

/**
 * A page in the main pane, so everything reads straight off the wrapper. It
 * was a teleported modal once and the tests had to go to `document.body` for
 * it; the page is the simpler thing in both places.
 */
/**
 * The row menu teleports to `<body>`, so a leftover from the previous test is
 * a button that answers to a component nobody is mounted on any more.
 */
afterEach(() => {
  document.body.innerHTML = '';
});

const open = () => {
  const wrapper = mount(SchemaManager, { props: { kinds }, global: { plugins: [i18n] } });
  return {
    wrapper,
    text: () => wrapper.text(),
    search: () => wrapper.find('input'),
    // Read from the row, not from a button: a row holds two of them now — one
    // that opens the kind and one that deletes it — and the counts sit between.
    rows: () =>
      wrapper.findAll('.group').map(d => d.text()).filter(t => /animal|task|book/.test(t)),
    rowFor: (name: string) =>
      wrapper.findAll('.group').find(d => d.text().includes(name))?.find('button'),
    menuFor: (name: string) =>
      wrapper.findAll('.group').find(d => d.text().includes(name))?.findAll('button')[1],
    inMenu: (label: string) =>
      Array.from(document.body.querySelectorAll('button'))
        .find(b => b.textContent?.includes(label)),
  };
};

describe('the place kinds are managed', () => {
  /**
   * The column that earns the screen. `màu` is on the files and not in the
   * kind's shape, so `animal` reads as 1 while `task` reads as none — and the
   * kind worth looking at is visible without opening anything.
   */
  it('counts the keys a kind’s shape does not account for', () => {
    const panel = open();
    const animal = panel.rows().find(t => t.includes('animal')) ?? '';
    const task = panel.rows().find(t => t.includes('task')) ?? '';

    // A row with nothing loose prints a dash, so the two rows have to differ
    // in that column — asserting on the digit alone would pass on any number
    // that happened to be somewhere in the row.
    expect(animal).not.toContain('—');
    expect(task).toContain('—');
  });

  /** `order` is the app's. Counting it as loose would flag every kind forever. */
  it('does not count the app’s own keys as loose', () => {
    const text = open().rows().find(t => t.includes('task')) ?? '';
    expect(text).toContain('—');
  });

  /**
   * A kind that exists only because a shape was designed for it. Before this,
   * a kind with no nodes could not appear anywhere at all, which made
   * designing one ahead of time pointless.
   *
   * It carried a "designed" badge, which said the same thing as the 0 beside
   * it — and as a Fields count that can only be a declaration when nothing
   * carries it. Being listed at all is the claim; the row states it.
   */
  it('lists a kind that has been designed and never used', () => {
    const panel = open();
    const row = panel.rows().find(t => t.includes('book')) ?? '';

    expect(row).toBeTruthy();
    expect(row, 'its emptiness is the count, not a label').toContain('0');
    expect(panel.text()).not.toContain('designed');
  });

  /** Looking for `birthday` and being told which kinds have one. */
  it('searches field names, not just kind names', async () => {
    const panel = open();
    await panel.search().setValue('màu');

    const listed = panel.rows();
    expect(listed.some(t => t.includes('animal'))).toBe(true);
    expect(listed.some(t => t.includes('task'))).toBe(false);
  });

  it('finds a kind by its own name too', async () => {
    const panel = open();
    await panel.search().setValue('boo');

    expect(panel.rows().some(t => t.includes('book'))).toBe(true);
  });

  it('says so plainly when a search matches nothing', async () => {
    const panel = open();
    await panel.search().setValue('spaceship');

    expect(panel.text()).toContain('Nothing matches that.');
  });

  it('opens a kind when its row is clicked', async () => {
    const panel = open();
    await panel.rowFor('animal')?.trigger('click');

    expect(panel.wrapper.emitted('open')?.[0]).toEqual(['animal']);
  });

  /** Most things first, so the vault's centre of gravity is at the top. */
  it('leads with the kind holding the most', () => {
    const listed = open().rows();
    expect(listed[0]).toContain('task');
  });

  /**
   * Leaving is a back arrow, not a dismissed overlay. The page keeps the rail
   * beside it and behaves like the Notes manager, which is the pattern
   * somebody has already learned everywhere else in the app.
   */
  it('leaves by going back rather than being dismissed', async () => {
    const panel = open();
    await panel.wrapper.findAll('button')[0].trigger('click');

    expect(panel.wrapper.emitted('close')).toBeTruthy();
  });

  /**
   * Designing happens on this page, in place. A dialog over a page that is
   * itself about kinds would be a second screen for the same subject.
   */
  it('designs a new kind without leaving the page', async () => {
    const panel = open();
    const newKind = panel.wrapper.findAll('button').find(b => b.text().includes('New kind'));
    await newKind?.trigger('click');

    expect(panel.text()).toContain('Design a kind');
    expect(panel.rows(), 'the list gives way to the form').toEqual([]);
  });

  /** And the back arrow then means "back to the list", not "leave". */
  it('returns to the list from the designer without closing', async () => {
    const panel = open();
    await panel.wrapper.findAll('button').find(b => b.text().includes('New kind'))?.trigger('click');
    await panel.wrapper.findAll('button')[0].trigger('click');

    expect(panel.wrapper.emitted('close'), 'still on the manager').toBeFalsy();
    expect(panel.rows().length).toBeGreaterThan(0);
  });
});

/**
 * Taking a kind away.
 *
 * The row opens a kind and the bin removes it, which is why the row stopped
 * being one big button: two acts on one line need two targets.
 */
describe('removing a kind from the list', () => {
  /**
   * Both verbs on the row that lists kinds. Renaming used to live on the
   * kind's own page and removing here, so managing kinds meant knowing which
   * of two screens held which.
   */
  it('offers renaming and removing from the same row', async () => {
    const panel = open();
    await panel.menuFor('animal')?.trigger('click');

    expect(document.body.textContent).toContain('Rename…');
    expect(document.body.textContent).toContain('Remove…');
  });

  it('asks for the kind whose menu was opened', async () => {
    const panel = open();
    await panel.menuFor('animal')?.trigger('click');
    panel.inMenu('Remove…')?.click();
    await panel.wrapper.vm.$nextTick();

    expect(panel.wrapper.emitted('remove')?.[0]).toEqual(['animal']);
  });

  it('renames the same one', async () => {
    const panel = open();
    await panel.menuFor('task')?.trigger('click');
    panel.inMenu('Rename…')?.click();
    await panel.wrapper.vm.$nextTick();

    expect(panel.wrapper.emitted('rename')?.[0]).toEqual(['task']);
  });

  /** And the name still opens it, rather than removing it. */
  it('keeps opening on the name', async () => {
    const panel = open();
    await panel.rowFor('animal')?.trigger('click');

    expect(panel.wrapper.emitted('open')?.[0]).toEqual(['animal']);
    expect(panel.wrapper.emitted('remove')).toBeFalsy();
  });
});
