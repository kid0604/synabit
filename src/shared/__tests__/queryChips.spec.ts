import { describe, it, expect } from 'vitest';
import { chipsOf, SINGULAR, tokenise, withFilter, withTag, without } from '../queryChips';

describe('The query bar, both ways round', () => {
    /// The property the whole design rests on: chips are the text, so there is
    /// nothing to keep in step. Anything typed slices into chips whose tokens
    /// join back to exactly what was typed.
    it('round-trips anything anybody types', () => {
        for (const text of [
            'with:khánh when:2019',
            'is:task -status:done #gia-đình',
            'when:2019/2026 sort:-when limit:20',
            'with:"Bùi Văn Phương" magnitude:>4',
            'cà phê with:khánh',
        ]) {
            expect(chipsOf(text).map(c => c.text).join(' ')).toBe(text);
        }
    });

    it('keeps a quoted name whole', () => {
        expect(tokenise('with:"Bùi Văn Phương" when:2019')).toEqual([
            'with:"Bùi Văn Phương"',
            'when:2019',
        ]);
    });

    /// Somebody mid-sentence has an unclosed quote. Repairing it would rewrite
    /// what they are still typing.
    it('leaves a half-typed quote exactly as typed', () => {
        expect(tokenise('with:"Bùi Văn')).toEqual(['with:"Bùi Văn']);
    });

    it('reads a key off a token, and knows when there is not one', () => {
        expect(chipsOf('with:khánh')[0]).toEqual({
            text: 'with:khánh',
            key: 'with',
            label: 'khánh',
        });
        expect(chipsOf('#gia-đình')[0].key).toBe('#');
        expect(chipsOf('cà')[0]).toEqual({ text: 'cà', key: '', label: 'cà' });
        // A colon inside a bare word is not a key.
        expect(chipsOf('19:30')[0].key).toBe('19');
        expect(chipsOf('-status:done')[0].key).toBe('status');
    });

    it('shows a quoted value without its quotes', () => {
        expect(chipsOf('with:"Bùi Văn Phương"')[0].label).toBe('Bùi Văn Phương');
    });

    it('takes a chip out and leaves the rest tidy', () => {
        expect(without('with:khánh when:2019 #gia-đình', 1)).toBe('with:khánh #gia-đình');
        expect(without('with:khánh', 0)).toBe('');
    });

    // ─── What pressing things does ──────────────────────────────

    it('adds a filter by pressing something', () => {
        expect(withFilter('', 'with', 'khánh')).toBe('with:khánh');
        expect(withFilter('when:2019', 'with', 'khánh')).toBe('when:2019 with:khánh');
    });

    /// Two people means both were there, which is a real question. Two dates
    /// is not — the engine keeps one, so the bar must too.
    it('repeats the keys the engine repeats and replaces the ones it does not', () => {
        expect(withFilter('with:khánh', 'with', 'minh')).toBe('with:khánh with:minh');
        expect(withFilter('when:2019', 'when', '2021')).toBe('when:2021');
        expect(withFilter('is:note', 'is', 'task')).toBe('is:task');
    });

    /// The same gesture that put a filter on takes it off. Without this,
    /// pressing the strip twice leaves a date behind that nobody can see.
    it('takes a filter off when the same one is pressed again', () => {
        expect(withFilter('when:2019', 'when', '2019')).toBe('');
        expect(withFilter('with:khánh with:minh', 'with', 'khánh')).toBe('with:minh');
    });

    it('quotes a name with a space in it, so it survives the trip back', () => {
        const text = withFilter('', 'with', 'Bùi Văn Phương');
        expect(text).toBe('with:"Bùi Văn Phương"');
        expect(chipsOf(text)).toHaveLength(1);
        expect(chipsOf(text)[0].label).toBe('Bùi Văn Phương');
    });

    it('toggles a tag by its own mark rather than a key', () => {
        expect(withTag('with:khánh', 'gia-đình')).toBe('with:khánh #gia-đình');
        expect(withTag('#gia-đình', '#gia-đình')).toBe('');
    });

    it('ignores being pressed with nothing', () => {
        expect(withFilter('is:note', 'with', '  ')).toBe('is:note');
        expect(withTag('is:note', ' # ')).toBe('is:note');
    });

    /// The list mirrors which fields are an `Option` on the Rust side. If the
    /// two drift, pressing twice builds a query the engine cannot answer.
    it('treats the timeline keys the way the parser declares them', () => {
        for (const one of ['when', 'shape', 'magnitude']) {
            expect(SINGULAR).toContain(one);
        }
        for (const many of ['with', 'where', 'about']) {
            expect(SINGULAR).not.toContain(many);
        }
    });
});
