import * as THREE from 'three';
import type { CharacterRenderer } from '../../renderer/CharacterRenderer';
import type { HitRegion, HitRegionPayload } from '../../ipc/schemas';
import type { CharacterBoneRole } from '../mmd/BoneMap';
import type { MmdRuntime } from '../mmd/MmdRuntime';

interface ScreenPoint {
  x: number;
  y: number;
}

export class HitRegionController {
  constructor(
    private readonly runtime: MmdRuntime,
    private readonly renderer: CharacterRenderer,
  ) {}

  measure(): HitRegionPayload | undefined {
    const bounds = this.runtime.getBounds();
    if (!bounds || bounds.isEmpty()) return undefined;

    const screenBounds = this.projectBounds(bounds);
    const width = screenBounds.max.x - screenBounds.min.x;
    const height = screenBounds.max.y - screenBounds.min.y;
    if (width <= 0 || height <= 0) return undefined;

    const centerX = (screenBounds.min.x + screenBounds.max.x) / 2;
    const head = this.projectBone('head') ?? {
      x: centerX,
      y: screenBounds.min.y + height * 0.14,
    };
    const neck = this.projectBone('neck') ?? {
      x: centerX,
      y: screenBounds.min.y + height * 0.23,
    };
    const center = this.projectBone('center') ?? {
      x: centerX,
      y: screenBounds.min.y + height * 0.57,
    };
    const headSize = Math.max(width * 0.25, height * 0.105);
    const torsoWidth = Math.max(width * 0.34, 42);
    const regions: HitRegion[] = [
      {
        shape: 'ellipse',
        x: head.x - headSize / 2,
        y: head.y - headSize / 2,
        width: headSize,
        height: headSize,
      },
      {
        shape: 'rect',
        x: centerX - torsoWidth / 2,
        y: neck.y,
        width: torsoWidth,
        height: Math.max(center.y - neck.y + height * 0.1, 36),
      },
      {
        shape: 'ellipse',
        x: centerX - width * 0.19,
        y: center.y,
        width: width * 0.38,
        height: Math.max(screenBounds.max.y - center.y, 50),
      },
    ];

    this.addHandRegion(regions, 'leftHand', Math.max(width * 0.12, 28));
    this.addHandRegion(regions, 'rightHand', Math.max(width * 0.12, 28));

    const footBones = [
      this.projectBone('leftToe'),
      this.projectBone('rightToe'),
      this.projectBone('leftAnkle'),
      this.projectBone('rightAnkle'),
    ].filter((point): point is ScreenPoint => point !== undefined);
    const footX = footBones.length > 0
      ? footBones.reduce((sum, point) => sum + point.x, 0) / footBones.length
      : centerX;
    const footBoneY = footBones.length > 0
      ? Math.max(...footBones.map((point) => point.y))
      : screenBounds.max.y;
    const footY = Math.min(screenBounds.max.y, footBoneY + height * 0.015);

    return {
      regions,
      footX,
      footY,
      scaleFactor: window.devicePixelRatio,
    };
  }

  private addHandRegion(regions: HitRegion[], role: 'leftHand' | 'rightHand', size: number): void {
    const hand = this.projectBone(role);
    if (!hand) return;
    regions.push({
      shape: 'ellipse',
      x: hand.x - size / 2,
      y: hand.y - size / 2,
      width: size,
      height: size,
    });
  }

  private projectBone(role: CharacterBoneRole): ScreenPoint | undefined {
    const bone = this.runtime.getBone(role);
    if (!bone) return undefined;
    return this.project(bone.getWorldPosition(new THREE.Vector3()));
  }

  private projectBounds(bounds: THREE.Box3): { min: ScreenPoint; max: ScreenPoint } {
    const min = { x: Number.POSITIVE_INFINITY, y: Number.POSITIVE_INFINITY };
    const max = { x: Number.NEGATIVE_INFINITY, y: Number.NEGATIVE_INFINITY };
    for (const x of [bounds.min.x, bounds.max.x]) {
      for (const y of [bounds.min.y, bounds.max.y]) {
        for (const z of [bounds.min.z, bounds.max.z]) {
          const point = this.project(new THREE.Vector3(x, y, z));
          min.x = Math.min(min.x, point.x);
          min.y = Math.min(min.y, point.y);
          max.x = Math.max(max.x, point.x);
          max.y = Math.max(max.y, point.y);
        }
      }
    }
    return { min, max };
  }

  private project(point: THREE.Vector3): ScreenPoint {
    const projected = point.project(this.renderer.camera);
    return {
      x: ((projected.x + 1) / 2) * this.renderer.canvas.clientWidth,
      y: ((1 - projected.y) / 2) * this.renderer.canvas.clientHeight,
    };
  }
}
