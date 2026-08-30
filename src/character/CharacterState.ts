export type VisualCharacterState = 'idle' | 'dragged' | 'falling' | 'landing';

export interface CharacterVisualSnapshot {
  state: VisualCharacterState;
  updatedAt: number;
}

