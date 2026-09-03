import { describe, it, expect } from 'vitest';

// `vite/client` declares `*?raw`, so reading these needs no Node types in a
// config that has none.
import source from '../ThingsApp.vue?raw';

/**
 * Whether the rail still shows what the file says after somebody edits it.
 *
 * The list is a snapshot of the SQLite index; the pane beside it is memory.
 * `saveOrCreate` re-ran the query when it *created* a node and returned early
 * when it merely changed one, and Things listened to no events at all — so
 * renaming an open node left the row reading `not from things` under a heading
 * reading `note from things`, until something else happened to re-run.
 *
 * A wiring guard, and named as one. `ThingsApp.vue` is not mounted anywhere in
 * this suite, so what the listeners *do* is covered where it can be — the bus
 * itself in `useEventBus.spec.ts`, the query in `useThingsQuery.spec.ts`. What
 * is left is whether the two are joined, and that is a thing only the file can
 * answer.
 */
describe('the rail hears about a node changing', () => {
  it('subscribes to all three node events', () => {
    for (const event of ['node:created', 'node:updated', 'node:deleted']) {
      expect(
        source,
        `nothing refreshes the rail when ${event} is raised`,
      ).toContain(`'${event}'`);
    }
    expect(source, 'the events are named but never subscribed to').toMatch(/bus\.on\(/);
  });

  it('refreshes both the list and the pinned section', () => {
    const wiring = source.slice(source.indexOf('const refreshSoon'));
    expect(wiring.slice(0, 400), 'the query is not re-run').toContain('rerun()');
    expect(
      wiring.slice(0, 400),
      'a pinned row keeps its old title',
    ).toContain('loadPinned()');
  });

  /**
   * One save raises one event, but renaming a field across a kind raises one
   * per type and a sync pass can raise a burst. Without coalescing that is a
   * query each, for a list nobody reads that fast.
   */
  it('coalesces a burst into one refresh', () => {
    const wiring = source.slice(source.indexOf('const refreshSoon'), source.indexOf('const refreshSoon') + 400);
    expect(wiring).toContain('clearTimeout');
    expect(wiring).toContain('setTimeout');
  });
});
