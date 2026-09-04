import { describe, expect, it } from 'vitest';
import { DEFAULT_CHARACTER_SETTINGS } from '../config/CharacterSettings';
import { BehaviorPlanner } from './BehaviorPlanner';

describe('BehaviorPlanner', () => {
  it('maps an allowed semantic proposal without exposing model controls', () => {
    const intent = new BehaviorPlanner().resolve(
      {
        turnId: 'turn',
        text: '你好',
        speech: { text: '你好' },
        emotion: { type: 'happy', intensity: 0.7, durationMs: 1000 },
        actionIntent: { id: 'greeting', intensity: 0.6 },
        degradedReasons: [],
      },
      DEFAULT_CHARACTER_SETTINGS,
    );
    expect(intent).toEqual({
      emotion: { kind: 'happy', intensity: 0.7, durationMs: 1000 },
      actionId: 'greeting',
      actionIntensity: 0.6,
    });
  });

  it('drops disabled AI actions while preserving emotion', () => {
    const intent = new BehaviorPlanner().resolve(
      {
        turnId: 'turn',
        text: '你好',
        emotion: { type: 'curious', intensity: 0.5, durationMs: 1000 },
        actionIntent: { id: 'greeting', intensity: 0.6 },
        degradedReasons: [],
      },
      { ...DEFAULT_CHARACTER_SETTINGS, aiMotionEnabled: false },
    );
    expect(intent.actionId).toBeUndefined();
    expect(intent.emotion?.kind).toBe('curious');
  });
});
