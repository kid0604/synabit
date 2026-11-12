import { describe, expect, it } from 'vitest';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';
import {
  CATEGORIES, NUMBER_FORMATS, TOOLS, availability, cleansInPlace, linesToText, ocrLanguages, everyN, fitWithin, fits, fold, isJpegName, kindOf, markCentres,
  matches, numberText, outputName, ownFormat, pageImageName, parseRanges, previewMayRefuse, sortDrop, span,
  suggestionsFor, toolById, turnedBox,
} from '../tools';
import { toolsForFiles } from '../handoff';

describe('the tool list', () => {
  it('has unique ids, each in a group that exists', () => {
    const ids = TOOLS.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const t of TOOLS) expect(CATEGORIES).toContain(t.category);
  });

  /** A card with a key for a name is a card nobody can read. */
  it('names and describes every tool in both languages', () => {
    for (const locale of [en, vi] as const) {
      const tools = (locale as any).utilities.tools;
      for (const t of TOOLS) {
        for (const field of ['name', 'desc', 'verb', 'running']) {
          expect(tools[t.id]?.[field], `${t.id}.${field}`).toBeTruthy();
        }
      }
      for (const c of CATEGORIES) expect((locale as any).utilities.categories[c]?.name).toBeTruthy();
    }
  });

  it('has the same keys in English and Vietnamese', () => {
    const keys = (o: any, prefix = ''): string[] =>
      Object.entries(o).flatMap(([k, v]) => (v && typeof v === 'object' ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`]));
    expect(keys((vi as any).utilities).sort()).toEqual(keys((en as any).utilities).sort());
  });
});

describe('which files a tool takes', () => {
  it('knows a PDF from a picture by its name', () => {
    expect(kindOf('Hợp đồng.PDF')).toBe('pdf');
    expect(kindOf('IMG_2041.HEIC')).toBe('image');
    expect(kindOf('notes.zip')).toBeNull();
    expect(isJpegName('a.jpeg')).toBe(true);
    expect(isJpegName('a.png')).toBe(false);
  });

  it('respects the least and most files a tool can run with', () => {
    expect(fits(toolById('merge')!, 1)).toBe(false);
    expect(fits(toolById('merge')!, 2)).toBe(true);
    expect(fits(toolById('split')!, 2)).toBe(false);
  });

  it('offers merge for several PDFs and split for one', () => {
    expect(suggestionsFor('pdf', 3).map((t) => t.id).slice(0, 2)).toEqual(['merge', 'compress']);
    expect(suggestionsFor('pdf', 3).map((t) => t.id)).not.toContain('split');
    expect(suggestionsFor('pdf', 1).map((t) => t.id).slice(0, 2)).toEqual(['compress', 'split']);
    expect(suggestionsFor('pdf', 1).map((t) => t.id)).not.toContain('merge');
    expect(suggestionsFor('image', 2).map((t) => t.id)).not.toContain('crop');
    expect(suggestionsFor('image', 1).map((t) => t.id)).toContain('crop');
  });

  it('sorts a mixed drop by its larger kind and says what it left out', () => {
    const files = [{ name: 'a.pdf' }, { name: 'b.jpg' }, { name: 'c.pdf' }, { name: 'd.txt' }];
    const { kind, taken, left } = sortDrop(files);
    expect(kind).toBe('pdf');
    expect(taken.map((f) => f.name)).toEqual(['a.pdf', 'c.pdf']);
    expect(left.map((f) => f.name)).toEqual(['b.jpg', 'd.txt']);
    expect(sortDrop([{ name: 'x.txt' }]).kind).toBeNull();
  });
});

describe('search', () => {
  it('ignores case and Vietnamese marks', () => {
    expect(fold('Gộp ĐƠN')).toBe('gop don');
    const merge = toolById('merge')!;
    expect(matches('gop', merge, 'Gộp PDF', 'Ghép nhiều PDF')).toBe(true);
    expect(matches('merge', merge, 'Gộp PDF', 'Ghép nhiều PDF')).toBe(true);
    expect(matches('xoay', merge, 'Gộp PDF', 'Ghép nhiều PDF')).toBe(false);
    expect(matches('  ', merge, 'Gộp PDF', '')).toBe(true);
  });
});

describe('page ranges', () => {
  it('reads ranges as people write them', () => {
    expect(parseRanges('1-3, 5, 8-10', 10)).toEqual({ ranges: [[1, 3], [5, 5], [8, 10]], bad: null });
    expect(parseRanges('1–2; 3', 5)).toEqual({ ranges: [[1, 2], [3, 3]], bad: null });
  });

  it('takes an open range to the last page', () => {
    expect(parseRanges('9-', 12)).toEqual({ ranges: [[9, 12]], bad: null });
  });

  it('names the part that is wrong', () => {
    expect(parseRanges('1-3, 4-20', 10).bad).toBe('4-20');
    expect(parseRanges('5-2', 10).bad).toBe('5-2');
    expect(parseRanges('0', 10).bad).toBe('0');
    expect(parseRanges('abc', 10).bad).toBe('abc');
  });

  it('splits every N pages, the last part shorter', () => {
    expect(everyN(4, 10)).toEqual([[1, 4], [5, 8], [9, 10]]);
    expect(everyN(0, 2)).toEqual([[1, 1], [2, 2]]);
    expect(span([3, 5])).toEqual([3, 4, 5]);
  });
});

describe('what results are called', () => {
  it('names a result after the first input and what was done', () => {
    expect(outputName('merge', 'Hop-dong.pdf')).toBe('Hop-dong-gop.pdf');
    expect(outputName('split', 'Hop-dong.pdf', 2)).toBe('Hop-dong-phan-2.pdf');
    expect(outputName('img2pdf', 'IMG_2041.HEIC')).toBe('IMG_2041.pdf');
  });

  it('pads page numbers so the pictures sort in order', () => {
    expect(pageImageName('a.pdf', 7, 120, 'png')).toBe('a-007.png');
    expect(pageImageName('a.pdf', 7, 9, 'jpeg')).toBe('a-7.jpg');
  });
});

describe('U-2 helpers', () => {
  it('names image results after their format', () => {
    expect(outputName('imgcompress', 'IMG_1.HEIC', undefined, 'jpg')).toBe('IMG_1-nen.jpg');
    expect(outputName('imgconvert', 'a.png', undefined, 'webp')).toBe('a.webp');
    expect(outputName('exif', 'a.jpeg')).toBe('a-sach.jpeg');
    expect(outputName('pdf2text', 'Hop-dong.pdf')).toBe('Hop-dong.txt');
    expect(outputName('protect', 'Hop-dong.pdf')).toBe('Hop-dong-khoa.pdf');
  });

  it('writes pictures back in their own format when it can', () => {
    expect(ownFormat('a.png', false)).toBe('png');
    expect(ownFormat('a.webp', true)).toBe('webp');
    expect(ownFormat('a.webp', false)).toBe('jpeg');
    expect(ownFormat('a.HEIC', true)).toBe('jpeg');
    expect(cleansInPlace('a.JPG')).toBe(true);
    expect(cleansInPlace('a.heic')).toBe(false);
  });

  it('fits a picture in a box without enlarging it unless asked', () => {
    expect(fitWithin(4000, 3000, 1600, 1600)).toEqual([1600, 1200]);
    expect(fitWithin(800, 600, 1600, 1600)).toEqual([800, 600]);
    expect(fitWithin(800, 600, 1600, 1600, true)).toEqual([1600, 1200]);
  });

  /** The same cases as `tiles_cover_the_page` and the grid in `stamp.rs`. */
  it('places watermarks where the Rust side does', () => {
    const tiles = markCentres({ position: 4, tile: true }, [600, 800], [200, 60]);
    expect(tiles.length).toBeGreaterThanOrEqual(10);
    expect(tiles.some(([x, y]) => x < 200 && y > 700)).toBe(true);
    expect(markCentres({ position: 4, tile: false }, [600, 800], [200, 60])).toEqual([[300, 400]]);
    expect(markCentres({ position: 0, tile: false }, [600, 800], [200, 60])).toEqual([[130, 740]]);
    const [w, h] = turnedBox(100, 20, 90);
    expect(Math.round(w)).toBe(20);
    expect(Math.round(h)).toBe(100);
  });

  it('warns about passwords Preview cannot take', () => {
    expect(previewMayRefuse('matkhau 123!')).toBe(false);
    expect(previewMayRefuse('mật khẩu')).toBe(true);
  });

  it('writes page numbers in the chosen style', () => {
    expect(numberText('Trang {n} / {total}', 3, 12)).toBe('Trang 3 / 12');
    for (const f of NUMBER_FORMATS) expect(/^[\x20-\x7e]+$/.test(numberText(f, 1, 2)), f).toBe(true);
  });

  it('offers Utilities from Files only for one kind of file', () => {
    expect(toolsForFiles(['a.pdf', 'b.pdf']).map((t) => t.id)[0]).toBe('merge');
    expect(toolsForFiles(['a.pdf']).map((t) => t.id)[0]).toBe('compress');
    expect(toolsForFiles(['a.jpg']).map((t) => t.id)).toContain('imgcompress');
    expect(toolsForFiles(['a.pdf', 'b.jpg'])).toEqual([]);
    expect(toolsForFiles(['a.docx'])).toEqual([]);
  });
});

describe('U-3 helpers', () => {
  const mac = { subject: true, text: true, languages: ['en-US', 'vi-VT'] };

  it('says why a Vision tool cannot run', () => {
    const removebg = toolById('removebg')!;
    expect(availability(removebg, mac)).toBe('ready');
    expect(availability(removebg, null)).toBe('not-mac');
    expect(availability(removebg, { ...mac, subject: false })).toBe('old-mac');
    expect(availability(toolById('merge')!, null)).toBe('ready');
  });

  it('asks for Vietnamese only where it is offered', () => {
    expect(ocrLanguages(mac, 'vi')).toEqual(['vi-VT', 'en-US']);
    expect(ocrLanguages({ ...mac, languages: ['en-US'] }, 'vi')).toEqual(['en-US']);
    expect(ocrLanguages(mac, 'auto')).toEqual([]);
  });

  /** The same cases as `lines_read_top_to_bottom_then_left_to_right` in `searchable.rs`. */
  it('joins lines into rows as the Rust side does', () => {
    const line = (text: string, x: number, y: number) => ({ text, x, y, w: 0.2, h: 0.02 });
    expect(linesToText([line('trái', 0.1, 0.8), line('phải', 0.6, 0.805), line('dưới', 0.1, 0.1)])).toBe('trái phải\ndưới');
  });

  it('names text and cut-out results', () => {
    expect(outputName('imgtext', 'Screenshot 1.png')).toBe('Screenshot 1.txt');
    expect(outputName('removebg', 'a.jpg', undefined, 'png')).toBe('a-tach-nen.png');
    expect(outputName('ocr', 'scan.pdf')).toBe('scan-tim-duoc.pdf');
  });
});

describe('U-4 helpers', () => {
  const mac = { subject: true, text: true, languages: [] };

  it('knows Office files', () => {
    expect(kindOf('Báo cáo.DOCX')).toBe('office');
    expect(kindOf('so-lieu.xlsx')).toBe('office');
    expect(sortDrop([{ name: 'a.docx' }, { name: 'b.pptx' }, { name: 'c.pdf' }]).kind).toBe('office');
    expect(suggestionsFor('office', 2).map((t) => t.id)).toEqual(['office2pdf']);
  });

  it('says what a room-backed tool is waiting for', () => {
    const office2pdf = toolById('office2pdf')!;
    expect(availability(office2pdf, null, null)).toBe('not-mac');
    expect(availability(office2pdf, mac, { room: false, libreoffice: true, pdf2docx: true })).toBe('old-mac');
    expect(availability(office2pdf, mac, { room: true, libreoffice: false, pdf2docx: true })).toBe('needs-install');
    expect(availability(office2pdf, mac, { room: true, libreoffice: true, pdf2docx: false })).toBe('ready');
    expect(availability(toolById('compress')!, null, null)).toBe('ready');
  });

  it('names converted files by their new kind', () => {
    expect(outputName('office2pdf', 'Báo cáo.docx')).toBe('Báo cáo.pdf');
    expect(outputName('pdf2word', 'Hợp đồng.pdf')).toBe('Hợp đồng.docx');
    expect(outputName('compress', 'scan.pdf')).toBe('scan-nen.pdf');
  });
});
