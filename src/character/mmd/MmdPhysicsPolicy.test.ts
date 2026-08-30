import { describe, expect, it } from 'vitest';
import { chestAnchorBoneName, shouldStabilizeRigidBody } from './MmdPhysicsPolicy';

describe('idle rigid-body stability policy', () => {
  it.each([
    '左胸_前',
    '右胸',
    'xd2',
    'qd_3_1',
    'hf_11_1',
    'Bone_F_hair01_02',
    'Bone_L_hair02_01',
    'Bone_R_hair01_02',
    'eha2',
    'ehb1',
    'fs4',
    'yx_4_1',
    'zx_8_1',
    'bd_6_1',
    'Bone_L_flower_01',
    'Bone_R_flower_02',
  ])(
    'stabilizes chest, hair, and accessory body %s',
    (name) => {
      expect(shouldStabilizeRigidBody(name)).toBe(true);
    },
  );

  it('maps unbound chest helpers back to their chest anchor bones', () => {
    expect(chestAnchorBoneName('左胸_前')).toBe('左胸上2');
    expect(chestAnchorBoneName('右胸_後')).toBe('右胸上2');
    expect(chestAnchorBoneName('Bone_F_hair01_01')).toBeUndefined();
  });

  it.each(['qh_5_2', 'qzq_8_9'])(
    'keeps skirt physics for %s',
    (name) => {
      expect(shouldStabilizeRigidBody(name)).toBe(false);
    },
  );
});
