import './styles.css';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { loadCharacterSettings } from './config/CharacterSettings';
import { SettingsPanel } from './config/SettingsPanel';
import { hideSettingsWindow, sendSettingsChange } from './config/SettingsWindow';
import { BrainBridge } from './brain/BrainBridge';
import { runMemoryAction } from './config/MemoryAction';
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
const providerStatus = requiredElement<HTMLElement>('[data-memory-provider-status]');
const rebuildButtons = [
  ...document.querySelectorAll<HTMLButtonElement>('[data-rebuild-memory]'),
];

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
    const [memories, manager] = await Promise.all([
      brain.listMemories(memoryScope()),
      brain.memoryStatus(),
    ]);
    providerStatus.replaceChildren(
      ...manager.providers.map((provider) => {
        const row = document.createElement('p');
        row.className = 'settings-note';
        row.textContent = `${provider.id}：${provider.ready ? '就绪' : '降级'} · ${provider.detail}`;
        return row;
      }),
    );
    const queue = document.createElement('p');
    queue.className = 'settings-note';
    queue.textContent = manager.enabled
      ? `后台${manager.workerAlive ? '运行中' : '已停止'} · 待提取 ${manager.extractionQueue.pending} 条 · 最长等待 ${Math.floor(manager.extractionQueue.oldestWaitMs / 1000)} 秒 · 失败尝试 ${manager.extractionQueue.failedAttempts} 次`
      : '长期记忆已关闭';
    if (manager.enabled) queue.textContent += ` · 待同步 ${manager.indexQueue.pending} 条 · 同步失败尝试 ${manager.indexQueue.failedAttempts} 次`;
    const failure = manager.workerError || manager.extractionQueue.lastError || manager.indexQueue.lastError;
    if (failure) queue.textContent += ` · ${failure}`;
    providerStatus.append(queue);
    if (manager.enabled) {
      const retry = document.createElement('button');
      retry.type = 'button';
      retry.className = 'settings-utility-button';
      retry.textContent = '重试后台任务';
      const retryScope = memoryScope();
      retry.addEventListener('click', () => void runMemoryAction([retry], queue, async () => {
        await brain.retryMemory(retryScope);
        await renderMemories();
      }));
      providerStatus.append(retry);
    }
    if (memories.length === 0) {
      memoryList.textContent = '当前角色还没有长期记忆。';
      return;
    }
    memoryList.replaceChildren(
      ...memories.map((memory) => {
        const item = document.createElement('article');
        const scope = memoryScope();
        item.className = 'memory-item';
        const input = document.createElement('textarea');
        input.value = memory.content;
        input.maxLength = 2000;
        input.setAttribute('aria-label', '记忆内容');
        input.readOnly = memory.status === 'pending';
        const meta = document.createElement('p');
        meta.className = 'settings-note';
        meta.textContent = `${memory.kind === 'episodic' ? '情景记忆' : '语义记忆'} · 重要性 ${memory.importance.toFixed(2)} · ${memory.sources.join(' + ') || 'SQLite'}`;
        if (memory.updatedAt) meta.textContent += ` · 更新于 ${new Date(memory.updatedAt).toLocaleString()}`;
        if (memory.metadata && typeof memory.metadata === 'object' && 'evidence' in memory.metadata
          && typeof memory.metadata.evidence === 'string') meta.textContent += ` · 依据：${memory.metadata.evidence}`;
        const feedback = document.createElement('p');
        feedback.className = 'settings-note';
        feedback.setAttribute('role', 'status');
        const actions = document.createElement('div');
        const save = document.createElement('button');
        save.type = 'button';
        save.className = 'settings-utility-button';
        save.textContent = '保存修改';
        const run = (action: () => Promise<void>) => runMemoryAction(
          [...actions.querySelectorAll<HTMLButtonElement>('button')], feedback, action,
        );
        save.addEventListener('click', () => void run(async () => {
          await brain.updateMemory(scope, memory.id, input.value);
          await renderMemories();
        }));
        const remove = document.createElement('button');
        remove.type = 'button';
        remove.className = 'settings-utility-button memory-delete-button';
        remove.textContent = '删除';
        remove.addEventListener('click', () => void run(async () => {
          await brain.deleteMemory(scope, memory.id);
          item.remove();
        }));
        if (memory.status === 'pending') {
          const approve = document.createElement('button');
          approve.type = 'button';
          approve.className = 'settings-utility-button';
          approve.textContent = '批准';
          approve.addEventListener('click', () => void run(async () => {
            await brain.approveMemory(scope, memory.id);
            await renderMemories();
          }));
          const reject = document.createElement('button');
          reject.type = 'button';
          reject.className = 'settings-utility-button memory-delete-button';
          reject.textContent = '拒绝';
          reject.addEventListener('click', () => void run(async () => {
            await brain.rejectMemory(scope, memory.id);
            await renderMemories();
          }));
          actions.append(approve, reject);
        } else {
          actions.append(save, remove);
        }
        item.append(meta, input, actions, feedback);
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
for (const button of rebuildButtons) {
  button.addEventListener('click', async () => {
    const provider = button.dataset.rebuildMemory;
    if (!provider) return;
    button.disabled = true;
    try {
      const count = await brain.rebuildMemory(memoryScope(), provider);
      button.title = `已排队 ${count} 条本地记忆`;
      await renderMemories();
    } catch (error: unknown) {
      button.title = error instanceof Error ? error.message : '索引重建失败';
    } finally {
      button.disabled = false;
    }
  });
}

const dispose: Array<() => void> = [
  () => panel.dispose(),
  () => window.clearInterval(statusTimer),
  () => refreshMemories.replaceWith(refreshMemories.cloneNode(true)),
  () => rebuildButtons.forEach((button) => button.replaceWith(button.cloneNode(true))),
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
