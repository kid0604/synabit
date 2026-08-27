import { describe, it, expect } from 'vitest';
import { readCheckboxes, openCheckboxes } from '../checkboxes';

/**
 * The line this draws: a box is a box because markdown says so. Nothing here
 * reads prose and decides what somebody probably meant — a tool that invents
 * work items out of sentences is wrong often enough that people stop trusting
 * the ones it gets right.
 */
describe('readCheckboxes', () => {
    it('reads the shapes markdown calls a checkbox', () => {
        const note = [
            '- [ ] call Anh back',
            '* [x] send the deck',
            '+ [ ] book the room',
            '1. [ ] chase the invoice',
            '2) [X] file the notes',
        ].join('\n');
        expect(readCheckboxes(note).map(b => [b.text, b.done])).toEqual([
            ['call Anh back', false],
            ['send the deck', true],
            ['book the room', false],
            ['chase the invoice', false],
            ['file the notes', true],
        ]);
    });

    it('reads an indented box, because a nested list is still a list', () => {
        expect(readCheckboxes('  - [ ] sub item')[0].text).toBe('sub item');
        expect(readCheckboxes('\t\t- [x] deeper')[0].done).toBe(true);
    });

    /** So a task is named after what to do, not after a URL. */
    it('flattens a link to the words in it', () => {
        const note = '- [ ] read [the proposal](synabit://note/Notes/proposal.md) before Friday';
        expect(readCheckboxes(note)[0].text).toBe('read the proposal before Friday');
    });

    it('keeps the line, so the same box stays the same box', () => {
        const note = 'Notes from the meeting\n\n- [ ] first\n- [ ] second';
        expect(readCheckboxes(note).map(b => b.line)).toEqual([2, 3]);
    });

    /** A checkbox inside a fence is an example of one, not one. */
    it('ignores boxes inside a code fence', () => {
        const note = [
            '- [ ] real one',
            '```markdown',
            '- [ ] this is documentation',
            '```',
            '- [ ] another real one',
            '~~~',
            '- [x] also documentation',
            '~~~',
        ].join('\n');
        expect(readCheckboxes(note).map(b => b.text)).toEqual(['real one', 'another real one']);
    });

    it('is not fooled by things that only look like boxes', () => {
        const note = [
            'A sentence with [ ] in it',
            '- [] no space in the box',
            '- [ ]',
            '- [y] not a state',
            '-[ ] no space after the bullet',
            '- [ ]    ',
        ].join('\n');
        expect(readCheckboxes(note)).toEqual([]);
    });

    it('has nothing to say about an empty note', () => {
        expect(readCheckboxes('')).toEqual([]);
        expect(readCheckboxes('Just prose, no boxes at all.')).toEqual([]);
    });
});

describe('openCheckboxes', () => {
    /**
     * Ticked boxes are read but not offered: turning one into a task would
     * create work that has already been done.
     */
    it('offers only the ones still open', () => {
        const note = '- [ ] outstanding\n- [x] finished\n- [ ] also outstanding';
        expect(openCheckboxes(note).map(b => b.text)).toEqual(['outstanding', 'also outstanding']);
    });
});
