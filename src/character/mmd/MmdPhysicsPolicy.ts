import { PmxObject, type MMDLoaderPluginFactory } from '@moeru/three-mmd';

const IDLE_STABLE_PREFIXES = [
  '左胸',
  '右胸',
  'xd',
  'qd_',
  'hf_',
  'Bone_F_hair',
  'Bone_L_hair',
  'Bone_R_hair',
  'eha',
  'ehb',
  'fs',
  'yx_',
  'zx_',
  'bd_',
  'Bone_L_flower',
  'Bone_R_flower',
] as const;

export function shouldStabilizeRigidBody(name: string): boolean {
  return IDLE_STABLE_PREFIXES.some((prefix) => name.startsWith(prefix));
}

export function chestAnchorBoneName(rigidBodyName: string): string | undefined {
  if (rigidBodyName.startsWith('左胸')) return '左胸上2';
  if (rigidBodyName.startsWith('右胸')) return '右胸上2';
  return undefined;
}

export const idleStabilityPlugin: MMDLoaderPluginFactory = () => ({
  name: 'IdleStabilityPlugin',
  afterParse(pmx) {
    const boneIndices = new Map(pmx.bones.map((bone, index) => [bone.name, index]));
    return {
      ...pmx,
      rigidBodies: pmx.rigidBodies.map((body) => {
        if (!shouldStabilizeRigidBody(body.name)) return body;
        const anchorName = chestAnchorBoneName(body.name);
        return {
          ...body,
          boneIndex:
            body.boneIndex >= 0 || !anchorName
              ? body.boneIndex
              : (boneIndices.get(anchorName) ?? body.boneIndex),
          physicsMode: PmxObject.RigidBody.PhysicsMode.FollowBone,
        };
      }),
    };
  },
});
