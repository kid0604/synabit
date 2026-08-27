import { describe, it, expect } from 'vitest';
import { taskProperties, type TaskPropertySource } from '../types';

/**
 * Five call sites used to spell the frontmatter out by hand and had drifted
 * apart. These pin what the shared helper promises, because the failures it
 * prevents are all silent: a field quietly blanked, a hand-written key quietly
 * dropped, and neither visible until the user opens the file.
 */
describe('taskProperties', () => {
  const fullTask: TaskPropertySource = {
    status: 'in_progress',
    is_transferred: false,
    transferred_to: '',
    track_progress: true,
    priority: 'P1',
    start_date: '2026-08-01',
    due_date: '2026-08-30',
    comment: 'blocked on review',
    source_link: '',
    tags: ['work'],
    project_id: 'Projects/x.md',
    completed_at: '',
    custom_fields: {},
  };

  it('writes every field the task owns', () => {
    expect(taskProperties(fullTask)).toMatchObject({
      status: 'in_progress',
      priority: 'P1',
      due_date: '2026-08-30',
      tags: ['work'],
      project_id: 'Projects/x.md',
    });
  });

  /**
   * The reason `custom_fields` is spread first. Somebody's `aliases:` has to
   * survive a save made by a form that has never heard of it.
   */
  it('keeps a key the app has no field for', () => {
    const props = taskProperties({
      ...fullTask,
      custom_fields: { aliases: ['the big one'], order: 1700000000000 },
    });
    expect(props.aliases).toEqual(['the big one']);
    expect(props.order).toBe(1700000000000);
  });

  it('lets the task overwrite a stale copy in custom_fields', () => {
    const props = taskProperties({
      ...fullTask,
      status: 'done',
      custom_fields: { status: 'in_progress' },
    });
    expect(props.status).toBe('done');
  });

  /**
   * The Calendar's task type carries no `priority` and no `is_transferred`.
   * Blanking those on its behalf is how ticking a checkbox in the Calendar
   * would erase a priority set in Tasks.
   */
  it('leaves a field the caller does not have at what the file says', () => {
    const props = taskProperties({
      status: 'done',
      start_date: '2026-08-01',
      due_date: '',
      comment: '',
      source_link: '',
      tags: [],
      custom_fields: { priority: 'P2', is_transferred: true, transferred_to: 'Mai' },
    });
    expect(props.priority).toBe('P2');
    expect(props.is_transferred).toBe(true);
    expect(props.transferred_to).toBe('Mai');
    expect(props.status).toBe('done');
  });

  it('applies overrides last', () => {
    const props = taskProperties(fullTask, { status: 'done', completed_at: '2026-08-23' });
    expect(props.status).toBe('done');
    expect(props.completed_at).toBe('2026-08-23');
  });

  /**
   * `title` and `type` are the backend's to write; it skips them if a caller
   * sends them anyway. Naming them here would be a second, drifting source.
   */
  it('never emits title or type', () => {
    const props = taskProperties({ ...fullTask, custom_fields: {} });
    expect(props).not.toHaveProperty('title');
    expect(props).not.toHaveProperty('type');
  });

  it('copes with a brand-new task that has nothing yet', () => {
    expect(taskProperties({ status: 'todo', tags: [] })).toEqual({ status: 'todo', tags: [] });
  });
});
