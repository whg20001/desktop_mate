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
  apiBaseUrl: z.string().trim().max(2048).default(''),
  apiModel: z.string().trim().max(128).default(''),
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
