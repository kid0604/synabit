/**
 * The workbench's run lifecycle: a run belongs to the screen it started on,
 * and work that finishes after the screen changed must not touch the next one.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type * as Api from '../api';

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

const pdfOpen = { current: deferred<{ numPages: number }>() };

vi.mock('../api', () => ({
  newJob: vi.fn(),
  cancel: vi.fn(() => Promise.resolve()),
  discard: vi.fn(() => Promise.resolve()),
  assemble: vi.fn(),
  unsavedResults: vi.fn(() => Promise.resolve([])),
  lockStatus: vi.fn(() => Promise.resolve(['none'])),
  recognize: vi.fn(() => Promise.resolve([{ text: 'read by Vision', x: 0, y: 0, w: 1, h: 0.1 }])),
  putOutput: vi.fn(() => Promise.resolve({ name: 'x.txt', size: 1, path: '/x.txt' })),
  searchable: vi.fn(),
  getJob: vi.fn(),
  officeStatus: vi.fn(() => Promise.resolve({ room: true, libreoffice: false, pdf2docx: false, ffmpeg: true })),
  videoProbe: vi.fn(() =>
    Promise.resolve({ duration: 12.5, bytes: 1000, format: 'mov', video: { codec: 'h264', width: 1920, height: 1080, rotation: -90, fps: 30, pixFmt: 'yuv420p', bitrate: null }, audio: null, subtitles: [] }),
  ),
  video: vi.fn(),
  videoMerge: vi.fn(),
  audio: vi.fn(),
  audioMerge: vi.fn(),
}));

vi.mock('../render', () => ({
  openPdf: vi.fn(() => pdfOpen.current.promise),
  closePdf: vi.fn(() => Promise.resolve()),
  loadImage: vi.fn(),
  canEncode: () => true,
  // Page 1 has text of its own; page 2 is a scan.
  pageWords: vi.fn((_path: string, page: number) => Promise.resolve(page === 1 ? 'typed on page one' : '')),
  pagePixels: vi.fn(() => Promise.resolve({ width: 1, height: 1, rgba: new Uint8Array(4) })),
}));

vi.mock('../../../composables/useAppNotice', () => ({ showAppNotice: vi.fn() }));

const api = (await import('../api')) as unknown as { [K in keyof typeof Api]: ReturnType<typeof vi.fn> };
const { useWorkbench, resetWorkbench } = await import('../useWorkbench');

const input = (name: string): Api.Input => ({ id: name.replace(/\W/g, ''), name, size: 1000, path: `/cache/${name}` });
const job = (id: string): Api.Job => ({ id, tool: 'merge', createdMs: 0, outputs: [], saved: false });
const settle = () => new Promise((r) => setTimeout(r, 0));

describe('a run', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pdfOpen.current = deferred();
    resetWorkbench();
  });
  afterEach(() => resetWorkbench());

  it('is cancelled and thrown away when another tool is opened', async () => {
    const wb = useWorkbench();
    wb.openTool('merge', [input('a.pdf'), input('b.pdf')]);
    pdfOpen.current.resolve({ numPages: 2 });
    await settle();
    expect(wb.ready.value).toBe(true);

    const assembled = deferred<Api.Job>();
    api.newJob.mockResolvedValue(job('j1'));
    api.assemble.mockReturnValue(assembled.promise);
    const running = wb.run();
    await settle();
    expect(wb.phase.value).toBe('running');

    wb.openTool('split');
    expect(api.cancel).toHaveBeenCalledWith('j1');
    // The work finishes anyway; its result is not wanted.
    assembled.resolve({ ...job('j1'), outputs: [{ name: 'a-gop.pdf', size: 1, path: '/x' }] });
    await running;

    expect(api.discard).toHaveBeenCalledWith('j1');
    expect(wb.tool.value).toBe('split');
    expect(wb.job.value).toBeNull();
    expect(wb.phase.value).toBe('empty');
    expect(wb.failure.value).toBeNull();
  });

  it('cancelled from its own screen goes back to the files, with no error shown', async () => {
    const wb = useWorkbench();
    wb.openTool('merge', [input('a.pdf'), input('b.pdf')]);
    pdfOpen.current.resolve({ numPages: 1 });
    await settle();

    api.newJob.mockResolvedValue(job('j2'));
    api.assemble.mockRejectedValue(new Error('cancelled'));
    const running = wb.run();
    await settle();
    wb.cancel();
    await running;

    expect(wb.phase.value).toBe('loaded');
    expect(wb.failure.value).toBeNull();
    expect(wb.job.value).toBeNull();
  });
});

describe('a file being read', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pdfOpen.current = deferred();
    resetWorkbench();
  });

  it('does not reset the next tool’s options once it has gone', async () => {
    const wb = useWorkbench();
    wb.openTool('organize', [input('a.pdf')]);
    wb.openTool('split');
    wb.options.split.ranges = '1-2';
    pdfOpen.current.resolve({ numPages: 4 });
    await settle();
    expect(wb.options.split.ranges).toBe('1-2');
  });
});

describe('the run button', () => {
  beforeEach(() => {
    pdfOpen.current = deferred();
    resetWorkbench();
  });

  it('stays off while a number field is emptied', async () => {
    const wb = useWorkbench();
    wb.openTool('numbers', [input('a.pdf')]);
    pdfOpen.current.resolve({ numPages: 3 });
    await settle();
    expect(wb.ready.value).toBe(true);
    // v-model.number gives '' for an emptied field.
    (wb.options.numbers as { start: unknown }).start = '';
    expect(wb.ready.value).toBe(false);
    wb.options.numbers.start = 1;
    wb.options.numbers.size = 3;
    expect(wb.ready.value).toBe(false);
    wb.options.numbers.size = 10;
    expect(wb.ready.value).toBe(true);
  });
});

describe('a one-file tool', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pdfOpen.current = deferred();
    resetWorkbench();
  });

  it('keeps its file when given one of the wrong kind', async () => {
    const wb = useWorkbench();
    wb.openTool('split', [input('a.pdf')]);
    pdfOpen.current.resolve({ numPages: 2 });
    await settle();
    expect(wb.addInputs([input('photo.png')])).toBe(1);
    expect(wb.items.value.map((i) => i.input.name)).toEqual(['a.pdf']);
    expect(wb.phase.value).toBe('loaded');
  });

  it('swaps in a file it takes', async () => {
    const wb = useWorkbench();
    wb.openTool('split', [input('a.pdf')]);
    pdfOpen.current.resolve({ numPages: 2 });
    await settle();
    wb.addInputs([input('b.pdf')]);
    expect(wb.items.value.map((i) => i.input.name)).toEqual(['b.pdf']);
  });
});

describe('reading a scan', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pdfOpen.current = deferred();
    resetWorkbench();
  });

  it('writes a page’s own text into the .txt when it skips reading it', async () => {
    const wb = useWorkbench();
    wb.vision.value = { subject: true, text: true, languages: ['vi-VT', 'en-US'] } as never;
    wb.openTool('ocr', [input('scan.pdf')]);
    pdfOpen.current.resolve({ numPages: 2 });
    await settle();
    wb.options.ocr.pdf = false;
    wb.options.ocr.text = true;
    expect(wb.ready.value).toBe(true);
    api.newJob.mockResolvedValue(job('j3'));
    api.getJob.mockResolvedValue(job('j3'));
    await wb.run();

    expect(api.recognize).toHaveBeenCalledTimes(1); // page 2 only
    const bytes = api.putOutput.mock.calls[0][2] as Uint8Array;
    const text = new TextDecoder().decode(bytes);
    expect(text).toContain('typed on page one');
    expect(text).toContain('read by Vision');
  });
});

describe('the unsaved list', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    pdfOpen.current = deferred();
    resetWorkbench();
  });

  it('leaves out a run that is still going', async () => {
    const wb = useWorkbench();
    wb.openTool('merge', [input('a.pdf'), input('b.pdf')]);
    pdfOpen.current.resolve({ numPages: 1 });
    await settle();
    api.newJob.mockResolvedValue(job('running'));
    api.assemble.mockReturnValue(new Promise(() => {}));
    void wb.run();
    await settle();
    api.unsavedResults.mockResolvedValue([job('running'), job('older')]);
    await wb.refreshUnsaved();
    expect(wb.unsaved.value.map((j) => j.id)).toEqual(['older']);
    wb.cancel();
  });
});

describe('a video', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetWorkbench();
  });
  afterEach(() => resetWorkbench());

  it('is looked at once ffmpeg is known to be there, and its cut starts as the whole of it', async () => {
    const wb = useWorkbench();
    wb.openTool('vidtrim', [input('clip.mov')]);
    await settle();
    // Not yet known whether ffmpeg is here: the file waits.
    expect(api.videoProbe).not.toHaveBeenCalled();
    await wb.askOffice();
    await settle();
    expect(api.videoProbe).toHaveBeenCalledWith('clipmov');
    expect(wb.options.vidtrim).toMatchObject({ start: 0, end: 12.5 });
    // The whole video is no cut.
    expect(wb.ready.value).toBe(false);
    wb.options.vidtrim.start = 2;
    expect(wb.ready.value).toBe(true);
  });

  it('runs with the settings as Rust takes them, and names the result for what it becomes', async () => {
    const wb = useWorkbench();
    await wb.askOffice();
    wb.openTool('vidcompress', [input('Đám cưới.mov')]);
    await settle();
    wb.options.vidcompress.bySize = true;
    wb.options.vidcompress.targetMb = 0.1;
    expect(wb.ready.value).toBe(false);
    wb.options.vidcompress.targetMb = 16;
    expect(wb.ready.value).toBe(true);
    api.newJob.mockResolvedValue(job('j1'));
    api.video.mockResolvedValue({ ...job('j1'), outputs: [{ name: 'Đám cưới-nen.mp4', size: 500, path: '/o' }] });
    await wb.run();
    expect(api.video).toHaveBeenCalledWith('j1', [{ input: 'mcimov', name: 'Đám cưới-nen.mp4' }], {
      tool: 'compress', height: 1080, codec: 'h264', quality: 'balanced', targetMb: 16,
    });
    expect(wb.phase.value).toBe('done');
  });

  it('says a size too small for its length in words', async () => {
    const wb = useWorkbench();
    await wb.askOffice();
    wb.openTool('vidmute', [input('a.mp4')]);
    await settle();
    api.newJob.mockResolvedValue(job('j2'));
    api.video.mockRejectedValue('video-too-small: 4.2');
    await wb.run();
    expect(wb.failure.value).toEqual({ key: 'utilities.errors.video_too_small', values: { mb: (4.2).toLocaleString('en') } });
  });
});

describe('sound', () => {
  const voice = { duration: 90, bytes: 1000, format: 'matroska,webm', video: null, audio: { codec: 'opus', channels: 1, rate: 48000, bitrate: 32000 }, subtitles: [] };
  beforeEach(() => {
    vi.clearAllMocks();
    resetWorkbench();
  });
  afterEach(() => resetWorkbench());

  it('a voice note in a .webm is said to be sound only in a video tool, and taken by an audio tool', async () => {
    const wb = useWorkbench();
    await wb.askOffice();
    api.videoProbe.mockResolvedValue(voice);
    wb.openTool('vidcompress', [input('note.webm')]);
    await settle();
    expect(wb.items.value[0].problem).toBe('audio_only');
    wb.openTool('audloud', [input('note.webm')]);
    await settle();
    expect(wb.items.value[0].problem).toBeUndefined();
    expect(wb.ready.value).toBe(true);
    api.newJob.mockResolvedValue(job('j3'));
    api.audio.mockResolvedValue({ ...job('j3'), outputs: [{ name: 'note-chuan-am.m4a', size: 1, path: '/o' }] });
    await wb.run();
    expect(api.audio).toHaveBeenCalledWith('j3', [{ input: 'notewebm', name: 'note-chuan-am.m4a' }], { tool: 'loudness', mode: 'podcast', gain: 6 }, false);
  });

  it('a ringtone starts at most 30 seconds long and may not pass 40', async () => {
    const wb = useWorkbench();
    await wb.askOffice();
    api.videoProbe.mockResolvedValue(voice);
    wb.openTool('audring', [input('song.mp3')]);
    await settle();
    expect(wb.options.audring).toMatchObject({ start: 0, end: 30 });
    expect(wb.ready.value).toBe(true);
    wb.options.audring.end = 45;
    expect(wb.ready.value).toBe(false);
  });

  it('loudness for a video keeps the picture', async () => {
    const wb = useWorkbench();
    await wb.askOffice();
    api.videoProbe.mockResolvedValue({ ...voice, video: { codec: 'h264', width: 640, height: 360, rotation: 0, fps: 30, pixFmt: 'yuv420p', bitrate: null } });
    wb.openTool('vidloud', [input('talk.mov')]);
    await settle();
    wb.options.audloud.mode = 'speech';
    api.newJob.mockResolvedValue(job('j4'));
    api.audio.mockResolvedValue({ ...job('j4'), outputs: [] });
    await wb.run();
    expect(api.audio).toHaveBeenCalledWith('j4', [{ input: 'talkmov', name: 'talk-chuan-am.mov' }], { tool: 'loudness', mode: 'speech', gain: 6 }, true);
  });
});
