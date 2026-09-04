export const AI_MOTION_IDS = ['idle', 'greeting', 'talking'] as const;
export type AiMotionId = (typeof AI_MOTION_IDS)[number];

export type MotionOwner = 'ai' | 'system';
export type MotionSource = 'procedural' | 'vmd';

export interface MotionCatalogEntry {
  id: string;
  displayName: string;
  description: string;
  scenes: readonly string[];
  owner: MotionOwner;
  source: MotionSource;
}

export const MOTION_CATALOG: readonly MotionCatalogEntry[] = [
  {
    id: 'idle',
    displayName: '待机',
    description: '极轻微重心变化、头部停顿与偶尔歪头。',
    scenes: ['等待', '倾听', '空闲'],
    owner: 'ai',
    source: 'procedural',
  },
  {
    id: 'greeting',
    displayName: '问候',
    description: '抬手并轻微点头，约 1–2 秒后回到待机。',
    scenes: ['打招呼', '欢迎', '回应点击'],
    owner: 'ai',
    source: 'procedural',
  },
  {
    id: 'talking',
    displayName: '说话',
    description: '嘴部 Morph、轻微点头与表情变化。',
    scenes: ['回答', '播报', '对话'],
    owner: 'ai',
    source: 'procedural',
  },
  {
    id: 'dragged',
    displayName: '拖拽',
    description: '身体向桌面拖拽方向倾斜。',
    scenes: ['用户拖拽'],
    owner: 'system',
    source: 'procedural',
  },
  {
    id: 'falling',
    displayName: '下落',
    description: '离开支撑面时手臂略微张开。',
    scenes: ['桌面物理下落'],
    owner: 'system',
    source: 'procedural',
  },
  {
    id: 'landing',
    displayName: '落地',
    description: '身体短暂下沉并回弹。',
    scenes: ['接触支撑面'],
    owner: 'system',
    source: 'procedural',
  },
];

export const DEFAULT_AI_MOTION_IDS: readonly AiMotionId[] = [...AI_MOTION_IDS];
