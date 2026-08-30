import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';
import TableView from '../TableView.vue';
import type { QueryResult } from '../types';

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: {
    en: {
      note: {
        query_summary: '{shown} rows in {ms}ms',
        query_more: 'and more',
      },
    },
  },
});

const mountWith = (result: QueryResult | null, props: Record<string, unknown> = {}) =>
  mount(TableView, {
    props: { result, ...props },
    global: { plugins: [i18n] },
  });

const result = (over: Partial<QueryResult> = {}): QueryResult => ({
  columns: ['title', 'author', 'rating'],
  rows: [
    { id: 'Books/sapiens.md', node_type: 'book', title: 'Sapiens', cells: ['Sapiens', 'Harari', '5'] },
    { id: 'Books/dune.md', node_type: 'book', title: 'Dune', cells: ['Dune', 'Herbert', '4'] },
  ],
  total: 2,
  query_time_ms: 3,
  ...over,
});

/**
 * The table was the only generic view this app had, and it lived inside the
 * Notes folder with the fetching baked into it. Moving it to `shared/views`
 * split those apart, and nothing was testing either half — the query block in
 * a note was verified by opening one and looking.
 *
 * These pin the half that is now shared, and they pin it as a *view*: given a
 * result, what does it draw, and what does it refuse to know.
 */
describe('TableView', () => {
  it('draws a header per column the engine returned', () => {
    const headers = mountWith(result()).findAll('th').map(h => h.text());
    expect(headers).toEqual(['title', 'author', 'rating']);
  });

  it('draws a row per node, in the order they arrived', () => {
    const titles = mountWith(result())
      .findAll('tbody tr td:first-child')
      .map(td => td.text());
    expect(titles).toEqual(['Sapiens', 'Dune']);
  });

  /**
   * A type nobody wrote code for still draws. The view asks `node_type` for an
   * icon and for nothing else — deciding behaviour by type is how a generic
   * view stops being generic.
   */
  it('draws a type it has never heard of', () => {
    const invented = result({
      columns: ['title', 'species'],
      rows: [
        { id: 'Animal/meo.md', node_type: 'animal', title: 'Mèo Mun', cells: ['Mèo Mun', 'mèo'] },
      ],
      total: 1,
    });
    const wrapper = mountWith(invented);
    expect(wrapper.findAll('tbody tr')).toHaveLength(1);
    expect(wrapper.text()).toContain('Mèo Mun');
  });

  it('emits the row it was clicked on rather than navigating itself', async () => {
    const wrapper = mountWith(result());
    await wrapper.findAll('tbody tr')[1].trigger('click');

    const opened = wrapper.emitted('open');
    expect(opened).toHaveLength(1);
    expect((opened![0][0] as { id: string }).id).toBe('Books/dune.md');
  });

  /**
   * `total` is the count and `rows` is a page, so a table that stops at the
   * limit has to say so. Reading the count off `rows.length` is the bug that
   * once reported two tasks out of a hundred and twenty-six.
   */
  it('says there is more when the page is not the whole answer', () => {
    expect(mountWith(result({ total: 126 })).text()).toContain('and more');
    expect(mountWith(result()).text()).not.toContain('and more');
  });

  /** A node with no title of its own still needs a row you can click. */
  it('falls back to a label rather than an empty cell', () => {
    const untitled = result({
      columns: ['title'],
      rows: [{ id: 'Notes/x.md', node_type: 'note', title: '', cells: [''] }],
      total: 1,
    });
    expect(mountWith(untitled, { untitledLabel: 'Untitled' }).text()).toContain('Untitled');
  });

  it('draws nothing at all before a result arrives', () => {
    expect(mountWith(null).find('table').exists()).toBe(false);
  });
});
