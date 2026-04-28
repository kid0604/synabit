import { ref, computed } from 'vue';
import { type, platform, version } from '@tauri-apps/plugin-os';
import { useWindowSize } from '@vueuse/core';

export function usePlatform() {
  const osType = ref<string>('');
  const osPlatform = ref<string>('');
  const osVersion = ref<string>('');
  const isMobileOS = ref(false);

  const { width } = useWindowSize();

  // Screen size based detection (fallback/responsive)
  const isSmallScreen = computed(() => width.value < 768);

  // Initialize OS info
  const initOS = async () => {
    try {
      osType.value = await type();
      osPlatform.value = await platform();
      osVersion.value = await version();
      
      // tauri-plugin-os type() returns 'ios' or 'android' for mobile
      isMobileOS.value = ['ios', 'android'].includes(osType.value.toLowerCase());
    } catch (e) {
      console.warn("Failed to get OS info, falling back to screen size", e);
      // Fallback if not running in Tauri or plugin fails
      const ua = navigator.userAgent.toLowerCase();
      isMobileOS.value = /android|webos|iphone|ipad|ipod|blackberry|iemobile|opera mini/i.test(ua);
    }
  };

  // Run initialization
  initOS();

  // Determine if we should use mobile layout
  // Uses Mobile OS detection, but falls back to screen size if OS info is unavailable/desktop window is small
  const useMobileLayout = computed(() => isMobileOS.value || isSmallScreen.value);

  const isMac = computed(() => osType.value.toLowerCase() === 'macos' || osType.value.toLowerCase() === 'darwin');
  const isWindows = computed(() => osType.value.toLowerCase() === 'windows');
  const isLinux = computed(() => osType.value.toLowerCase() === 'linux');

  return {
    osType,
    osPlatform,
    osVersion,
    isMobileOS,
    isSmallScreen,
    useMobileLayout,
    isMac,
    isWindows,
    isLinux,
    initOS
  };
}
