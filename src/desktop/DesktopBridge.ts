import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { commands } from '../ipc/commands';
import { events } from '../ipc/events';
import {
  characterStateSchema,
  cursorPositionSchema,
  desktopWorldSchema,
  hitRegionPayloadSchema,
  type CharacterState,
  type CursorPosition,
  type DesktopWorld,
  type HitRegionPayload,
} from '../ipc/schemas';

type CursorListener = (cursor: CursorPosition) => void;
type StateListener = (state: CharacterState) => void;

export class DesktopBridge {
  private readonly unlisten: UnlistenFn[] = [];
  private readonly cursorListeners = new Set<CursorListener>();
  private readonly stateListeners = new Set<StateListener>();

  static isTauri(): boolean {
    return '__TAURI_INTERNALS__' in window;
  }

  async connect(): Promise<void> {
    if (!DesktopBridge.isTauri()) return;
    this.unlisten.push(
      await listen(events.cursorPosition, (event) => {
        const parsed = cursorPositionSchema.safeParse(event.payload);
        if (parsed.success) this.cursorListeners.forEach((listener) => listener(parsed.data));
      }),
      await listen(events.characterState, (event) => {
        const parsed = characterStateSchema.safeParse(event.payload);
        if (parsed.success) this.stateListeners.forEach((listener) => listener(parsed.data));
      }),
    );
  }

  onCursor(listener: CursorListener): () => void {
    this.cursorListeners.add(listener);
    return () => this.cursorListeners.delete(listener);
  }

  onCharacterState(listener: StateListener): () => void {
    this.stateListeners.add(listener);
    return () => this.stateListeners.delete(listener);
  }

  async updateHitRegions(payload: HitRegionPayload): Promise<void> {
    const validated = hitRegionPayloadSchema.parse(payload);
    if (DesktopBridge.isTauri()) await commands.updateHitRegions(validated);
  }

  async beginDrag(): Promise<void> {
    if (DesktopBridge.isTauri()) await commands.beginDrag();
  }

  async endDrag(): Promise<void> {
    if (DesktopBridge.isTauri()) await commands.endDrag();
  }

  async getDesktopWorld(): Promise<DesktopWorld | undefined> {
    if (!DesktopBridge.isTauri()) return undefined;
    return desktopWorldSchema.parse(await commands.getDesktopWorld());
  }

  dispose(): void {
    this.unlisten.splice(0).forEach((unlisten) => unlisten());
    this.cursorListeners.clear();
    this.stateListeners.clear();
  }
}
