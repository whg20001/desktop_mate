import type {
  SpeechControllerEvent,
  SpeechControllerListener,
  SpeechEngine,
  SpeechEngineFrame,
  SpeechMotionTarget,
  SpeechRequest,
  SpeechUtterance,
} from './SpeechTypes';

interface ActiveSpeech {
  utterance: SpeechUtterance;
  abort: AbortController;
  started: boolean;
}

const SILENT_FRAME = { active: false, level: 0 } as const;

export class SpeechController {
  private readonly listeners = new Set<SpeechControllerListener>();
  private active?: ActiveSpeech;
  private sequence = 0;

  constructor(
    private readonly engine: SpeechEngine,
    private readonly motion: SpeechMotionTarget,
  ) {}

  onEvent(listener: SpeechControllerListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async speak(request: SpeechRequest): Promise<void> {
    const text = request.text.trim();
    if (!text) throw new Error('语音文本不能为空');

    this.cancel();
    const active: ActiveSpeech = {
      utterance: { ...request, text, id: 'speech-' + ++this.sequence },
      abort: new AbortController(),
      started: false,
    };
    this.active = active;
    this.emit({ type: 'preparing', utterance: active.utterance });

    try {
      await this.engine.speak(
        active.utterance,
        {
          onStarted: () => this.start(active),
          onFrame: (frame) => this.applyFrame(active, frame),
        },
        active.abort.signal,
      );
      if (this.active !== active) return;
      this.motion.setSpeechFrame(SILENT_FRAME);
      this.active = undefined;
      this.emit({ type: 'completed', utterance: active.utterance });
    } catch (error: unknown) {
      if (this.active !== active) return;
      this.motion.setSpeechFrame(SILENT_FRAME);
      this.active = undefined;
      const failure = error instanceof Error ? error : new Error('语音播放失败');
      this.emit({
        type: 'failed',
        utterance: active.utterance,
        error: failure,
      });
      throw failure;
    }
  }

  cancel(): void {
    const active = this.active;
    if (!active) return;
    this.active = undefined;
    active.abort.abort();
    this.motion.setSpeechFrame(SILENT_FRAME);
    this.emit({ type: 'cancelled', utterance: active.utterance });
  }

  dispose(): void {
    this.cancel();
    this.engine.dispose?.();
    this.listeners.clear();
  }

  private start(active: ActiveSpeech): void {
    if (this.active !== active || active.started) return;
    active.started = true;
    this.motion.setSpeechFrame({ active: true, level: 0 });
    this.emit({ type: 'started', utterance: active.utterance });
  }

  private applyFrame(active: ActiveSpeech, frame: SpeechEngineFrame): void {
    if (this.active !== active) return;
    this.start(active);
    this.motion.setSpeechFrame({
      active: true,
      level: Math.min(Math.max(frame.level, 0), 1),
      ...(frame.viseme ? { viseme: frame.viseme } : {}),
    });
  }

  private emit(event: SpeechControllerEvent): void {
    this.listeners.forEach((listener) => listener(event));
  }
}
