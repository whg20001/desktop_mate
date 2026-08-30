import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import type { CharacterState } from '../../ipc/schemas';
import type { CharacterBoneRole } from '../mmd/BoneMap';
import type { CharacterMorphRole } from '../mmd/MorphMap';
import type { MmdRuntime, PMXCompatibilityReport } from '../mmd/MmdRuntime';
import { MotionController } from './MotionController';

class RuntimeStub implements MmdRuntime {
  readonly bones = new Map<CharacterBoneRole, THREE.Bone>();
  readonly morphs = new Map<CharacterMorphRole, number>();

  constructor() {
    for (const role of ['center', 'upperBody', 'neck', 'leftArm', 'rightArm', 'rightHand'] as const) {
      this.bones.set(role, new THREE.Bone());
    }
  }

  async load(): Promise<PMXCompatibilityReport> {
    throw new Error('not used');
  }

  update(): void {}

  setMorph(role: CharacterMorphRole, weight: number): void {
    this.morphs.set(role, weight);
  }

  getBone(role: CharacterBoneRole): THREE.Bone | undefined {
    return this.bones.get(role);
  }

  getRoot(): THREE.Object3D | undefined {
    return undefined;
  }

  getBounds(): THREE.Box3 | undefined {
    return undefined;
  }

  dispose(): void {}
}

function state(mode: CharacterState['mode'], x = 0, y = 0): CharacterState {
  return {
    x,
    y,
    vx: 0,
    vy: 0,
    mode,
    grabbed: mode === 'dragged',
    supportSurface: null,
  };
}

function advance(controller: MotionController, seconds: number): void {
  for (let elapsed = 0; elapsed < seconds; elapsed += 1 / 60) controller.update(1 / 60);
}

describe('MotionController', () => {
  it('creates a stable idle pose with lowered arms', () => {
    const runtime = new RuntimeStub();
    const controller = new MotionController(runtime, () => 0.5);
    controller.attach();

    advance(controller, 1);

    expect(runtime.bones.get('leftArm')!.rotation.z).toBeLessThan(-1);
    expect(runtime.bones.get('rightArm')!.rotation.z).toBeGreaterThan(1);
  });

  it('leans toward drag motion and opens the arms while falling', () => {
    const runtime = new RuntimeStub();
    const controller = new MotionController(runtime, () => 0.5);
    controller.attach();
    controller.setDesktopState(state('dragged', 0, 0));
    controller.setDesktopState(state('dragged', 100, 0));
    advance(controller, 0.5);

    expect(runtime.bones.get('upperBody')!.rotation.z).toBeLessThan(0);

    controller.setDesktopState(state('falling', 100, 50));
    advance(controller, 0.5);

    expect(Math.abs(runtime.bones.get('leftArm')!.rotation.z)).toBeLessThan(
      THREE.MathUtils.degToRad(45),
    );
    expect(Math.abs(runtime.bones.get('rightArm')!.rotation.z)).toBeLessThan(
      THREE.MathUtils.degToRad(45),
    );
  });

  it('keeps the landing rebound after the native state returns to idle', () => {
    const runtime = new RuntimeStub();
    const controller = new MotionController(runtime, () => 0.5);
    controller.attach();
    controller.setDesktopState(state('landing'));
    controller.setDesktopState(state('idle'));

    advance(controller, 0.2);

    expect(runtime.bones.get('center')!.position.y).toBeLessThan(-0.05);
  });

  it('cancels the landing offset when a new fall starts', () => {
    const runtime = new RuntimeStub();
    const controller = new MotionController(runtime, () => 0.5);
    controller.attach();
    controller.setDesktopState(state('landing'));
    controller.setDesktopState(state('idle'));
    advance(controller, 0.15);

    controller.setDesktopState(state('falling'));
    advance(controller, 0.3);

    expect(runtime.bones.get('center')!.position.y).toBeCloseTo(0, 2);
  });

  it('layers greeting and talking before returning to neutral', () => {
    const runtime = new RuntimeStub();
    const controller = new MotionController(runtime, () => 0.5);
    controller.attach();
    controller.greet();
    controller.talk(0.5);

    advance(controller, 0.35);

    expect(runtime.bones.get('rightArm')!.rotation.z).toBeLessThan(
      THREE.MathUtils.degToRad(60),
    );
    expect(Math.max(...(['a', 'i', 'u', 'e', 'o'] as const).map((morph) => runtime.morphs.get(morph) ?? 0))).toBeGreaterThan(0);
    expect(runtime.morphs.get('smile')).toBeGreaterThan(0);

    advance(controller, 2);

    expect(runtime.bones.get('rightArm')!.rotation.z).toBeCloseTo(
      THREE.MathUtils.degToRad(68),
      2,
    );
    expect(Math.max(...(['a', 'i', 'u', 'e', 'o'] as const).map((morph) => runtime.morphs.get(morph) ?? 0))).toBe(0);
    expect(runtime.morphs.get('smile')).toBeCloseTo(0, 3);
  });
});
