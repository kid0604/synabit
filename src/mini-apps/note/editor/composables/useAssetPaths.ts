import { convertFileSrc } from '@tauri-apps/api/core';

/**
 * Provides functions to convert between vault-relative asset paths
 * and absolute Tauri asset URLs for rendering in the editor.
 */
export function useAssetPaths(vaultPath: string) {
  const injectLocalAssets = (md: string): string => {
    if (!vaultPath) return md;
    let processed = md;

    // Preserve <img> tags but convert src to absolute asset URL
    processed = processed.replace(/<img\s+([^>]*)src="assets\/([^"]+)"([^>]*)>/g, (_m, before, filename, after) => {
      const sep = vaultPath.includes('\\') ? '\\' : '/';
      const decodedName = decodeURIComponent(filename);
      const absPath = `${vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath);
      return `<img ${before}src="${assetUrl}"${after}>`;
    });
    processed = processed.replace(/<video\s+([^>]*)src="assets\/([^"]+)"([^>]*)>/g, (_m, before, filename, after) => {
      const sep = vaultPath.includes('\\') ? '\\' : '/';
      const decodedName = decodeURIComponent(filename);
      const absPath = `${vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath);
      return `<video ${before}src="${assetUrl}"${after}>`;
    });
    processed = processed.replace(/<audio\s+([^>]*)src="assets\/([^"]+)"([^>]*)>/g, (_m, before, filename, after) => {
      const sep = vaultPath.includes('\\') ? '\\' : '/';
      const decodedName = decodeURIComponent(filename);
      const absPath = `${vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath);
      return `<audio ${before}src="${assetUrl}"${after}>`;
    });
    processed = processed.replace(/\[([^\]]*)\]\(synabit:\/\/(note|node|person|task|quickcap)\/([^)]+)\)/g, (_match, label, type, uri) => {
      const decoded = decodeURIComponent(uri);
      return `[${label}](synabit://${type}/${encodeURIComponent(decoded)})`;
    });

    return processed.replace(/\]\(assets\/([^\)]+)\)/g, (_m: string, filename: string) => {
      const sep = vaultPath.includes('\\') ? '\\' : '/';
      // Decode URI in case it was encoded (e.g. spaces as %20)
      const decodedName = decodeURIComponent(filename);
      const absPath = `${vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath);
      return `](${assetUrl})`;
    });
  };

  const stripLocalAssets = (md: string): string => {
    let processed = md;

    // Preserve <img> tags and their inline styles/width/height but make src relative.
    // Ensure it ends with /> to prevent markdown-it from swallowing text.
    processed = processed.replace(/<img\s+([^>]*)src="([^"]+)"([^>]*)>/gi, (_m, before, src, after) => {
      const match = src.match(/(?:https?:\/\/asset\.localhost|asset:\/\/localhost|tauri:\/\/localhost)[^\"]+(?:\/|%2F)assets(?:\/|%2F)([^\"]+)/);
      let newSrc = src;
      if (match) {
        const decodedName = decodeURIComponent(match[1]);
        newSrc = `assets/${encodeURI(decodedName)}`;
      }

      let newAfter = after;
      if (!newAfter.trim().endsWith('/')) {
        newAfter = newAfter + '/';
      }
      return `<img ${before}src="${newSrc}"${newAfter}>`;
    });

    processed = processed.replace(/<video\s+([^>]*)src="([^"]+)"([^>]*)>/g, (m, before, src, after) => {
      const match = src.match(/(?:https?:\/\/asset\.localhost|asset:\/\/localhost|tauri:\/\/localhost)[^\"]+(?:\/|%2F)assets(?:\/|%2F)([^\"]+)/);
      if (match) {
        const decodedName = decodeURIComponent(match[1]);
        return `<video ${before}src="assets/${encodeURI(decodedName)}"${after}>`;
      }
      return m;
    });

    processed = processed.replace(/<audio\s+([^>]*)src="([^"]+)"([^>]*)>/g, (m, before, src, after) => {
      const match = src.match(/(?:https?:\/\/asset\.localhost|asset:\/\/localhost|tauri:\/\/localhost)[^\"]+(?:\/|%2F)assets(?:\/|%2F)([^\"]+)/);
      if (match) {
        const decodedName = decodeURIComponent(match[1]);
        return `<audio ${before}src="assets/${encodeURI(decodedName)}"${after}>`;
      }
      return m;
    });

    return processed.replace(/\]\((?:https?:\/\/asset\.localhost|asset:\/\/localhost|tauri:\/\/localhost)[^\)]+(?:\/|%2F)assets(?:\/|%2F)([^\)]+)\)/g, (_m: string, filename: string) => {
      // Decode first to get real filename, then encode for valid Markdown URL
      const decodedName = decodeURIComponent(filename);
      return `](assets/${encodeURI(decodedName)})`;
    });
  };

  return { injectLocalAssets, stripLocalAssets };
}
