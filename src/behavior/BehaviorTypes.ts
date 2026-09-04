export type EmotionKind =
  | 'neutral'
  | 'happy'
  | 'concerned'
  | 'curious'
  | 'surprised'
  | 'sad';

export interface EmotionIntent {
  kind: EmotionKind;
  intensity: number;
  durationMs: number;
}

export type ApprovedActionId = 'idle' | 'greeting' | 'talking';

export interface BehaviorIntent {
  emotion?: EmotionIntent;
  actionId?: ApprovedActionId;
  actionIntensity?: number;
}

