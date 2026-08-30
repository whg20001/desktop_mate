export interface CharacterMorphMap {
  blink?: string;
  smile?: string;
  a?: string;
  i?: string;
  u?: string;
  e?: string;
  o?: string;
}

export type CharacterMorphRole = keyof CharacterMorphMap;

export const MORPH_ALIASES: Readonly<Record<CharacterMorphRole, readonly string[]>> = {
  blink: ['まばたき', 'Blink', '眨眼', '閉眼', '闭眼'],
  smile: ['笑い', 'Smile', '笑', '微笑'],
  a: ['あ', 'A', '啊'],
  i: ['い', 'I', '咿'],
  u: ['う', 'U', '呜'],
  e: ['え', 'E', '诶'],
  o: ['お', 'O', '哦'],
};

