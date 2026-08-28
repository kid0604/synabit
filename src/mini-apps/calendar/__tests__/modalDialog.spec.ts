import { describe, it, expect } from 'vitest';
// The component's own source, as text. Imported rather than read off disk:
// `vite/client` declares `*?raw`, so this needs no Node types in a config
// that has none.
import source from '../components/ModalDialog.vue?raw';

/**
 * The `class="…"` on the `<dialog>` tag itself, not on anything inside it.
 *
 * `\s` rather than `\b` after the name: the file talks about `<dialog>` in a
 * comment, and a word boundary matches that too.
 */
const dialogClasses = (): string => {
    const tag = /<dialog\s[\s\S]*?>/.exec(source)?.[0] ?? '';
    return /class="([\s\S]*?)"/.exec(tag)?.[1] ?? '';
};

describe('the dialog element', () => {
    it('is in the file at all', () => {
        expect(source).toContain('<dialog');
        expect(dialogClasses()).not.toBe('');
    });

    /**
     * Any of these sets `display`, and setting `display` on the element is
     * what un-hides a closed dialog. The layout belongs on the sheet inside.
     */
    it('carries no class that would set its display', () => {
        const forbidden = [
            'flex', 'inline-flex', 'grid', 'inline-grid',
            'block', 'inline-block', 'inline', 'contents', 'table', 'hidden',
        ];
        const classes = dialogClasses().split(/\s+/).filter(Boolean);
        const offending = classes.filter(c => forbidden.includes(c));
        expect(offending, `display utilities on <dialog>: ${offending.join(', ')}`).toEqual([]);
    });

    /** Said out loud, so no future utility can quietly undo it. */
    it('is told in the stylesheet to stay hidden while closed', () => {
        expect(source).toMatch(/dialog:not\(\[open\]\)\s*\{[^}]*display:\s*none/);
    });

    /** Without this the browser does not make the rest of the page inert. */
    it('is opened as a modal rather than merely shown', () => {
        expect(source).toContain('showModal()');
        expect(source).not.toMatch(/\.show\(\)/);
    });
});
