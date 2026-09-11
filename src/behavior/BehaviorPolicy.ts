import {
  getMotionCatalogEntry,
  type MotionCatalogEntry,
} from '../character/animation/MotionCatalog';
import type {
  BehaviorIntent,
  BehaviorProposal,
  BehaviorSource,
  EmotionIntent,
} from './BehaviorTypes';

interface PolicySettings {
  aiMotionEnabled: boolean;
  enabledAiMotionIds: readonly string[];
}

const PRIORITIES: Record<BehaviorSource, number> = {
  system: 100,
  interaction: 80,
  audio: 60,
  ai: 40,
};

export class BehaviorPolicy {
  constructor(private readonly now: () => number = () => performance.now()) {}

  evaluate(proposal: BehaviorProposal, settings?: PolicySettings): BehaviorIntent {
    const intent: BehaviorIntent = {
      source: proposal.source,
      priority: PRIORITIES[proposal.source],
      issuedAt: proposal.issuedAt ?? this.now(),
    };
    if (proposal.emotion) intent.emotion = normalizeEmotion(proposal.emotion);

    const motion = proposal.actionId
      ? getMotionCatalogEntry(proposal.actionId)
      : undefined;
    if (!motion || !this.sourceCanUse(motion, proposal.source, settings)) return intent;

    intent.actionId = motion.id;
    intent.priority = motion.id === 'idle' ? 10 : PRIORITIES[proposal.source];
    intent.actionIntensity = clamp(proposal.intensity ?? 1, 0, 1);
    intent.durationMs = motion.loop
      ? 0
      : clamp(proposal.durationMs ?? motion.defaultDurationMs, 100, motion.defaultDurationMs);
    return intent;
  }

  private sourceCanUse(
    motion: MotionCatalogEntry,
    source: BehaviorSource,
    settings?: PolicySettings,
  ): boolean {
    if (source === 'system') return motion.owner === 'system';
    if (source === 'audio') return motion.id === 'talking';
    if (source === 'interaction') return motion.owner === 'ai';
    return Boolean(
      motion.owner === 'ai' &&
        settings?.aiMotionEnabled &&
        settings.enabledAiMotionIds.includes(motion.id),
    );
  }
}

function normalizeEmotion(emotion: EmotionIntent): EmotionIntent {
  return {
    kind: emotion.kind,
    intensity: clamp(emotion.intensity, 0, 1),
    durationMs: clamp(emotion.durationMs, 250, 10_000),
  };
}

function clamp(value: number, minimum: number, maximum: number): number {
  if (!Number.isFinite(value)) return minimum;
  return Math.min(Math.max(value, minimum), maximum);
}
