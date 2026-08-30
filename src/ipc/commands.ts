import { invoke } from '@tauri-apps/api/core';
import type { HitRegionPayload } from './schemas';

export const commands = {
  updateHitRegions: (payload: HitRegionPayload): Promise<void> =>
    invoke('update_hit_regions', { payload }),
  beginDrag: (): Promise<void> => invoke('begin_drag'),
  endDrag: (): Promise<void> => invoke('end_drag'),
  setInteractionLocked: (locked: boolean): Promise<void> =>
    invoke('set_interaction_locked', { locked }),
  getDesktopWorld: (): Promise<unknown> => invoke('get_desktop_world'),
  getCharacterState: (): Promise<unknown> => invoke('get_character_state'),
};

