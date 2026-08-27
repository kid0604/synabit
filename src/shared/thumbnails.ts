import { readFile } from '@tauri-apps/plugin-fs';

/**
 * Shrinking an image attachment, using the codecs the webview already has.
 *
 * A card shows a picture a couple of hundred pixels tall; the file behind it
 * is whatever came off a camera. The webview decodes the whole thing to
 * paint that strip, so a grid of holiday photos costs hundreds of megabytes
 * of decoded pixels to display at the size of postage stamps.
 *
 * The obvious fix is to generate thumbnails in Rust, which means shipping an
 * image decoder inside an Android package that already goes through a
 * release-size review. This does it on a `<canvas>` instead: the browser has
 * the codecs, and the cost is a few dozen lines rather than a dependency.
 *
 * # Why the bytes are read rather than the URL loaded
 *
 * The first version pointed an `<img>` at the `asset://` URL and drew that.
 * It worked, right up until `toBlob`, which threw `SecurityError: The
 * operation is insecure` — drawing an image from another origin *taints* the
 * canvas, and a tainted canvas will not hand its pixels back. The asset
 * protocol is a different origin from the page, so every thumbnail failed.
 *
 * Reading the file and building a `Blob` in the page makes the source
 * same-origin, so nothing is tainted and the pixels come back. It also skips
 * a round trip through the URL layer entirely.
 *
 * Everything here fails soft. A thumbnail is an optimisation — if the file
 * cannot be read, or the webview cannot encode WebP, the card falls back to
 * the original and the user sees nothing different except a slower paint.
 */

/** The longest edge a thumbnail is allowed to have. */
const MAX_EDGE = 640;

/** WebP quality. High enough that a card looks untouched at 2× density. */
const QUALITY = 0.82;

const MIME_BY_EXTENSION: Record<string, string> = {
  png: 'image/png',
  jpg: 'image/jpeg',
  jpeg: 'image/jpeg',
  gif: 'image/gif',
  webp: 'image/webp',
  bmp: 'image/bmp',
  avif: 'image/avif',
};

function mimeFor(path: string): string {
  const extension = path.split('.').pop()?.toLowerCase() ?? '';
  return MIME_BY_EXTENSION[extension] ?? 'application/octet-stream';
}

/**
 * A downscaled WebP of the image at `absolutePath`, or `null` when there is
 * no point in making one.
 *
 * `null` is the normal answer for an image that is already small — storing a
 * second copy of a 400px screenshot would cost disk and buy nothing.
 */
export async function makeThumbnail(absolutePath: string): Promise<Uint8Array | null> {
  const bytes = await readFile(absolutePath);
  return (await shrink(bytes, absolutePath)).thumbnail;
}

/** A thumbnail and the picture's real size, from bytes the caller already has. */
export interface Shrunk {
  /** `null` when the picture is already small enough to show as it is. */
  thumbnail: Uint8Array | null;
  width: number;
  height: number;
}

/**
 * The same work, for a caller that has read the file for another reason too.
 *
 * The Files app reads a photo's bytes to pull the camera and the shot date out
 * of its header; decoding them a second time here would double the cost of
 * building a grid. It hands the bytes over instead.
 */
export async function shrink(bytes: Uint8Array, path: string): Promise<Shrunk> {
  const blob = new Blob([bytes as BlobPart], { type: mimeFor(path) });

  const bitmap = await createImageBitmap(blob);
  try {
    const longestEdge = Math.max(bitmap.width, bitmap.height);
    if (!longestEdge || longestEdge <= MAX_EDGE) {
      return { thumbnail: null, width: bitmap.width, height: bitmap.height };
    }

    const scale = MAX_EDGE / longestEdge;
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));

    const size = { width: bitmap.width, height: bitmap.height };
    const context = canvas.getContext('2d');
    if (!context) return { thumbnail: null, ...size };

    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);

    const encoded = await new Promise<Blob | null>((resolve) => {
      canvas.toBlob(resolve, 'image/webp', QUALITY);
    });

    // An older webview without WebP encoding hands back a PNG or nothing at
    // all. Neither is worth storing under a .webp name, so give up quietly.
    if (!encoded || encoded.type !== 'image/webp') return { thumbnail: null, ...size };

    return { thumbnail: new Uint8Array(await encoded.arrayBuffer()), ...size };
  } finally {
    // A decoded bitmap holds width × height × 4 bytes until it is released,
    // which for a camera photo is tens of megabytes.
    bitmap.close();
  }
}

/** The thumbnail filename an asset would have: `abc.png` → `abc.webp`. */
export function thumbnailNameFor(assetFilename: string): string {
  const stem = assetFilename.replace(/\.[^.]*$/, '');
  return `${stem}.webp`;
}
