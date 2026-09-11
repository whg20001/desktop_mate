import type { CharacterResponse } from '../brain/BrainTypes';
import type { CharacterSettings } from '../config/CharacterSettings';
import type { ApprovedActionId, BehaviorIntent } from './BehaviorTypes';
import { BehaviorPolicy } from './BehaviorPolicy';

export class BehaviorPlanner {
  constructor(private readonly policy = new BehaviorPolicy()) {}

  resolve(response: CharacterResponse, settings: CharacterSettings): BehaviorIntent {
    return this.policy.evaluate(
      {
        source: 'ai',
        actionId: response.actionIntent?.id,
        intensity: response.actionIntent?.intensity,
        emotion: response.emotion
          ? {
              kind: response.emotion.type,
              intensity: response.emotion.intensity,
              durationMs: response.emotion.durationMs,
            }
          : undefined,
      },
      settings,
    );
  }

  interaction(actionId: ApprovedActionId): BehaviorIntent {
    return this.policy.evaluate({ source: 'interaction', actionId });
  }

  audio(durationMs: number): BehaviorIntent {
    return this.policy.evaluate({ source: 'audio', actionId: 'talking', durationMs });
  }
}

