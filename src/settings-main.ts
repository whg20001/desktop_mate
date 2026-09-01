import './styles.css';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { loadCharacterSettings } from './config/CharacterSettings';
import { SettingsPanel } from './config/SettingsPanel';
import { hideSettingsWindow, sendSettingsChange } from './config/SettingsWindow';

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error('缺少设置窗口元素: ' + selector);
  return element;
}

const panel = new SettingsPanel(requiredElement<HTMLElement>('#settings-panel'), {
  initialSettings: loadCharacterSettings(),
  onPreview(settings) {
    return sendSettingsChange('preview', settings);
  },
  onVoicePreview(settings) {
    return sendSettingsChange('voice-preview', settings);
  },
  onSave(settings) {
    return sendSettingsChange('saved', settings);
  },
  onClose() {
    return hideSettingsWindow();
  },
});
panel.open();

const dispose: Array<() => void> = [() => panel.dispose()];
if ('__TAURI_INTERNALS__' in window) {
  const settingsWindow = WebviewWindow.getCurrent();
  void settingsWindow
    .onCloseRequested((event) => {
      event.preventDefault();
      panel.close();
    })
    .then((unlisten) => dispose.push(unlisten));
}

window.addEventListener(
  'beforeunload',
  () => dispose.splice(0).forEach((release) => release()),
  { once: true },
);
