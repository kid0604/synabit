import { describe, expect, it } from 'vitest';
import { findOnBoard } from '../boardSearch';
import type { WBNode } from '../boardFile';

const item = (id: string, label: string, x: number, y: number, type: WBNode['type'] = 'sticky'): WBNode => ({ id, type, position: { x, y }, data: { label } });

describe('finding words on a board', () => {
  it('finds without the accents, in reading order', () => {
    const nodes = [item('b', 'Điện lực', 500, 0), item('a', '**Dien** thoai', 0, 10), item('c', 'Nước', 0, 300)];
    expect(findOnBoard(nodes, 'dien').map((n) => n.id)).toEqual(['a', 'b']);
  });

  it('reads card titles, and skips what a folded branch hides', () => {
    const card: WBNode = { id: 'k', type: 'card', position: { x: 0, y: 0 }, data: { ref: 'Tasks/x.md', title: 'Call the bank' } };
    expect(findOnBoard([card], 'bank').map((n) => n.id)).toEqual(['k']);
    expect(findOnBoard([card], 'bank', new Set(['k']))).toEqual([]);
  });
});
