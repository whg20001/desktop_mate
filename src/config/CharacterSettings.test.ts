import { describe, expect, it } from 'vitest';
import { characterSettingsSchema, DEFAULT_CHARACTER_SETTINGS } from './CharacterSettings';

describe('character settings', () => {
  it('keeps the character still by default', () => {
    expect(DEFAULT_CHARACTER_SETTINGS).toMatchObject({
      characterModelId: 'lingguang',
      scale: 1,
      followCursor: true,
      breathing: false,
      materialAmbientScale: 0.25,
      hemisphereLightIntensity: 0.3,
      directionalLightIntensity: 1.1,
      speechEnabled: true,
      speechVoiceId: '',
      speechLanguage: 'zh-CN',
      speechRate: 1,
      speechPitch: 1,
      speechVolume: 1,
      aiMotionEnabled: true,
      enabledAiMotionIds: ['idle', 'greeting', 'talking'],
    });
  });

  it('fills new defaults for settings saved by older versions', () => {
    expect(characterSettingsSchema.parse({ displayName: '陵光' })).toMatchObject({
      materialAmbientScale: 0.25,
      hemisphereLightIntensity: 0.3,
      directionalLightIntensity: 1.1,
      speechEnabled: true,
      speechVoiceId: '',
      speechLanguage: 'zh-CN',
      speechRate: 1,
      speechPitch: 1,
      speechVolume: 1,
      aiMotionEnabled: true,
      enabledAiMotionIds: ['idle', 'greeting', 'talking'],
    });
  });

  it('rejects scale values outside the supported range', () => {
    expect(characterSettingsSchema.safeParse({ scale: 0.5 }).success).toBe(false);
    expect(characterSettingsSchema.safeParse({ scale: 1.5 }).success).toBe(false);
  });

  it('rejects color controls outside the supported range', () => {
    expect(characterSettingsSchema.safeParse({ materialAmbientScale: 1.1 }).success).toBe(false);
    expect(characterSettingsSchema.safeParse({ hemisphereLightIntensity: 2.1 }).success).toBe(
      false,
    );
    expect(characterSettingsSchema.safeParse({ directionalLightIntensity: 4.1 }).success).toBe(
      false,
    );
  });

  it('rejects speech controls outside the Web Speech ranges', () => {
    expect(characterSettingsSchema.safeParse({ speechRate: 0.49 }).success).toBe(false);
    expect(characterSettingsSchema.safeParse({ speechRate: 2.01 }).success).toBe(false);
    expect(characterSettingsSchema.safeParse({ speechPitch: -0.01 }).success).toBe(false);
    expect(characterSettingsSchema.safeParse({ speechPitch: 2.01 }).success).toBe(false);
    expect(characterSettingsSchema.safeParse({ speechVolume: -0.01 }).success).toBe(false);
    expect(characterSettingsSchema.safeParse({ speechVolume: 1.01 }).success).toBe(false);
    expect(characterSettingsSchema.safeParse({ speechLanguage: '' }).success).toBe(false);
  });
  it('rejects unknown AI motion identifiers', () => {
    expect(
      characterSettingsSchema.safeParse({ enabledAiMotionIds: ['unknown'] }).success,
    ).toBe(false);
  });
});
