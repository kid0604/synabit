/**
 * The few things a camera writes into a photo that a person browses by.
 *
 * # Why this is here and not in Rust
 *
 * Two reasons, and the second is the real one.
 *
 * Adding an EXIF crate would put a new dependency into an Android package that
 * goes through a release-size review — the same constraint that keeps an image
 * decoder out of `src-tauri` and puts thumbnailing on a `<canvas>` instead.
 *
 * More to the point, the bytes are already here. `makeThumbnail` reads the file
 * to shrink it; reading the header again from Rust would be a second pass over
 * the same file for a few dozen bytes that were in memory a moment earlier.
 *
 * # What it does not do
 *
 * Only JPEG, which is what cameras and phones produce. HEIC and PNG store their
 * metadata in entirely different containers and are left alone rather than
 * half-supported.
 *
 * No GPS. It is a genuinely useful field for a photo library and it is also the
 * fiddliest part of the format — rationals, and a separate tag saying which
 * hemisphere — so it is left for whoever adds a map to browse by.
 *
 * Everything fails soft. A photo whose header is truncated, unusual, or simply
 * absent returns nothing, and nothing downstream treats that as an error: a
 * picture with no camera recorded is an ordinary picture.
 */

export interface PhotoFacts {
  /** Make and model as one readable string — "Fujifilm X-T5". */
  camera?: string;
  /** When the shutter fired, as `YYYY-MM-DD HH:MM:SS`. */
  shotAt?: string;
  /** EXIF orientation, 1–8. Only present when the camera bothered to say. */
  orientation?: number;
}

// Tags, in the order they appear in the specification.
const TAG_MAKE = 0x010f;
const TAG_MODEL = 0x0110;
const TAG_ORIENTATION = 0x0112;
const TAG_EXIF_IFD = 0x8769;
const TAG_DATE_TIME_ORIGINAL = 0x9003;

const TYPE_ASCII = 2;
const TYPE_SHORT = 3;
const TYPE_LONG = 4;

/** Bytes in one IFD entry: tag, type, count, then a value or a pointer to one. */
const ENTRY_SIZE = 12;

/**
 * Read what the camera recorded, or nothing.
 *
 * `bytes` is the whole file; only the first APP1 segment is looked at.
 */
export function readPhotoFacts(bytes: Uint8Array): PhotoFacts {
  try {
    const exifStart = findExifSegment(bytes);
    if (exifStart === null) return {};
    return readTiff(bytes, exifStart);
  } catch {
    // A DataView read past the end throws, which for a truncated or unusual
    // file is the expected outcome rather than a bug worth reporting.
    return {};
  }
}

/**
 * Where the TIFF header inside the APP1 segment begins.
 *
 * A JPEG is a sequence of marker segments. Walking them is the only reliable
 * way in: searching the file for the string "Exif" would happily match image
 * data further down and then read nonsense as a header.
 */
function findExifSegment(bytes: Uint8Array): number | null {
  if (bytes.length < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8) return null; // Not a JPEG.

  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let at = 2;

  while (at + 4 <= bytes.length) {
    if (bytes[at] !== 0xff) return null; // Lost the marker structure.
    const marker = bytes[at + 1];

    // Start of scan: image data from here on, and no more metadata.
    if (marker === 0xda) return null;

    const length = view.getUint16(at + 2, false);
    if (length < 2) return null;

    if (marker === 0xe1) {
      const payload = at + 4;
      // "Exif\0\0", then the TIFF header.
      const isExif =
        bytes[payload] === 0x45 &&
        bytes[payload + 1] === 0x78 &&
        bytes[payload + 2] === 0x69 &&
        bytes[payload + 3] === 0x66;
      if (isExif) return payload + 6;
    }

    at += 2 + length;
  }
  return null;
}

function readTiff(bytes: Uint8Array, tiffStart: number): PhotoFacts {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);

  // "II" is Intel, little-endian; "MM" is Motorola, big-endian. Both occur in
  // the wild, and reading one as the other yields plausible-looking rubbish.
  const marker = view.getUint16(tiffStart, false);
  if (marker !== 0x4949 && marker !== 0x4d4d) return {};
  const little = marker === 0x4949;

  if (view.getUint16(tiffStart + 2, little) !== 42) return {};

  const facts: PhotoFacts = {};
  const ifd0 = tiffStart + view.getUint32(tiffStart + 4, little);
  const exifIfdOffset = readIfd(view, bytes, tiffStart, ifd0, little, facts);

  // Date lives in the Exif sub-IFD rather than in IFD0, which is why this walks
  // two directories rather than one.
  if (exifIfdOffset !== null) {
    readIfd(view, bytes, tiffStart, tiffStart + exifIfdOffset, little, facts);
  }
  return facts;
}

/** Reads one directory into `facts`, returning the Exif sub-IFD pointer if seen. */
function readIfd(
  view: DataView,
  bytes: Uint8Array,
  tiffStart: number,
  ifdStart: number,
  little: boolean,
  facts: PhotoFacts,
): number | null {
  if (ifdStart + 2 > bytes.length) return null;
  const count = view.getUint16(ifdStart, little);

  // A corrupt count would otherwise send this reading megabytes of image data
  // as though it were directory entries.
  if (ifdStart + 2 + count * ENTRY_SIZE > bytes.length) return null;

  let exifIfd: number | null = null;
  let make = '';
  let model = '';

  for (let i = 0; i < count; i++) {
    const entry = ifdStart + 2 + i * ENTRY_SIZE;
    const tag = view.getUint16(entry, little);
    const type = view.getUint16(entry + 2, little);
    const length = view.getUint32(entry + 4, little);

    switch (tag) {
      case TAG_MAKE:
        if (type === TYPE_ASCII) make = readAscii(view, bytes, tiffStart, entry, length, little);
        break;
      case TAG_MODEL:
        if (type === TYPE_ASCII) model = readAscii(view, bytes, tiffStart, entry, length, little);
        break;
      case TAG_ORIENTATION:
        if (type === TYPE_SHORT) {
          const value = view.getUint16(entry + 8, little);
          if (value >= 1 && value <= 8) facts.orientation = value;
        }
        break;
      case TAG_EXIF_IFD:
        if (type === TYPE_LONG) exifIfd = view.getUint32(entry + 8, little);
        break;
      case TAG_DATE_TIME_ORIGINAL:
        if (type === TYPE_ASCII) {
          const raw = readAscii(view, bytes, tiffStart, entry, length, little);
          const parsed = normaliseExifDate(raw);
          if (parsed) facts.shotAt = parsed;
        }
        break;
    }
  }

  const camera = joinCamera(make, model);
  if (camera) facts.camera = camera;
  return exifIfd;
}

/**
 * An ASCII value, which lives inline when it fits in four bytes and behind a
 * pointer when it does not.
 */
function readAscii(
  view: DataView,
  bytes: Uint8Array,
  tiffStart: number,
  entry: number,
  length: number,
  little: boolean,
): string {
  if (length === 0 || length > 512) return '';
  const at = length <= 4 ? entry + 8 : tiffStart + view.getUint32(entry + 8, little);
  if (at < 0 || at + length > bytes.length) return '';

  let out = '';
  for (let i = 0; i < length; i++) {
    const code = bytes[at + i];
    if (code === 0) break; // NUL terminated, and the count includes the NUL.
    out += String.fromCharCode(code);
  }
  return out.trim();
}

/**
 * "Fujifilm" + "X-T5" reads as one name; "NIKON CORPORATION" + "NIKON Z 6" does
 * not, because the model already carries the make. Dropping the repetition is
 * what keeps a camera filter from listing "NIKON CORPORATION NIKON Z 6".
 */
function joinCamera(make: string, model: string): string {
  if (!make) return model;
  if (!model) return make;
  const firstWord = make.split(/\s+/)[0].toLowerCase();
  if (model.toLowerCase().startsWith(firstWord)) return model;
  return `${make} ${model}`;
}

/**
 * EXIF writes `2026:08:26 14:30:00`. Everything else in this app writes
 * `2026-08-26 14:30:00`, and a filter comparing the two as strings would sort
 * them into different centuries.
 */
function normaliseExifDate(raw: string): string | undefined {
  const match = raw.match(/^(\d{4}):(\d{2}):(\d{2})[ T](\d{2}):(\d{2}):(\d{2})/);
  if (!match) return undefined;
  const [, year, month, day, hour, minute, second] = match;
  // Cameras with an unset clock write zeroes, which is not a date.
  if (year === '0000') return undefined;
  return `${year}-${month}-${day} ${hour}:${minute}:${second}`;
}
