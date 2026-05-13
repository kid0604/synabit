import { defineAsyncComponent, type Component } from 'vue';

const PdfFileViewer = defineAsyncComponent(() => import('../viewers/PdfFileViewer.vue'));
const ImageFileViewer = defineAsyncComponent(() => import('../viewers/ImageFileViewer.vue'));
const VideoFileViewer = defineAsyncComponent(() => import('../viewers/VideoFileViewer.vue'));
const AudioFileViewer = defineAsyncComponent(() => import('../viewers/AudioFileViewer.vue'));
const TextFileViewer = defineAsyncComponent(() => import('../viewers/TextFileViewer.vue'));
const FallbackViewer = defineAsyncComponent(() => import('../viewers/FallbackViewer.vue'));

const registry = new Map<string, Component>();

// PDF
['pdf'].forEach(ext => registry.set(ext, PdfFileViewer));

// Images
['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico', 'tiff', 'heic', 'avif'].forEach(ext => registry.set(ext, ImageFileViewer));

// Video
['mp4', 'mov', 'avi', 'webm', 'mkv', 'flv', 'wmv', 'm4v'].forEach(ext => registry.set(ext, VideoFileViewer));

// Audio
['mp3', 'wav', 'ogg', 'm4a', 'flac', 'aac', 'wma', 'alac'].forEach(ext => registry.set(ext, AudioFileViewer));

// Text / Code
['md', 'txt', 'json', 'csv', 'yaml', 'yml', 'toml', 'xml', 'html', 'css', 'js', 'ts', 'vue', 'rs', 'py', 'java', 'c', 'cpp', 'h', 'sh', 'bash', 'log', 'ini', 'conf', 'env', 'sql', 'graphql'].forEach(ext => registry.set(ext, TextFileViewer));

export function getViewer(extension: string): Component {
  return registry.get(extension.toLowerCase()) || FallbackViewer;
}

export function getViewerType(extension: string): 'pdf' | 'image' | 'video' | 'audio' | 'text' | 'fallback' {
  const ext = extension.toLowerCase();
  if (ext === 'pdf') return 'pdf';
  if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico', 'tiff', 'heic', 'avif'].includes(ext)) return 'image';
  if (['mp4', 'mov', 'avi', 'webm', 'mkv', 'flv', 'wmv', 'm4v'].includes(ext)) return 'video';
  if (['mp3', 'wav', 'ogg', 'm4a', 'flac', 'aac', 'wma', 'alac'].includes(ext)) return 'audio';
  if (registry.has(ext)) return 'text';
  return 'fallback';
}

export function getFileIcon(ext: string): string {
  const type = getViewerType(ext);
  switch (type) {
    case 'pdf': return 'file-text';
    case 'image': return 'image';
    case 'video': return 'video';
    case 'audio': return 'music';
    case 'text': return 'code';
    default: return 'file';
  }
}
