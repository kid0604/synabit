/**
 * Turning the boxes in a meeting note into tasks.
 *
 * Parsing, not guessing. `- [ ] call Anh back` is a checkbox because markdown
 * says it is, and this reads exactly that — it does not look at prose and
 * decide what somebody probably meant. A tool that invents work items out of
 * sentences is wrong often enough that people stop trusting the ones it gets
 * right; a tool that reads the boxes you ticked is right every time.
 */

export interface NoteCheckbox {
    /** The text after the box, with markdown links flattened to their label. */
    text: string;
    /** Already ticked. Offered greyed out rather than hidden, so the list of
     *  what came out of a meeting stays complete. */
    done: boolean;
    /** 0-based line in the note, so the same box is the same box. */
    line: number;
}

/** `- [ ] text`, `* [x] text`, `1. [ ] text`, with any indentation. */
const BOX = /^\s*(?:[-*+]|\d+[.)])\s+\[([ xX])\]\s+(.*)$/;

/** `[label](target)` → `label`, so a task is not named after a URL. */
const flattenLinks = (text: string): string =>
    text.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').trim();

export const readCheckboxes = (markdown: string): NoteCheckbox[] => {
    if (!markdown) return [];
    const out: NoteCheckbox[] = [];
    let inFence = false;

    markdown.split('\n').forEach((raw, line) => {
        // A checkbox inside a code fence is an example of a checkbox, not one.
        if (/^\s*(```|~~~)/.test(raw)) {
            inFence = !inFence;
            return;
        }
        if (inFence) return;

        const match = BOX.exec(raw);
        if (!match) return;
        const text = flattenLinks(match[2]);
        if (!text) return;
        out.push({ text, done: match[1].toLowerCase() === 'x', line });
    });
    return out;
};

/** The ones worth offering to turn into tasks. */
export const openCheckboxes = (markdown: string): NoteCheckbox[] =>
    readCheckboxes(markdown).filter(b => !b.done);
