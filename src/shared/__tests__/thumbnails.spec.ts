import { describe, it, expect } from 'vitest';
import { thumbnailNameFor } from '../thumbnails';

/**
 * This rule has a twin: `thumb_name` in
 * `src-tauri/src/commands/thumbnails.rs` derives the same filename when it
 * stores the thumbnail, and this one derives it again when the card decides
 * what to load. If the two ever disagree, the card looks for a file that was
 * saved under another name.
 *
 * They are not held to a shared contract the way the tag grammar is, and
 * deliberately so: disagreement here costs a slower paint, because the lookup
 * simply misses and the card falls back to the original image. The tag
 * grammar earns a contract because disagreement there corrupts what is
 * stored. These cases mirror the Rust ones instead — cheaper, and matched to
 * what going wrong actually costs.
 */
describe('thumbnailNameFor', () => {
  it('keeps the asset stem and swaps the extension', () => {
    expect(thumbnailNameFor('abc123.png')).toBe('abc123.webp');
    expect(thumbnailNameFor('abc123.jpeg')).toBe('abc123.webp');
  });

  it('handles a name with no extension', () => {
    expect(thumbnailNameFor('abc123')).toBe('abc123.webp');
  });

  it('only replaces the last extension', () => {
    expect(thumbnailNameFor('archive.tar.gz')).toBe('archive.tar.webp');
  });

  it('agrees with the Rust side on a content-addressed name', () => {
    // What `utils::asset_naming::content_name` produces: 32 hex characters.
    expect(thumbnailNameFor('a3f9c2d1e4b78095a3f9c2d1e4b78095.jpg')).toBe(
      'a3f9c2d1e4b78095a3f9c2d1e4b78095.webp',
    );
  });
});
