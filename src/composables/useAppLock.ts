import { onMounted, onUnmounted, type Ref } from 'vue';
import { useAppLockStore } from '../stores/useAppLockStore';

export function useAppLock(currentAppId?: Ref<string | null>) {
  const store = useAppLockStore();
  let idleCheckInterval: ReturnType<typeof setInterval> | null = null;

  const activityHandler = () => {
    store.resetActivity();
    // Refresh session for current protected app (so it doesn't expire while user is on it)
    if (currentAppId?.value && store.isAppProtected(currentAppId.value)) {
      store.touchMiniAppSession(currentAppId.value);
    }
  };

  function startActivityMonitor() {
    const events = ['mousemove', 'keydown', 'click', 'scroll', 'touchstart'];
    events.forEach(evt =>
      window.addEventListener(evt, activityHandler, { passive: true })
    );

    // Check every 10 seconds
    idleCheckInterval = setInterval(() => {
      if (!store.isEnabled || store.isAppLocked) return;
      if (store.autoLockTimeoutSecs <= 0) return;

      // Tier 1: global idle check
      if (store.appLockActive) {
        const elapsed = (Date.now() - store.lastActivityTime) / 1000;
        if (elapsed >= store.autoLockTimeoutSecs) {
          store.lock();
        }
      }
    }, 10000);
  }

  function stopActivityMonitor() {
    const events = ['mousemove', 'keydown', 'click', 'scroll', 'touchstart'];
    events.forEach(evt =>
      window.removeEventListener(evt, activityHandler)
    );
    if (idleCheckInterval) {
      clearInterval(idleCheckInterval);
      idleCheckInterval = null;
    }
  }

  onMounted(() => {
    startActivityMonitor();
  });

  onUnmounted(() => {
    stopActivityMonitor();
  });

  return {
    startActivityMonitor,
    stopActivityMonitor,
  };
}
