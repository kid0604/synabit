import { describe, it, expect, vi, beforeEach } from 'vitest';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { useThingsQuery } from '../composables/useThingsQuery';

const page = (ids: string[], total: number) => ({
  rows: ids.map(id => ({ id, node_type: 'note', title: id, cells: [] })),
  total,
  took_ms: 1,
});

/**
 * Reaching past the engine's cap.
 *
 * Every query returns at most 500 rows whatever it is asked for, while `total`
 * is a real count of matches — so a kind with a thousand nodes has a second
 * half that no amount of asking the same question will reveal. Skipping is the
 * only way to it.
 */
describe('walking a kind that does not fit in one answer', () => {
  beforeEach(() => invoke.mockReset());

  const started = async () => {
    invoke.mockResolvedValue(page(['a', 'b'], 5));
    const q = useThingsQuery();
    await q.run('type:note');
    return q;
  };

  it('asks for the first page by name, not by accident', async () => {
    await started();
    expect(invoke).toHaveBeenCalledWith('run_node_query', {
      query: 'type:note',
      offset: 0,
    });
  });

  it('skips exactly what it already has', async () => {
    const q = await started();
    invoke.mockResolvedValue(page(['c', 'd'], 5));

    await q.more();

    expect(invoke).toHaveBeenLastCalledWith('run_node_query', {
      query: 'type:note',
      offset: 2,
    });
  });

  /** Appended, so a sort or a group keeps seeing one list. */
  it('adds the page to what is on screen rather than replacing it', async () => {
    const q = await started();
    invoke.mockResolvedValue(page(['c', 'd'], 5));

    await q.more();

    expect(q.result.value?.rows.map(r => r.id)).toEqual(['a', 'b', 'c', 'd']);
    expect(q.result.value?.total).toBe(5);
  });

  it('stops asking once it holds every match', async () => {
    invoke.mockResolvedValue(page(['a', 'b'], 2));
    const q = useThingsQuery();
    await q.run('type:note');
    invoke.mockClear();

    await q.more();

    expect(invoke).not.toHaveBeenCalled();
  });

  /**
   * A page that lands after the question changed belongs to a list nobody is
   * looking at. Appending it would splice one kind's rows into another's.
   */
  it('throws away a page that arrives after a new query', async () => {
    const q = await started();

    let release: (v: unknown) => void = () => {};
    invoke.mockReturnValueOnce(new Promise(r => { release = r; }));
    const inFlight = q.more();

    invoke.mockResolvedValue(page(['x'], 1));
    await q.run('type:task');

    release(page(['c', 'd'], 5));
    await inFlight;

    expect(q.result.value?.rows.map(r => r.id)).toEqual(['x']);
  });
});
