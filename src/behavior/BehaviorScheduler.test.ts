import { describe, expect, it } from 'vitest';
import { BehaviorPolicy } from './BehaviorPolicy';
import {
  BehaviorScheduler,
  type BehaviorExecutionTarget,
} from './BehaviorScheduler';
import type { EmotionIntent, MotionBehaviorIntent } from './BehaviorTypes';

class RecordingTarget implements BehaviorExecutionTarget {
  readonly motions: MotionBehaviorIntent[] = [];
  readonly emotions: EmotionIntent[] = [];

  applyEmotion(emotion: EmotionIntent): void {
    this.emotions.push(emotion);
  }

  startMotion(intent: MotionBehaviorIntent): void {
    this.motions.push(intent);
  }
}

describe('BehaviorScheduler', () => {
  it('queues a primary motion during falling and restarts it after landing', () => {
    const target = new RecordingTarget();
    const scheduler = new BehaviorScheduler(target);
    const policy = new BehaviorPolicy(() => 100);

    scheduler.setPhysicalState('falling', 100);
    expect(scheduler.submit(policy.evaluate({ source: 'interaction', actionId: 'greeting' })))
      .toBe('queued');
    scheduler.setPhysicalState('landing', 500);
    expect(target.motions).toHaveLength(0);
    scheduler.setPhysicalState('idle', 900);
    expect(target.motions).toHaveLength(0);
    scheduler.completeLanding(1_020);

    expect(target.motions.map((motion) => motion.actionId)).toEqual(['greeting']);
  });

  it('preempts an active primary motion for physics and restarts it while fresh', () => {
    const target = new RecordingTarget();
    const scheduler = new BehaviorScheduler(target);
    const policy = new BehaviorPolicy(() => 100);

    expect(scheduler.submit(policy.evaluate({ source: 'interaction', actionId: 'greeting' })))
      .toBe('started');
    scheduler.setPhysicalState('falling', 300);
    scheduler.setPhysicalState('landing', 600);
    scheduler.setPhysicalState('idle', 900);
    scheduler.completeLanding(1_120);

    expect(target.motions.map((motion) => motion.actionId)).toEqual([
      'greeting',
      'greeting',
    ]);
  });

  it('drops stale AI motions instead of replaying old context after landing', () => {
    const target = new RecordingTarget();
    const scheduler = new BehaviorScheduler(target);
    const policy = new BehaviorPolicy(() => 0);

    scheduler.setPhysicalState('falling', 0);
    expect(
      scheduler.submit(
        policy.evaluate(
          { source: 'ai', actionId: 'greeting' },
          { aiMotionEnabled: true, enabledAiMotionIds: ['greeting'] },
        ),
      ),
    ).toBe('queued');
    scheduler.setPhysicalState('idle', 3_001);

    expect(target.motions).toHaveLength(0);
  });

  it('does not let an obsolete landing completion override a new fall', () => {
    const target = new RecordingTarget();
    const scheduler = new BehaviorScheduler(target);
    const policy = new BehaviorPolicy(() => 0);

    scheduler.setPhysicalState('landing', 0);
    scheduler.setPhysicalState('idle', 20);
    scheduler.setPhysicalState('falling', 40);
    scheduler.completeLanding(600);
    expect(scheduler.submit(policy.evaluate({ source: 'interaction', actionId: 'greeting' })))
      .toBe('queued');
    expect(target.motions).toHaveLength(0);

    scheduler.setPhysicalState('idle', 700);
    expect(target.motions.map((motion) => motion.actionId)).toEqual(['greeting']);
  });

  it('lets direct interaction preempt AI and queues lower-priority work', () => {
    const target = new RecordingTarget();
    const scheduler = new BehaviorScheduler(target);
    let now = 0;
    const policy = new BehaviorPolicy(() => now);
    const settings = { aiMotionEnabled: true, enabledAiMotionIds: ['greeting'] as const };

    expect(
      scheduler.submit(policy.evaluate({ source: 'ai', actionId: 'greeting' }, settings)),
    ).toBe('started');
    now = 200;
    expect(
      scheduler.submit(policy.evaluate({ source: 'interaction', actionId: 'greeting' })),
    ).toBe('started');
    now = 400;
    expect(
      scheduler.submit(policy.evaluate({ source: 'ai', actionId: 'greeting' }, settings)),
    ).toBe('queued');
    scheduler.update(1_800);

    expect(target.motions.map((motion) => motion.source)).toEqual([
      'ai',
      'interaction',
    ]);
  });

  it('does not let a new AI motion bypass an older queued interaction', () => {
    const target = new RecordingTarget();
    const scheduler = new BehaviorScheduler(target);
    let now = 0;
    const policy = new BehaviorPolicy(() => now);
    const settings = { aiMotionEnabled: true, enabledAiMotionIds: ['greeting'] as const };

    expect(scheduler.submit(policy.evaluate({ source: 'interaction', actionId: 'greeting' })))
      .toBe('started');
    now = 100;
    expect(scheduler.submit(policy.evaluate({ source: 'interaction', actionId: 'greeting' })))
      .toBe('queued');
    scheduler.update(1_600);

    now = 4_100;
    expect(
      scheduler.submit(policy.evaluate({ source: 'ai', actionId: 'greeting' }, settings)),
    ).toBe('queued');
    expect(target.motions.map((motion) => motion.source)).toEqual([
      'interaction',
      'interaction',
    ]);
  });

  it('enforces cooldown after completion', () => {
    const target = new RecordingTarget();
    const scheduler = new BehaviorScheduler(target);
    const policy = new BehaviorPolicy(() => 0);

    expect(scheduler.submit(policy.evaluate({ source: 'interaction', actionId: 'greeting' })))
      .toBe('started');
    scheduler.update(1_600);
    expect(
      scheduler.submit(
        policy.evaluate({ source: 'interaction', actionId: 'greeting', issuedAt: 1_601 }),
      ),
    ).toBe('rejected');
    expect(
      scheduler.submit(
        policy.evaluate({ source: 'interaction', actionId: 'greeting', issuedAt: 4_101 }),
      ),
    ).toBe('started');
  });

  it('allows talking overlay and emotion while physical motion owns the body', () => {
    const target = new RecordingTarget();
    const scheduler = new BehaviorScheduler(target);
    const policy = new BehaviorPolicy(() => 100);

    scheduler.setPhysicalState('falling', 100);
    expect(
      scheduler.submit(
        policy.evaluate({
          source: 'audio',
          actionId: 'talking',
          emotion: { kind: 'concerned', intensity: 0.5, durationMs: 700 },
        }),
      ),
    ).toBe('started');

    expect(target.motions[0]?.actionId).toBe('talking');
    expect(target.emotions[0]?.kind).toBe('concerned');
  });
});
