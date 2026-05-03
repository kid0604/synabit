import { describe, it, expect, vi, beforeEach } from 'vitest';
import { usePlatform } from '../usePlatform';
import * as osPlugin from '@tauri-apps/plugin-os';

// Mock the Tauri OS plugin
vi.mock('@tauri-apps/plugin-os', () => ({
  type: vi.fn(),
  platform: vi.fn(),
  version: vi.fn()
}));

// Mock useWindowSize from vueuse
vi.mock('@vueuse/core', () => ({
  useWindowSize: vi.fn(() => ({ width: { value: 1024 } }))
}));

// Mock logger to avoid console noise
vi.mock('../../utils/logger', () => ({
  logger: {
    warn: vi.fn(),
    info: vi.fn()
  }
}));

describe('usePlatform', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('detects macOS correctly', async () => {
    vi.mocked(osPlugin.type).mockResolvedValue('macos');
    
    const { isMac, isWindows, isMobileOS, initOS } = usePlatform();
    await initOS(); // wait for async init
    
    expect(isMac.value).toBe(true);
    expect(isWindows.value).toBe(false);
    expect(isMobileOS.value).toBe(false);
  });

  it('detects Android correctly and triggers mobile layout', async () => {
    vi.mocked(osPlugin.type).mockResolvedValue('android');
    
    const { isMobileOS, useMobileLayout, initOS } = usePlatform();
    await initOS();
    
    expect(isMobileOS.value).toBe(true);
    expect(useMobileLayout.value).toBe(true);
  });
});
