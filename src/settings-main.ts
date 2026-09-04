import './styles.css';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { loadCharacterSettings } from './config/CharacterSettings';
import { SettingsPanel } from './config/SettingsPanel';
import { hideSettingsWindow, sendSettingsChange } from './config/SettingsWindow';
import { BrainBridge } from './brain/BrainBridge';
import { brainSettingsFromCharacter } from './brain/BrainSettings';

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error('缺少设置窗口元素: ' + selector);
  return element;
}

const brain = new BrainBridge();
let currentSettings = loadCharacterSettings();
const statusBadge = requiredElement<HTMLElement>('.settings-status-badge');

async function refreshBrainStatus(): Promise<void> {
  try {
    const status = await brain.status();
    statusBadge.dataset.kind = status.phase;
    statusBadge.textContent = status.ready ? 'Brain 已就绪' : 'Brain ' + status.phase;
    statusBadge.title = status.detail;
  } catch {
    statusBadge.dataset.kind = 'failed';
    statusBadge.textContent = 'Brain 状态不可用';
  }
}

const panel = new SettingsPanel(requiredElement<HTMLElement>('#settings-panel'), {
  initialSettings: currentSettings,
  onPreview(settings) {
    return sendSettingsChange('preview', settings);
  },
  onVoicePreview(settings) {
    return sendSettingsChange('voice-preview', settings);
  },
  async onSave(settings) {
    try {
      await brain.configure(brainSettingsFromCharacter(settings));
    } catch (error: unknown) {
      const detail = error instanceof Error ? error.message : String(error);
      statusBadge.dataset.kind = 'failed';
      statusBadge.textContent = 'Brain 配置未应用';
      statusBadge.title = detail;
      console.warn('[brain configuration]', detail);
    }
    currentSettings = settings;
    await sendSettingsChange('saved', settings);
  },
  onClose() {
    return hideSettingsWindow();
  },
});
panel.open();
void refreshBrainStatus();
const statusTimer = window.setInterval(() => void refreshBrainStatus(), 2000);
const memoryList = requiredElement<HTMLElement>('[data-memory-list]');
const refreshMemories = requiredElement<HTMLButtonElement>('[data-refresh-memories]');

function memoryScope() {
  return {
    userId: 'local-user',
    characterId: currentSettings.characterModelId,
    sessionId: 'settings',
  };
}

async function renderMemories(): Promise<void> {
  refreshMemories.disabled = true;
  memoryList.textContent = '正在读取本地记忆…';
  try {
    const memories = await brain.listMemories(memoryScope());
    if (memories.length === 0) {
      memoryList.textContent = '当前角色还没有长期记忆。';
      return;
    }
    memoryList.replaceChildren(
      ...memories.map((memory) => {
        const item = document.createElement('article');
        item.className = 'memory-item';
        const input = document.createElement('textarea');
        input.value = memory.content;
        input.maxLength = 2000;
        input.setAttribute('aria-label', '记忆内容');
        const actions = document.createElement('div');
        const save = document.createElement('button');
        save.type = 'button';
        save.className = 'settings-utility-button';
        save.textContent = '保存修改';
        save.addEventListener('click', async () => {
          save.disabled = true;
          try {
            await brain.updateMemory(memoryScope(), memory.id, input.value);
          } finally {
            save.disabled = false;
          }
        });
        const remove = document.createElement('button');
        remove.type = 'button';
        remove.className = 'settings-utility-button memory-delete-button';
        remove.textContent = '删除';
        remove.addEventListener('click', async () => {
          remove.disabled = true;
          try {
            await brain.deleteMemory(memoryScope(), memory.id);
            item.remove();
          } finally {
            remove.disabled = false;
          }
        });
        actions.append(save, remove);
        item.append(input, actions);
        return item;
      }),
    );
  } catch (error: unknown) {
    memoryList.textContent =
      error instanceof Error ? error.message : '无法读取本地长期记忆';
  } finally {
    refreshMemories.disabled = false;
  }
}

refreshMemories.addEventListener('click', () => void renderMemories());

const dispose: Array<() => void> = [
  () => panel.dispose(),
  () => window.clearInterval(statusTimer),
  () => refreshMemories.replaceWith(refreshMemories.cloneNode(true)),
];
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
