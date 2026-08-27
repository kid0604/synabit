import { describe, it, expect } from 'vitest';
import { canThumbnail, thumbKeyFor } from '../composables/useFileThumbnails';

/**
 * The key is what makes a thumbnail go stale at the right moment, and it has
 * to survive a round trip through `save_thumbnail`, which rejects anything
 * carrying a path separator and keeps only the stem.
 *
 * Getting this wrong is cheap in one direction and expensive in the other. A
 * key that changes too often costs a rebuild; a key that changes too rarely
 * shows the user last week's picture and nothing in the app will ever correct
 * it. The cases below are written from that asymmetry.
 */
describe('thumbKeyFor', () => {
  const digest = 'a'.repeat(64);

  it('is the digest of what is inside the file', () => {
    expect(thumbKeyFor({ id: `Files/${digest}.md` })).toBe(`${digest}.webp`);
  });

  it('changes when the picture does, because the identity does', () => {
    const before = thumbKeyFor({ id: `Files/${'a'.repeat(64)}.md` });
    const after = thumbKeyFor({ id: `Files/${'b'.repeat(64)}.md` });
    expect(before).not.toBe(after);
  });

  it('gives two copies of one photo the same thumbnail', () => {
    // Copies share a node id, because they share their contents.
    const id = `Files/${digest}.md`;
    expect(thumbKeyFor({ id })).toBe(thumbKeyFor({ id }));
  });

  it('is a plain filename — `save_thumbnail` refuses anything else', () => {
    const key = thumbKeyFor({ id: 'Files/../../escape.md' });
    expect(key).not.toContain('/');
    expect(key).not.toContain('\\');
    expect(key).not.toContain('..');
    expect(key.endsWith('.webp')).toBe(true);
  });
});

/**
 * The shrink happens on a `<canvas>`, so the list is bounded by what a webview
 * can decode — not by what the Files app is willing to show an icon for.
 */
describe('canThumbnail', () => {
  it('covers the raster formats the webview decodes', () => {
    for (const ext of ['jpg', 'JPG', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'avif']) {
      expect(canThumbnail(ext)).toBe(true);
    }
  });

  it('leaves alone what a canvas cannot help with', () => {
    // SVG is already small and scales by itself; the rest have no decoder here.
    for (const ext of ['svg', 'pdf', 'mp4', 'heic', 'tiff', 'txt', '']) {
      expect(canThumbnail(ext)).toBe(false);
    }
  });
});
