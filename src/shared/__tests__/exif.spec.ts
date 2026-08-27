import { describe, it, expect } from 'vitest';
import { readPhotoFacts } from '../exif';

/**
 * A JPEG carrying exactly the tags this parser reads, built byte by byte.
 *
 * Written as a builder rather than as a checked-in fixture because the cases
 * worth testing are the *shapes* — the two byte orders, a value short enough to
 * sit inline against one behind a pointer, a truncated header — and a fixture
 * file can only ever be one of them.
 */
function buildJpeg(options: {
  little: boolean;
  make?: string;
  model?: string;
  date?: string;
  orientation?: number;
  /** Cut the file off after the header, as a partial download would. */
  truncateAfter?: number;
}): Uint8Array {
  const { little } = options;
  const tiff: number[] = [];

  const u16 = (v: number) => (little ? [v & 0xff, (v >> 8) & 0xff] : [(v >> 8) & 0xff, v & 0xff]);
  const u32 = (v: number) =>
    little
      ? [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >> 24) & 0xff]
      : [(v >> 24) & 0xff, (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff];

  // TIFF header: byte order, the number 42, then the offset of IFD0.
  tiff.push(...(little ? [0x49, 0x49] : [0x4d, 0x4d]));
  tiff.push(...u16(42));
  tiff.push(...u32(8));

  // Values too long to sit inline go in a heap after both directories. Their
  // offsets are relative to the start of the TIFF header.
  const heap: number[] = [];
  const ascii = (text: string) => {
    const bytes = [...text].map((c) => c.charCodeAt(0));
    bytes.push(0);
    return bytes;
  };

  const ifd0: number[][] = [];
  const exifIfd: number[][] = [];
  const entryCount0 = (options.make ? 1 : 0) + (options.model ? 1 : 0) +
    (options.orientation ? 1 : 0) + 1; // + the Exif IFD pointer
  const entryCountExif = options.date ? 1 : 0;

  const ifd0Start = 8;
  const exifIfdStart = ifd0Start + 2 + entryCount0 * 12 + 4;
  const heapStart = exifIfdStart + 2 + entryCountExif * 12 + 4;

  const pushHeap = (bytes: number[]) => {
    const at = heapStart + heap.length;
    heap.push(...bytes);
    return at;
  };

  const entry = (tag: number, type: number, count: number, payload: number[]) => [
    ...u16(tag),
    ...u16(type),
    ...u32(count),
    ...payload,
  ];

  if (options.make) {
    const bytes = ascii(options.make);
    ifd0.push(entry(0x010f, 2, bytes.length, u32(pushHeap(bytes))));
  }
  if (options.model) {
    const bytes = ascii(options.model);
    ifd0.push(entry(0x0110, 2, bytes.length, u32(pushHeap(bytes))));
  }
  if (options.orientation) {
    // A SHORT fits inline, in the first two bytes of the four-byte field.
    ifd0.push(entry(0x0112, 3, 1, [...u16(options.orientation), 0, 0]));
  }
  ifd0.push(entry(0x8769, 4, 1, u32(exifIfdStart)));

  if (options.date) {
    const bytes = ascii(options.date);
    exifIfd.push(entry(0x9003, 2, bytes.length, u32(pushHeap(bytes))));
  }

  tiff.push(...u16(ifd0.length));
  for (const e of ifd0) tiff.push(...e);
  tiff.push(...u32(0)); // No IFD1.

  tiff.push(...u16(exifIfd.length));
  for (const e of exifIfd) tiff.push(...e);
  tiff.push(...u32(0));

  tiff.push(...heap);

  const payload = [0x45, 0x78, 0x69, 0x66, 0, 0, ...tiff]; // "Exif\0\0" + TIFF
  const segmentLength = payload.length + 2;

  const jpeg = [
    0xff, 0xd8, // SOI
    0xff, 0xe1, (segmentLength >> 8) & 0xff, segmentLength & 0xff,
    ...payload,
    0xff, 0xda, 0x00, 0x02, // Start of scan — image data would follow.
  ];

  const out = new Uint8Array(jpeg);
  return options.truncateAfter === undefined ? out : out.slice(0, options.truncateAfter);
}

describe('readPhotoFacts', () => {
  it('reads the camera and the moment the shutter fired', () => {
    const jpeg = buildJpeg({
      little: true,
      make: 'FUJIFILM',
      model: 'X-T5',
      date: '2026:06:14 09:12:33',
    });

    expect(readPhotoFacts(jpeg)).toEqual({
      camera: 'FUJIFILM X-T5',
      shotAt: '2026-06-14 09:12:33',
    });
  });

  /// Both byte orders occur in the wild, and reading one as the other yields
  /// plausible-looking rubbish rather than an obvious failure.
  it('reads big-endian files the same as little-endian ones', () => {
    const common = { make: 'Canon', model: 'Canon EOS R6', date: '2026:06:14 09:12:33' };
    expect(readPhotoFacts(buildJpeg({ little: false, ...common }))).toEqual(
      readPhotoFacts(buildJpeg({ little: true, ...common })),
    );
  });

  /// A filter listing "NIKON CORPORATION NIKON Z 6" is a filter nobody wants to
  /// read, and the make is already inside the model on most bodies.
  it('does not repeat the make when the model already carries it', () => {
    const jpeg = buildJpeg({ little: true, make: 'NIKON CORPORATION', model: 'NIKON Z 6' });
    expect(readPhotoFacts(jpeg).camera).toBe('NIKON Z 6');
  });

  /// EXIF writes colons in the date. Everything else in this app writes dashes,
  /// and a filter comparing the two as strings would sort them centuries apart.
  it('rewrites the date into the form the rest of the app uses', () => {
    const jpeg = buildJpeg({ little: true, date: '2026:08:26 14:30:00' });
    expect(readPhotoFacts(jpeg).shotAt).toBe('2026-08-26 14:30:00');
  });

  /// A camera whose clock was never set writes zeroes. That is not a date, and
  /// treating it as one puts a pile of photos in the year zero.
  it('ignores the date a camera with an unset clock writes', () => {
    const jpeg = buildJpeg({ little: true, date: '0000:00:00 00:00:00' });
    expect(readPhotoFacts(jpeg).shotAt).toBeUndefined();
  });

  it('reads orientation when the camera recorded it', () => {
    expect(readPhotoFacts(buildJpeg({ little: true, orientation: 6 })).orientation).toBe(6);
  });

  // ── Files that are not what they claim ────────────────────

  /// The whole parser runs over bytes from the user's disk. Every one of these
  /// used to be a way to read past the end of a buffer; none of them may throw.
  it('returns nothing rather than throwing on a file it cannot read', () => {
    const cases: Record<string, Uint8Array> = {
      empty: new Uint8Array(0),
      'not a jpeg': new Uint8Array([0x89, 0x50, 0x4e, 0x47]),
      'jpeg with no exif': new Uint8Array([0xff, 0xd8, 0xff, 0xda, 0x00, 0x02]),
      'truncated mid-header': buildJpeg({ little: true, make: 'Leica', truncateAfter: 12 }),
      'truncated mid-directory': buildJpeg({ little: true, make: 'Leica', truncateAfter: 24 }),
      'declared length beyond the file': new Uint8Array([0xff, 0xd8, 0xff, 0xe1, 0xff, 0xff]),
    };

    for (const [name, bytes] of Object.entries(cases)) {
      expect(() => readPhotoFacts(bytes), name).not.toThrow();
      expect(readPhotoFacts(bytes), name).toEqual({});
    }
  });

  /// A photo with no camera recorded is an ordinary photo, not a failure.
  it('reads what is there when only some tags are present', () => {
    const jpeg = buildJpeg({ little: true, date: '2026:01:02 03:04:05' });
    expect(readPhotoFacts(jpeg)).toEqual({ shotAt: '2026-01-02 03:04:05' });
  });
});
