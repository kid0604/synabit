import { describe, it, expect } from 'vitest';
import { mount } from '@vue/test-utils';
import TaskSidebar from '../components/TaskSidebar.vue';

const mountSidebar = (variant: 'desktop' | 'mobile' = 'desktop') =>
  mount(TaskSidebar, {
    props: {
      variant,
      isMobileOpen: true,
      activeCategory: 'today',
      categoryCounts: { all: 0, today: 0, upcoming: 0, someday: 0, transferred: 0 },
      projects: [],
      filters: [
        { id: 'Filters/a.md', name: 'Overdue' },
        { id: 'Filters/b.md', name: 'Delegated' },
      ],
    },
    global: { mocks: { $t: (key: string) => key } },
  });

const renameButton = (w: ReturnType<typeof mountSidebar>, index = 0) =>
  w.findAll('[aria-label="task.filter_rename"]')[index];

/**
 * Renamed in place rather than through a dialog — this app's WebView has no
 * text prompt to fall back on, which is what made the save button appear to do
 * nothing the first time round.
 */
describe('renaming a saved search', () => {
  it('shows the name, not a field, to begin with', () => {
    const w = mountSidebar();
    expect(w.find('input[aria-label="task.filter_rename"]').exists()).toBe(false);
    expect(w.text()).toContain('Overdue');
  });

  it('opens a field on the rename button', async () => {
    const w = mountSidebar();
    await renameButton(w).trigger('click');
    expect(w.find('input[aria-label="task.filter_rename"]').exists()).toBe(true);
  });

  it('starts from the current name', async () => {
    const w = mountSidebar();
    await renameButton(w).trigger('click');
    const input = w.find('input[aria-label="task.filter_rename"]');
    expect((input.element as HTMLInputElement).value).toBe('Overdue');
  });

  it('renames on Enter', async () => {
    const w = mountSidebar();
    await renameButton(w).trigger('click');
    const input = w.find('input[aria-label="task.filter_rename"]');
    await input.setValue('Overdue at work');
    await input.trigger('keydown.enter');
    expect(w.emitted('rename-filter')).toEqual([['Filters/a.md', 'Overdue at work']]);
  });

  it('trims the name', async () => {
    const w = mountSidebar();
    await renameButton(w).trigger('click');
    const input = w.find('input[aria-label="task.filter_rename"]');
    await input.setValue('   Spaced   ');
    await input.trigger('keydown.enter');
    expect(w.emitted('rename-filter')).toEqual([['Filters/a.md', 'Spaced']]);
  });

  /** An empty name would leave a row with nothing to click on. */
  it('refuses an empty name', async () => {
    const w = mountSidebar();
    await renameButton(w).trigger('click');
    const input = w.find('input[aria-label="task.filter_rename"]');
    await input.setValue('   ');
    await input.trigger('keydown.enter');
    expect(w.emitted('rename-filter')).toBeUndefined();
  });

  it('closes the field once renamed', async () => {
    const w = mountSidebar();
    await renameButton(w).trigger('click');
    const input = w.find('input[aria-label="task.filter_rename"]');
    await input.setValue('Done');
    await input.trigger('keydown.enter');
    expect(w.find('input[aria-label="task.filter_rename"]').exists()).toBe(false);
  });

  /**
   * Escape clears the draft before the field goes away, so the blur that
   * follows it cannot commit what was abandoned.
   */
  it('cancels on Escape without renaming', async () => {
    const w = mountSidebar();
    await renameButton(w).trigger('click');
    const input = w.find('input[aria-label="task.filter_rename"]');
    await input.setValue('Never mind');
    await input.trigger('keydown.escape');
    expect(w.emitted('rename-filter')).toBeUndefined();
    expect(w.find('input[aria-label="task.filter_rename"]').exists()).toBe(false);
  });

  it('renames the search that was clicked, not the first one', async () => {
    const w = mountSidebar();
    await renameButton(w, 1).trigger('click');
    const input = w.find('input[aria-label="task.filter_rename"]');
    await input.setValue('Handed over');
    await input.trigger('keydown.enter');
    expect(w.emitted('rename-filter')).toEqual([['Filters/b.md', 'Handed over']]);
  });

  it('does not select the search while its name is being edited', async () => {
    const w = mountSidebar();
    await renameButton(w).trigger('click');
    expect(w.emitted('update:activeCategory')).toBeUndefined();
  });
});

/** A phone has no hover, so a control that only appears on one is not there. */
describe('on a phone', () => {
  it('shows the rename and delete actions without hovering', () => {
    const w = mountSidebar('mobile');
    expect(w.findAll('[aria-label="task.filter_rename"]').length).toBe(2);
    expect(w.findAll('[aria-label="task.filter_delete"]').length).toBe(2);
  });

  it('renames from there too', async () => {
    const w = mountSidebar('mobile');
    await renameButton(w).trigger('click');
    const input = w.find('input[aria-label="task.filter_rename"]');
    await input.setValue('Phone rename');
    await input.trigger('keydown.enter');
    expect(w.emitted('rename-filter')).toEqual([['Filters/a.md', 'Phone rename']]);
  });
});
