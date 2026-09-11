import type { AiMotionId, MotionId } from '../character/animation/MotionCatalog';

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

export type ApprovedActionId = AiMotionId;
export type BehaviorSource = 'system' | 'interaction' | 'audio' | 'ai';

export interface BehaviorProposal {
  source: BehaviorSource;
  actionId?: string;
  emotion?: EmotionIntent;
  intensity?: number;
  durationMs?: number;
  issuedAt?: number;
}

export interface BehaviorIntent {
  source: BehaviorSource;
  priority: number;
  issuedAt: number;
  emotion?: EmotionIntent;
  actionId?: MotionId;
  actionIntensity?: number;
  durationMs?: number;
}

export type MotionBehaviorIntent = BehaviorIntent & {
  actionId: MotionId;
  actionIntensity: number;
  durationMs: number;
};
