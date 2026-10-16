import { describe, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, relative } from 'node:path';
import { mountNoteEditor } from './noteEditor';
import { INVARIANTS } from '../shared/invariants';
import {
  auditNote,
  bodyOf,
  claimWithoutTools,
  compareManifests,
  noteFails,
  readLog,
  type ClaimFinding,
  type Manifest,
  type NoteFinding,
} from './checks';

/**
 * Every invariant in `contracts/invariants.json`, over a vault as it is on
 * disk. Reads only; writes a report and a manifest beside the run.
 *
 * ```bash
 * SYNABIT_AUDIT_VAULT=~/Documents/vault npm run audit
 * ```
 *
 * - `SYNABIT_AUDIT_LOGS` — the app's log folder (default: macOS's).
 * - `SYNABIT_AUDIT_DAYS` — how far back the log is read (default 7).
 * - `SYNABIT_AUDIT_OUT` — where the report goes (default `audit-reports/`).
 * - `SYNABIT_AUDIT_COMPARE` — another device's `manifest.json`, for
 *   `sync.converges`: run the audit there too and pass its manifest here.
 *
 * Without `SYNABIT_AUDIT_VAULT` it is skipped, so the ordinary test run does
 * not read anybody's notes.
 */

// As Tauri makes a file's address, so saving finds and undoes it (`useAssetPaths`).
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async () => null), convertFileSrc: (p: string) => `asset://localhost/${encodeURIComponent(p)}` }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => {}), emit: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: vi.fn(async () => true), message: vi.fn(async () => undefined) }));
vi.mock('../utils/logger', () => ({ logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() } }));

/** The environment, read without giving the app's code a `process` to reach for. See `src/__tests__/node.d.ts`. */
const env = (globalThis as unknown as { process: { env: Record<string, string | undefined> } }).process.env;

const VAULT = env.SYNABIT_AUDIT_VAULT?.replace(/^~/, homedir());
const LOGS = env.SYNABIT_AUDIT_LOGS?.replace(/^~/, homedir()) ?? join(homedir(), 'Library/Logs/com.synabit.app');
const DAYS = Number(env.SYNABIT_AUDIT_DAYS ?? 7);
const OUT = env.SYNABIT_AUDIT_OUT ?? 'audit-reports';
const COMPARE = env.SYNABIT_AUDIT_COMPARE;

/** Every file under `dir`, hidden folders left out, as paths relative to the vault. */
function filesUnder(root: string, dir = root): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    if (entry.name.startsWith('.')) return [];
    const at = join(dir, entry.name);
    return entry.isDirectory() ? filesUnder(root, at) : [relative(root, at)];
  });
}

/** The names in a folder that end with `ext`. */
const named = (dir: string, ext: string) =>
  readdirSync(dir, { withFileTypes: true })
    .filter((e) => !e.isDirectory() && e.name.endsWith(ext))
    .map((e) => e.name);

const list = (items: string[], max = 15) =>
  items.slice(0, max).map((i) => `- ${i}`).join('\n') + (items.length > max ? `\n- … and ${items.length - max} more` : '');

describe.skipIf(!VAULT)('audit', () => {
  it('checks every invariant over the vault', async () => {
    const vault = VAULT!;
    const files = filesUnder(vault);
    const report: string[] = [`# Synabit audit — ${new Date().toISOString().slice(0, 16)}`, '', `Vault: \`${vault}\``, ''];
    let failed = 0;

    // ── note.roundtrip ──
    // One editor for every note, made again after a note that breaks it, so
    // one failure is one note's and not every note's after it.
    const make = () => mountNoteEditor(vault);
    let ed = await make();

    const notes = files.filter((f) => f.endsWith('.md') && !f.startsWith('Syn/') && !f.startsWith('SynConnectors/'));
    const broken: [string, NoteFinding][] = [];
    const unstable: string[] = [];
    const unopened: string[] = [];
    for (const path of notes) {
      try {
        const finding = auditNote(ed.editor, ed.open, bodyOf(readFileSync(join(vault, path), 'utf8')), ed.save);
        if (noteFails(finding)) broken.push([path, finding]);
        else if (finding.unstable) unstable.push(path);
      } catch (e) {
        unopened.push(`\`${path}\`: ${String(e).split('\n')[0].slice(0, 100)}`);
        try {
          ed.wrapper.unmount();
        } catch {
          // Already broken; a new one follows.
        }
        ed = await make();
      }
    }
    ed.wrapper.unmount();
    failed += broken.length;
    report.push(
      `## note.roundtrip — ${broken.length} of ${notes.length} notes lose something`,
      '',
      broken.length
        ? broken
            .map(([p, f]) =>
              `- \`${p}\`: ${f.lostOnOpen.length ? `opening drops ${f.lostOnOpen.length} address(es) (${f.lostOnOpen.slice(0, 2).join(', ')})` : ''}` +
              `${f.lostOnSave.words || f.lostOnSave.links ? ` saving loses ${f.lostOnSave.words} word(s), ${f.lostOnSave.links} link(s) from block ${f.lostOnSave.firstBlock}` : ''}`,
            )
            .join('\n')
        : 'None.',
      '',
      `Notes the audit could not open in its editor (the audit's limit, or a note that breaks the editor): ${unopened.length}.`,
      unopened.length ? list(unopened) : '',
      '',
      `Notes rewritten every time they are opened (saving does not settle): ${unstable.length}.`,
      unstable.length ? list(unstable.map((p) => `\`${p}\``)) : '',
      '',
    );

    // ── syn.claims-match-tools ──
    const runsDir = join(vault, 'Syn/runs');
    const claims: ClaimFinding[] = [];
    if (existsSync(runsDir)) {
      for (const name of named(runsDir, '.json')) {
        try {
          const found = claimWithoutTools(JSON.parse(readFileSync(join(runsDir, name), 'utf8')));
          if (found) claims.push(found);
        } catch {
          // A run file that does not read is not this rule's to judge.
        }
      }
    }
    claims.sort((a, b) => a.at.localeCompare(b.at));
    failed += claims.length;
    report.push(
      `## syn.claims-match-tools — ${claims.length} answers said they did something with nothing called`,
      '',
      claims.length ? list(claims.map((c) => `${c.at.slice(0, 16)} · ${c.surface} · ${c.model} · run \`${c.run}\``), 30) : 'None.',
      '',
    );

    // ── What the app wrote down while it ran ──
    const since = new Date(Date.now() - DAYS * 86_400_000).toISOString().slice(0, 10);
    const lines = existsSync(LOGS)
      ? named(LOGS, '.log').flatMap((n) => readFileSync(join(LOGS, n), 'utf8').split('\n'))
      : [];
    const log = readLog(lines, since);
    const byRule = new Map<string, typeof log.violations>();
    for (const v of log.violations) byRule.set(v.invariant, [...(byRule.get(v.invariant) ?? []), v]);
    failed += log.violations.length;
    report.push(`## While it ran — since ${since}`, '');
    for (const rule of INVARIANTS.filter((r) => r.checked.includes('runtime'))) {
      const seen = byRule.get(rule.id) ?? [];
      report.push(
        `### ${rule.id} — ${seen.length}`,
        `_${rule.rule}_`,
        '',
        seen.length ? list(seen.map((v) => `${v.at} · ${v.subject ?? ''} · ${JSON.stringify(v.detail ?? {})}`), 20) : 'None.',
        '',
      );
    }
    const uncaught = log.errors.filter((e) => e.uncaught);
    failed += uncaught.length;
    report.push(
      `### ui.no-uncaught-error — ${uncaught.reduce((n, e) => n + e.count, 0)}`,
      '',
      uncaught.length ? list(uncaught.map((e) => `×${e.count} (${e.first} → ${e.last}) ${e.shape}`)) : 'None.',
      '',
      `### Other errors in the log, most frequent first — ${log.errors.length - uncaught.length} kinds`,
      '',
      list(log.errors.filter((e) => !e.uncaught).map((e) => `×${e.count} (${e.first} → ${e.last}) ${e.shape}`)) || 'None.',
      '',
    );

    // ── sync.converges ──
    const manifest: Manifest = {};
    for (const path of files) manifest[path] = createHash('sha256').update(readFileSync(join(vault, path))).digest('hex');
    mkdirSync(OUT, { recursive: true });
    writeFileSync(join(OUT, 'manifest.json'), JSON.stringify(manifest, null, 1));
    if (COMPARE) {
      const d = compareManifests(manifest, JSON.parse(readFileSync(COMPARE.replace(/^~/, homedir()), 'utf8')));
      failed += d.differ.length + d.onlyHere.length + d.onlyThere.length;
      report.push(
        `## sync.converges — ${d.differ.length} differ, ${d.onlyHere.length} only here, ${d.onlyThere.length} only there`,
        '',
        d.differ.length ? `Different:\n${list(d.differ)}` : '',
        d.onlyHere.length ? `Only here:\n${list(d.onlyHere)}` : '',
        d.onlyThere.length ? `Only there:\n${list(d.onlyThere)}` : '',
        '',
      );
    } else {
      report.push('## sync.converges — not compared', '', `Run the audit on the other device and pass its \`manifest.json\` as \`SYNABIT_AUDIT_COMPARE\`. This one is at \`${join(OUT, 'manifest.json')}\`.`, '');
    }

    report.splice(4, 0, failed ? `**${failed} breach(es).**` : '**Nothing broke a rule.**', '');
    const text = report.join('\n');
    const file = join(OUT, `${new Date().toISOString().slice(0, 16).replace(':', '')}.md`);
    writeFileSync(file, text);
    console.log(`\n${text}\n\nWritten to ${file}`);
  }, 30 * 60_000);
});
