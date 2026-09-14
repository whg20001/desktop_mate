import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { z } from 'zod';

export const voiceInfoSchema = z.object({ id: z.string(), name: z.string(), language: z.string(), isDefault: z.boolean() });
export type VoiceInfo = z.infer<typeof voiceInfoSchema>;
export const audioEventSchema = z.object({
  sessionId: z.string().uuid(), generation: z.number().int().positive(), sequence: z.number().int().positive(),
  eventSequence: z.number().int().positive(),
  type: z.enum(['preparing', 'started', 'frame', 'completed', 'cancelled', 'failed']),
  positionMs: z.number().int().nonnegative(), level: z.number().min(0).max(1),
  error: z.object({ code: z.string(), message: z.string() }).optional(),
});
export const audioTimingsSchema = z.object({
  sessionId: z.string(), queueMs: z.number(), serviceQueueMs: z.number(), modelMs: z.number(), codecDecodeMs: z.number(),
  firstChunkMs: z.number().nullable(), wavDecodeMs: z.number(), firstChunkReceivedMs: z.number().nullable(),
  firstPlaybackMs: z.number().nullable(), synthesisDoneMs: z.number().nullable(), totalMs: z.number().nullable(),
  chunks: z.number(), underrunMs: z.number(),
  startupBufferMs: z.number().optional(), startupAudioMs: z.number().optional(), bufferTargetMs: z.number().optional(),
  generationRtf: z.number().nullable().optional(), rebufferCount: z.number().optional(),
});
export type AudioTimings = z.infer<typeof audioTimingsSchema>;
export const audioStatusSchema = z.object({ generation: z.number().int().nonnegative(), workerAlive: z.boolean(), timings: audioTimingsSchema.nullable().optional(), lastEvent: audioEventSchema.nullable() });
export interface AudioKey { sessionId: string; generation: number; sequence: number }
export interface AudioTransport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen(callback: (payload: unknown) => void): Promise<() => void>;
}
export const audioTransport: AudioTransport = { invoke, listen: (callback) => listen('audio://event', (event) => callback(event.payload)) };
export function isNativeAudio(): boolean { return '__TAURI_INTERNALS__' in window; }
export async function listAudioVoices(): Promise<VoiceInfo[]> { return z.array(voiceInfoSchema).parse(await invoke('list_audio_voices')); }
export function audioError(error: unknown): Error {
  if (error instanceof Error) return error;
  const structured = z.object({ message: z.string() }).safeParse(error);
  return new Error(structured.success ? structured.data.message : '原生语音服务不可用');
}

export function resolveVoice(voices: readonly VoiceInfo[], id: string, language: string): { id: string; migrated: boolean } {
  if (voices.some((v) => v.id === id)) return { id, migrated: false };
  const named = id ? voices.find((v) => v.name === id && v.language.toLowerCase() === language.toLowerCase()) : undefined;
  const candidates = voices.filter((v) => (!v.language || v.language.toLowerCase() === language.toLowerCase()));
  const fallback = named ?? candidates.find((v) => v.isDefault) ?? candidates[0];
  return { id: fallback?.id ?? '', migrated: Boolean(id) };
}

export const audioModelStatusSchema = z.object({
  selectedModel: z.string(), name: z.string(), phase: z.enum(['loading', 'ready', 'failed']),
  detail: z.string(), device: z.string().nullable(), pid: z.number().int().nullable(),
  speakers: z.array(z.string()),
  models: z.array(z.object({ id: z.string(), name: z.string() })),
});
export type AudioModelStatus = z.infer<typeof audioModelStatusSchema>;
export async function getAudioModels(): Promise<AudioModelStatus> {
  return audioModelStatusSchema.parse(await invoke('get_audio_models'));
}
export async function selectAudioModel(modelId: string): Promise<AudioModelStatus> {
  return audioModelStatusSchema.parse(await invoke('select_audio_model', { modelId }));
}
export function listenAudioModelChanges(callback: () => void): Promise<() => void> {
  return listen('audio://model-changed', callback);
}

export async function getAudioTimings(): Promise<AudioTimings | null> {
  return audioStatusSchema.parse(await invoke('get_audio_status')).timings ?? null;
}
export function formatAudioTimings(t: AudioTimings | null): string {
  if (!t) return '尚无语音请求，试听后显示分阶段耗时。';
  const ms = (n: number | null) => n === null ? '等待中' : (n / 1000).toFixed(2) + ' 秒';
  return ['请求排队 ' + ms(t.queueMs + t.serviceQueueMs), '模型生成（累计）' + ms(t.modelMs),
    '音频解码（累计）' + ms(t.codecDecodeMs + t.wavDecodeMs), '首块收到 ' + ms(t.firstChunkReceivedMs),
    '首块播放 ' + ms(t.firstPlaybackMs), '生成结束 ' + ms(t.synthesisDoneMs),
    t.chunks + ' 个音频块', '启动缓冲等待 ' + ms(t.startupBufferMs ?? 0),
    '启动储备 ' + ms(t.startupAudioMs ?? 0), '自适应目标 ' + ms(t.bufferTargetMs ?? 0),
    '生成耗时/音频时长 ' + (t.generationRtf == null ? '待测' : t.generationRtf.toFixed(2)),
    '重新缓冲 ' + (t.rebufferCount ?? 0) + ' 次', '缓冲欠载 ' + ms(t.underrunMs)].join(' · ');
}
