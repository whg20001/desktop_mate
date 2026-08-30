import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import type { CharacterBoneRole } from '../mmd/BoneMap';
import type { MmdRuntime, PMXCompatibilityReport } from '../mmd/MmdRuntime';
import { LookAtController } from './LookAtController';

class RuntimeStub implements MmdRuntime {
  readonly head = new THREE.Bone();
  readonly eyes = new THREE.Bone();

  async load(): Promise<PMXCompatibilityReport> {
    throw new Error('not used');
  }

  update(): void {}
  setMorph(): void {}

  getBone(role: CharacterBoneRole): THREE.Bone | undefined {
    if (role === 'head') return this.head;
    if (role === 'eyes') return this.eyes;
    return undefined;
  }

  getRoot(): THREE.Object3D | undefined {
    return undefined;
  }

  getBounds(): THREE.Box3 | undefined {
    return undefined;
  }

  dispose(): void {}
}

describe('LookAtController', () => {
  it('turns the head toward a cursor on the right', () => {
    const runtime = new RuntimeStub();
    const controller = new LookAtController(runtime);
    controller.attach();
    controller.setTarget({ x: 100, y: 50, viewportWidth: 100, viewportHeight: 100 });

    controller.update(1);

    expect(runtime.head.rotation.y).toBeGreaterThan(0);
    expect(runtime.eyes.rotation.y).toBeGreaterThan(0);
  });

  it('restores the base pose when cursor following is disabled', () => {
    const runtime = new RuntimeStub();
    const controller = new LookAtController(runtime);
    controller.attach();
    controller.setTarget({ x: 100, y: 50, viewportWidth: 100, viewportHeight: 100 });
    controller.update(1);

    controller.setEnabled(false);

    expect(runtime.head.rotation.y).toBe(0);
    expect(runtime.eyes.rotation.y).toBe(0);
  });
});
