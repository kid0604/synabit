import { ref } from 'vue';

import { logger } from '../../utils/logger';

/**
 * Recording a voice note, using the recorder the webview already has.
 *
 * The plan put transcription in a later phase and recording in this one, and
 * the split is the right way round: turning speech into text is the expensive
 * part, and it is not what makes voice capture worth having. What makes it
 * worth having is that a thought arriving while walking, driving or holding
 * something can be kept at all — a recording nobody has transcribed is still
 * the note, and it is still searchable by when it was taken and what sits
 * around it in the cap.
 *
 * `MediaRecorder` means no native recorder on three platforms and no audio
 * dependency in the bundle. Tauri's Android WebView already answers the
 * permission request by asking for RECORD_AUDIO, and macOS needs only the
 * usage string in `Info.plist`.
 */

/** What the recorder is doing, for a button that has to say so. */
export type RecordingState = 'idle' | 'requesting' | 'recording';

export interface Recording {
  bytes: Uint8Array;
  /** Taken from the recorder, not assumed: Chromium gives webm, Safari mp4. */
  extension: string;
}

/** The container the platform actually produced, as a file extension. */
function extensionFor(mimeType: string): string {
  const type = mimeType.split(';')[0]?.trim().toLowerCase() ?? '';
  switch (type) {
    case 'audio/webm':
      return 'webm';
    case 'audio/mp4':
    case 'audio/x-m4a':
      return 'm4a';
    case 'audio/ogg':
      return 'ogg';
    case 'audio/mpeg':
      return 'mp3';
    case 'audio/wav':
    case 'audio/x-wav':
      return 'wav';
    default:
      // Storing unknown bytes under a name that claims a format would be
      // worse than admitting the format is unknown.
      return 'bin';
  }
}

export function useAudioCapture() {
  const state = ref<RecordingState>('idle');
  const durationMs = ref(0);

  let recorder: MediaRecorder | null = null;
  let stream: MediaStream | null = null;
  let chunks: Blob[] = [];
  let startedAt = 0;
  let ticker: ReturnType<typeof setInterval> | null = null;

  /** Whether this build can record at all. */
  const isSupported = () =>
    typeof MediaRecorder !== 'undefined' &&
    typeof navigator !== 'undefined' &&
    Boolean(navigator.mediaDevices?.getUserMedia);

  const releaseMicrophone = () => {
    // Every track, explicitly. A stream left open keeps the recording
    // indicator lit in the system UI, which reads as the app listening after
    // the user stopped it.
    stream?.getTracks().forEach((track) => track.stop());
    stream = null;
    if (ticker) {
      clearInterval(ticker);
      ticker = null;
    }
  };

  async function start(): Promise<boolean> {
    if (state.value !== 'idle' || !isSupported()) return false;

    state.value = 'requesting';
    try {
      stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch (e) {
      // Refused, or no microphone. Either way there is nothing to say beyond
      // returning to a button the user can press again.
      logger.error('Microphone unavailable', e);
      state.value = 'idle';
      return false;
    }

    chunks = [];
    recorder = new MediaRecorder(stream);
    recorder.ondataavailable = (event) => {
      if (event.data.size > 0) chunks.push(event.data);
    };
    recorder.start();

    startedAt = Date.now();
    durationMs.value = 0;
    ticker = setInterval(() => {
      durationMs.value = Date.now() - startedAt;
    }, 200);

    state.value = 'recording';
    return true;
  }

  /** Stop and hand back the audio, or `null` if nothing was captured. */
  async function stop(): Promise<Recording | null> {
    if (state.value !== 'recording' || !recorder) {
      releaseMicrophone();
      state.value = 'idle';
      return null;
    }

    const active = recorder;
    const mimeType = active.mimeType || 'audio/webm';

    const finished = new Promise<void>((resolve) => {
      active.onstop = () => resolve();
    });
    active.stop();
    await finished;

    releaseMicrophone();
    recorder = null;
    state.value = 'idle';

    if (chunks.length === 0) return null;

    const blob = new Blob(chunks, { type: mimeType });
    chunks = [];
    if (blob.size === 0) return null;

    return {
      bytes: new Uint8Array(await blob.arrayBuffer()),
      extension: extensionFor(mimeType),
    };
  }

  /** Throw the recording away — the microphone must not stay open. */
  function cancel() {
    if (recorder && state.value === 'recording') {
      recorder.onstop = null;
      recorder.stop();
    }
    recorder = null;
    chunks = [];
    releaseMicrophone();
    state.value = 'idle';
    durationMs.value = 0;
  }

  return { state, durationMs, isSupported, start, stop, cancel };
}

/** `mm:ss`, which is the only resolution a voice note needs. */
export function formatDuration(ms: number): string {
  const total = Math.floor(ms / 1000);
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
}
