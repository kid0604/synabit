/**
 * The app's side of a Syn visual's frame: what it answers, and what it does
 * with what the frame says.
 *
 * The frame is `synvisual://…/v/<id>`, sandboxed to `allow-scripts`, and its
 * kit (`src-tauri/src/syn/visual/kit.js`) speaks JSON-RPC 2.0 with the MCP Apps
 * message names. This answers four things and ignores everything else:
 *
 * - `ui/initialize` — with the app's theme and locale, which the visual uses
 *   rather than what its URL said.
 * - `ui/notifications/size-changed` — the frame is resized to it, capped.
 * - `ui/message` — a question the visual asks back. It goes into the chat box
 *   for the person to send; it is never sent from here (plan §7, decision 2):
 *   a visual drawn after reading a page could otherwise talk to Syn by itself.
 * - `synabit/error` — shown under the frame, so a broken visual says so.
 *
 * Nothing is ever sent *into* the frame but the theme and the locale. The
 * frame is the one thing in the app that runs code a model wrote, and on
 * macOS the invoke key is what keeps it from the app's commands (plan §8); a
 * host that sent it anything more would be a host that could send that.
 */

/** Where a custom scheme lives: its own scheme everywhere but Windows. */
export function visualOrigin(userAgent: string): string {
  return /Windows/i.test(userAgent) ? 'http://synvisual.localhost' : 'synvisual://localhost';
}

export type VisualTheme = 'light' | 'dark';

/** The frame's address for one kept visual. */
export function visualSrc(origin: string, id: string, theme: VisualTheme): string {
  return `${origin}/v/${encodeURIComponent(id)}?theme=${theme}`;
}

/** The locale a visual writes numbers in, from the app's language. */
export function visualLocale(appLocale: string): string {
  return appLocale.toLowerCase().startsWith('en') ? 'en-US' : 'vi-VN';
}

/** How tall a frame may grow, folded and expanded. Past that it scrolls. */
export const HEIGHT_CAP = 900;
export const EXPANDED_CAP = 2400;
export const MIN_HEIGHT = 60;

export function frameHeight(reported: number, expanded: boolean): number {
  const cap = expanded ? EXPANDED_CAP : HEIGHT_CAP;
  if (!Number.isFinite(reported)) return MIN_HEIGHT;
  return Math.max(MIN_HEIGHT, Math.min(cap, Math.ceil(reported)));
}

export interface HostContext {
  theme: VisualTheme;
  locale: string;
}

/** What the host does with what the frame said. */
export interface HostEvents {
  height(height: number): void;
  ask(text: string): void;
  error(error: { kind: string; message: string }): void;
  initialized?(): void;
}

interface Message {
  jsonrpc?: string;
  id?: unknown;
  method?: string;
  params?: Record<string, unknown>;
}

/**
 * One message from the frame: the reply to send back, if it was a request
 * this host answers, or `null`. Anything that is not JSON-RPC 2.0, or a method
 * this host does not know, is ignored without an answer.
 */
export function answer(data: unknown, context: HostContext, events: HostEvents): Record<string, unknown> | null {
  const message = data as Message | null;
  if (!message || typeof message !== 'object' || message.jsonrpc !== '2.0' || typeof message.method !== 'string') return null;
  const reply = (result: unknown) => (message.id == null ? null : { jsonrpc: '2.0', id: message.id, result });
  switch (message.method) {
    case 'ui/initialize':
      return reply({
        protocolVersion: '2026-01-26',
        hostInfo: { name: 'synabit', version: '1' },
        hostCapabilities: {},
        hostContext: { theme: context.theme, locale: context.locale },
      });
    case 'ui/notifications/initialized':
      events.initialized?.();
      return null;
    case 'ui/notifications/size-changed':
      events.height(Number(message.params?.height));
      return null;
    case 'ui/message': {
      const content = message.params?.content as { type?: string; text?: unknown } | undefined;
      const text = typeof content?.text === 'string' ? content.text.trim().slice(0, 2000) : '';
      if (text) events.ask(text);
      return reply({});
    }
    case 'synabit/error':
      events.error({
        kind: String(message.params?.kind ?? 'error').slice(0, 40),
        message: String(message.params?.message ?? '').slice(0, 500),
      });
      return null;
    default:
      return null;
  }
}

/** What the host sends when the theme or the locale changes. */
export function contextChanged(context: HostContext): Record<string, unknown> {
  return { jsonrpc: '2.0', method: 'ui/notifications/host-context-changed', params: { theme: context.theme, locale: context.locale } };
}

/** The visuals an answer's tool log shows, oldest first: one per `show_visual` that kept one. */
export function visualsIn(toolCalls: { tool_name: string; tool_args?: unknown }[] | undefined): string[] {
  const ids: string[] = [];
  for (const call of toolCalls ?? []) {
    if (call.tool_name !== 'show_visual') continue;
    const id = (call.tool_args as { visual_id?: unknown } | undefined)?.visual_id;
    if (typeof id === 'string' && /^[0-9a-f]{32}$/.test(id) && !ids.includes(id)) ids.push(id);
  }
  return ids;
}
