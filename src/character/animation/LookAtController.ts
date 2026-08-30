import * as THREE from 'three';
import type { MmdRuntime } from '../mmd/MmdRuntime';

export interface LookTarget {
  x: number;
  y: number;
  viewportWidth: number;
  viewportHeight: number;
}

interface BonePose {
  bone: THREE.Bone;
  base: THREE.Euler;
}

export class LookAtController {
  private target?: LookTarget;
  private enabled = true;
  private head?: BonePose;
  private eyes?: BonePose;

  constructor(private readonly runtime: MmdRuntime) {}

  attach(): void {
    const head = this.runtime.getBone('head');
    const eyes = this.runtime.getBone('eyes');
    this.head = head ? { bone: head, base: head.rotation.clone() } : undefined;
    this.eyes = eyes ? { bone: eyes, base: eyes.rotation.clone() } : undefined;
  }

  setTarget(target: LookTarget): void {
    this.target = target;
  }

  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
    if (!enabled) {
      this.restore(this.eyes);
      this.restore(this.head);
    }
  }

  update(delta: number): void {
    if (!this.enabled || !this.target) return;
    const nx = THREE.MathUtils.clamp((this.target.x / this.target.viewportWidth) * 2 - 1, -1, 1);
    const ny = THREE.MathUtils.clamp((this.target.y / this.target.viewportHeight) * 2 - 1, -1, 1);
    const smoothing = 1 - Math.exp(-delta * 8);

    this.apply(this.eyes, nx, ny, 9, 5, smoothing);
    this.apply(this.head, nx, ny, 13, 8, smoothing);
  }

  private restore(pose: BonePose | undefined): void {
    if (pose) pose.bone.rotation.copy(pose.base);
  }

  private apply(
    pose: BonePose | undefined,
    normalizedX: number,
    normalizedY: number,
    maxYawDegrees: number,
    maxPitchDegrees: number,
    smoothing: number,
  ): void {
    if (!pose) return;
    const targetYaw = pose.base.y + THREE.MathUtils.degToRad(maxYawDegrees) * normalizedX;
    const targetPitch = pose.base.x + THREE.MathUtils.degToRad(maxPitchDegrees) * normalizedY;
    pose.bone.rotation.y = THREE.MathUtils.lerp(pose.bone.rotation.y, targetYaw, smoothing);
    pose.bone.rotation.x = THREE.MathUtils.lerp(pose.bone.rotation.x, targetPitch, smoothing);
  }
}

