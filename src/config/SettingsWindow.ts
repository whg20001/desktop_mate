import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { monitorFromPoint, PhysicalPosition } from '@tauri-apps/api/window';
import { z } from 'zod';
import { characterSettingsSchema, type CharacterSettings } from './CharacterSettings';

const SETTINGS_WINDOW_LABEL = 'settings';
const MESSAGE_SOURCE = 'desktop-companion-settings';
const SETTINGS_CHANGE_EVENT = 'settings://change';
const WINDOW_GAP = 20;

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface SettingsHandlers {
  onPreview: (settings: CharacterSettings) => void;
  onVoicePreview: (settings: CharacterSettings) => void;
  onSave: (settings: CharacterSettings) => void;
}

const settingsChangeSchema = z.object({
  event: z.enum(['preview', 'voice-preview', 'saved']),
  settings: characterSettingsSchema,
});

type SettingsChange = z.infer<typeof settingsChangeSchema>;
type BrowserSettingsMessage = SettingsChange & { source: typeof MESSAGE_SOURCE };

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.min(Math.max(value, minimum), Math.max(minimum, maximum));
}

export function chooseSettingsWindowPosition(
  character: Rect,
  settings: Pick<Rect, 'width' | 'height'>,
  monitor: Rect,
  gap = WINDOW_GAP,
): Pick<Rect, 'x' | 'y'> {
  const monitorRight = monitor.x + monitor.width;
  const monitorBottom = monitor.y + monitor.height;
  const rightCandidate = character.x + character.width + gap;
  const leftCandidate = character.x - settings.width - gap;
  const fitsRight = rightCandidate + settings.width <= monitorRight;
  const fitsLeft = leftCandidate >= monitor.x;
  const rightSpace = monitorRight - (character.x + character.width);
  const leftSpace = character.x - monitor.x;
  const preferredX =
    fitsRight || (!fitsLeft && rightSpace >= leftSpace) ? rightCandidate : leftCandidate;

  return {
    x: clamp(preferredX, monitor.x, monitorRight - settings.width),
    y: clamp(character.y, monitor.y, monitorBottom - settings.height),
  };
}

export async function openSettingsWindow(): Promise<void> {
  if (!isTauri()) {
    const left = window.screenX + window.outerWidth + WINDOW_GAP;
    const popup = window.open(
      '/settings.html',
      SETTINGS_WINDOW_LABEL,
      'popup=yes,width=960,height=720,left=' + left + ',top=' + window.screenY,
    );
    if (!popup) throw new Error('设置窗口被浏览器拦截');
    popup.focus();
    return;
  }

  const characterWindow = WebviewWindow.getCurrent();
  const settingsWindow = await WebviewWindow.getByLabel(SETTINGS_WINDOW_LABEL);
  if (!settingsWindow) throw new Error('找不到独立设置窗口');

  const [characterPosition, characterSize, settingsSize] = await Promise.all([
    characterWindow.outerPosition(),
    characterWindow.outerSize(),
    settingsWindow.outerSize(),
  ]);
  const monitor = await monitorFromPoint(
    characterPosition.x + characterSize.width / 2,
    characterPosition.y + characterSize.height / 2,
  );
  if (monitor) {
    const position = chooseSettingsWindowPosition(
      {
        x: characterPosition.x,
        y: characterPosition.y,
        width: characterSize.width,
        height: characterSize.height,
      },
      settingsSize,
      {
        x: monitor.workArea.position.x,
        y: monitor.workArea.position.y,
        width: monitor.workArea.size.width,
        height: monitor.workArea.size.height,
      },
    );
    await settingsWindow.setPosition(new PhysicalPosition(position.x, position.y));
  }

  await settingsWindow.unminimize();
  await settingsWindow.show();
  await settingsWindow.setFocus();
}

export async function hideSettingsWindow(): Promise<void> {
  if (isTauri()) {
    await WebviewWindow.getCurrent().hide();
  } else {
    window.close();
  }
}

export async function sendSettingsChange(
  event: SettingsChange['event'],
  settings: CharacterSettings,
): Promise<void> {
  if (isTauri()) {
    await emitTo(
      'character',
      SETTINGS_CHANGE_EVENT,
      { event, settings } satisfies SettingsChange,
    );
    return;
  }
  window.opener?.postMessage(
    { source: MESSAGE_SOURCE, event, settings } satisfies BrowserSettingsMessage,
    window.location.origin,
  );
}

function applySettingsChange(change: unknown, handlers: SettingsHandlers): void {
  const parsed = settingsChangeSchema.safeParse(change);
  if (!parsed.success) return;
  if (parsed.data.event === 'preview') handlers.onPreview(parsed.data.settings);
  if (parsed.data.event === 'voice-preview') handlers.onVoicePreview(parsed.data.settings);
  if (parsed.data.event === 'saved') handlers.onSave(parsed.data.settings);
}

export async function listenForSettingsChanges(
  handlers: SettingsHandlers,
): Promise<UnlistenFn> {
  if (isTauri()) {
    return listen<unknown>(SETTINGS_CHANGE_EVENT, (event) => {
      applySettingsChange(event.payload, handlers);
    });
  }

  const onMessage = (event: MessageEvent<BrowserSettingsMessage>): void => {
    if (event.origin !== window.location.origin || event.data?.source !== MESSAGE_SOURCE) return;
    applySettingsChange(event.data, handlers);
  };
  window.addEventListener('message', onMessage);
  return () => window.removeEventListener('message', onMessage);
}
