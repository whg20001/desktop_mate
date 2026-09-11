import { describe, expect, it } from 'vitest';
import { DEFAULT_CHARACTER_SETTINGS } from '../config/CharacterSettings';
import { BehaviorPolicy } from './BehaviorPolicy';

describe('BehaviorPolicy', () => {
  const policy = new BehaviorPolicy(() => 250);

  it('approves only catalogued AI motions in the user allow-list', () => {
    expect(
      policy.evaluate(
        { source: 'ai', actionId: 'greeting', intensity: 4 },
        DEFAULT_CHARACTER_SETTINGS,
      ),
    ).toMatchObject({
      source: 'ai',
      priority: 40,
      issuedAt: 250,
      actionId: 'greeting',
      actionIntensity: 1,
      durationMs: 1600,
    });

    expect(
      policy.evaluate(
        { source: 'ai', actionId: 'falling' },
        DEFAULT_CHARACTER_SETTINGS,
      ).actionId,
    ).toBeUndefined();
    expect(
      policy.evaluate(
        { source: 'ai', actionId: 'missing' },
        DEFAULT_CHARACTER_SETTINGS,
      ).actionId,
    ).toBeUndefined();
  });

  it('keeps emotion when an AI motion is disabled', () => {
    const intent = policy.evaluate(
      {
        source: 'ai',
        actionId: 'greeting',
        emotion: { kind: 'curious', intensity: -2, durationMs: 99_000 },
      },
      { ...DEFAULT_CHARACTER_SETTINGS, aiMotionEnabled: false },
    );

    expect(intent.actionId).toBeUndefined();
    expect(intent.emotion).toEqual({
      kind: 'curious',
      intensity: 0,
      durationMs: 10_000,
    });
  });

  it('assigns source priorities and restricts the audio channel to talking', () => {
    expect(policy.evaluate({ source: 'interaction', actionId: 'greeting' })).toMatchObject({
      priority: 80,
      actionId: 'greeting',
    });
    expect(policy.evaluate({ source: 'audio', actionId: 'talking', durationMs: 1_200 }))
      .toMatchObject({
        priority: 60,
        actionId: 'talking',
        durationMs: 1_200,
      });
    expect(policy.evaluate({ source: 'audio', actionId: 'greeting' }).actionId).toBeUndefined();
  });
});
