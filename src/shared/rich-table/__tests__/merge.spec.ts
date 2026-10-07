import { describe, it, expect } from 'vitest';
import { rebase } from '../merge';

const lines = (...xs: string[]) => xs.join('\n');
const HEAD = ['| A | B |', '| --- | --- |'];
const TAIL = ['<!-- rich-table', 'version: 1', '-->'];

describe('undoing an edit after someone else changed the table', () => {
  it('takes back only the edit, and keeps the row that arrived since', () => {
    const before = lines(...HEAD, '| x | 1 |', ...TAIL);
    const edited = lines(...HEAD, '| x | 5 |', ...TAIL);
    const synced = lines(...HEAD, '| x | 5 |', '| SYN | 99 |', ...TAIL);
    expect(rebase(edited, before, synced)).toBe(lines(...HEAD, '| x | 1 |', '| SYN | 99 |', ...TAIL));
  });

  it('puts a deleted row back where it was', () => {
    const before = lines(...HEAD, '| a | 1 |', '| b | 2 |', '| c | 3 |', ...TAIL);
    const edited = lines(...HEAD, '| a | 1 |', '| c | 3 |', ...TAIL);
    const synced = lines(...HEAD, '| a | 1 |', '| c | 3 |', '| d | 4 |', ...TAIL);
    expect(rebase(edited, before, synced)).toBe(lines(...HEAD, '| a | 1 |', '| b | 2 |', '| c | 3 |', '| d | 4 |', ...TAIL));
  });

  it('takes an added row away again', () => {
    const before = lines(...HEAD, '| a | 1 |', ...TAIL);
    const edited = lines(...HEAD, '| a | 1 |', '|  |  |', ...TAIL);
    const synced = lines('| A | B |', '| --- | --- |', '| z | 0 |', '| a | 1 |', '|  |  |', ...TAIL);
    expect(rebase(edited, before, synced)).toBe(lines(...HEAD, '| z | 0 |', '| a | 1 |', ...TAIL));
  });

  it('is plain undo when nothing else happened', () => {
    expect(rebase('b', 'a', 'b')).toBe('a');
  });

  it('keeps the other side when both changed the same row', () => {
    const before = lines(...HEAD, '| x | 1 |', ...TAIL);
    const edited = lines(...HEAD, '| x | 5 |', ...TAIL);
    const synced = lines(...HEAD, '| x | 7 |', ...TAIL);
    expect(rebase(edited, before, synced)).toBe(synced);
  });

  it('undoes a change to the settings as well as to the rows', () => {
    const before = lines(...HEAD, '| x | 1 |', '<!-- rich-table', 'version: 1', 'columns:', '  B: number', '-->');
    const edited = lines(...HEAD, '| x | 1 |', '<!-- rich-table', 'version: 1', 'columns:', '  B: date', '-->');
    const synced = lines(...HEAD, '| x | 1 |', '| y | 2 |', '<!-- rich-table', 'version: 1', 'columns:', '  B: date', '-->');
    expect(rebase(edited, before, synced)).toContain('  B: number');
    expect(rebase(edited, before, synced)).toContain('| y | 2 |');
  });
});
