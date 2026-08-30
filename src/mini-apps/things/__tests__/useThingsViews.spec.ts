import { describe, it, expect, vi, beforeEach } from 'vitest';

const getNodeSummaries = vi.fn();
const writeNode = vi.fn();
const trashNode = vi.fn();

vi.mock('../../../composables/useNodeService', () => ({
  useNodeService: () => ({ getNodeSummaries, writeNode, trashNode }),
}));

import { useThingsViews } from '../composables/useThingsViews';

/**
 * A saved view is a file, which is the whole reason it is worth having — it
 * syncs, it goes into git, it can be corrected in any editor — and also the
 * reason none of its fields can be trusted. It may have been hand-edited,
 * merged between two devices character by character, or written by a version
 * that spelled things differently.
 *
 * The failure that matters is not a crash. It is a view that quietly stops
 * appearing, or one whose layout leaves the pane blank.
 */
describe('reading a saved view', () => {
  beforeEach(() => {
    getNodeSummaries.mockReset();
    writeNode.mockReset();
    trashNode.mockReset();
  });

  const load = async (properties: Record<string, unknown>, title = 'Đang đọc') => {
    getNodeSummaries.mockResolvedValue([{ id: 'Views/x.md', title, properties }]);
    const views = useThingsViews();
    await views.load();
    return views.views.value[0];
  };

  it('reads a view somebody saved', async () => {
    const view = await load({
      query: 'type:book status:reading',
      layout: 'table',
      sort: 'rating',
      sort_descending: false,
      group: 'status',
      columns: ['author', 'rating'],
      home: 'sidebar',
    });

    expect(view.name).toBe('Đang đọc');
    expect(view.query).toBe('type:book status:reading');
    expect(view.layout).toBe('table');
    expect(view.sort).toBe('rating');
    expect(view.sortDescending).toBe(false);
    expect(view.group).toBe('status');
    expect(view.columns).toEqual(['author', 'rating']);
    expect(view.home).toBe('sidebar');
  });

  /**
   * A layout this app does not have would leave the middle of the screen
   * blank; a `home` it does not know would drop the view out of both places it
   * could appear. Falling back beats propagating.
   */
  it('falls back rather than trusting a value it does not know', async () => {
    const view = await load({ layout: 'gantt', home: 'desktop', sort: 42, columns: 'author' });

    expect(view.layout).toBe('list');
    expect(view.home).toBe('things');
    expect(view.sort).toBe('updated_at');
    expect(view.columns).toEqual([]);
  });

  it('survives a file with nothing in it at all', async () => {
    const view = await load({}, '');
    expect(view.name).toBe('Untitled');
    expect(view.query).toBe('');
    expect(view.layout).toBe('list');
    expect(view.home).toBe('things');
  });

  /** A merge can leave a list holding something that is not a column name. */
  it('drops a column that is not a name', async () => {
    const view = await load({ columns: ['author', 7, null, 'rating'] });
    expect(view.columns).toEqual(['author', 'rating']);
  });

  /**
   * The ladder is one field. Pinning has to be a write of `home` and nothing
   * else, or moving a view between the rail and the sidebar would quietly
   * rewrite the query somebody tuned.
   */
  it('pins by moving one field and leaving the rest alone', async () => {
    getNodeSummaries.mockResolvedValue([{
      id: 'Views/x.md',
      title: 'Đang đọc',
      properties: { query: 'type:book', columns: ['rating'], home: 'things' },
    }]);
    const views = useThingsViews();
    await views.load();

    await views.setHome(views.views.value[0], 'sidebar');

    const written = writeNode.mock.calls[0][0];
    expect(written.properties.home).toBe('sidebar');
    expect(written.properties.query).toBe('type:book');
    expect(written.properties.columns).toEqual(['rating']);
    expect(views.pinned.value).toHaveLength(1);
  });

  /**
   * A view is somebody's arrangement of their own vault, and the gesture that
   * loses it is a mis-aimed click on a small icon. `trashNode` moves the file
   * into `.trash/`; `deleteNode` unlinks it.
   */
  it('trashes a view rather than unlinking it', async () => {
    getNodeSummaries.mockResolvedValue([{ id: 'Views/x.md', title: 'x', properties: {} }]);
    const views = useThingsViews();
    await views.load();

    await views.remove(views.views.value[0]);

    expect(trashNode).toHaveBeenCalledWith({ relPath: 'Views/x.md' });
    expect(views.views.value).toHaveLength(0);
  });

  it('writes a new view into Views/ with an id of its own', async () => {
    getNodeSummaries.mockResolvedValue([]);
    const views = useThingsViews();
    await views.load();

    const made = await views.save({
      name: 'Việc lúc mệt',
      query: 'type:task -status:done energy:low',
      layout: 'list',
      sort: 'due_date',
      sortDescending: false,
      group: '',
      columns: ['energy'],
      home: 'things',
    });

    expect(made?.id).toMatch(/^Views\/.+\.md$/);
    expect(writeNode.mock.calls[0][0].nodeType).toBe('view');
    expect(writeNode.mock.calls[0][0].eventType).toBe('created');
  });
});
