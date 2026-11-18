import { describe, expect, it } from 'vitest';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';
import {
  CATEGORIES, NUMBER_FORMATS, TOOLS, availability, cleansInPlace, linesToText, ocrLanguages, everyN, fitWithin, fits, fold, isJpegName, kindOf, markCentres,
  matches, numberText, outputName, ownFormat, pageImageName, parseRanges, previewMayRefuse, sortDrop, span,
  suggestionsFor, toolById, turnedBox, clock, parseClock, videoExt, leastTarget, shownSize, LONGEST_GIF, audioExt, takes,
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
    // A one-picture tool for one picture only; crop takes a batch now.
    expect(suggestionsFor('image', 2).map((t) => t.id)).not.toContain('idphoto');
    expect(suggestionsFor('image', 1).map((t) => t.id)).toContain('idphoto');
    expect(suggestionsFor('image', 2).map((t) => t.id)).toContain('crop');
    // Putting pictures together needs two.
    expect(suggestionsFor('image', 1).map((t) => t.id)).not.toContain('collage');
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
    expect(availability(office2pdf, mac, { room: false, libreoffice: true, pdf2docx: true, ffmpeg: true })).toBe('old-mac');
    expect(availability(office2pdf, mac, { room: true, libreoffice: false, pdf2docx: true, ffmpeg: true })).toBe('needs-install');
    expect(availability(office2pdf, mac, { room: true, libreoffice: true, pdf2docx: false, ffmpeg: true })).toBe('ready');
    expect(availability(toolById('compress')!, null, null)).toBe('ready');
  });

  it('names converted files by their new kind', () => {
    expect(outputName('office2pdf', 'Báo cáo.docx')).toBe('Báo cáo.pdf');
    expect(outputName('pdf2word', 'Hợp đồng.pdf')).toBe('Hợp đồng.docx');
    expect(outputName('compress', 'scan.pdf')).toBe('scan-nen.pdf');
  });
});

describe('what Redact finds', async () => {
  const { finder, foldChars } = await import('../tools');
  const found = (kind: Parameters<typeof finder>[0], query: string, text: string) => {
    const folded = foldChars(text);
    return finder(kind, query)(folded).map(([a, b]) => Array.from(text).slice(a, b).join(''));
  };

  it('finds words however they are accented or capitalised, in place', () => {
    expect(found('text', 'nguyen van a', 'Họ tên: NGUYỄN VĂN A, sinh năm 1990')).toEqual(['NGUYỄN VĂN A']);
    expect(found('text', 'Đà Nẵng', 'Đà Nẵng và da nang')).toEqual(['Đà Nẵng', 'da nang']);
    expect(found('text', '   ', 'anything')).toEqual([]);
  });

  it('finds phone numbers as people write them, and not inside longer numbers', () => {
    expect(found('phone', '', 'Gọi 0912 345 678 hoặc +84 912.345.678')).toEqual(['0912 345 678', '+84 912.345.678']);
    expect(found('phone', '', 'Mã 1234567890123')).toEqual([]);
  });

  it('finds emails and ID numbers', () => {
    expect(found('email', '', 'Liên hệ: An.Nguyen@vi-du.vn.')).toEqual(['An.Nguyen@vi-du.vn']);
    expect(found('id', '', 'CCCD 001090012345, CMND 123456789, số 12345678901234')).toEqual(['001090012345', '123456789']);
  });
});

describe('video', () => {
  const o = { convert: 'webm', trimExact: false, audio: 'mp3', framesPng: false };

  it('knows a video by its name, and offers video tools for it', () => {
    expect(kindOf('Quay lại.MOV')).toBe('video');
    expect(kindOf('clip.mkv')).toBe('video');
    expect(sortDrop([{ name: 'a.mp4' }, { name: 'b.mov' }, { name: 'c.pdf' }]).kind).toBe('video');
    expect(suggestionsFor('video', 1)[0].id).toBe('vidcompress');
    expect(suggestionsFor('video', 1).map((t) => t.id)).not.toContain('vidmerge');
    expect(suggestionsFor('video', 3).map((t) => t.id)).toContain('vidmerge');
    expect(suggestionsFor('video', 3).map((t) => t.id)).not.toContain('vidtrim');
  });

  it('waits for ffmpeg, and says so', () => {
    const mac = { subject: true, text: true, languages: [] };
    const tool = toolById('vidcompress')!;
    expect(availability(tool, mac, { room: true, libreoffice: false, pdf2docx: false, ffmpeg: false })).toBe('needs-install');
    expect(availability(tool, mac, { room: true, libreoffice: false, pdf2docx: false, ffmpeg: true })).toBe('ready');
    expect(availability(tool, null, null)).toBe('not-mac');
  });

  it('names a result for what it becomes, as Rust will', () => {
    expect(outputName('vidcompress', 'Đám cưới.mov', undefined, videoExt('vidcompress', 'Đám cưới.mov', o))).toBe('Đám cưới-nen.mp4');
    expect(outputName('vidconvert', 'a.mov', undefined, videoExt('vidconvert', 'a.mov', o))).toBe('a.webm');
    expect(videoExt('vidtrim', 'a.MOV', o)).toBe('mov');
    expect(videoExt('vidtrim', 'a.mov', { ...o, trimExact: true })).toBe('mp4');
    expect(videoExt('vidaudio', 'a.mov', o)).toBe('mp3');
    expect(videoExt('vidrotate', 'a.avi', o)).toBe('mp4');
    expect(videoExt('vidrotate', 'a.webm', o)).toBe('webm');
    expect(videoExt('vidstrip', 'a.flv', o)).toBe('flv');
    expect(videoExt('vidgif', 'a.mov', o)).toBe('gif');
    expect(outputName('vidmute', 'a.mp4', undefined, 'mp4')).toBe('a-tat-tieng.mp4');
  });

  it('reads and writes times as people type them', () => {
    expect(clock(75.55)).toBe('1:15.5');
    expect(clock(3723, false)).toBe('1:02:03');
    expect(clock(5)).toBe('0:05.0');
    expect(parseClock('1:15.5')).toBeCloseTo(75.5);
    expect(parseClock('75,5')).toBeCloseTo(75.5);
    expect(parseClock('1:02:03')).toBe(3723);
    expect(parseClock('abc')).toBeNaN();
    expect(parseClock('1::2')).toBeNaN();
    expect(LONGEST_GIF).toBe(60);
  });

  it('knows the least size a target can be, as Rust refuses', () => {
    // 12.5 s with sound: (150 + 128) × 12.5 / 8000 / 0.97, up to a tenth.
    expect(leastTarget(12.5, true, false)).toBe(0.5);
    expect(leastTarget(600, false, false)).toBe(11.6);
  });

  it('turns an upright phone video the way it is seen', () => {
    expect(shownSize({ codec: 'hevc', width: 1920, height: 1080, rotation: -90, fps: 30, pixFmt: '', bitrate: null })).toEqual([1080, 1920]);
    expect(shownSize({ codec: 'h264', width: 1920, height: 1080, rotation: 180, fps: 30, pixFmt: '', bitrate: null })).toEqual([1920, 1080]);
  });
});

describe('audio', () => {
  it('knows a sound file, and a .ts file is code, not video', () => {
    expect(kindOf('Ghi âm.m4a')).toBe('audio');
    expect(kindOf('song.FLAC')).toBe('audio');
    expect(kindOf('main.ts')).toBeNull();
    expect(kindOf('clip.mts')).toBe('video');
    expect(sortDrop([{ name: 'a.mp3' }, { name: 'b.wav' }, { name: 'c.mp4' }]).kind).toBe('audio');
    expect(suggestionsFor('audio', 1)[0].id).toBe('audconvert');
    expect(suggestionsFor('audio', 2).map((t) => t.id)).toContain('audmerge');
    expect(suggestionsFor('audio', 2).map((t) => t.id)).not.toContain('audring');
  });

  it('lets the audio tools take a video’s sound, and not the other way round', () => {
    expect(takes(toolById('audloud')!, 'video')).toBe(true);
    expect(takes(toolById('audloud')!, 'audio')).toBe(true);
    expect(takes(toolById('vidcompress')!, 'audio')).toBe(false);
    expect(takes(toolById('audloud')!, null)).toBe(false);
  });

  it('names a result for what Rust will make', () => {
    expect(audioExt('audtrim', 'a.mp3', 'mp3')).toBe('mp3');
    expect(audioExt('audtrim', 'a.wma', 'mp3')).toBe('m4a');
    expect(audioExt('auddenoise', 'clip.mov', 'mp3')).toBe('m4a');
    expect(audioExt('audloud', 'a.aif', 'mp3')).toBe('aiff');
    expect(audioExt('audconvert', 'a.mp3', 'alac')).toBe('m4a');
    expect(audioExt('audring', 'Bài hát.mp3', 'mp3')).toBe('m4r');
    expect(outputName('audring', 'Bài hát.mp3', undefined, 'm4r')).toBe('Bài hát.m4r');
    expect(outputName('audsilence', 'hop.m4a', undefined, 'm4a')).toBe('hop-bo-lang.m4a');
    expect(videoExt('vidloud', 'a.mov', { convert: 'mp4', trimExact: false, audio: 'm4a', framesPng: false })).toBe('mov');
  });
});
