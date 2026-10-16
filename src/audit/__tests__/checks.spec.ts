import { describe, it, expect, afterEach, vi } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import Link from '@tiptap/extension-link';
import { Markdown } from 'tiptap-markdown';

vi.mock('../../mini-apps/note/nodes/DetailsNodeView.vue', () => ({ default: {} }));
vi.mock('@tiptap/vue-3', async (real) => ({ ...(await real<object>()), VueNodeViewRenderer: () => null }));

import { DetailsExtension } from '../../mini-apps/note/extensions/DetailsExtension';
import { HtmlBlockBreaks } from '../../mini-apps/note/extensions/HtmlBlockBreaks';
import {
  addressesIn,
  auditNote,
  bodyOf,
  claimWithoutTools,
  compareManifests,
  noteFails,
  readLog,
  shapeOf,
} from '../checks';

const LINK = 'https://chat.google.com/room/AAQAv1dGQyM/qbDhtbXAlT0/qbDhtbXAlT0?cls=10';
/** The note as it was on disk on 2026-10-15. */
const OLD_NOTE = `---\ntitle: 2026-10-15\n---\n<details class="synabit-details">\n<summary>Toggle</summary>\n<div class="details-content">\n\nVợ đi du lịch Phú Quốc\n\n</div>\n</details>\nThread về việc rà soát domain, TLS cert: <${LINK}>\n`;

let editors: Editor[] = [];
afterEach(() => {
  editors.forEach((e) => e.destroy());
  editors = [];
});

const editorWith = (fixed: boolean) => {
  const e = new Editor({
    element: document.createElement('div'),
    extensions: [StarterKit, Link, DetailsExtension, ...(fixed ? [HtmlBlockBreaks] : []), Markdown.configure({ html: true })],
  });
  editors.push(e);
  const open = (md: string) => e.commands.setContent(md);
  return { e, open };
};

describe('note.roundtrip over a file', () => {
  it('finds the address a note on disk loses when opened', () => {
    const { e, open } = editorWith(false);
    const f = auditNote(e, open, bodyOf(OLD_NOTE));
    expect(f.lostOnOpen).toEqual([LINK]);
    expect(noteFails(f)).toBe(true);
  });

  it('passes the same note once it opens whole', () => {
    const { e, open } = editorWith(true);
    const f = auditNote(e, open, bodyOf(OLD_NOTE));
    expect(f.lostOnOpen).toEqual([]);
    expect(noteFails(f)).toBe(false);
  });

  it('reads addresses as a reader would', () => {
    expect(addressesIn('see <https://a.b/c?x=1&amp;y=2>, and https://d.e/f.')).toEqual(['https://a.b/c?x=1&y=2', 'https://d.e/f']);
    expect(addressesIn('[note](synabit://note/abc)')).toEqual(['synabit://note/abc']);
    expect(addressesIn('[x](https://a.sharepoint.com/\\_layouts/Doc.aspx?file=\\[062026\\])')).toEqual(['https://a.sharepoint.com/_layouts/Doc.aspx?file=[062026]']);
    expect(addressesIn('[Cam](synabit://person/People%2Fx.md)')).toEqual(['synabit://person/People/x.md']);
  });
});

describe('syn.claims-match-tools over run files', () => {
  const run = (answer: string, steps: object[] = []) => ({
    id: 'r1',
    created_at: '2026-10-14T13:22:12Z',
    surface: 'telegram',
    model: 'gpt-5.6-luna',
    steps: [...steps, { kind: 'assistant', preview: answer }],
  });

  it('finds an answer that said it saved with nothing called', () => {
    expect(claimWithoutTools(run('Đã lưu vào QuickCap.'))).toMatchObject({ run: 'r1', surface: 'telegram' });
  });

  it('passes one that called a tool, or claimed nothing', () => {
    expect(claimWithoutTools(run('Đã lưu vào QuickCap.', [{ kind: 'tool_call', ok: true }]))).toBeNull();
    expect(claimWithoutTools(run('Pi đang 57,5°C.'))).toBeNull();
    expect(claimWithoutTools(run('```mermaid\nflowchart LR\n```\n\nĐã cập nhật: các item dạng chấm tròn'))).toBeNull();
  });

  it('counts a call that failed as nothing called', () => {
    expect(claimWithoutTools(run('Đã lưu vào QuickCap.', [{ kind: 'tool_call', ok: false }]))).not.toBeNull();
  });
});

describe('the log', () => {
  const lines = [
    '[2026-10-15][15:13:47][webview:x][WARN] [invariant] {"invariant":"ui.responsive","subject":"/syn","detail":{"blocked_ms":11462}}',
    '[2026-10-15][17:08:25][synabit_lib::error][ERROR] Backend Error [GENERAL_ERROR]: `syn_pane_room` cannot be called from this window',
    '[2026-10-15][17:08:26][synabit_lib::error][ERROR] Backend Error [GENERAL_ERROR]: `syn_pane_room` cannot be called from this window',
    '[2026-10-15][17:09:00][webview:y][ERROR] [main] Uncaught (promise): TypeError: x is undefined',
    '[2026-10-01][00:00:00][webview:y][ERROR] older than the window',
    'not a log line',
  ];

  it('reads breaches written while the app ran, and groups the errors', () => {
    const found = readLog(lines, '2026-10-10');
    expect(found.violations).toEqual([
      { invariant: 'ui.responsive', subject: '/syn', detail: { blocked_ms: 11462 }, at: '2026-10-15 15:13:47' },
    ]);
    expect(found.errors.map((e) => [e.count, e.uncaught])).toEqual([[1, true], [2, false]]);
    const later = readLog([lines[2], lines[1]], '2026-10-10').errors[0];
    expect([later.first, later.last], 'earliest and latest, whatever order the files were read in').toEqual(['2026-10-15 17:08:25', '2026-10-15 17:08:26']);
  });

  it('takes ids and numbers out of an error so its repeats group', () => {
    expect(shapeOf('run 3c77fd35-1bb4-4f6e-9d3b-1a2b3c4d5e6f failed after 42 ms')).toBe('run <id> failed after # ms');
  });
});

describe('sync.converges', () => {
  it('names what differs between two devices', () => {
    expect(compareManifests({ a: '1', b: '2', c: '3' }, { a: '1', b: 'x', d: '4' })).toEqual({
      differ: ['b'],
      onlyHere: ['c'],
      onlyThere: ['d'],
    });
  });
});
