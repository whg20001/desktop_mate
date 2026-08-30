import type * as THREE from 'three';
import type { CharacterBoneRole } from './BoneMap';
import type { CharacterMorphRole } from './MorphMap';

export interface PMXCompatibilityReport {
  materialCount: number;
  textureCount: number;
  bones: Record<CharacterBoneRole, boolean>;
  morphs: Record<CharacterMorphRole, boolean>;
  warnings: string[];
}

export interface MmdRuntime {
  load(url: string): Promise<PMXCompatibilityReport>;
  update(delta: number): void;
  setMorph(role: CharacterMorphRole, weight: number): void;
  getBone(role: CharacterBoneRole): THREE.Bone | undefined;
  getRoot(): THREE.Object3D | undefined;
  getBounds(): THREE.Box3 | undefined;
  dispose(): void;
}

