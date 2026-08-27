import { describe, it, expect, vi } from 'vitest';
import { ref, computed } from 'vue';
import { useBoardLogic } from '../composables/useBoardLogic';
import { isOrderKey } from '../ordering';
import type { TaskMetadata } from '../types';

vi.mock('../../../shared/platformScope', () => ({ taskViewInPlatformScope: () => true }));

const task = (id: string, over: Partial<TaskMetadata> = {}): TaskMetadata =>
  ({
    id, path: id, title: id, status: 'todo', recurrence: 'none',
    created_at: '2026-01-01 00:00:00', custom_fields: {}, tags: [], ...over,
  }) as TaskMetadata;

const harness = (list: TaskMetadata[]) => {
  const tasks = ref(list);
  const visible = computed(() => tasks.value);
  const category = ref('all');
  const project = computed(() => null);
  const writeNode = vi.fn(async (_p: Record<string, unknown>) => {});
  const toast = vi.fn();
  const api = useBoardLogic(tasks, visible, category, project, { writeNode }, toast);
  return { api, tasks, writeNode, toast };
};

const ids = (list: TaskMetadata[]) => list.map((t) => t.id);

describe('sorting a column of mixed vintages', () => {
  /** Hand-arranged cards are the only ones the user actually placed. */
  it('puts cards with string keys above everything else', () => {
    const h = harness([
      task('float', { custom_fields: { order: 1700000000000 } }),
      task('keyed', { custom_fields: { order: 'V' } }),
      task('never-dragged'),
    ]);
    expect(ids(h.api.tasksByStatus.value.todo)[0]).toBe('keyed');
  });

  it('orders string keys against each other lexicographically', () => {
    const h = harness([
      task('c', { custom_fields: { order: 'W' } }),
      task('a', { custom_fields: { order: 'A' } }),
      task('b', { custom_fields: { order: 'V' } }),
    ]);
    expect(ids(h.api.tasksByStatus.value.todo)).toEqual(['a', 'b', 'c']);
  });

  /** What the board did before: floats ascending, untouched cards newest first. */
  it('keeps the old ordering among cards that have no key', () => {
    const h = harness([
      task('later', { custom_fields: { order: 200 } }),
      task('earlier', { custom_fields: { order: 100 } }),
    ]);
    expect(ids(h.api.tasksByStatus.value.todo)).toEqual(['earlier', 'later']);
  });

  it('sorts undragged cards newest first', () => {
    const h = harness([
      task('old', { created_at: '2026-01-01 00:00:00' }),
      task('new', { created_at: '2026-06-01 00:00:00' }),
    ]);
    expect(ids(h.api.tasksByStatus.value.todo)).toEqual(['new', 'old']);
  });

  /** Reading a board must never write to it. */
  it('writes nothing just to sort', () => {
    const h = harness([
      task('float', { custom_fields: { order: 1 } }),
      task('keyed', { custom_fields: { order: 'V' } }),
    ]);
    void h.api.tasksByStatus.value;
    expect(h.writeNode).not.toHaveBeenCalled();
  });
});

/**
 * The point of the change. A float halves its gap on each insert and collapses
 * after about fifty; these keys subdivide as long as anyone keeps dragging.
 */
describe('keys minted by dropping', () => {
  const drop = (api: any, taskId: string, status: string, clientY: number, cards: string[]) => {
    // A column element that reports the cards in it, which is how the drop
    // handler works out where the pointer landed.
    const columnContent = {
      querySelectorAll: () => cards.map((id) => ({
        getAttribute: () => id,
        getBoundingClientRect: () => ({ top: 0, height: 10 }),
      })),
    };
    const event = {
      dataTransfer: { getData: () => taskId },
      currentTarget: { querySelector: () => columnContent },
      clientY,
    };
    return api.onDrop(event as unknown as DragEvent, status);
  };

  it('gives the first card in an empty column a valid key', async () => {
    const h = harness([task('a')]);
    await drop(h.api, 'a', 'in_progress', 0, []);
    expect(isOrderKey(h.tasks.value[0].custom_fields.order)).toBe(true);
  });

  it('writes a key, not a number', async () => {
    const h = harness([task('a'), task('b', { status: 'in_progress', custom_fields: { order: 'V' } })]);
    await drop(h.api, 'a', 'in_progress', 0, ['b']);
    expect(typeof h.tasks.value[0].custom_fields.order).toBe('string');
  });

  /**
   * The migration. A column still on floats gets written down in the order it
   * is already showing, once, so a key can be minted between its cards.
   */
  it('converts a legacy column on the first drop into it', async () => {
    const h = harness([
      task('dragged'),
      task('x', { status: 'in_progress', custom_fields: { order: 100 } }),
      task('y', { status: 'in_progress', custom_fields: { order: 200 } }),
    ]);
    await drop(h.api, 'dragged', 'in_progress', 0, ['x', 'y']);
    for (const t of h.tasks.value) {
      expect(isOrderKey(t.custom_fields.order), t.id).toBe(true);
    }
  });

  it('keeps the order the legacy column was already showing', async () => {
    const h = harness([
      task('dragged'),
      task('second', { status: 'in_progress', custom_fields: { order: 200 } }),
      task('first', { status: 'in_progress', custom_fields: { order: 100 } }),
    ]);
    await drop(h.api, 'dragged', 'in_progress', 999, ['first', 'second']);
    const column = ids(h.api.tasksByStatus.value.in_progress);
    expect(column.indexOf('first')).toBeLessThan(column.indexOf('second'));
  });

  it('does not migrate a column that is already keyed', async () => {
    const h = harness([
      task('dragged'),
      task('x', { status: 'in_progress', custom_fields: { order: 'V' } }),
    ]);
    await drop(h.api, 'dragged', 'in_progress', 999, ['x']);
    // One write for the dragged card, none for the card that was already fine.
    expect(h.writeNode).toHaveBeenCalledTimes(1);
  });

  it('lands the card where it was dropped', async () => {
    const h = harness([
      task('dragged'),
      task('x', { status: 'in_progress', custom_fields: { order: 'D' } }),
      task('y', { status: 'in_progress', custom_fields: { order: 'W' } }),
    ]);
    await drop(h.api, 'dragged', 'in_progress', 0, ['x', 'y']);
    expect(ids(h.api.tasksByStatus.value.in_progress)[0]).toBe('dragged');
  });

  /** Fifty is roughly where the float ordering used to give up. */
  it('survives sixty drops onto the same spot', async () => {
    const cards = [
      task('x', { status: 'in_progress', custom_fields: { order: 'D' } }),
      task('y', { status: 'in_progress', custom_fields: { order: 'W' } }),
    ];
    const movers = Array.from({ length: 60 }, (_, i) => task(`m${i}`));
    const h = harness([...movers, ...cards]);

    for (let i = 0; i < 60; i += 1) {
      const column = ids(h.api.tasksByStatus.value.in_progress);
      await drop(h.api, `m${i}`, 'in_progress', 5, column);
    }

    const keys = h.api.tasksByStatus.value.in_progress.map(
      (t: TaskMetadata) => t.custom_fields.order as string,
    );
    expect(new Set(keys).size).toBe(keys.length);
    for (let i = 1; i < keys.length; i += 1) {
      expect(keys[i - 1] < keys[i], `${keys[i - 1]} !< ${keys[i]}`).toBe(true);
    }
  });
});
