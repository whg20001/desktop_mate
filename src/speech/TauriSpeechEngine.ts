import { z } from 'zod';
import { audioError, audioEventSchema, audioStatusSchema, audioTransport, type AudioKey, type AudioTransport } from './AudioIpc';
import type { SpeechEngine, SpeechEngineObserver, SpeechUtterance } from './SpeechTypes';

interface Active {
  key?: AudioKey;
  done: boolean;
  eventSequence: number;
  finish(error?: unknown): void;
  receive(payload: unknown): void;
}

// Owns IPC subscriptions only. Rust owns the audio device and serializes playback.
export class TauriSpeechEngine implements SpeechEngine {
  private connection?: Promise<number>;
  private unlisten?: () => void;
  private sequence = 0;
  private active?: Active;
  private disposed = false;

  constructor(private readonly transport: AudioTransport = audioTransport) {}

  speak(utterance: SpeechUtterance, observer: SpeechEngineObserver, signal: AbortSignal): Promise<void> {
    if (this.disposed) return Promise.reject(new Error('语音引擎已关闭'));
    if (signal.aborted) return Promise.resolve();
    if (this.active) { this.cancel(this.active); this.active.finish(); }
    const sequence = ++this.sequence;
    return new Promise<void>((resolve, reject) => {
      let polling = false;
      let timer: ReturnType<typeof setInterval> | undefined;
      let timeout: ReturnType<typeof setTimeout> | undefined;
      const onAbort = (): void => { this.cancel(active); active.finish(); };
      const active: Active = {
        done: false,
        eventSequence: 0,
        finish: (error) => {
          if (active.done) return;
          active.done = true;
          signal.removeEventListener('abort', onAbort);
          if (timer) clearInterval(timer);
          if (timeout) clearTimeout(timeout);
          if (this.active === active) this.active = undefined;
          if (error !== undefined) reject(audioError(error)); else resolve();
        },
        receive: (payload) => {
          if (active.done || !active.key) return;
          const parsed = audioEventSchema.safeParse(payload);
          if (!parsed.success) { this.cancel(active); active.finish(new Error('音频事件格式无效')); return; }
          const event = parsed.data;
          const key = active.key;
          if (event.sessionId !== key.sessionId || event.generation !== key.generation || event.sequence !== key.sequence || event.eventSequence <= active.eventSequence) return;
          active.eventSequence = event.eventSequence;
          switch (event.type) {
            case 'started': observer.onStarted(); break;
            case 'frame': observer.onFrame({ level: event.level }); break;
            case 'completed': case 'cancelled': active.finish(); break;
            case 'failed': active.finish(event.error ?? new Error('原生语音播放失败')); break;
          }
        },
      };
      this.active = active;
      signal.addEventListener('abort', onAbort, { once: true });
      timeout = setTimeout(() => { this.cancel(active); active.finish(new Error('语音会话超时')); }, 305_000);
      void this.connect().then(async (generation) => {
        if (active.done || this.disposed) return;
        active.key = { sessionId: crypto.randomUUID(), generation, sequence };
        // Poll only during a request: recover a missed terminal event and detect a lost connection.
        timer = setInterval(() => {
          if (polling || active.done) return;
          polling = true;
          void this.call('get_audio_status').then((value) => {
            if (active.done) return;
            const status = audioStatusSchema.parse(value);
            if (!status.workerAlive || status.generation !== generation) throw new Error('音频会话连接已失效');
            if (status.lastEvent) active.receive(status.lastEvent);
          }).catch((error: unknown) => { this.cancel(active); active.finish(error); this.connection = undefined; })
            .finally(() => { polling = false; });
        }, 1000);
        await this.call('start_audio', {
          request: {
            key: active.key,
            speech: { text: utterance.text, voiceId: utterance.voiceId ?? null, language: utterance.language ?? null, rate: utterance.rate ?? null, pitch: utterance.pitch ?? null, volume: utterance.volume ?? null },
            timeoutSeconds: 180,
          },
        });
      }).catch((error: unknown) => {
        if (!active.done) this.connection = undefined;
        this.cancel(active); active.finish(error);
      });
    });
  }

  dispose(): void {
    this.disposed = true;
    if (this.active) { this.cancel(this.active); this.active.finish(); }
    this.unlisten?.(); this.unlisten = undefined;
  }

  private cancel(active: Active): void {
    if (active.key) void this.call('cancel_audio', { key: active.key }).catch(() => { this.connection = undefined; });
  }

  private connect(): Promise<number> {
    if (!this.connection) {
      this.connection = (async () => {
        this.unlisten?.();
        const unlisten = await this.transport.listen((payload) => this.active?.receive(payload));
        if (this.disposed) { unlisten(); throw new Error('语音引擎已关闭'); }
        this.unlisten = unlisten;
        return z.number().int().positive().parse(await this.call('open_audio_session'));
      })().catch((error: unknown) => { this.connection = undefined; throw error; });
    }
    return this.connection;
  }

  private call(command: string, args?: Record<string, unknown>): Promise<unknown> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('音频服务连接超时')), 5000);
      this.transport.invoke(command, args).then(resolve, reject).finally(() => clearTimeout(timer));
    });
  }
}
