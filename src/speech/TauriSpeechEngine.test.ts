import { afterEach, describe, expect, it, vi } from 'vitest';
import { TauriSpeechEngine } from './TauriSpeechEngine';
import { resolveVoice, type AudioTransport } from './AudioIpc';

function harness() {
  let receive: (event: unknown) => void = () => {};
  const unlisten = vi.fn();
  const invoke = vi.fn(async (command: string): Promise<unknown> => command === 'open_audio_session' ? 1 : null);
  const transport: AudioTransport = {
    invoke: invoke as AudioTransport['invoke'],
    listen: async (listener) => { receive = listener; return unlisten; },
  };
  const engine = new TauriSpeechEngine(transport);
  const observer = { onStarted: vi.fn(), onFrame: vi.fn() };
  const start = async () => {
    const abort = new AbortController();
    const done = engine.speak({ id: 'speech-1', text: 'hello', source: 'ai' }, observer, abort.signal);
    await vi.waitFor(() => expect(invoke.mock.calls.some(([name]) => name === 'start_audio')).toBe(true));
    const call = invoke.mock.calls.find(([name]) => name === 'start_audio') as unknown as [string, { request: { key: Record<string, unknown> } }];
    const key = call[1].request.key;
    return { abort, done, key };
  };
  const event = (key: Record<string, unknown>, type: string, eventSequence: number, extra = {}) => receive({ ...key, type, eventSequence, positionMs: 20, level: 0.4, ...extra });
  return { engine, invoke, observer, unlisten, start, event };
}
afterEach(() => vi.useRealTimers());

describe('TauriSpeechEngine', () => {
  it('waits for terminal playback and ignores stale or reordered frames', async () => {
    const h = harness(); const { done, key } = await h.start();
    let completed = false; void done.then(() => { completed = true; });
    await Promise.resolve(); expect(completed).toBe(false);
    h.event(key, 'started', 1); h.event(key, 'frame', 3);
    h.event(key, 'frame', 2); h.event({ ...key, generation: 2 }, 'frame', 4);
    expect(h.observer.onStarted).toHaveBeenCalledOnce(); expect(h.observer.onFrame).toHaveBeenCalledOnce();
    h.event(key, 'completed', 4); await done;
    h.event(key, 'frame', 5); expect(h.observer.onFrame).toHaveBeenCalledOnce(); h.engine.dispose(); expect(h.unlisten).toHaveBeenCalledOnce();
  });
  it('sends scoped cancellation and releases a pending request immediately', async () => {
    const h = harness(); const { abort, done, key } = await h.start();
    abort.abort(); await done;
    expect(h.invoke).toHaveBeenCalledWith('cancel_audio', { key });
    h.event(key, 'frame', 3); expect(h.observer.onFrame).not.toHaveBeenCalled(); h.engine.dispose();
  });
  it('does not launch speech cancelled while the connection is opening', async () => {
    const h = harness(); const abort = new AbortController();
    const done = h.engine.speak({ id: '1', text: 'hello', source: 'ai' }, h.observer, abort.signal);
    abort.abort(); await done; await Promise.resolve(); await Promise.resolve();
    expect(h.invoke.mock.calls.some(([name]) => name === 'start_audio')).toBe(false); h.engine.dispose();
  });
  it('propagates native errors and releases subscriptions on disposal', async () => {
    const h = harness(); const { done, key } = await h.start();
    const result = expect(done).rejects.toThrow('没有输出设备');
    h.event(key, 'failed', 1, { error: { code: 'output_unavailable', message: '没有输出设备' } });
    await result; h.engine.dispose(); expect(h.unlisten).toHaveBeenCalledOnce();
  });
  it('reconnects after a rejected stale session instead of failing every later request', async () => {
    const h = harness();
    let generation = 0;
    h.invoke.mockImplementation(async (command) => {
      if (command === 'open_audio_session') return ++generation;
      if (command === 'start_audio') throw { code: 'stale_session', message: '会话已过期' };
      return false;
    });
    for (let index = 0; index < 2; index++) {
      await expect(h.engine.speak({ id: String(index), text: 'hello', source: 'ai' }, h.observer, new AbortController().signal)).rejects.toThrow('会话已过期');
    }
    expect(generation).toBe(2); h.engine.dispose();
  });
  it('recovers a missed completion through status polling', async () => {
    const h = harness(); const { done, key } = await h.start();
    h.invoke.mockImplementation(async (command) => command === 'get_audio_status' ? {
      generation: 1, workerAlive: true,
      lastEvent: { ...key, type: 'completed', eventSequence: 2, positionMs: 10, level: 0 },
    } : null);
    await done; expect(h.invoke).toHaveBeenCalledWith('get_audio_status', undefined); h.engine.dispose();
  });
});

describe('native voice migration', () => {
  const voices = [{ id: 'native-1', name: '中文音色', language: 'zh-CN', isDefault: false }];
  it('keeps native IDs and matches legacy names or language without claiming compatibility', () => {
    expect(resolveVoice(voices, 'native-1', 'en-US')).toEqual({ id: 'native-1', migrated: false });
    expect(resolveVoice(voices, '中文音色', 'zh-CN')).toEqual({ id: 'native-1', migrated: true });
    expect(resolveVoice(voices, 'web-uri', 'zh-CN')).toEqual({ id: 'native-1', migrated: true });
    expect(resolveVoice(voices, 'web-uri', 'en-US')).toEqual({ id: '', migrated: true });
  });
});

it('selects a multilingual model speaker independently of the text language', () => {
  const voices = [
    { id: 'vivian', name: 'vivian', language: '', isDefault: false },
    { id: 'serena', name: 'serena', language: '', isDefault: true },
  ];
  expect(resolveVoice(voices, 'windows-voice', 'zh-CN')).toEqual({ id: 'serena', migrated: true });
  expect(resolveVoice(voices, 'vivian', 'en-US')).toEqual({ id: 'vivian', migrated: false });
});
