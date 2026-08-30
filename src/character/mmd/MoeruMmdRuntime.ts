import { MMDLoader, type MMD } from '@moeru/three-mmd';
import { MMDAmmoPlugin } from '@moeru/three-mmd-physics-ammo';
import * as THREE from 'three';
import { BONE_ALIASES, type CharacterBoneRole } from './BoneMap';
import type { MmdRuntime, PMXCompatibilityReport } from './MmdRuntime';
import { MORPH_ALIASES, type CharacterMorphRole } from './MorphMap';
import type { ModelManifest } from './ModelManifest';
import { idleStabilityPlugin } from './MmdPhysicsPolicy';

const BONE_ROLES = Object.keys(BONE_ALIASES) as CharacterBoneRole[];
const MORPH_ROLES = Object.keys(MORPH_ALIASES) as CharacterMorphRole[];
const MMD_AMBIENT_SCALE = 0.25;

export class MoeruMmdRuntime implements MmdRuntime {
  private mmd?: MMD;
  private readonly resolvedBones = new Map<CharacterBoneRole, THREE.Bone>();
  private readonly resolvedMorphs = new Map<CharacterMorphRole, number>();
  private readonly baseAmbientColors = new Map<THREE.Material, THREE.Color>();
  private materialAmbientScale = MMD_AMBIENT_SCALE;

  constructor(
    private readonly scene: THREE.Scene,
    private readonly manifest: ModelManifest,
  ) {}

  async load(url: string): Promise<PMXCompatibilityReport> {
    this.dispose();

    const loader = new MMDLoader().register(idleStabilityPlugin).register(MMDAmmoPlugin);
    this.mmd = await loader.loadAsync(url);
    this.mmd.setScalar(this.manifest.scale);

    const mesh = this.mmd.mesh;
    mesh.name = this.manifest.name;
    mesh.frustumCulled = false;
    mesh.castShadow = false;
    mesh.receiveShadow = false;
    mesh.position.y += this.manifest.groundOffset;

    const materials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
    for (const material of materials) {
      if ('ambient' in material && material.ambient instanceof THREE.Color) {
        this.baseAmbientColors.set(material, material.ambient.clone());
      }
    }
    this.applyMaterialAmbientScale();

    this.scene.add(mesh);
    this.resolveBones();
    this.resolveMorphs();

    const textureCount = materials.reduce((count, material) => {
      const textured = 'map' in material && material.map instanceof THREE.Texture;
      return count + Number(textured);
    }, 0);

    const warnings: string[] = [];
    if (textureCount === 0) warnings.push('未检测到主纹理，请目视确认材质是否正常。');
    if (!this.resolvedMorphs.has('blink')) warnings.push('未找到眨眼 Morph。');
    if (!this.resolvedBones.has('head')) warnings.push('未找到头部骨骼。');

    return {
      materialCount: materials.length,
      textureCount,
      bones: Object.fromEntries(BONE_ROLES.map((role) => [role, this.resolvedBones.has(role)])) as Record<CharacterBoneRole, boolean>,
      morphs: Object.fromEntries(MORPH_ROLES.map((role) => [role, this.resolvedMorphs.has(role)])) as Record<CharacterMorphRole, boolean>,
      warnings,
    };
  }

  update(delta: number): void {
    this.mmd?.update(delta);
  }

  setMorph(role: CharacterMorphRole, weight: number): void {
    const index = this.resolvedMorphs.get(role);
    const influences = this.mmd?.mesh.morphTargetInfluences;
    if (index !== undefined && influences) {
      influences[index] = THREE.MathUtils.clamp(weight, 0, 1);
    }
  }

  setMaterialAmbientScale(scale: number): void {
    this.materialAmbientScale = THREE.MathUtils.clamp(scale, 0, 1);
    this.applyMaterialAmbientScale();
  }

  getBone(role: CharacterBoneRole): THREE.Bone | undefined {
    return this.resolvedBones.get(role);
  }

  getRoot(): THREE.Object3D | undefined {
    return this.mmd?.mesh;
  }

  getBounds(): THREE.Box3 | undefined {
    const root = this.getRoot();
    if (!root) return undefined;
    root.updateWorldMatrix(true, true);
    return new THREE.Box3().setFromObject(root);
  }

  dispose(): void {
    const mesh = this.mmd?.mesh;
    if (mesh) {
      this.scene.remove(mesh);
      mesh.geometry.dispose();
      const materials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
      materials.forEach((material) => material.dispose());
    }
    this.mmd?.dispose();
    this.mmd = undefined;
    this.resolvedBones.clear();
    this.resolvedMorphs.clear();
    this.baseAmbientColors.clear();
  }

  private applyMaterialAmbientScale(): void {
    for (const [material, baseAmbient] of this.baseAmbientColors) {
      if ('ambient' in material && material.ambient instanceof THREE.Color) {
        material.ambient.copy(baseAmbient).multiplyScalar(this.materialAmbientScale);
      }
    }
  }

  private resolveBones(): void {
    const skeleton = this.mmd?.mesh.skeleton;
    if (!skeleton) return;

    for (const role of BONE_ROLES) {
      const configured = this.manifest.bones[role];
      const candidates = configured ? [configured, ...BONE_ALIASES[role]] : BONE_ALIASES[role];
      const found = candidates
        .map((name) => skeleton.getBoneByName(name))
        .find((bone): bone is THREE.Bone => bone !== undefined);
      if (found) this.resolvedBones.set(role, found);
    }
  }

  private resolveMorphs(): void {
    const dictionary = this.mmd?.mesh.morphTargetDictionary;
    if (!dictionary) return;

    for (const role of MORPH_ROLES) {
      const configured = this.manifest.morphs[role];
      const candidates = configured ? [configured, ...MORPH_ALIASES[role]] : MORPH_ALIASES[role];
      const name = candidates.find((candidate) => dictionary[candidate] !== undefined);
      if (name !== undefined) this.resolvedMorphs.set(role, dictionary[name]!);
    }
  }
}

