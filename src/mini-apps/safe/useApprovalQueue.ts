/**
 * The cards something outside the app raises for a yes — the SSH agent, the
 * command line — queued, and kept honest: a card whose question already timed
 * out in Rust (a no) is taken down, and every card goes when the Safe locks,
 * since Rust has turned every open question into a no by then.
 */
import { onMounted, onUnmounted, ref, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { LOCKED_EVENT } from './api';

export interface Asked {
  id: string;
  timeout_secs?: number;
}

export function useApprovalQueue<T extends Asked>(event: string) {
  const queue = ref([]) as Ref<(T & { expires: number })[]>;
  let timer: ReturnType<typeof setInterval> | undefined;
  const unlisteners: UnlistenFn[] = [];

  function prune() {
    const now = Date.now();
    queue.value = queue.value.filter((a) => a.expires > now);
  }

  async function answer(allow: boolean) {
    const ask = queue.value.shift();
    if (ask) await invoke('safe_ssh_answer', { id: ask.id, allow }).catch(() => undefined);
  }

  onMounted(async () => {
    unlisteners.push(
      await listen<T>(event, (e) => {
        const secs = e.payload.timeout_secs ?? 60;
        queue.value = [...queue.value, { ...e.payload, expires: Date.now() + secs * 1000 }];
      }),
      await listen(LOCKED_EVENT, () => (queue.value = [])),
    );
    timer = setInterval(prune, 1000);
  });
  onUnmounted(() => {
    unlisteners.forEach((u) => u());
    clearInterval(timer);
  });

  return { queue, answer };
}
