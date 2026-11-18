/**
 * The Rust side of Utilities (`commands::utilities`), as functions.
 *
 * Files are named by id once they are in the workspace; the only paths that
 * come back are for drawing them through the asset protocol. Saving outside
 * the vault goes through a dialog Rust opens (`pickSavePath`, `pickFolder`),
 * because `app_shell::gate` lets those commands write only where one was
 * answered.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { pickFolder, pickSavePath } from '../../shared/pickedPath';
import type { InputKind, OfficeStatus, TextLine, VideoProbe, VisionAbilities } from './tools';

export interface Input {
  id: string;
  name: string;
  size: number;
  path: string;
}

export interface Output {
  name: string;
  size: number;
  path: string;
}

export interface Job {
  id: string;
  tool: string;
  createdMs: number;
  outputs: Output[];
  saved: boolean;
  savedTo?: string | null;
  /** The vault folder the first input came from, for "Save beside the original". */
  originDir?: string | null;
}

/** One run of pages from one source; see `utilities::pdf::Pick`. */
export interface Pick {
  source: number;
  pages: number[] | null;
  rotate: number;
}

export interface PdfOutput {
  name: string;
  picks: Pick[];
  /** Bookmarks: each source's as they were, or under one per file (`pdf::Outline`). */
  outline?: 'keep' | { byFile: string[] };
}

export interface Layout {
  /** Pictures a page: 1, 2 or 4 (`images::Layout::per_page`). */
  perPage?: 1 | 2 | 4;
  size: 'a4' | 'letter' | 'fit';
  orientation: 'auto' | 'portrait' | 'landscape';
  margin: number;
}

export function pickInputs(kinds: InputKind[], multiple: boolean): Promise<Input[]> {
  return invoke<Input[]>('utility_pick_inputs', { kinds, multiple });
}

/** Hand over bytes the window holds: a dropped file, a redrawn picture. */
export function stage(name: string, bytes: Uint8Array): Promise<Input> {
  return invoke<Input>('utility_stage', bytes, { headers: { 'x-utility-name': encodeURIComponent(name) } });
}

/** How much of a file goes over at once: a video is never in memory whole. */
const PART = 32 << 20;

/**
 * Hand over a file dropped on the window, in parts when it is large: the
 * WebView reads 32 MB at a time and Rust adds each to the copy it keeps.
 */
export async function stageFile(file: File): Promise<Input> {
  let input = await stage(file.name, new Uint8Array(await file.slice(0, PART).arrayBuffer()));
  for (let at = PART; at < file.size; at += PART) {
    const bytes = new Uint8Array(await file.slice(at, at + PART).arrayBuffer());
    input = await invoke<Input>('utility_stage', bytes, { headers: { 'x-utility-append': input.id } });
  }
  return input;
}

export function newJob(tool: string): Promise<Job> {
  return invoke<Job>('utility_new_job', { tool });
}

export function cancel(job: string): Promise<void> {
  return invoke('utility_cancel', { job });
}

export function assemble(job: string, inputs: string[], outputs: PdfOutput[]): Promise<Job> {
  return invoke<Job>('utility_assemble', { job, inputs, outputs });
}

export function imagesToPdf(job: string, inputs: string[], layout: Layout, name: string): Promise<Job> {
  return invoke<Job>('utility_images_to_pdf', { job, inputs, layout, name });
}

export function putOutput(job: string, name: string, bytes: Uint8Array): Promise<Output> {
  return invoke<Output>('utility_put_output', bytes, {
    headers: { 'x-utility-job': encodeURIComponent(job), 'x-utility-name': encodeURIComponent(name) },
  });
}

export function getJob(job: string): Promise<Job> {
  return invoke<Job>('utility_job', { job });
}

export function unsavedResults(): Promise<Job[]> {
  return invoke<Job[]>('utility_results');
}

export function discard(job: string): Promise<void> {
  return invoke('utility_discard', { job });
}

export function openOutput(job: string, output: string): Promise<void> {
  return invoke('utility_open', { job, output });
}

export function revealSaved(job: string): Promise<void> {
  return invoke('utility_reveal_saved', { job });
}

/** Ask where, then save one output there. Null when the dialog was closed. */
export async function saveAs(job: string, output: string, suggested: string): Promise<Job | null> {
  const ext = suggested.split('.').pop()?.toLowerCase() ?? '';
  const filters = ext ? [{ name: ext.toUpperCase(), extensions: [ext] }] : [];
  const destination = await pickSavePath(suggested, filters);
  if (!destination) return null;
  return invoke<Job>('utility_save', { job, output, destination });
}

export async function saveAllToFolder(job: string, title: string): Promise<Job | null> {
  const folder = await pickFolder(title, 'utility_save_all');
  if (!folder) return null;
  return invoke<Job>('utility_save_all', { job, folder });
}

export async function saveZip(job: string, suggested: string): Promise<Job | null> {
  const destination = await pickSavePath(suggested, [{ name: 'ZIP', extensions: ['zip'] }]);
  if (!destination) return null;
  return invoke<Job>('utility_save_zip', { job, destination });
}

export function saveToVault(vaultPath: string, job: string, output: string, name: string): Promise<string> {
  return invoke<string>('utility_save_to_vault', { vaultPath, job, output, name });
}

/** Use a result as the input of the next tool, without saving it first. */
export function inputFromOutput(job: string, output: string): Promise<Input> {
  return invoke<Input>('utility_input_from_output', { job, output });
}

export function onProgress(handler: (p: { job: string; done: number; total: number }) => void): Promise<UnlistenFn> {
  return listen<{ job: string; done: number; total: number }>('utility-progress', (e) => handler(e.payload));
}

/** One input and the name its result gets. */
export interface PerFile {
  input: string;
  name: string;
}

export interface WatermarkLayout {
  position: number;
  tile: boolean;
  size: number;
  rotate: number;
  under: boolean;
}

export interface Numbering {
  position: number;
  format: string;
  start: number;
  skipFirst: boolean;
  size: number;
}

export type Lock = 'none' | 'restrictions' | 'password';

/** Stamp a picture (RGBA) on each file's pages. */
export function watermark(
  job: string,
  files: PerFile[],
  picture: { width: number; height: number; rgba: Uint8Array },
  layout: WatermarkLayout,
  exceptFirst: boolean,
): Promise<Job> {
  const args = { job, files, width: picture.width, height: picture.height, layout, exceptFirst };
  return invoke<Job>('utility_watermark', picture.rgba, { headers: { 'x-utility-args': encodeURIComponent(JSON.stringify(args)) } });
}

export function numberPages(job: string, files: PerFile[], numbering: Numbering): Promise<Job> {
  return invoke<Job>('utility_number_pages', { job, files, numbering });
}

/** Pictures to put on a PDF's pages: where each goes, and its pixels one after another. */
export function place(
  job: string,
  files: PerFile[],
  marks: { page: number; share: [number, number, number, number]; width: number; height: number }[],
  pixels: Uint8Array,
): Promise<Job> {
  const args = { job, files, marks };
  return invoke<Job>('utility_place', pixels, { headers: { 'x-utility-args': encodeURIComponent(JSON.stringify(args)) } });
}

/** The corners of the sheet of paper in a photo, top left first, as shares; `null` if none (or off macOS). */
export function findDocument(width: number, height: number, rgba: Uint8Array): Promise<[number, number][] | null> {
  return invoke<[number, number][] | null>('utility_find_document', rgba, { headers: { 'x-utility-size': `${width}x${height}` } });
}

/** The faces in a picture, largest first, as shares from its top left: [x, y, w, h]; none off macOS. */
export function findFaces(width: number, height: number, rgba: Uint8Array): Promise<[number, number, number, number][]> {
  return invoke<[number, number, number, number][]>('utility_find_faces', rgba, { headers: { 'x-utility-size': `${width}x${height}` } });
}

/** JPEGs turned without re-encoding: only the orientation they say changes. */
export function turnJpegs(job: string, files: PerFile[], turn: { quarters: number; flipH: boolean; flipV: boolean }): Promise<Job> {
  return invoke<Job>('utility_turn_jpegs', { job, files, turn });
}

/** A page drawn with its black boxes, waiting for `redact`. */
export function pagePut(job: string, input: string, page: number, jpeg: Uint8Array): Promise<void> {
  return invoke('utility_page_put', jpeg, { headers: { 'x-utility-job': job, 'x-utility-input': input, 'x-utility-page': String(page) } });
}

/** Replace the pages put with `pagePut` by those pictures; `clean` takes the document information out too. */
export function redact(job: string, files: PerFile[], pages: { page: number; lines: TextLine[] }[], clean: boolean): Promise<Job> {
  return invoke<Job>('utility_redact', { job, files, pages, clean });
}

/** One form box's value for Rust (`form::Filled`). */
export interface Filled {
  widget: string;
  kind: 'text' | 'check' | 'radio' | 'choice';
  value: string;
}

/** Fill in a form: the values into its fields, or flattened, their pictures placed and the form taken off. */
export function fillForm(
  job: string,
  files: PerFile[],
  values: Filled[],
  flatten: boolean,
  marks: Parameters<typeof place>[2],
  pixels: Uint8Array,
): Promise<Job> {
  const args = { job, files, values, flatten, marks };
  return invoke<Job>('utility_fill_form', pixels, { headers: { 'x-utility-args': encodeURIComponent(JSON.stringify(args)) } });
}

/** One PDF to crop: its boxes, one for every page or one per page (`null` leaves a page). */
export interface FramedFile extends PerFile {
  boxes: ([number, number, number, number] | null)[];
}

/** Crop each file's pages, or with `paper`, put every page on paper of that size. */
export function frame(job: string, files: FramedFile[], paper: { width: number; height: number; orientation: Layout['orientation'] } | null): Promise<Job> {
  return invoke<Job>('utility_frame', { job, files, paper });
}

export function protect(job: string, files: PerFile[], password: string): Promise<Job> {
  return invoke<Job>('utility_protect', { job, files, password });
}

export function unlock(job: string, files: PerFile[], password: string): Promise<Job> {
  return invoke<Job>('utility_unlock', { job, files, password });
}

export function stripMetadata(job: string, files: PerFile[]): Promise<Job> {
  return invoke<Job>('utility_strip_metadata', { job, files });
}

export function lockStatus(inputs: string[]): Promise<Lock[]> {
  return invoke<Lock[]>('utility_lock_status', { inputs });
}

/** Files Files shows — in the vault or a source folder — as inputs. */
export function addFromVault(vaultPath: string, paths: string[]): Promise<Input[]> {
  return invoke<Input[]>('utility_add_from_vault', { vaultPath, paths });
}

export function saveBeside(vaultPath: string, job: string, output: string, name: string): Promise<string> {
  return invoke<string>('utility_save_beside', { vaultPath, job, output, name });
}

/** What Vision can do here; null off macOS. */
export function visionStatus(): Promise<VisionAbilities | null> {
  return invoke<VisionAbilities | null>('utility_vision_status');
}

/** A picture's subject, one mask byte a pixel. */
export async function subjectMask(width: number, height: number, rgba: Uint8Array): Promise<Uint8Array> {
  const out = await invoke<ArrayBuffer | number[]>('utility_subject_mask', rgba, { headers: { 'x-utility-size': `${width}x${height}` } });
  return out instanceof ArrayBuffer ? new Uint8Array(out) : Uint8Array.from(out);
}

/** The text in a picture, a line at a time, in reading order. */
export function recognize(width: number, height: number, rgba: Uint8Array, languages: string[]): Promise<TextLine[]> {
  return invoke<TextLine[]>('utility_recognize', rgba, {
    headers: { 'x-utility-size': `${width}x${height}`, 'x-utility-languages': languages.join(',') },
  });
}

export interface SearchableFile {
  input: string;
  name: string;
  pages: { page: number; lines: TextLine[] }[];
}

export function searchable(job: string, files: SearchableFile[]): Promise<Job> {
  return invoke<Job>('utility_searchable', { job, files });
}

export interface PdfPicture {
  id: string;
  width: number;
  height: number;
  stored: 'jpeg' | 'rgb' | 'gray';
  bytes: number;
}

export function pdfPictures(input: string): Promise<{ pictures: PdfPicture[]; pageLongSide: number }> {
  return invoke('utility_pdf_pictures', { input });
}

export async function pdfPicture(input: string, id: string): Promise<Uint8Array> {
  const out = await invoke<ArrayBuffer | number[]>('utility_pdf_picture', { input, id });
  return out instanceof ArrayBuffer ? new Uint8Array(out) : Uint8Array.from(out);
}

export function compressPut(job: string, input: string, id: string, jpeg: Uint8Array): Promise<void> {
  return invoke('utility_compress_put', jpeg, { headers: { 'x-utility-job': job, 'x-utility-input': input, 'x-utility-id': id } });
}

export function compressPdf(job: string, files: PerFile[]): Promise<Job> {
  return invoke<Job>('utility_compress', { job, files });
}

export function officeStatus(): Promise<OfficeStatus> {
  return invoke<OfficeStatus>('utility_office_status');
}

export function officeToPdf(job: string, files: PerFile[]): Promise<Job> {
  return invoke<Job>('utility_office_to_pdf', { job, files });
}

export function installPdf2docx(): Promise<void> {
  return invoke('utility_install_pdf2docx');
}

export function pdfToWord(job: string, files: PerFile[]): Promise<Job> {
  return invoke<Job>('utility_pdf_to_word', { job, files });
}

// ── Video (ffmpeg in a sealed room) ──

export function videoProbe(input: string): Promise<VideoProbe> {
  return invoke<VideoProbe>('utility_video_probe', { input });
}

/** One frame as a JPEG, for a file the WebView cannot play. */
export async function videoPicture(input: string, at: number, width: number): Promise<Blob> {
  const out = await invoke<ArrayBuffer | number[]>('utility_video_picture', { input, at, width });
  return new Blob([out instanceof ArrayBuffer ? out : Uint8Array.from(out)], { type: 'image/jpeg' });
}

/** A tool and its settings, as `utilities::video::Task` takes them. */
export type VideoTask =
  | { tool: 'compress'; height: number | null; codec: 'h264' | 'hevc'; quality: 'high' | 'balanced' | 'small'; targetMb: number | null }
  | { tool: 'convert'; format: 'mp4' | 'mov' | 'mkv' | 'webm' }
  | { tool: 'trim'; start: number; end: number; exact: boolean }
  | { tool: 'audio'; format: 'm4a' | 'mp3' | 'wav' | 'flac' | 'opus' }
  | { tool: 'strip' }
  | { tool: 'rotate'; turn: 'right' | 'left' | 'half' }
  | { tool: 'mute' }
  | { tool: 'frames'; every: number | null; at: number | null; png: boolean }
  | { tool: 'gif'; start: number; end: number; width: number; fps: number };

export function video(job: string, files: PerFile[], task: VideoTask): Promise<Job> {
  return invoke<Job>('utility_video', { job, files, task });
}

export function videoMerge(job: string, inputs: string[], name: string): Promise<Job> {
  return invoke<Job>('utility_video_merge', { job, inputs, name });
}

// ── Audio (the same room) ──

/** As `utilities::audio::AudioTask`. */
export type AudioTask =
  | { tool: 'convert'; format: 'mp3' | 'm4a' | 'opus' | 'flac' | 'alac' | 'wav'; level: 'high' | 'medium' | 'small' }
  | { tool: 'trim'; start: number; end: number; fadeIn: number; fadeOut: number }
  | { tool: 'loudness'; mode: 'podcast' | 'music' | 'speech' | 'gain'; gain: number }
  | { tool: 'denoise'; strength: 'light' | 'medium' | 'strong' }
  | { tool: 'silence'; pause: number }
  | { tool: 'speed'; factor: number }
  | { tool: 'ringtone'; start: number; end: number };

/** `keepPicture`: a video stays a video (loudness for video). */
export function audio(job: string, files: PerFile[], task: AudioTask, keepPicture: boolean): Promise<Job> {
  return invoke<Job>('utility_audio', { job, files, task, keepPicture });
}

export function audioMerge(job: string, inputs: string[], name: string, crossfade: number): Promise<Job> {
  return invoke<Job>('utility_audio_merge', { job, inputs, name, crossfade });
}

/** A file's sound as `buckets` peaks from 0 to 1. */
export function audioWaveform(input: string, duration: number, buckets: number): Promise<{ peaks: number[] }> {
  return invoke('utility_audio_waveform', { input, duration, buckets });
}
