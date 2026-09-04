import { z } from 'zod';
import { AI_MOTION_IDS, DEFAULT_AI_MOTION_IDS } from '../character/animation/MotionCatalog';
import { DEFAULT_CHARACTER_MODEL_ID } from '../character/CharacterCatalog';

const STORAGE_KEY = 'desktop-companion.character-settings';

export const DEFAULT_COLOR_SETTINGS = {
  materialAmbientScale: 0.25,
  hemisphereLightIntensity: 0.3,
  directionalLightIntensity: 1.1,
} as const;

export const DEFAULT_SPEECH_SETTINGS = {
  speechEnabled: true,
  speechVoiceId: '',
  speechLanguage: 'zh-CN',
  speechRate: 1,
  speechPitch: 1,
  speechVolume: 1,
} as const;

export const DEFAULT_BRAIN_SETTINGS = {
  llmBaseUrl: 'http://127.0.0.1:11434/v1',
  llmModel: 'qwen2.5:7b',
  embeddingBaseUrl: 'http://127.0.0.1:11434/v1',
  embeddingModel: 'nomic-embed-text',
  embeddingDimensions: 768,
  memoryEnabled: true,
  memoryRecallEnabled: true,
  memoryWriteEnabled: true,
  memoryRecallLimit: 6,
} as const;

function localEndpoint(value: string): boolean {
  try {
    const url = new URL(value);
    const ipv4 = url.hostname.split('.').map(Number);
    const isIpv4Loopback =
      ipv4.length === 4 &&
      ipv4[0] === 127 &&
      ipv4.every((part) => Number.isInteger(part) && part >= 0 && part <= 255);
    return (
      url.protocol === 'http:' &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash &&
      (url.hostname === 'localhost' ||
        url.hostname === '[::1]' ||
        isIpv4Loopback)
    );
  } catch {
    return false;
  }
}

export const characterSettingsSchema = z.object({
  characterModelId: z.string().trim().min(1).max(128).default(DEFAULT_CHARACTER_MODEL_ID),
  displayName: z.string().trim().min(1).max(32).default('陵光'),
  scale: z.number().min(0.75).max(1.25).default(1),
  followCursor: z.boolean().default(true),
  breathing: z.boolean().default(false),
  materialAmbientScale: z
    .number()
    .min(0)
    .max(1)
    .default(DEFAULT_COLOR_SETTINGS.materialAmbientScale),
  hemisphereLightIntensity: z
    .number()
    .min(0)
    .max(2)
    .default(DEFAULT_COLOR_SETTINGS.hemisphereLightIntensity),
  directionalLightIntensity: z
    .number()
    .min(0)
    .max(4)
    .default(DEFAULT_COLOR_SETTINGS.directionalLightIntensity),
  speechEnabled: z.boolean().default(DEFAULT_SPEECH_SETTINGS.speechEnabled),
  speechVoiceId: z
    .string()
    .trim()
    .max(512)
    .default(DEFAULT_SPEECH_SETTINGS.speechVoiceId),
  speechLanguage: z
    .string()
    .trim()
    .min(2)
    .max(35)
    .default(DEFAULT_SPEECH_SETTINGS.speechLanguage),
  speechRate: z.number().min(0.5).max(2).default(DEFAULT_SPEECH_SETTINGS.speechRate),
  speechPitch: z.number().min(0).max(2).default(DEFAULT_SPEECH_SETTINGS.speechPitch),
  speechVolume: z.number().min(0).max(1).default(DEFAULT_SPEECH_SETTINGS.speechVolume),
  aiMotionEnabled: z.boolean().default(true),
  enabledAiMotionIds: z
    .array(z.enum(AI_MOTION_IDS))
    .max(AI_MOTION_IDS.length)
    .default([...DEFAULT_AI_MOTION_IDS]),
  llmBaseUrl: z
    .string()
    .trim()
    .max(2048)
    .refine(localEndpoint, 'LLM 地址必须是本机 HTTP 回环地址')
    .default(DEFAULT_BRAIN_SETTINGS.llmBaseUrl),
  llmModel: z.string().trim().min(1).max(128).default(DEFAULT_BRAIN_SETTINGS.llmModel),
  embeddingBaseUrl: z
    .string()
    .trim()
    .max(2048)
    .refine(localEndpoint, 'Embedding 地址必须是本机 HTTP 回环地址')
    .default(DEFAULT_BRAIN_SETTINGS.embeddingBaseUrl),
  embeddingModel: z
    .string()
    .trim()
    .min(1)
    .max(128)
    .default(DEFAULT_BRAIN_SETTINGS.embeddingModel),
  embeddingDimensions: z
    .number()
    .int()
    .min(64)
    .max(8192)
    .default(DEFAULT_BRAIN_SETTINGS.embeddingDimensions),
  memoryEnabled: z.boolean().default(DEFAULT_BRAIN_SETTINGS.memoryEnabled),
  memoryRecallEnabled: z.boolean().default(DEFAULT_BRAIN_SETTINGS.memoryRecallEnabled),
  memoryWriteEnabled: z.boolean().default(DEFAULT_BRAIN_SETTINGS.memoryWriteEnabled),
  memoryRecallLimit: z
    .number()
    .int()
    .min(1)
    .max(20)
    .default(DEFAULT_BRAIN_SETTINGS.memoryRecallLimit),
});

export type CharacterSettings = z.infer<typeof characterSettingsSchema>;

export const DEFAULT_CHARACTER_SETTINGS: CharacterSettings = characterSettingsSchema.parse({});

export function loadCharacterSettings(): CharacterSettings {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return stored ? characterSettingsSchema.parse(JSON.parse(stored)) : DEFAULT_CHARACTER_SETTINGS;
  } catch {
    return DEFAULT_CHARACTER_SETTINGS;
  }
}

export function saveCharacterSettings(settings: CharacterSettings): void {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}
