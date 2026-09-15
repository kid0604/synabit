import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { nextTick } from 'vue';
import { parseTimeFragment, splitMediaLink, timeFragment, clock } from '../mediaTime';
import AudioFileViewer from '../../mini-apps/files/viewers/AudioFileViewer.vue';

vi.mock('@tauri-apps/api/core', () => ({ convertFileSrc: (path: string) => `asset://${path}` }));

describe('mediaTime', () => {
  it('reads the moment a citation points at', () => {
    expect(parseTimeFragment('t=192,230')).toEqual({ start: 192, end: 230 });
    expect(parseTimeFragment('#t=12.5')).toEqual({ start: 12.5 });
    expect(parseTimeFragment('t=30,10')).toEqual({ start: 30 });
    expect(parseTimeFragment('contract renewal')).toBeNull();
    expect(splitMediaLink('Files/9f3a.md#t=192,230')).toEqual({ id: 'Files/9f3a.md', fragment: { start: 192, end: 230 } });
    expect(timeFragment(192, 230.44)).toBe('t=192,230.4');
    expect(clock(192)).toBe('3:12');
    expect(clock(3792)).toBe('1:03:12');
  });
});

describe('AudioFileViewer', () => {
  /** Gate: a `#t=` citation opens the recording at that moment. */
  it('starts at the cited moment and stops at its end', async () => {
    const wrapper = mount(AudioFileViewer, { props: { filePath: '/vault/assets/memo.m4a', vaultPath: '/vault', initialTime: 192, endTime: 230 } });
    const audio = wrapper.find('audio').element as HTMLAudioElement;
    let now = 0;
    Object.defineProperty(audio, 'currentTime', { get: () => now, set: (value: number) => { now = value; }, configurable: true });
    const pause = vi.fn();
    audio.pause = pause;
    // The player learns of its element after mounting, as it would in the app.
    await nextTick();
    expect(now).toBe(0);

    audio.dispatchEvent(new Event('loadedmetadata'));
    expect(now).toBe(192);

    now = 229;
    audio.dispatchEvent(new Event('timeupdate'));
    expect(pause).not.toHaveBeenCalled();
    now = 230.2;
    audio.dispatchEvent(new Event('timeupdate'));
    audio.dispatchEvent(new Event('timeupdate'));
    expect(pause).toHaveBeenCalledTimes(1);
  });
});
