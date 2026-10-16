/**
 * The little of Node the source-reading specs use.
 *
 * Vitest runs on Node, but this tsconfig describes a WebView app and carries
 * no Node types — nor should it, or `process` and `Buffer` would type-check in
 * code that will never have them. So the specs that read the tree from disk
 * (designFloor, ipcContract, the invariants contract) get exactly these
 * signatures and nothing else. `npm run audit` (src/audit) reads a vault and
 * writes its report, which is what the rest are for.
 */
declare module 'node:fs' {
  export function readFileSync(path: string, encoding: 'utf8'): string;
  export function readFileSync(path: string): Uint8Array;
  export function readdirSync(
    path: string,
    options: { withFileTypes: true },
  ): Array<{ name: string; isDirectory(): boolean }>;
  export function existsSync(path: string): boolean;
  export function mkdirSync(path: string, options: { recursive: true }): void;
  export function writeFileSync(path: string, data: string): void;
}

declare module 'node:crypto' {
  export function createHash(algorithm: 'sha256'): {
    update(data: Uint8Array | string): { digest(encoding: 'hex'): string };
  };
}

declare module 'node:os' {
  export function homedir(): string;
}

declare module 'node:path' {
  export function join(...parts: string[]): string;
  export function relative(from: string, to: string): string;
  export function resolve(...parts: string[]): string;
}
