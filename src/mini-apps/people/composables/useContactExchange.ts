import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { logger } from '../../../utils/logger';

/**
 * Bringing an address book in, and taking one out.
 *
 * Reading and writing the formats themselves is in Rust — one place, with the
 * three vCard versions and the quoted-printable that older phones still send.
 * What is here is the part that has to be here: the file dialogs, and writing
 * each contact through the same `writeNode` everything else in this app uses,
 * so somebody who arrived in a file is indexed, synced and linked exactly like
 * somebody typed in by hand. The calendar's `.ics` exchange is split the same
 * way, for the same reason.
 */

export interface ContactImport {
    title: string;
    properties: Record<string, any>;
    body: string;
}

export interface ContactBatch {
    format: 'vcard' | 'csv';
    contacts: ContactImport[];
    /** Rows that named nobody; there was nothing to call them. */
    skipped: number;
    /** Headers nothing recognised, for the mapping step. */
    unmapped: string[];
}

export type DuplicateReason =
    | { on: 'email'; value: string }
    | { on: 'phone'; value: string }
    | { on: 'name'; value: string };

export interface DuplicateReport {
    incoming: number;
    existing_id: string | null;
    existing_title: string | null;
    /** Set when the match is against an earlier row of the same file. */
    existing_incoming: number | null;
    reason: DuplicateReason;
    /** A shared address or number. A shared name is not certain. */
    certain: boolean;
}

/** What to do with one incoming contact. */
export type Decision = 'add' | 'merge' | 'skip';

export interface ImportReport {
    added: number;
    merged: number;
    skipped: number;
    /** Contacts that could not be written, with the reason. */
    failed: Array<{ title: string; error: string }>;
}

export interface ImportProgress {
    done: number;
    total: number;
}

/** Whether this is a phone, for the one place the answer changes behaviour. */
async function isMobile(): Promise<boolean> {
    try {
        const { type } = await import('@tauri-apps/plugin-os');
        return ['android', 'ios'].includes((await type()).toLowerCase());
    } catch {
        // Not running under Tauri, or the plugin is unavailable. The desktop
        // filters are the safe answer: worst case somebody sees more files.
        return false;
    }
}

export function useContactExchange(ns: any, vaultPath: () => string) {
    const busy = ref(false);
    const progress = ref<ImportProgress | null>(null);

    /**
     * Ask for a file. Returns null when the dialog is closed.
     *
     * The filters differ by platform because the dialogs do. A desktop picker
     * matches on the file's extension; Android's matches on the MIME type its
     * provider reports, and anything not on the list is greyed out. A `.vcf`
     * shared out of Google Contacts arrives as `text/vcard`, while Android's
     * own extension table answers `text/x-vcard` — name only one and the file
     * somebody came to import cannot be picked.
     *
     * The plugin passes anything containing a slash through as a MIME type,
     * which is what makes saying both possible.
     */
    const pickFile = async (): Promise<string | null> => {
        const onPhone = await isMobile();
        const filters = onPhone
            ? [{
                name: 'Contacts',
                extensions: [
                    'text/vcard', 'text/x-vcard', 'text/directory',
                    'text/csv', 'text/comma-separated-values',
                    'text/tab-separated-values', 'text/plain',
                    // Some file managers report a download with no type at all.
                    'application/octet-stream',
                ],
            }]
            : [
                { name: 'Contacts', extensions: ['vcf', 'vcard', 'csv', 'tsv', 'txt'] },
                { name: 'vCard', extensions: ['vcf', 'vcard'] },
                { name: 'Spreadsheet', extensions: ['csv', 'tsv'] },
            ];

        const picked = await open({ multiple: false, filters });
        return typeof picked === 'string' ? picked : null;
    };

    /** Read a file. `columns` re-reads a spreadsheet with a hand-made mapping. */
    const readContacts = async (source: string, columns?: any[]): Promise<ContactBatch> =>
        await invoke<ContactBatch>('read_contacts', {
            vaultPath: vaultPath(),
            source,
            columns: columns ?? null,
        });

    /** The columns of a spreadsheet, for the screen that maps them. */
    const readColumns = async (source: string) =>
        await invoke<{
            headers: string[];
            columns: any[];
            sample: string[][];
            total_rows: number;
        }>('read_contact_columns', { source });

    const findDuplicates = async (contacts: ContactImport[]): Promise<DuplicateReport[]> =>
        await invoke<DuplicateReport[]>('find_contact_duplicates', { contacts });

    /**
     * Write the contacts somebody decided to keep.
     *
     * Two rows of one file that turn out to be the same person are folded
     * together before anything is written, rather than written twice and
     * cleaned up after — the second write would be a second person.
     */
    const commit = async (
        contacts: ContactImport[],
        duplicates: DuplicateReport[],
        decisions: Decision[]
    ): Promise<ImportReport> => {
        const report: ImportReport = { added: 0, merged: 0, skipped: 0, failed: [] };
        const byIncoming = new Map(duplicates.map(d => [d.incoming, d]));

        // Fold within-file duplicates into the row they matched, so the file
        // contributes one person rather than two.
        const folded = contacts.map(c => ({ ...c, properties: { ...c.properties } }));
        for (let i = 0; i < folded.length; i++) {
            const duplicate = byIncoming.get(i);
            const into = duplicate?.existing_incoming;
            if (into === null || into === undefined || decisions[i] === 'skip') continue;
            try {
                const patch = await invoke<Record<string, any>>('merge_contact', {
                    existing: folded[into].properties,
                    incoming: folded[i].properties,
                });
                folded[into].properties = { ...folded[into].properties, ...patch };
                if (!folded[into].body && folded[i].body) folded[into].body = folded[i].body;
            } catch (e) {
                logger.error('Failed to fold a repeated row', e);
            }
            decisions[i] = 'skip';
        }

        // Counted once, here, rather than as each kind of skip is decided:
        // a contact folded into an earlier row also has its decision set to
        // `skip`, and counting at both places reported it twice while a
        // contact somebody simply deselected was not counted at all.
        report.skipped = decisions.filter(d => d === 'skip').length;

        const writable = folded
            .map((contact, i) => ({ contact, i }))
            .filter(({ i }) => decisions[i] !== 'skip');
        progress.value = { done: 0, total: writable.length };

        for (const { contact, i } of writable) {
            const duplicate = byIncoming.get(i);
            try {
                if (decisions[i] === 'merge' && duplicate?.existing_id) {
                    const existing = await ns.getNode(duplicate.existing_id);
                    const patch = await invoke<Record<string, any>>('merge_contact', {
                        existing: existing?.properties ?? {},
                        incoming: contact.properties,
                    });
                    if (Object.keys(patch).length > 0) {
                        await ns.writeNode({
                            relPath: duplicate.existing_id,
                            title: existing?.title || contact.title,
                            nodeType: 'person',
                            properties: patch,
                            silent: true,
                        });
                    }
                    report.merged++;
                } else {
                    await ns.writeNode({
                        relPath: `People/${crypto.randomUUID()}.md`,
                        title: contact.title,
                        nodeType: 'person',
                        properties: contact.properties,
                        content: contact.body,
                        eventType: 'created',
                        silent: true,
                    });
                    report.added++;
                }
            } catch (e) {
                logger.error(`Failed to import ${contact.title}`, e);
                report.failed.push({ title: contact.title, error: String(e) });
            }
            progress.value = { done: progress.value.done + 1, total: writable.length };
        }

        progress.value = null;
        return report;
    };

    /**
     * Write every contact to a file.
     *
     * Returns null when the dialog is closed — a cancelled save is not a
     * failure, and reporting it as one is how people learn to ignore errors.
     */
    const exportContacts = async (format: 'vcard' | 'csv'): Promise<number | null> => {
        busy.value = true;
        try {
            // Built from the local date rather than `toISOString()`: before
            // seven in the morning in Hanoi that still reads as yesterday.
            const now = new Date();
            const stamp = [
                now.getFullYear(),
                String(now.getMonth() + 1).padStart(2, '0'),
                String(now.getDate()).padStart(2, '0'),
            ].join('-');
            const extension = format === 'vcard' ? 'vcf' : 'csv';
            const destination = await save({
                defaultPath: `synabit-contacts-${stamp}.${extension}`,
                filters: [
                    format === 'vcard'
                        ? { name: 'vCard', extensions: ['vcf'] }
                        : { name: 'Spreadsheet', extensions: ['csv'] },
                ],
            });
            if (!destination) return null;
            return await invoke<number>('export_contacts', {
                vaultPath: vaultPath(),
                destination,
                format,
            });
        } finally {
            busy.value = false;
        }
    };

    return {
        busy,
        progress,
        pickFile,
        readContacts,
        readColumns,
        findDuplicates,
        commit,
        exportContacts,
    };
}
