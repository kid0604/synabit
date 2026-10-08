import { describe, it, expect } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

// Vitest's own; see ./node.d.ts for why Node's types are not loaded.
declare const __dirname: string;

/**
 * The IPC contract between the front end and the Rust side.
 *
 * `invoke('name')` is a string. Nothing checks it against the commands Rust
 * registers, so a call to a command that was never written — or was renamed —
 * compiles, lints, ships, and fails only when somebody presses the button.
 * Settings → Devices did exactly that for seven `p2p_*` commands, and the
 * screen showed "Command p2p_pair_initiate not found".
 *
 * This reads `tauri::generate_handler![…]` in src-tauri/src/lib.rs and every
 * literal `invoke('…')` under src/, and lists any name the backend does not
 * have. A call whose name is a variable is not seen — keep command names
 * literal at the call site so it is.
 *
 * Events get the same, weaker check: a name passed to `listen('…')` must
 * appear as a string somewhere in the Rust source, or be sent by the front end
 * itself with `emit` / `emitTo`. Weaker because an event name can be built at
 * runtime; but a name that appears nowhere at all is certainly one nothing
 * sends — `sync-progress`, `sync-conflict` and `quickcap:compose` were all
 * listened for long after the code that sent them was gone.
 */

const ROOT = resolve(__dirname, '../..');
const SRC = join(ROOT, 'src');
const RUST = join(ROOT, 'src-tauri/src');

function walk(dir: string, accept: (file: string) => boolean): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === 'node_modules' || entry.name === '__tests__') continue;
      out.push(...walk(path, accept));
    } else if (accept(path)) out.push(path);
  }
  return out;
}

const isTest = (file: string) => /\.(spec|test)\.[cm]?[jt]sx?$/.test(file);

/** The commands the Rust side registers: the last path segment of each entry. */
function registeredCommands(libRs: string): Set<string> {
  const start = libRs.indexOf('generate_handler![');
  if (start < 0) throw new Error('generate_handler! not found in lib.rs');
  // Comments and `#[cfg(…)]` attributes out first: the attribute's own `]`
  // would otherwise end the list early.
  const rest = libRs
    .slice(start + 'generate_handler!['.length)
    .replace(/\/\/[^\n]*/g, '')
    .replace(/#\[[^\]]*\]/g, '');
  const body = rest.slice(0, rest.indexOf(']'));
  const names = new Set<string>();
  for (const entry of body.split(',')) {
    const path = entry.trim();
    if (!path) continue;
    names.add(path.split('::').pop()!.trim());
  }
  return names;
}

/**
 * Every literal first argument to a call of `callee`, with the line it is on.
 *
 * A type argument is skipped by counting angle brackets — `invoke<Array<{ a:
 * string }>>(` is ordinary here — with `=>` set aside so a function type does
 * not close one early.
 */
function literalCalls(src: string, callee: string): Array<{ name: string; line: number }> {
  const found: Array<{ name: string; line: number }> = [];
  const head = new RegExp(`(?<![\\w$.])${callee}\\b`, 'g');
  for (const m of src.matchAll(head)) {
    let i = m.index! + m[0].length;
    while (/\s/.test(src[i] ?? '')) i++;
    if (src[i] === '<') {
      let depth = 0;
      for (; i < src.length; i++) {
        if (src[i] === '=' && src[i + 1] === '>') { i++; continue; }
        if (src[i] === '<') depth++;
        else if (src[i] === '>' && --depth === 0) { i++; break; }
      }
      while (/\s/.test(src[i] ?? '')) i++;
    }
    if (src[i] !== '(') continue;
    i++;
    while (/\s/.test(src[i] ?? '')) i++;
    const quote = src[i];
    if (quote !== '\'' && quote !== '"' && quote !== '`') continue;
    const close = src.indexOf(quote, i + 1);
    const name = src.slice(i + 1, close);
    if (quote === '`' && name.includes('${')) continue;
    found.push({ name, line: src.slice(0, m.index!).split('\n').length });
  }
  return found;
}

const frontEnd = walk(SRC, (f) => /\.(ts|vue)$/.test(f) && !isTest(f)).map((file) => ({
  file: relative(ROOT, file),
  src: readFileSync(file, 'utf8'),
}));

describe('IPC contract', () => {
  it('reads what it is looking for', () => {
    expect([...registeredCommands(
      'tauri::generate_handler![\n  // Notes\n  nodes::get_node,\n  #[cfg(desktop)]\n  a::b::quit_now,\n]',
    )]).toEqual(['get_node', 'quit_now']);
    expect(literalCalls(
      "invoke('a'); invoke<Array<{ x: () => void }>>(\n  \"b\", {}); invoke(cmd); safeInvoke('c'); invoke(`d`);",
      'invoke',
    ).map((c) => c.name)).toEqual(['a', 'b', 'd']);
  });

  it('invokes only commands the backend registers', () => {
    const registered = registeredCommands(readFileSync(join(RUST, 'lib.rs'), 'utf8'));
    // Sanity: an empty set would pass everything and mean the parse broke.
    expect(registered.size).toBeGreaterThan(100);
    expect(registered.has('sync_full')).toBe(true);

    const missing: string[] = [];
    let seen = 0;
    for (const { file, src } of frontEnd) {
      for (const { name, line } of literalCalls(src, 'invoke')) {
        seen++;
        // Plugin commands are registered by the plugin, not by lib.rs.
        if (name.startsWith('plugin:')) continue;
        if (!registered.has(name)) missing.push(`${file}:${line}  ${name}`);
      }
    }
    // And the other side of the same sanity check: the front end is read at all.
    expect(seen).toBeGreaterThan(200);
    expect(missing, `Invoked but not registered in generate_handler!:\n${missing.join('\n')}`).toEqual([]);
  });

  it('listens only for events something sends', () => {
    const rust = walk(RUST, (f) => f.endsWith('.rs')).map((f) => readFileSync(f, 'utf8')).join('\n');
    // Sent by the front end itself, from one window to another: `emit('name')`,
    // `win.emit('name')`, `emitTo(target, 'name')`.
    const sentHere = new Set<string>();
    for (const { src } of frontEnd) {
      for (const m of src.matchAll(/\b(?:emit|tauriEmit)\s*(?:<[^>()]*>)?\(\s*['"`]([^'"`$]+)['"`]/g)) sentHere.add(m[1]);
      for (const m of src.matchAll(/\bemitTo\s*(?:<[^>()]*>)?\([^,]+,\s*['"`]([^'"`$]+)['"`]/g)) sentHere.add(m[1]);
    }

    const unsent: string[] = [];
    for (const { file, src } of frontEnd) {
      for (const { name, line } of [...literalCalls(src, 'listen'), ...literalCalls(src, 'once')]) {
        // Tauri's own: window close, drag-drop and the like.
        if (name.startsWith('tauri://')) continue;
        if (!rust.includes(`"${name}"`) && !sentHere.has(name)) unsent.push(`${file}:${line}  ${name}`);
      }
    }
    expect(unsent, `Listened for, but nothing sends it:\n${unsent.join('\n')}`).toEqual([]);
  });
});
