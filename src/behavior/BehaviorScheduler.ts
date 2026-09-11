import {
  getMotionCatalogEntry,
  type MotionId,
  type PhysicalBehaviorState,
} from '../character/animation/MotionCatalog';
import type {
  BehaviorIntent,
  EmotionIntent,
  MotionBehaviorIntent,
} from './BehaviorTypes';

export type BehaviorScheduleResult = 'applied' | 'started' | 'queued' | 'rejected';

export interface BehaviorExecutionTarget {
  applyEmotion(emotion: EmotionIntent): void;
  startMotion(intent: MotionBehaviorIntent): void;
}

interface ActiveMotion {
  intent: MotionBehaviorIntent;
  endsAt: number;
}

interface QueuedMotion {
  intent: MotionBehaviorIntent;
  expiresAt: number;
}

const MAX_QUEUE_LENGTH = 8;

export class BehaviorScheduler {
  private physicalState: PhysicalBehaviorState = 'idle';
  private reportedPhysicalState: PhysicalBehaviorState = 'idle';
  private active?: ActiveMotion;
  private queue: QueuedMotion[] = [];
  private readonly cooldownUntil = new Map<MotionId, number>();
  private currentTime = 0;

  constructor(private readonly target: BehaviorExecutionTarget) {}

  submit(intent: BehaviorIntent): BehaviorScheduleResult {
    const now = this.advanceClock(intent.issuedAt);
    this.finishActive(now);
    if (intent.emotion) this.target.applyEmotion(intent.emotion);

    const motion = this.executable(intent);
    if (!motion) return 'applied';
    const definition = getMotionCatalogEntry(motion.actionId);
    if (!definition) return 'rejected';

    if (definition.channel === 'overlay') {
      if (!definition.allowedPhysicalStates.includes(this.physicalState)) return 'rejected';
      this.target.startMotion(motion);
      return 'started';
    }
    if (motion.actionId === 'idle') return 'applied';
    if (this.physicalState !== 'idle') {
      this.enqueue(motion, now);
      return 'queued';
    }
    if (this.active) {
      const activeDefinition = getMotionCatalogEntry(this.active.intent.actionId);
      if (
        motion.priority <= this.active.intent.priority ||
        activeDefinition?.interruptible === false
      ) {
        this.enqueue(motion, now);
        return 'queued';
      }
      this.active = undefined;
    }
    if (this.queue.length > 0) {
      if (!this.enqueue(motion, now)) return 'rejected';
      return this.startNext(now) === motion ? 'started' : 'queued';
    }
    if ((this.cooldownUntil.get(motion.actionId) ?? 0) > now) return 'rejected';
    this.start(motion, now);
    return 'started';
  }

  setPhysicalState(state: PhysicalBehaviorState, now: number): void {
    const time = this.advanceClock(now);
    this.reportedPhysicalState = state;
    if (state === 'idle' && this.physicalState === 'landing') return;
    if (state === this.physicalState) return;
    this.physicalState = state;
    if (state !== 'idle') {
      if (this.active) this.enqueue(this.active.intent, time);
      this.active = undefined;
      return;
    }
    this.startNext(time);
  }

  completeLanding(now: number): void {
    const time = this.advanceClock(now);
    if (this.physicalState !== 'landing' || this.reportedPhysicalState !== 'idle') return;
    this.physicalState = 'idle';
    this.startNext(time);
  }

  update(now: number): void {
    const time = this.advanceClock(now);
    this.finishActive(time);
    if (!this.active && this.physicalState === 'idle') this.startNext(time);
  }

  clear(): void {
    this.active = undefined;
    this.queue = [];
    this.cooldownUntil.clear();
    this.reportedPhysicalState = 'idle';
  }

  private executable(intent: BehaviorIntent): MotionBehaviorIntent | undefined {
    if (!intent.actionId) return undefined;
    const definition = getMotionCatalogEntry(intent.actionId);
    if (!definition) return undefined;
    return {
      ...intent,
      actionId: definition.id,
      actionIntensity: clamp(intent.actionIntensity ?? 1, 0, 1),
      durationMs: definition.loop
        ? 0
        : clamp(intent.durationMs ?? definition.defaultDurationMs, 100, definition.defaultDurationMs),
    };
  }

  private start(intent: MotionBehaviorIntent, now: number): void {
    const definition = getMotionCatalogEntry(intent.actionId);
    if (!definition) return;
    this.target.startMotion(intent);
    if (definition.channel === 'primary' && intent.durationMs > 0) {
      this.active = { intent, endsAt: now + intent.durationMs };
    }
  }

  private finishActive(now: number): void {
    const active = this.active;
    if (!active || active.endsAt > now) return;
    this.active = undefined;
    const definition = getMotionCatalogEntry(active.intent.actionId);
    if (definition?.cooldownMs) {
      this.cooldownUntil.set(active.intent.actionId, active.endsAt + definition.cooldownMs);
    }
  }

  private enqueue(intent: MotionBehaviorIntent, now: number): boolean {
    const expiresAt = intent.issuedAt + freshnessWindow(intent.source);
    if (expiresAt <= now) return false;
    this.queue = this.queue.filter(
      (queued) =>
        queued.intent.actionId !== intent.actionId || queued.intent.source !== intent.source,
    );
    this.queue.push({ intent, expiresAt });
    this.queue.sort(
      (left, right) =>
        right.intent.priority - left.intent.priority ||
        left.intent.issuedAt - right.intent.issuedAt,
    );
    if (this.queue.length > MAX_QUEUE_LENGTH) this.queue.length = MAX_QUEUE_LENGTH;
    return this.queue.some((queued) => queued.intent === intent);
  }

  private startNext(now: number): MotionBehaviorIntent | undefined {
    this.queue = this.queue.filter((queued) => queued.expiresAt > now);
    const coolingDown: QueuedMotion[] = [];
    while (this.queue.length > 0) {
      const queued = this.queue.shift();
      if (!queued) return undefined;
      const definition = getMotionCatalogEntry(queued.intent.actionId);
      if (!definition || !definition.allowedPhysicalStates.includes(this.physicalState)) {
        continue;
      }
      if ((this.cooldownUntil.get(queued.intent.actionId) ?? 0) > now) {
        coolingDown.push(queued);
        continue;
      }
      this.queue = [...coolingDown, ...this.queue];
      this.start(queued.intent, now);
      return queued.intent;
    }
    this.queue = coolingDown;
    return undefined;
  }

  private advanceClock(now: number): number {
    if (Number.isFinite(now)) this.currentTime = Math.max(this.currentTime, now);
    return this.currentTime;
  }
}

function freshnessWindow(source: MotionBehaviorIntent['source']): number {
  if (source === 'interaction') return 5_000;
  if (source === 'ai') return 3_000;
  if (source === 'audio') return 1_000;
  return Number.POSITIVE_INFINITY;
}

function clamp(value: number, minimum: number, maximum: number): number {
  if (!Number.isFinite(value)) return minimum;
  return Math.min(Math.max(value, minimum), maximum);
}
