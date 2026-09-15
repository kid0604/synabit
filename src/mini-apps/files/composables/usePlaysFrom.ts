import { onBeforeUnmount, watch, type Ref } from 'vue';

/**
 * Start a recording or video at a moment, and pause once at the end of it.
 *
 * This is where a `#t=192,230` citation lands (`src/shared/mediaTime.ts`). The
 * seek waits for the element's metadata, because setting `currentTime` before
 * the duration is known is ignored by WebKit.
 */
export function usePlaysFrom(
  element: Ref<HTMLMediaElement | null>,
  start: () => number | undefined,
  end: () => number | undefined,
) {
  let stopAt: number | undefined;

  const seek = () => {
    const el = element.value;
    const at = start();
    if (!el || at === undefined) return;
    el.currentTime = at;
    stopAt = end();
  };

  const onTime = () => {
    const el = element.value;
    if (el && stopAt !== undefined && el.currentTime >= stopAt) {
      stopAt = undefined;
      el.pause();
    }
  };

  watch(
    [element, start],
    ([el]) => {
      if (!el) return;
      el.addEventListener('timeupdate', onTime);
      if (el.readyState >= 1) seek();
      else el.addEventListener('loadedmetadata', seek, { once: true });
    },
    { immediate: true },
  );

  onBeforeUnmount(() => element.value?.removeEventListener('timeupdate', onTime));
}
