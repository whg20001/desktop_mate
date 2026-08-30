export interface CharacterBoneMap {
  center?: string;
  upperBody?: string;
  neck?: string;
  head?: string;
  eyes?: string;
  leftEye?: string;
  rightEye?: string;
  leftArm?: string;
  rightArm?: string;
  leftHand?: string;
  rightHand?: string;
  leftAnkle?: string;
  rightAnkle?: string;
  leftToe?: string;
  rightToe?: string;
}

export type CharacterBoneRole = keyof CharacterBoneMap;

export const BONE_ALIASES: Readonly<Record<CharacterBoneRole, readonly string[]>> = {
  center: ['センター', 'Center', '中心', '全ての親'],
  upperBody: ['上半身2', '上半身', 'UpperBody2', 'UpperBody'],
  neck: ['首', 'Neck', '脖子'],
  head: ['頭', 'Head', '头'],
  eyes: ['両目', 'Eyes', '双目'],
  leftEye: ['左目', 'LeftEye', '左眼'],
  rightEye: ['右目', 'RightEye', '右眼'],
  leftArm: ['左腕', 'LeftArm', '左臂'],
  rightArm: ['右腕', 'RightArm', '右臂'],
  leftHand: ['左手首', 'LeftWrist', '左手腕'],
  rightHand: ['右手首', 'RightWrist', '右手腕'],
  leftAnkle: ['左足首', 'LeftAnkle', '左脚踝'],
  rightAnkle: ['右足首', 'RightAnkle', '右脚踝'],
  leftToe: ['左つま先', '左足先EX', 'LeftToe', '左脚尖'],
  rightToe: ['右つま先', '右足先EX', 'RightToe', '右脚尖'],
};
