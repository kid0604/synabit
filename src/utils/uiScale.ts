import { getCurrentWebview } from '@tauri-apps/api/webview';
import { type, version } from '@tauri-apps/plugin-os';

/**
 * The sizes offered in Settings. 1 is the size the screens were drawn at.
 *
 * Nothing smaller: the smallest text in the app is 12px, the floor the UI/UX
 * review set, and a 0.9 step took it to 10.8.
 */
export const UI_SCALES = [
  { value: 1, key: 'default' },
  { value: 1.15, key: 'large' },
  { value: 1.3, key: 'xlarge' },
] as const;

/**
 * Whether the webview can zoom itself.
 *
 * Its own zoom is the better tool: it scales the page the way a browser's zoom
 * does, so layout, pointer coordinates and canvases (the graph, the
 * whiteboard) all stay in agreement. Android's webview has none — Tauri's call
 * does nothing there — and WKWebView only gained it in macOS 11; before that
 * the call is an unknown selector, which is a crash, not a quiet failure.
 */
function nativeZoomWorks(): boolean {
  try {
    const os = type();
    if (os === 'android' || os === 'ios') return false;
    if (os === 'macos') return Number(version().split('.')[0]) >= 11;
    return true;
  } catch {
    // Not running in Tauri at all.
    return false;
  }
}

/** A stored size that is no longer offered (the old 0.9) reads as the default. */
export function normaliseUiScale(scale: unknown): number {
  return UI_SCALES.some(s => s.value === scale) ? (scale as number) : 1;
}

/**
 * Make the whole interface larger.
 *
 * Where the webview cannot zoom, the root font size does most of the job:
 * Tailwind sizes text and spacing in `rem`, so both grow with it. This is not
 * CSS `zoom`, which was tried first — on the root it multiplies every `vh`
 * too, so the `h-screen` shell grew taller than the phone and the tab bar at
 * its foot was pushed off the screen. A few sizes written in `px` stay as they
 * are, which is the price of the layout still fitting.
 */
export async function applyUiScale(scale: number) {
  const root = document.documentElement;
  const value = normaliseUiScale(scale);
  if (nativeZoomWorks()) {
    try {
      await getCurrentWebview().setZoom(value);
      root.style.fontSize = '';
      return;
    } catch {
      // Permission missing or the call refused: fall through to CSS.
    }
  }
  root.style.fontSize = value === 1 ? '' : `${16 * value}px`;
}
