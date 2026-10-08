/**
 * The little of Node the source-reading specs use.
 *
 * Vitest runs on Node, but this tsconfig describes a WebView app and carries
 * no Node types — nor should it, or `process` and `Buffer` would type-check in
 * code that will never have them. So the specs that read the tree from disk
 * (designFloor, ipcContract) get exactly these signatures and nothing else.
 */
declare module 'node:fs' {
  export function readFileSync(path: string, encoding: 'utf8'): string;
  export function readdirSync(
    path: string,
    options: { withFileTypes: true },
  ): Array<{ name: string; isDirectory(): boolean }>;
}

declare module 'node:path' {
  export function join(...parts: string[]): string;
  export function relative(from: string, to: string): string;
  export function resolve(...parts: string[]): string;
}
