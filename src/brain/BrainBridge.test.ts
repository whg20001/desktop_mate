import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { BrainBridge } from './BrainBridge';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const scope = { userId: 'u', characterId: 'c', sessionId: 's' };

function turnIds(): string[] {
  return vi.mocked(invoke).mock.calls.map(([, args]) =>
    (args as { request: { turnId: string } }).request.turnId,
  );
}

describe('BrainBridge conversation retry', () => {
  beforeEach(() => { vi.mocked(invoke).mockReset(); });

  it('reuses a failed turn but gives the next successful message a new ID', async () => {
    const brain = new BrainBridge();
    vi.mocked(invoke).mockRejectedValueOnce(new Error('response lost'));
    await expect(brain.converse('你好', scope, [])).rejects.toThrow('response lost');
    vi.mocked(invoke).mockResolvedValue({ turnId: turnIds()[0], text: '你好' });
    await brain.converse('你好', scope, []);
    await brain.converse('你好', scope, []);
    const ids = turnIds();
    expect(ids[1]).toBe(ids[0]);
    expect(ids[2]).not.toBe(ids[0]);
  });

  it('starts a new turn when the draft or conversation scope changes', async () => {
    const brain = new BrainBridge();
    vi.mocked(invoke).mockRejectedValue(new Error('offline'));
    await expect(brain.converse('first', scope, [])).rejects.toThrow();
    await expect(brain.converse('edited', scope, [])).rejects.toThrow();
    await expect(brain.converse('edited', { ...scope, characterId: 'other' }, [])).rejects.toThrow();
    expect(new Set(turnIds()).size).toBe(3);
  });

  it('retains the turn ID when response validation fails', async () => {
    const brain = new BrainBridge();
    vi.mocked(invoke).mockResolvedValueOnce({});
    await expect(brain.converse('你好', scope, [])).rejects.toThrow();
    vi.mocked(invoke).mockResolvedValue({ turnId: turnIds()[0], text: '你好' });
    await brain.converse('你好', scope, []);
    expect(turnIds()[1]).toBe(turnIds()[0]);
  });
});
