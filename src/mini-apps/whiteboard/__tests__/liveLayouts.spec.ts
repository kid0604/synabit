import { describe, expect, it } from 'vitest';
import { dayAt, dayOf, kanbanLayout, laneAt, propertyChange, timelineLayout, TASK_STATUS_ORDER } from '../liveLayouts';
import { CARD_SIZE, today } from '../vaultCards';

describe('a kanban frame', () => {
  it('has a column for every task status, even an empty one, and puts each card under its own', () => {
    const k = kanbanLayout([{ id: 'a', value: 'todo' }, { id: 'b', value: 'todo' }, { id: 'c', value: 'in_progress' }], 'status');
    expect(k.lanes.map((l) => l.value)).toEqual(TASK_STATUS_ORDER);
    expect(k.lanes.find((l) => l.value === 'done')!.count).toBe(0);
    const todo = k.lanes.find((l) => l.value === 'todo')!;
    expect(k.positions.get('a')!.x).toBeGreaterThanOrEqual(todo.x);
    expect(k.positions.get('b')!.y).toBeGreaterThan(k.positions.get('a')!.y);
  });

  it('makes columns from any other field, with a last one for cards that have no value', () => {
    const k = kanbanLayout([{ id: 'a', value: 'high' }, { id: 'b', value: '' }, { id: 'c', value: 'low' }], 'priority');
    expect(k.lanes.map((l) => l.value)).toEqual(['high', 'low', '']);
  });

  it('finds the column a card was dropped in, or the nearest', () => {
    const { lanes } = kanbanLayout([], 'status');
    expect(laneAt(lanes, lanes[2].x + 10)!.value).toBe('in_progress');
    expect(laneAt(lanes, 99999)!.value).toBe('done');
  });
});

describe('a timeline frame', () => {
  const rows = [
    { id: 'a', value: '2026-10-05' },
    { id: 'b', value: '2026-10-05T09:00:00Z' },
    { id: 'c', value: '2026-10-20' },
    { id: 'd', value: '' },
  ];

  it('places cards by day, steps a clash down a row, and keeps the undated apart', () => {
    const tl = timelineLayout(rows, 640);
    const [a, b, c, d] = ['a', 'b', 'c', 'd'].map((id) => tl.positions.get(id)!);
    expect(a.x).toBe(b.x);
    expect(b.y).toBeGreaterThan(a.y);
    expect(c.x).toBeGreaterThan(a.x);
    expect(tl.undatedY).not.toBeNull();
    expect(d.y).toBeGreaterThan(tl.undatedY!);
    expect(tl.ticks.length).toBeGreaterThan(1);
  });

  it('reads the day back from where a card was dropped', () => {
    const tl = timelineLayout(rows, 640);
    expect(dayAt(tl.scale, tl.positions.get('c')!.x)).toBe('2026-10-20');
    expect(dayAt(tl.scale, -500)).toBe(dayAt(tl.scale, tl.scale.x0));
  });

  it('reads a date field however it is written', () => {
    expect(dayOf('2026-10-05T09:00:00Z')).toBe('2026-10-05');
    expect(dayOf('next week')).toBeNull();
    expect(CARD_SIZE.width).toBeGreaterThan(0);
  });
});

describe('what a dropped card changes', () => {
  it('dates a task moved into Done, and clears the date when it leaves', () => {
    expect(propertyChange('status', 'done', 'todo', 'task')).toEqual({ status: 'done', completed_at: today() });
    expect(propertyChange('status', 'todo', 'done', 'task')).toEqual({ status: 'todo', completed_at: null });
    expect(propertyChange('due_date', '', '2026-10-05', 'task')).toEqual({ due_date: null });
  });
});
