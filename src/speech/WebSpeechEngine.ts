import type {
  SpeechEngine,
  SpeechEngineObserver,
  SpeechUtterance,
  SpeechViseme,
} from './SpeechTypes';

const VISEMES: readonly SpeechViseme[] = ['a', 'i', 'u', 'e', 'o'];
const FRAME_INTERVAL_MS = 75;

function clamp(value: number | undefined, fallback: number, minimum: number, maximum: number) {
  return Math.min(Math.max(value ?? fallback, minimum), maximum);
}

export class WebSpeechEngine implements SpeechEngine {
  speak(
    utterance: SpeechUtterance,
    observer: SpeechEngineObserver,
    signal: AbortSignal,
  ): Promise<void> {
    if (!('speechSynthesis' in window) || !('SpeechSynthesisUtterance' in window)) {
      return Promise.reject(new Error('当前 WebView 不支持系统语音合成'));
    }

    return new Promise((resolve, reject) => {
      if (signal.aborted) {
        resolve();
        return;
      }

      const nativeUtterance = new SpeechSynthesisUtterance(utterance.text);
      nativeUtterance.lang = utterance.language ?? 'zh-CN';
      nativeUtterance.rate = clamp(utterance.rate, 1, 0.5, 2);
      nativeUtterance.pitch = clamp(utterance.pitch, 1, 0, 2);
      nativeUtterance.volume = clamp(utterance.volume, 1, 0, 1);
      nativeUtterance.voice = utterance.voiceId
        ? window.speechSynthesis
            .getVoices()
            .find(
              (voice) =>
                voice.voiceURI === utterance.voiceId || voice.name === utterance.voiceId,
            ) ?? null
        : null;

      let settled = false;
      let frameTimer: number | undefined;
      let frameIndex = 0;

      const cleanup = (): void => {
        if (frameTimer !== undefined) window.clearInterval(frameTimer);
        signal.removeEventListener('abort', onAbort);
      };
      const finish = (callback: () => void): void => {
        if (settled) return;
        settled = true;
        cleanup();
        callback();
      };
      const onAbort = (): void => {
        window.speechSynthesis.cancel();
        finish(resolve);
      };

      nativeUtterance.onstart = () => {
        observer.onStarted();
        frameTimer = window.setInterval(() => {
          const phase = frameIndex++;
          observer.onFrame({
            level: 0.3 + (Math.sin(phase * 1.7) + 1) * 0.22,
            viseme: VISEMES[phase % VISEMES.length],
          });
        }, FRAME_INTERVAL_MS);
      };
      nativeUtterance.onend = () => finish(resolve);
      nativeUtterance.onerror = (event) => {
        if (signal.aborted || event.error === 'canceled' || event.error === 'interrupted') {
          finish(resolve);
        } else {
          finish(() => reject(new Error('系统语音合成失败: ' + event.error)));
        }
      };

      signal.addEventListener('abort', onAbort, { once: true });
      window.speechSynthesis.speak(nativeUtterance);
    });
  }
}
