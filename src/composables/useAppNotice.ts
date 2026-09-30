import { ref } from 'vue';

/**
 * One short message from anywhere in the app, shown by `AppNotice` in the
 * shell's toast stack.
 *
 * For what used to be `alert()`, an OS `message()` dialog, or nothing at all
 * — a delete that failed after its undo window closed was the case that made
 * this necessary: the item came back and nobody said why. An error stays until
 * dismissed; anything else leaves by itself.
 */
export interface AppNoticeAction {
  label: string;
  run: () => void;
}

export interface AppNotice {
  id: number;
  text: string;
  kind: 'info' | 'error';
  /**
   * Buttons under the message, for a notice that offers a way forward rather
   * than only reporting. A notice with actions stays until it is answered or
   * closed: a button that leaves after five seconds is one a slow reader never
   * gets to press.
   */
  actions?: AppNoticeAction[];
}

export const appNotices = ref<AppNotice[]>([]);
let serial = 0;

export function showAppNotice(text: string, kind: AppNotice['kind'] = 'info', actions?: AppNoticeAction[]) {
  const id = ++serial;
  appNotices.value = [...appNotices.value, { id, text, kind, actions }].slice(-3);
  if (kind === 'info' && !actions?.length) setTimeout(() => dismissAppNotice(id), 5000);
  return id;
}

export function dismissAppNotice(id: number) {
  appNotices.value = appNotices.value.filter(n => n.id !== id);
}
