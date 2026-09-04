export interface CharacterCatalogEntry {
  id: string;
  displayName: string;
  description: string;
  manifestUrl: string;
  available: boolean;
}

export const DEFAULT_CHARACTER_MODEL_ID = 'lingguang';

export const CHARACTER_CATALOG: readonly CharacterCatalogEntry[] = [
  {
    id: DEFAULT_CHARACTER_MODEL_ID,
    displayName: '陵光',
    description: '当前内置的 PMX 助手模型，支持骨骼动作、表情、嘴型与鼠标视线跟随。',
    manifestUrl: '/LinGuang/manifest.json',
    available: true,
  },
];

export function findCharacterCatalogEntry(
  id: string,
): CharacterCatalogEntry | undefined {
  return CHARACTER_CATALOG.find((entry) => entry.id === id && entry.available);
}

export function getCharacterCatalogEntry(id: string): CharacterCatalogEntry {
  const selected = findCharacterCatalogEntry(id);
  const fallback = findCharacterCatalogEntry(DEFAULT_CHARACTER_MODEL_ID);
  if (selected) return selected;
  if (fallback) return fallback;
  throw new Error('角色目录中没有可用模型');
}