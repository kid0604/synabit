import { describe, expect, it } from 'vitest';
import { fitPage, jpegSize, jpegToPdf, MAX_PAGE_SIDE } from '../pdf';
import { style } from '../exporters';
import { exportable } from '../composables/useClipboardExport';
import { sharePage } from '../sharePage';

/** The start of a JPEG: SOI, an APP0 segment, padding, then a frame header of the given kind and size. */
function jpegHeader(width: number, height: number, sof = 0xc0): Uint8Array {
  return new Uint8Array([
    0xff, 0xd8,
    // APP0 (JFIF), 16 bytes long
    0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00,
    // a fill byte before the next marker
    0xff,
    // SOFn: length 17, precision 8, height, width, 3 components
    0xff, sof, 0x00, 0x11, 0x08, height >> 8, height & 0xff, width >> 8, width & 0xff, 0x03,
    0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01,
    0xff, 0xd9,
  ]);
}

describe('the size a JPEG says it is', () => {
  it('is read from a baseline frame header, past other segments and fill bytes', () => {
    expect(jpegSize(jpegHeader(16383, 9001))).toEqual({ width: 16383, height: 9001 });
  });

  it('is read from a progressive frame header too', () => {
    expect(jpegSize(jpegHeader(640, 480, 0xc2))).toEqual({ width: 640, height: 480 });
  });

  it('is nothing when there is no frame header, or no JPEG', () => {
    expect(jpegSize(new Uint8Array([0xff, 0xd8, 0xff, 0xd9]))).toBeNull();
    expect(jpegSize(new Uint8Array([0x89, 0x50, 0x4e, 0x47]))).toBeNull();
    expect(jpegSize(jpegHeader(640, 480).slice(0, 25))).toBeNull();
  });

  it('is what the PDF declares, whatever size it was asked for', () => {
    const pdf = new TextDecoder('latin1').decode(jpegToPdf(jpegHeader(1999, 1001), 2000, 1000, 1500, 750));
    expect(pdf).toContain('/Width 1999 /Height 1001');
  });

  it('falls back to the size given when the JPEG does not say', () => {
    const pdf = new TextDecoder('latin1').decode(jpegToPdf(new Uint8Array([0xff, 0xd8, 0xff, 0xd9]), 20, 10, 15, 7.5));
    expect(pdf).toContain('/Width 20 /Height 10');
  });
});

describe('the size of a PDF page', () => {
  it('is left alone when it is within the limit', () => {
    expect(fitPage(1000, 500)).toEqual({ width: 1000, height: 500 });
  });

  it('is shrunk to the limit on its longest side, with its shape kept', () => {
    const page = fitPage(28800, 7200);
    expect(page.width).toBe(MAX_PAGE_SIDE);
    expect(page.height).toBeCloseTo(3600);
    const tall = fitPage(2000, 40000);
    expect(tall.height).toBe(MAX_PAGE_SIDE);
    expect(tall.width).toBeCloseTo(720);
  });

  it('is what the PDF writes, while the picture keeps its pixels', () => {
    const pdf = new TextDecoder('latin1').decode(jpegToPdf(jpegHeader(16384, 4096), 16384, 4096, 24576, 6144));
    expect(pdf).toContain('/MediaBox [0 0 14400 3600]');
    expect(pdf).toContain('q 14400 0 0 3600 0 0 cm');
    expect(pdf).toContain('/Width 16384 /Height 4096');
  });
});

describe('a draw.io style', () => {
  it('lets no value add a key', () => {
    const s = style(['text', ['fontSize', '13;shape=image;image=https://evil' as unknown as number], ['fontColor', '#000']]);
    expect(s).toBe('text;fontSize=13;fontColor=#000;');
    expect(s.split(';').filter((e) => e.startsWith('image') || e.startsWith('shape'))).toEqual([]);
  });

  it('keeps a picture whole, base64 padding and all', () => {
    expect(style([['image', 'data:image/png,AAAA==']])).toBe('image=data:image/png,AAAA==;');
  });

  it('writes numbers as numbers, and leaves out what is not one', () => {
    expect(style([['strokeWidth', 2], ['opacity', 0], ['size', Number.NaN], ['arcSize', Infinity], ['locked', undefined]])).toBe('strokeWidth=2;opacity=0;');
  });
});

describe('what an export pictures', () => {
  const nodes = [
    { id: 'a', type: 'sticky' },
    { id: 'c', type: 'comment' },
    { id: 'm', type: 'mindmap', hidden: true },
    { id: 'n', type: 'mindmap' },
  ];

  it('leaves out comments and what a folded branch hides', () => {
    expect(exportable(nodes, null).map((n) => n.id)).toEqual(['a', 'n']);
  });

  it('keeps to the items asked for, even when a comment or a hidden item is asked for', () => {
    expect(exportable(nodes, new Set(['a', 'c', 'm'])).map((n) => n.id)).toEqual(['a']);
  });
});

describe('a shared page with no title', () => {
  const labels = { prev: 'Trước', next: 'Sau', fit: 'Vừa', zoomIn: '+', zoomOut: '-', hint: 'Kéo' };
  const page = (title: string, untitled?: string) =>
    sharePage({ title, svg: '<svg/>', width: 10, height: 10, frames: [], lang: 'vi', labels: { ...labels, untitled } });

  it('is named in the reader\'s language', () => {
    expect(page('', 'Chưa đặt tên')).toContain('<title>Chưa đặt tên</title>');
    expect(page('   ', 'Chưa đặt tên')).toContain('<header><span>Chưa đặt tên</span></header>');
  });

  it('still has a name before the caller gives one', () => {
    expect(page('')).toContain('<title>Board</title>');
  });

  it('keeps its own title when it has one', () => {
    expect(page('Plan', 'Chưa đặt tên')).toContain('<title>Plan</title>');
  });
});
