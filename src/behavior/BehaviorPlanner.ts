import type { CharacterResponse } from '../brain/BrainTypes';
import type { CharacterSettings } from '../config/CharacterSettings';
import type { ApprovedActionId, BehaviorIntent } from './BehaviorTypes';

const APPROVED_ACTIONS = new Set<ApprovedActionId>(['idle', 'greeting', 'talking']);

export class BehaviorPlanner {
  resolve(response: CharacterResponse, settings: CharacterSettings): BehaviorIntent {
    const behavior: BehaviorIntent = {};
    if (response.emotion) {
      behavior.emotion = {
        kind: response.emotion.type,
        intensity: response.emotion.intensity,
        durationMs: response.emotion.durationMs,
      };
    }
    const actionId = response.actionIntent?.id as ApprovedActionId | undefined;
    if (
      settings.aiMotionEnabled &&
      actionId &&
      APPROVED_ACTIONS.has(actionId) &&
      settings.enabledAiMotionIds.includes(actionId)
    ) {
      behavior.actionId = actionId;
      behavior.actionIntensity = response.actionIntent?.intensity;
    }
    return behavior;
  }
}

