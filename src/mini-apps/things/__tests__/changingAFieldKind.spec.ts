import { describe, it, expect, vi, beforeEach } from 'vitest';

const { getNode, writeNode } = vi.hoisted(() => ({
  getNode: vi.fn(),
  writeNode: vi.fn(),
}));

vi.mock('../../../composables/useNodeService', () => ({
  useNodeService: () => ({ getNode, writeNode }),
}));

import { useThingsNode } from '../composables/useThingsNode';

/**
 * Changing a field's declared kind, which is this app's ALTER COLUMN TYPE.
 *
 * In a database that statement rewrites every row and can fail halfway. Here
 * it must do neither, because the vault is the truth and a schema is a note
 * about it — so the whole of this file is about proving that a kind change
 * moves nothing, loses nothing, and reaches exactly one place: the box drawn
 * for a value that is not there yet.
 *
 * Written before the behaviour was checked, describing what it should do. The
 * cases that fail are the report.
 */

const taskWith = (props: Record<string, unknown>) => ({
  id: 'Tasks/a.md',
  node_type: 'task',
  title: 'Chaos Testing',
  content: '',
  properties: { node_id: 'kept', type: 'task', ...props },
});

const rowFor = async (props: Record<string, unknown>, shape: { key: string; kind: string }[] = []) => {
  getNode.mockResolvedValue(taskWith(props));
  const detail = useThingsNode();
  await detail.open('Tasks/a.md', () => shape as never);
  return detail;
};

beforeEach(() => {
  getNode.mockReset();
  writeNode.mockReset();
});

/**
 * The rule everything else follows from. A file that holds a date holds a
 * date, whatever a schema written last week says about it.
 */
describe('a field that already holds something', () => {
  const cases: [string, unknown, string][] = [
    ['a date stays a date', '2026-07-09', 'date'],
    ['a number stays a number', 5, 'number'],
    ['a boolean stays a boolean', false, 'boolean'],
    ['a list stays a list', ['mdp'], 'list'],
    ['a word stays a word', 'chưa xong', 'text'],
  ];

  for (const [name, value, kind] of cases) {
    it(`${name}, whatever the schema declares`, async () => {
      // Every declaration is wrong about this value on purpose.
      const detail = await rowFor({ f: value }, [{ key: 'f', kind: 'text' }]);
      const row = detail.fields.value.find(r => r.key === 'f');

      expect(row?.kind).toBe(kind);
    });
  }

  /**
   * Opening and saving under a changed declaration must be a no-op.
   *
   * The added row is what gets `save()` to write at all — it now declines when
   * nothing changed, and a declaration change is not a change to the file. Its
   * key is empty, which `save()` skips, so the patch below is the one this
   * test has always read.
   */
  it('is written back byte for byte', async () => {
    const detail = await rowFor({ due_date: '2026-07-09', priority: 5, done: false });
    detail.addField();
    await detail.save();

    const sent = writeNode.mock.calls[0][0].properties;
    expect(sent.due_date).toBe('2026-07-09');
    expect(sent.priority).toBe(5);
    expect(sent.done).toBe(false);
    expect(typeof sent.priority).toBe('number');
    expect(typeof sent.done).toBe('boolean');
  });
});

/**
 * The one place a declaration reaches — and the case that matters most in this
 * vault, where `due_date` is an empty string on 126 tasks of 127.
 *
 * `kindOf('')` answers text, because an empty string carries no evidence. That
 * is the right answer when nobody has said otherwise and the wrong one when
 * somebody has: declaring the field a date and then being handed a text box on
 * every task that has not been dated is the declaration doing nothing at all.
 */
describe('a field that is empty', () => {
  it('is drawn the way the kind was declared', async () => {
    const detail = await rowFor({ due_date: '' }, [{ key: 'due_date', kind: 'date' }]);
    const row = detail.fields.value.find(r => r.key === 'due_date');

    expect(row?.kind).toBe('date');
  });

  it('falls back to text when nothing was declared', async () => {
    const detail = await rowFor({ due_date: '' });
    const row = detail.fields.value.find(r => r.key === 'due_date');

    expect(row?.kind).toBe('text');
  });

  /** An empty value is still empty: drawing it differently writes nothing. */
  it('is not written just because it is drawn differently', async () => {
    const detail = await rowFor({ due_date: '' }, [{ key: 'due_date', kind: 'date' }]);

    // Nothing changed, so nothing is written — a stronger answer than the one
    // this test used to check, and now the first thing it checks.
    await detail.save();
    expect(writeNode, 'redrawing a row is not an edit').not.toHaveBeenCalled();

    // And when something else does make it write, the value is still empty
    // rather than a date invented from the declaration.
    detail.addField();
    await detail.save();
    expect(writeNode.mock.calls[0][0].properties.due_date).toBe('');
  });

  /**
   * Filling one in saves the kind that was drawn. This is the only way a
   * declaration ever changes what is in a file, and it takes a person doing it.
   */
  it('saves what somebody types into it', async () => {
    const detail = await rowFor({ done: '' }, [{ key: 'done', kind: 'boolean' }]);
    const row = detail.fields.value.find(r => r.key === 'done');
    row!.value = 'true';
    await detail.save();

    expect(writeNode.mock.calls[0][0].properties.done).toBe(true);
  });
});

/**
 * A declaration is never a promise about the values. Nothing here converts,
 * and nothing here refuses.
 */
describe('what a kind change must never do', () => {
  it('does not touch the nodes when the shape is rewritten', async () => {
    const detail = await rowFor({ due_date: '2026-07-09' }, [{ key: 'due_date', kind: 'text' }]);

    // Opening under a contrary declaration writes nothing on its own.
    expect(writeNode).not.toHaveBeenCalled();
    expect(detail.fields.value.find(r => r.key === 'due_date')?.value).toBe('2026-07-09');
  });

  /**
   * A kind the app does not have — from a hand-edited file, or a build that
   * spelled things differently — must not reach a component that cannot draw
   * it.
   */
  it('draws a kind it does not recognise as text', async () => {
    const detail = await rowFor({ f: '' }, [{ key: 'f', kind: 'gigajoules' }]);

    expect(detail.fields.value.find(r => r.key === 'f')?.kind).toBe('text');
  });

  /** A declaration for a key the node does not carry adds no row. */
  it('does not invent a row for a declared field the node lacks', async () => {
    const detail = await rowFor({ due_date: '' }, [
      { key: 'due_date', kind: 'date' },
      { key: 'energy', kind: 'text' },
    ]);

    expect(detail.fields.value.map(r => r.key)).toEqual(['due_date']);
  });
});
