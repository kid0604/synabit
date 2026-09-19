import { describe, it, expect } from 'vitest';
import {
    lensesFrom,
    nameFor,
    normalise,
    propertiesOf,
    SHELF_QUERY,
} from '../lenses';
import type { QueryResult } from '../views/types';

const shelf = (rows: QueryResult['rows']): QueryResult => ({
    columns: ['title', 'query', 'render', 'icon'],
    rows,
    total: rows.length,
    query_time_ms: 1,
});

describe('A lens', () => {
    /// The point of a lens being a node: listing them is an ordinary query, so
    /// the shelf is built out of the machine the shelf is for.
    it('is listed by asking the vault for one, not by a new read path', () => {
        expect(SHELF_QUERY).toContain('is:lens');
        expect(SHELF_QUERY).toContain('columns:title,query,render,icon');
    });

    it('reads its question and its rendering off the row', () => {
        const [lens] = lensesFrom(
            shelf([
                {
                    id: 'Lens/a.md',
                    node_type: 'lens',
                    title: 'Lens/a.md',
                    cells: ['Gặp Khánh', 'with:khánh when:2019/2026', 'dated', 'users'],
                },
            ]),
        );
        expect(lens).toEqual({
            id: 'Lens/a.md',
            title: 'Gặp Khánh',
            query: 'with:khánh when:2019/2026',
            render: 'dated',
            icon: 'users',
        });
    });

    /// Somebody can make a node of this type by hand, or empty the field. A
    /// button that does nothing when pressed is worse than no button.
    it('is not on the shelf without a question in it', () => {
        const kept = lensesFrom(
            shelf([
                { id: 'Lens/a.md', node_type: 'lens', title: 'Trống', cells: ['Trống', '   ', '', ''] },
                { id: 'Lens/b.md', node_type: 'lens', title: 'Thật', cells: ['Thật', 'is:note', '', ''] },
            ]),
        );
        expect(kept.map(l => l.id)).toEqual(['Lens/b.md']);
    });

    it('falls back to the node title when nobody named it', () => {
        const [lens] = lensesFrom(
            shelf([{ id: 'Lens/a.md', node_type: 'lens', title: 'a', cells: ['', 'is:note', '', ''] }]),
        );
        expect(lens.title).toBe('a');
    });

    it('treats a rendering it does not know as "let the answer choose"', () => {
        expect(normalise('dated')).toBe('dated');
        expect(normalise('bars')).toBe('bars');
        expect(normalise('BARS')).toBe('bars');
        expect(normalise('sparkline')).toBe('auto');
        expect(normalise(undefined)).toBe('auto');
    });

    /// Writing the defaults out would turn two choices nobody made into two
    /// choices the file says they made.
    it('writes only what was chosen', () => {
        expect(propertiesOf({ title: 'x', query: ' is:note ', render: 'auto', icon: '  ' })).toEqual({
            query: 'is:note',
        });
        expect(propertiesOf({ title: 'x', query: 'is:note', render: 'bars', icon: 'utensils' })).toEqual({
            query: 'is:note',
            render: 'bars',
            icon: 'utensils',
        });
    });

    /// The question itself, not a model's guess at what it meant.
    it('names an unnamed question after the question', () => {
        expect(nameFor('  with:khánh   when:2019  ')).toBe('with:khánh when:2019');
        const long = nameFor('a'.repeat(200));
        expect(long).toHaveLength(58);
        expect(long.endsWith('…')).toBe(true);
    });
});
