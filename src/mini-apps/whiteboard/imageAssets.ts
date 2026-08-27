/**
 * Putting a picture on a board.
 *
 * The bytes go where every other attachment in the vault goes — `assets/`,
 * named after their own contents by `save_asset`, so the same screenshot
 * pasted onto five boards is one file that syncs once. The board itself only
 * ever holds the path.
 *
 * Nothing here knows about the canvas; it is the file end of the job.
 */

import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { readFile } from '@tauri-apps/plugin-fs';

/** How large a pasted picture is allowed to arrive on the board, in points. */
export const MAX_PLACED_EDGE = 480;

/** Join a vault path to something inside it, on whichever platform this is. */
export function vaultJoin(vaultPath: string, relative: string): string {
  const sep = vaultPath.includes('\\') ? '\\' : '/';
  return `${vaultPath}${sep}${relative.split('/').join(sep)}`;
}

/**
 * A name to file the picture under.
 *
 * A pasted screenshot arrives as a `File` with no name at all, and the
 * extension is what later decides whether anything will open it.
 */
export function filenameFor(file: { name?: string; type?: string }): string {
  if (file.name) return file.name;
  const subtype = (file.type || '').split('/')[1] || 'png';
  // `image/svg+xml` → `svg`
  const extension = subtype.split('+')[0];
  return `pasted-image.${extension}`;
}

/**
 * Scale a picture down to fit, keeping its shape.
 *
 * A photo from a phone is four thousand points wide. Dropped at that size it
 * covers the board and everything on it, and the first thing the user has to
 * do is undo. Anything already small enough is left alone rather than
 * stretched up to the limit.
 */
export function fitWithin(
  natural: { width: number; height: number },
  maxEdge: number = MAX_PLACED_EDGE
): { width: number; height: number } {
  const { width, height } = natural;
  if (!(width > 0) || !(height > 0)) return { width: maxEdge, height: maxEdge };

  const longest = Math.max(width, height);
  if (longest <= maxEdge) return { width: Math.round(width), height: Math.round(height) };

  const scale = maxEdge / longest;
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

/**
 * The size a picture is in itself.
 *
 * A picture the browser cannot measure comes back as zero rather than as a
 * rejection: it still gets a box to sit in, and whether it can be drawn is
 * the node's problem, not this one's.
 */
export function naturalSizeOfUrl(url: string): Promise<{ width: number; height: number }> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve({ width: img.naturalWidth, height: img.naturalHeight });
    img.onerror = () => resolve({ width: 0, height: 0 });
    img.src = url;
  });
}

/** The same, for bytes that are in hand rather than in the vault. */
export async function naturalSize(file: Blob): Promise<{ width: number; height: number }> {
  const url = URL.createObjectURL(file);
  try {
    return await naturalSizeOfUrl(url);
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** Copy a picture that is already on disk into the vault, by path. */
export function importImagePath(vaultPath: string, sourcePath: string): Promise<string> {
  // Hashed in chunks on the other side rather than carried through the
  // bridge as a list of numbers, which is what a photo off a camera would
  // otherwise cost.
  return invoke<string>('copy_asset_to_vault', { vaultPath, sourcePath });
}

/** Copy a picture into the vault. Returns its path inside the vault. */
export async function saveImageToVault(vaultPath: string, file: File): Promise<string> {
  const buffer = await file.arrayBuffer();
  return invoke<string>('save_asset', {
    vaultPath,
    filename: filenameFor(file),
    bytes: Array.from(new Uint8Array(buffer)),
  });
}

/** How the webview should be asked for a picture that is in the vault. */
export function assetUrl(vaultPath: string, assetPath: string): string {
  return convertFileSrc(vaultJoin(vaultPath, assetPath));
}

/**
 * The picture as bytes in a URL, for the PNG export.
 *
 * The export screenshots the document, and the screenshot library re-fetches
 * every image it finds. Vault files are served over the webview's own asset
 * protocol, which `connect-src` in the app's content policy does not cover —
 * so they are read here, over the same channel the rest of the app uses for
 * files, and handed to the exporter as data it cannot fail to load.
 */
export async function assetDataUri(vaultPath: string, assetPath: string): Promise<string | null> {
  try {
    const bytes = await readFile(vaultJoin(vaultPath, assetPath));
    let binary = '';
    const chunk = 0x8000; // btoa on a huge spread argument overflows the stack
    for (let i = 0; i < bytes.length; i += chunk) {
      binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
    }
    return `data:${mimeFor(assetPath)};base64,${btoa(binary)}`;
  } catch {
    return null;
  }
}

/** Bring an angle into 0–359, whichever way round it was turned. */
export function normalizeAngle(degrees: number): number {
  if (!Number.isFinite(degrees)) return 0;
  return ((degrees % 360) + 360) % 360;
}

/**
 * How far a turned picture sticks out past the box it is in.
 *
 * The box on the canvas stays square to the world however far the picture
 * inside it is turned, so a turned picture overhangs it — by up to a third of
 * its own width at 45°. The export crops to the boxes, and would cut the
 * corners off. This is the margin that keeps them.
 *
 * Half the difference between the turned bounding box and the upright one,
 * taken on whichever axis is worse, because the overhang is symmetrical about
 * the centre.
 */
export function rotatedOverhang(width: number, height: number, degrees: number): number {
  const angle = (normalizeAngle(degrees) * Math.PI) / 180;
  const cos = Math.abs(Math.cos(angle));
  const sin = Math.abs(Math.sin(angle));

  const turnedWidth = width * cos + height * sin;
  const turnedHeight = width * sin + height * cos;

  return Math.max(0, (turnedWidth - width) / 2, (turnedHeight - height) / 2);
}

/** The content type an extension implies, for a data URL. */
export function mimeFor(path: string): string {
  const extension = path.split('.').pop()?.toLowerCase() ?? '';
  switch (extension) {
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'webp':
      return 'image/webp';
    case 'svg':
      return 'image/svg+xml';
    case 'avif':
      return 'image/avif';
    case 'bmp':
      return 'image/bmp';
    default:
      return 'application/octet-stream';
  }
}
