import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { shrink } from '../../../shared/thumbnails';
import { readPhotoFacts } from '../../../shared/exif';
import { readFile } from '@tauri-apps/plugin-fs';
import { logger } from '../../../utils/logger';
import type { FileMetadata } from './useFileStore';

/**
 * Small copies of indexed images, so the grid stops decoding originals.
 *
 * The grid shows a picture in a cell about 180px tall and was pointing an
 * `<img>` at the file itself. A 12-megapixel photo costs roughly 48MB of
 * decoded pixels to paint at that size, and a screenful of them costs that
 * several times over — `loading="lazy"` defers the work but does not make it
 * smaller. QuickCap solved this already; this is the same pipeline pointed at
 * a different set of files.
 *
 * # Why the key is the content digest
 *
 * A thumbnail has to go stale when the picture behind it changes, and with
 * content identity that is not a problem to solve — it is a property. A file's
 * node id is a digest of its bytes, so a picture that changes becomes a
 * different id and therefore asks for a different thumbnail; the old one is
 * simply never asked for again. Two copies of the same photo share an id and so
 * share one thumbnail, which is the same saving the duplicate finder gets.
 *
 * An earlier version keyed on the id *and* the modification time, because back
 * then an id was a UUID that said nothing about the contents. That is no longer
 * true, and the timestamp only produced orphans.
 */

/** Extensions the webview can decode well enough to shrink on a canvas. */
const THUMBNAILABLE = new Set(['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'avif']);

export function canThumbnail(extension: string): boolean {
  return THUMBNAILABLE.has(extension.toLowerCase());
}

/**
 * The thumbnail name for a file: the digest of what is inside it.
 *
 * `save_thumbnail` rejects anything carrying a path separator and keeps only
 * the stem, so this has to be a plain filename — hence stripping the
 * `Files/…​.md` wrapper off the node id rather than passing it whole.
 */
export function thumbKeyFor(file: Pick<FileMetadata, 'id'>): string {
  const digest = file.id.replace(/^Files\//, '').replace(/\.md$/, '');
  return `${digest.replace(/[^a-zA-Z0-9_-]/g, '')}.webp`;
}

export function useFileThumbnails(vaultPath: () => string) {
  /** Names present in `assets/.thumbs`, so the grid can ask without touching disk. */
  const stored = ref<Set<string>>(new Set());
  /** Guards against two visible cells racing to build the same thumbnail. */
  const inFlight = new Set<string>();

  const separator = () => (vaultPath().includes('\\') ? '\\' : '/');
  const trimmedVault = () => {
    const p = vaultPath();
    return p.endsWith('/') || p.endsWith('\\') ? p.slice(0, -1) : p;
  };

  const load = async () => {
    if (!vaultPath()) return;
    try {
      stored.value = new Set(await invoke<string[]>('list_thumbnails', { vaultPath: vaultPath() }));
    } catch (e) {
      logger.error('Failed to list file thumbnails', e);
    }
  };

  /**
   * The absolute path the grid should load for a file: the thumbnail when there
   * is one, the original otherwise. Never blocks — a cell without a thumbnail
   * yet simply paints the original once, which is what it did before.
   */
  const pathFor = (file: FileMetadata): string => {
    if (!canThumbnail(file.extension)) return file.path;
    const key = thumbKeyFor(file);
    if (!stored.value.has(key)) return file.path;
    return `${trimmedVault()}${separator()}assets${separator()}.thumbs${separator()}${key}`;
  };

  /**
   * Build a thumbnail for a file if it is worth one. Fire-and-forget by design:
   * everything here fails soft, and a failure costs a slower paint, not a
   * broken cell.
   */
  const ensure = async (file: FileMetadata) => {
    if (!vaultPath() || !canThumbnail(file.extension)) return;
    const key = thumbKeyFor(file);
    if (stored.value.has(key) || inFlight.has(key)) return;
    inFlight.add(key);

    try {
      // Read once, used twice. The bytes are needed anyway to build the
      // thumbnail without tainting the canvas — reading the file a second time
      // from Rust to pull out the camera would double the cost of a grid.
      const bytes = await readFile(file.path);
      const facts = readPhotoFacts(bytes);
      const { thumbnail, width, height } = await shrink(bytes, file.path);

      // Recorded even when the picture was too small to need a thumbnail: its
      // camera and its size are worth knowing either way.
      if (facts.camera || facts.shotAt || width) {
        await invoke('record_photo_facts', {
          nodeId: file.id,
          facts: {
            camera: facts.camera ?? null,
            shot_at: facts.shotAt ?? null,
            width: width || null,
            height: height || null,
          },
        });
      }

      if (!thumbnail) return; // Already small enough to show as it is.

      const savedAs = await invoke<string>('save_thumbnail', {
        vaultPath: vaultPath(),
        assetName: key,
        bytes: Array.from(thumbnail),
      });
      stored.value = new Set([...stored.value, savedAs]);
    } catch (e) {
      logger.error('Failed to build file thumbnail', e);
    } finally {
      inFlight.delete(key);
    }
  };

  return { stored, load, pathFor, ensure };
}
