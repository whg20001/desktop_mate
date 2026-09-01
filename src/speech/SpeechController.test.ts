import { describe, expect, it } from 'vitest';
import { SpeechController } from './SpeechController';
import type {
  SpeechControllerEvent,
  SpeechEngine,
  SpeechEngineObserver,
  SpeechMotionFrame,
  SpeechMotionTarget,
  SpeechUtterance,
} from './SpeechTypes';

class MotionStub implements SpeechMotionTarget {
  readonly frames: SpeechMotionFrame[] = [];

  setSpeechFrame(frame: SpeechMotionFrame): void {
    this.frames.push(frame);
  }
}

class ImmediateEngine implements SpeechEngine {
  utterance?: SpeechUtterance;

  async speak(
    utterance: SpeechUtterance,
    observer: SpeechEngineObserver,
  ): Promise<void> {
    this.utterance = utterance;
    observer.onStarted();
    observer.onFrame({ level: 1.4, viseme: 'i' });
  }
}

class PendingEngine implements SpeechEngine {
  speak(
    _utterance: SpeechUtterance,
    observer: SpeechEngineObserver,
    signal: AbortSignal,
  ): Promise<void> {
    observer.onStarted();
    return new Promise((resolve) => {
      signal.addEventListener('abort', () => resolve(), { once: true });
    });
  }
}

class FailingEngine implements SpeechEngine {
  async speak(
    _utterance: SpeechUtterance,
    observer: SpeechEngineObserver,
  ): Promise<void> {
    observer.onStarted();
    throw new Error('provider offline');
  }
}

describe('SpeechController', () => {
  it('routes an AI utterance through lifecycle events and normalized motion frames', async () => {
    const engine = new ImmediateEngine();
    const motion = new MotionStub();
    const controller = new SpeechController(engine, motion);
    const events: SpeechControllerEvent[] = [];
    controller.onEvent((event) => events.push(event));

    await controller.speak({ text: ' 你好 ', source: 'ai' });

    expect(engine.utterance).toMatchObject({ text: '你好', source: 'ai', id: 'speech-1' });
    expect(events.map((event) => event.type)).toEqual([
      'preparing',
      'started',
      'completed',
    ]);
    expect(motion.frames).toContainEqual({ active: true, level: 1, viseme: 'i' });
    expect(motion.frames.at(-1)).toEqual({ active: false, level: 0 });
  });

  it('cancels the active utterance and releases the motion target', async () => {
    const motion = new MotionStub();
    const controller = new SpeechController(new PendingEngine(), motion);
    const events: SpeechControllerEvent[] = [];
    controller.onEvent((event) => events.push(event));

    const speaking = controller.speak({ text: '较长的一句话', source: 'interaction' });
    controller.cancel();
    await speaking;

    expect(events.map((event) => event.type)).toEqual([
      'preparing',
      'started',
      'cancelled',
    ]);
    expect(motion.frames.at(-1)).toEqual({ active: false, level: 0 });
  });

  it('rejects empty speech before replacing an active request', async () => {
    const controller = new SpeechController(new ImmediateEngine(), new MotionStub());
    await expect(controller.speak({ text: '   ', source: 'system' })).rejects.toThrow(
      '语音文本不能为空',
    );
  });

  it('releases the mouth and propagates provider errors', async () => {
    const motion = new MotionStub();
    const controller = new SpeechController(new FailingEngine(), motion);
    const events: SpeechControllerEvent[] = [];
    controller.onEvent((event) => events.push(event));

    await expect(
      controller.speak({ text: '测试', source: 'system' }),
    ).rejects.toThrow('provider offline');

    expect(events.map((event) => event.type)).toEqual([
      'preparing',
      'started',
      'failed',
    ]);
    expect(motion.frames.at(-1)).toEqual({ active: false, level: 0 });
  });
});
