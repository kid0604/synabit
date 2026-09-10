import { describe, it, expect } from 'vitest';

import source from '../ThingsApp.vue?raw';

/**
 * What Things does once a version has been restored.
 *
 * `restore_node_version` writes the whole file, frontmatter included. Things
 * used to take the text the restore handed back and save it as the body, which
 * put the old frontmatter inside the body and wrapped the file in a second one.
 * The file on disk is already right, so the pane has to read it, not write it.
 *
 * A wiring guard, like `railStaysCurrent.spec.ts` and for the same reason:
 * `ThingsApp.vue` is mounted nowhere in this suite. What restoring does to the
 * file is covered in Rust (`commands::versions`), and the modal's side in
 * `NoteHistoryModal.spec.ts`.
 */
describe('a restored node is read back, not written back', () => {
  const handler = () => {
    const start = source.indexOf('const onVersionRestored');
    expect(start, 'the restore handler is gone').toBeGreaterThan(-1);
    return source.slice(start, source.indexOf('};', start));
  };

  it('opens the node again from disk', () => {
    expect(handler()).toContain('detail.open(');
  });

  it('neither sets the body nor saves it', () => {
    expect(handler()).not.toMatch(/detail\.body\.value\s*=/);
    expect(handler()).not.toContain('detail.save(');
  });

  // A restore keeps the version it replaces, and a version is what was saved.
  it('saves what the pane holds before the restore lands', () => {
    const modal = source.slice(source.indexOf('<NoteHistoryModal'));
    expect(modal.slice(0, modal.indexOf('/>'))).toContain(':before-restore="() => detail.save()"');
  });
});
