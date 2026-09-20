import { describe, it, expect } from 'vitest';
import { chipsOf, SINGULAR, spends, tokenise, withFilter, withTag, without } from '../queryChips';

describe('The query bar, both ways round', () => {
    /// The property the whole design rests on: chips are the text, so there is
    /// nothing to keep in step. Anything typed slices into chips whose tokens
    /// join back to exactly what was typed.
    it('round-trips anything anybody types', () => {
        for (const text of [
            'with:khánh when:2019',
            'is:task -status:done #gia-đình',
            'when:2019/2026 sort:-when limit:20',
            'with:"Bùi Văn Phương" size:>4',
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
        expect(chipsOf('with:khánh')[0]).toMatchObject({
            text: 'with:khánh',
            key: 'with',
            label: 'khánh',
            negated: false,
        });
        expect(chipsOf('#gia-đình')[0].key).toBe('#');
        expect(chipsOf('cà')[0]).toMatchObject({ text: 'cà', key: '', label: 'cà' });
        // A colon inside a bare word is not a key.
        expect(chipsOf('19:30')[0].key).toBe('19');
        expect(chipsOf('-status:done')[0].key).toBe('status');
    });

    /// The bar used to draw `-with:khánh` and `with:khánh` as the same chip:
    /// the minus was stripped off to find the key and then never shown. Two
    /// opposite questions, one picture.
    it('says when a chip is asking for the absence of something', () => {
        const away = chipsOf('-with:khánh')[0];
        expect(away).toMatchObject({ key: 'with', label: 'khánh', negated: true });
        expect(chipsOf('with:khánh')[0].negated).toBe(false);

        expect(chipsOf('NOT #gia-đình')[0]).toMatchObject({ key: '#', negated: true });
        expect(chipsOf('-#gia-đình')[0]).toMatchObject({ key: '#', label: '#gia-đình', negated: true });
    });

    // ─── Groups ─────────────────────────────────────────────────

    /// A chip is a piece that can be taken off without changing what the rest
    /// means. Half of an alternative is not such a piece.
    it('draws an alternative as one chip, because there is no smaller piece', () => {
        const chips = chipsOf('#gia-đình OR #công-việc');
        expect(chips).toHaveLength(1);
        expect(chips[0]).toMatchObject({ key: 'group', label: '#gia-đình OR #công-việc' });
    });

    /// `OR` binds loosest, so an unbracketed one swallows the whole question —
    /// and brackets are how somebody says otherwise.
    it('gives the chips back when brackets say where the alternative ends', () => {
        const chips = chipsOf('(#gia-đình OR #công-việc) with:khánh');
        expect(chips.map(c => c.key)).toEqual(['group', 'with']);
        expect(chips[0].text).toBe('( #gia-đình OR #công-việc )');
        expect(chips[1].label).toBe('khánh');
    });

    it('takes a whole group off in one press', () => {
        const text = '(#gia-đình OR #công-việc) with:khánh';
        const chips = chipsOf(text);
        expect(without(text, chips[0].from, chips[0].to)).toBe('with:khánh');
        expect(without(text, chips[1].from, chips[1].to)).toBe('( #gia-đình OR #công-việc )');
    });

    it('names the table in a chip of its own', () => {
        const chips = chipsOf('events with:khánh');
        expect(chips[0]).toMatchObject({ key: 'source', label: 'events' });
        expect(chips).toHaveLength(2);
        // Alone it is a word somebody is searching for, as the parser reads it.
        expect(chipsOf('events')[0].key).toBe('');
    });

    /// Lower case is a word, not an operator — the Rust side says so too.
    it('leaves a lower-case or alone', () => {
        expect(chipsOf('#a or #b')).toHaveLength(3);
    });

    // ─── Stages ─────────────────────────────────────────────────

    /// §10: a stage is one chip. `| stats count by month` is four words that
    /// mean one thing, and there is no half of it that means anything.
    it('draws each step of the pipeline as one chip', () => {
        const chips = chipsOf('events when:2019 | stats count by month | head 5');
        expect(chips.map(c => c.key)).toEqual(['source', 'when', 'stage', 'stage']);
        expect(chips[2].label).toBe('stats count by month');
        expect(chips[3].label).toBe('head 5');
    });

    /// The pipe belongs to the stage after it, so taking the last step off
    /// does not leave a dangling `|` for the engine to refuse.
    it('takes the pipe with the step it belongs to', () => {
        const text = 'events when:2019 | stats count by month | head 5';
        const chips = chipsOf(text);
        expect(without(text, chips[3].from, chips[3].to)).toBe(
            'events when:2019 | stats count by month',
        );
        expect(without(text, chips[2].from, chips[2].to)).toBe('events when:2019 | head 5');
    });

    /// A pipeline works on the answer, so a new filter belongs to the
    /// question. Appended at the end it would become three more words of the
    /// stage — and pressing a person on the graph is the bar's main path.
    it('adds a filter to the question, not to the end of the pipeline', () => {
        expect(withFilter('events when:2019 | stats count by month', 'with', 'khánh')).toBe(
            'events when:2019 with:khánh | stats count by month',
        );
        expect(withTag('when:2019 | head 5', 'work')).toBe('when:2019 #work | head 5');
    });

    /// And a key inside a stage is the stage's, not the bar's to replace.
    it('leaves a stage alone when replacing a single-valued key', () => {
        expect(withFilter('when:2019 | sort count desc', 'when', '2021')).toBe(
            'when:2021 | sort count desc',
        );
    });

    /// §13.3: the screen has to know which questions cost money, so the
    /// button that spends can look like one.
    it('knows which questions would pay a model', () => {
        expect(spends('is:note | explode sentences | ask 15')).toBe(true);
        expect(spends('is:note | ask 3')).toBe(true);
        expect(spends('is:note | stats count by month')).toBe(false);
        // A word, not a stage — `ask` only spends after a pipe.
        expect(spends('ask')).toBe(false);
        expect(spends('with:khánh ask 15')).toBe(false);
    });

    it('splits brackets the way the parser does, and leaves a call alone', () => {
        expect(tokenise('(#a OR #b)')).toEqual(['(', '#a', 'OR', '#b', ')']);
        expect(tokenise('when:same-day-as(today)')).toEqual(['when:same-day-as(today)']);
        expect(tokenise('-(#a OR #b)')).toEqual(['-', '(', '#a', 'OR', '#b', ')']);
        expect(tokenise('#a|stats count by month')).toEqual([
            '#a', '|', 'stats', 'count', 'by', 'month',
        ]);
    });

    it('shows a quoted value without its quotes', () => {
        expect(chipsOf('with:"Bùi Văn Phương"')[0].label).toBe('Bùi Văn Phương');
    });

    it('takes a chip out and leaves the rest tidy', () => {
        expect(without('with:khánh when:2019 #gia-đình', 1)).toBe('with:khánh #gia-đình');
        expect(without('with:khánh', 0)).toBe('');
    });

    /// The property the design rests on, now that a chip can span tokens:
    /// what is left after taking a chip off is what the other chips said.
    it('round-trips a question with brackets in it', () => {
        for (const text of [
            '(#gia-đình OR #công-việc) with:khánh',
            'events (with:khánh OR with:minh) when:2019',
            '#gia-đình OR #công-việc',
            'is:note -when:2019',
        ]) {
            expect(chipsOf(text).map(c => c.text).join(' ')).toBe(tokenise(text).join(' '));
        }
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

    /// `OR` binds loosest, so appending to `#a OR #b` would ask for `#a`, or
    /// for `#b` with Khánh. A gesture on the graph must never quietly change
    /// the question that was already there.
    it('brackets an alternative before adding to it', () => {
        expect(withFilter('#a OR #b', 'with', 'khánh')).toBe('( #a OR #b ) with:khánh');
        expect(withTag('#a OR #b', 'work')).toBe('( #a OR #b ) #work');
        // Already bracketed, so nothing to do.
        expect(withFilter('(#a OR #b)', 'with', 'khánh')).toBe('( #a OR #b ) with:khánh');
    });

    /// And a key inside a bracket belongs to the bracket, not to the bar.
    it('replaces a single-valued key only where it stands on its own', () => {
        expect(withFilter('(when:2019 OR #a) #b', 'when', '2021')).toBe(
            '( when:2019 OR #a ) #b when:2021',
        );
        expect(withFilter('when:2019 #b', 'when', '2021')).toBe('when:2021 #b');
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
        for (const one of ['when', 'shape', 'size']) {
            expect(SINGULAR).toContain(one);
        }
        for (const many of ['with', 'place', 'about']) {
            expect(SINGULAR).not.toContain(many);
        }
    });
});
