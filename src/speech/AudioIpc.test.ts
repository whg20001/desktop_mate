import { describe, expect, it } from 'vitest';
import { audioStatusSchema, formatAudioTimings } from './AudioIpc';

describe('audio timing diagnostics', () => {
  it('accepts legacy status and displays absence without inventing durations', () => {
    expect(audioStatusSchema.parse({generation: 1, workerAlive: true, lastEvent: null}).timings).toBeUndefined();
    expect(formatAudioTimings(null)).toContain('尚无语音请求');
  });
  it('distinguishes waiting first playback from measured stages and underrun', () => {
    const t = {sessionId: 'test', queueMs: 100, serviceQueueMs: 20, modelMs: 1250, codecDecodeMs: 30,
      firstChunkMs: 700, wavDecodeMs: 2, firstChunkReceivedMs: 820, firstPlaybackMs: null,
      synthesisDoneMs: null, totalMs: null, chunks: 2, underrunMs: 400,
      startupBufferMs: 2100, startupAudioMs: 2800, bufferTargetMs: 6000, generationRtf: 1.8, rebufferCount: 1};
    const status = audioStatusSchema.parse({generation:1, workerAlive:true, lastEvent:null, timings:t});
    const label = formatAudioTimings(status.timings!);
    expect(label).toContain('请求排队 0.12 秒');
    expect(label).toContain('首块播放 等待中');
    expect(label).toContain('缓冲欠载 0.40 秒');
    expect(label).toContain('2 个音频块');
    expect(label).toContain('启动缓冲等待 2.10 秒');
    expect(label).toContain('生成耗时/音频时长 1.80');
    expect(label).toContain('重新缓冲 1 次');
  });
});
