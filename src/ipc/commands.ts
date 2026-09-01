import { invoke } from '@tauri-apps/api/core';
import type { HitRegionPayload } from './schemas';

export const commands = {
  updateHitRegions: (payload: HitRegionPayload): Promise<void> =>
    invoke('update_hit_regions', { payload }),
  beginDrag: (): Promise<void> => invoke('begin_drag'),
  endDrag: (): Promise<void> => invoke('end_drag'),
  getDesktopWorld: (): Promise<unknown> => invoke('get_desktop_world'),
};
