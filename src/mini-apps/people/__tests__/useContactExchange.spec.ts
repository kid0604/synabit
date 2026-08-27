import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useContactExchange, type ContactImport, type DuplicateReport } from '../composables/useContactExchange';
import * as core from '@tauri-apps/api/core';
import * as dialog from '@tauri-apps/plugin-dialog';
import * as os from '@tauri-apps/plugin-os';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn().mockResolvedValue(null),
  save: vi.fn().mockResolvedValue(null),
}));
vi.mock('@tauri-apps/plugin-os', () => ({ type: vi.fn().mockResolvedValue('macos') }));

const contact = (title: string, properties: Record<string, any> = {}, body = ''): ContactImport => ({
  title,
  properties,
  body,
});

/** A node service that records what it was asked to write. */
const fakeNodeService = (existing: Record<string, any> = {}) => {
  const writes: any[] = [];
  return {
    writes,
    ns: {
      writeNode: vi.fn(async (params: any) => { writes.push(params); }),
      getNode: vi.fn(async (id: string) => existing[id] ?? null),
    },
  };
};

const exchangeFor = (ns: any) => useContactExchange(ns, () => '/mock/vault');

describe('committing an import', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // `merge_contact` is the only IPC the commit path uses. The real merge is
    // tested in Rust; here it only has to be distinguishable.
    vi.mocked(core.invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === 'merge_contact') {
        return { ...(args.incoming ?? {}), merged: true };
      }
      return undefined;
    });
  });

  it('writes a new person for each contact with nothing to clash with', async () => {
    const { ns, writes } = fakeNodeService();
    const report = await exchangeFor(ns).commit(
      [contact('An Nguyễn', { email: 'an@example.com' }, 'notes'), contact('Bình Trần')],
      [],
      ['add', 'add']
    );

    expect(report).toEqual({ added: 2, merged: 0, skipped: 0, failed: [] });
    expect(writes).toHaveLength(2);
    expect(writes[0].nodeType).toBe('person');
    expect(writes[0].relPath).toMatch(/^People\/[0-9a-f-]{36}\.md$/);
    expect(writes[0].content).toBe('notes');
    // Two thousand of these must not fire two thousand refreshes.
    expect(writes[0].silent).toBe(true);
  });

  it('patches the person already there instead of adding a second', async () => {
    const { ns, writes } = fakeNodeService({
      'People/an.md': { id: 'People/an.md', title: 'An Nguyễn', properties: { nickname: 'Ann' } },
    });
    const duplicates: DuplicateReport[] = [{
      incoming: 0,
      existing_id: 'People/an.md',
      existing_title: 'An Nguyễn',
      existing_incoming: null,
      reason: { on: 'email', value: 'an@example.com' },
      certain: true,
    }];

    const report = await exchangeFor(ns).commit(
      [contact('An N.', { birthday: '1994-03-02' })],
      duplicates,
      ['merge']
    );

    expect(report.merged).toBe(1);
    expect(report.added).toBe(0);
    expect(writes).toHaveLength(1);
    expect(writes[0].relPath).toBe('People/an.md');
    // The person keeps the name they already had; an export is not a better
    // authority on what somebody is called than the vault is.
    expect(writes[0].title).toBe('An Nguyễn');
    expect(writes[0].properties.merged).toBe(true);
  });

  it('writes nothing when a merge turns out to have nothing to add', async () => {
    vi.mocked(core.invoke).mockResolvedValue({});
    const { ns, writes } = fakeNodeService({
      'People/an.md': { id: 'People/an.md', title: 'An', properties: {} },
    });

    const report = await exchangeFor(ns).commit(
      [contact('An')],
      [{
        incoming: 0,
        existing_id: 'People/an.md',
        existing_title: 'An',
        existing_incoming: null,
        reason: { on: 'email', value: 'an@example.com' },
        certain: true,
      }],
      ['merge']
    );

    // Counted as merged — the contact was handled — but an empty patch would
    // touch the file, and its modified time, for no reason.
    expect(report.merged).toBe(1);
    expect(writes).toHaveLength(0);
  });

  it('skips what somebody deselected', async () => {
    const { ns, writes } = fakeNodeService();
    const report = await exchangeFor(ns).commit(
      [contact('An'), contact('Bình')],
      [],
      ['skip', 'add']
    );

    expect(report).toMatchObject({ added: 1, skipped: 1 });
    expect(writes.map(w => w.title)).toEqual(['Bình']);
  });

  it('joins two rows of one file into one person', async () => {
    // A file listing somebody twice must contribute one person, not two —
    // and the second row's details have to survive into the first.
    const { ns, writes } = fakeNodeService();
    const duplicates: DuplicateReport[] = [{
      incoming: 2,
      existing_id: null,
      existing_title: null,
      existing_incoming: 0,
      reason: { on: 'email', value: 'an@example.com' },
      certain: true,
    }];

    const report = await exchangeFor(ns).commit(
      [
        contact('An Nguyễn', { email: 'an@example.com' }),
        contact('Bình Trần', { email: 'binh@example.com' }),
        contact('An N.', { phone: '+84 90 123 4567' }, 'a note the first row lacked'),
      ],
      duplicates,
      ['add', 'add', 'add']
    );

    expect(report.added).toBe(2);
    expect(report.skipped).toBe(1);
    expect(writes.map(w => w.title)).toEqual(['An Nguyễn', 'Bình Trần']);
    // The repeated row was folded in rather than dropped.
    expect(writes[0].properties.merged).toBe(true);
    expect(writes[0].content).toBe('a note the first row lacked');
  });

  it('carries on past a contact that fails to write', async () => {
    const { ns, writes } = fakeNodeService();
    ns.writeNode = vi.fn(async (params: any) => {
      if (params.title === 'Bình') throw new Error('disk full');
      writes.push(params);
    });

    const report = await exchangeFor(ns).commit(
      [contact('An'), contact('Bình'), contact('Cường')],
      [],
      ['add', 'add', 'add']
    );

    // One bad row does not cost somebody the other nineteen hundred.
    expect(report.added).toBe(2);
    expect(report.failed).toEqual([{ title: 'Bình', error: 'Error: disk full' }]);
    expect(writes.map(w => w.title)).toEqual(['An', 'Cường']);
  });

  it('reports how far along it is, and stops reporting when it is done', async () => {
    const { ns } = fakeNodeService();
    const exchange = exchangeFor(ns);
    const seen: Array<number | null> = [];

    ns.writeNode = vi.fn(async () => {
      seen.push(exchange.progress.value?.done ?? null);
    });

    await exchange.commit([contact('An'), contact('Bình'), contact('Cường')], [], ['add', 'add', 'add']);

    expect(seen).toEqual([0, 1, 2]);
    expect(exchange.progress.value).toBeNull();
  });

  it('leaves the original list untouched', async () => {
    // The modal still shows these after the import; mutating them in place
    // would rewrite the summary the user is reading.
    const { ns } = fakeNodeService();
    const original = [contact('An', { email: 'an@example.com' })];
    const before = JSON.stringify(original);

    await exchangeFor(ns).commit(original, [], ['add']);
    expect(JSON.stringify(original)).toBe(before);
  });
});

describe('picking a file', () => {
  beforeEach(() => vi.clearAllMocks());

  it('asks for extensions on a desktop', async () => {
    const { ns } = fakeNodeService();
    await exchangeFor(ns).pickFile();

    const [args] = vi.mocked(dialog.open).mock.calls[0] as any[];
    const everything = args.filters.flatMap((f: any) => f.extensions);
    expect(everything).toContain('vcf');
    expect(everything.some((e: string) => e.includes('/'))).toBe(false);
  });

  it('asks for MIME types on a phone, under both names vCard goes by', async () => {
    // Android's picker matches what the provider reports. A `.vcf` shared out
    // of Google Contacts says `text/vcard`; Android's own extension table
    // answers `text/x-vcard`. Name only one and the file cannot be picked.
    vi.mocked(os.type).mockResolvedValue('android' as any);
    const { ns } = fakeNodeService();
    await exchangeFor(ns).pickFile();

    const [args] = vi.mocked(dialog.open).mock.calls[0] as any[];
    const everything = args.filters.flatMap((f: any) => f.extensions);
    expect(everything).toContain('text/vcard');
    expect(everything).toContain('text/x-vcard');
    expect(everything).toContain('text/csv');
    // The plugin passes anything with a slash through as a MIME type; a bare
    // extension would be looked up and dropped.
    expect(everything.every((e: string) => e.includes('/'))).toBe(true);
  });

  it('returns null when the dialog is closed', async () => {
    const { ns } = fakeNodeService();
    expect(await exchangeFor(ns).pickFile()).toBeNull();
  });
});

describe('exporting', () => {
  beforeEach(() => vi.clearAllMocks());

  it('does not treat a closed dialog as a failure', async () => {
    const { ns } = fakeNodeService();
    const exchange = exchangeFor(ns);

    await expect(exchange.exportContacts('vcard')).resolves.toBeNull();
    expect(core.invoke).not.toHaveBeenCalled();
    // And the button is usable again afterwards.
    expect(exchange.busy.value).toBe(false);
  });
});
