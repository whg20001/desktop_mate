import { z } from 'zod';

export const characterBoneMapSchema = z.object({
  center: z.string().min(1).optional(),
  upperBody: z.string().min(1).optional(),
  neck: z.string().min(1).optional(),
  head: z.string().min(1).optional(),
  eyes: z.string().min(1).optional(),
  leftEye: z.string().min(1).optional(),
  rightEye: z.string().min(1).optional(),
  leftArm: z.string().min(1).optional(),
  rightArm: z.string().min(1).optional(),
  leftHand: z.string().min(1).optional(),
  rightHand: z.string().min(1).optional(),
  leftAnkle: z.string().min(1).optional(),
  rightAnkle: z.string().min(1).optional(),
  leftToe: z.string().min(1).optional(),
  rightToe: z.string().min(1).optional(),
});

export const characterMorphMapSchema = z.object({
  blink: z.string().min(1).optional(),
  smile: z.string().min(1).optional(),
  a: z.string().min(1).optional(),
  i: z.string().min(1).optional(),
  u: z.string().min(1).optional(),
  e: z.string().min(1).optional(),
  o: z.string().min(1).optional(),
});

export const modelManifestSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  model: z.string().min(1),
  scale: z.number().positive().default(1),
  groundOffset: z.number().default(0),
  bones: characterBoneMapSchema.default({}),
  morphs: characterMorphMapSchema.default({}),
});

export type ModelManifest = z.infer<typeof modelManifestSchema>;

export async function loadModelManifest(url: string): Promise<ModelManifest> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`角色清单加载失败 (${response.status} ${response.statusText})`);
  }

  return modelManifestSchema.parse(await response.json());
}

export function resolveAssetUrl(manifestUrl: string, assetPath: string): string {
  return new URL(assetPath, new URL(manifestUrl, window.location.href)).toString();
}
