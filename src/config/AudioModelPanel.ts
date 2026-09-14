import { audioError, getAudioTimings, formatAudioTimings, getAudioModels, isNativeAudio, selectAudioModel, type AudioModelStatus } from '../speech/AudioIpc';

export class AudioModelPanel {
  private readonly select: HTMLSelectElement;
  private readonly button: HTMLButtonElement;
  private readonly status: HTMLElement;
  private readonly timings: HTMLElement | null;
  private readonly summary: HTMLElement | null;
  private readonly timer: ReturnType<typeof setInterval> | undefined;
  private disposed = false;
  private polling = false;
  private switching = false;
  private version = 0;
  private current?: AudioModelStatus;

  constructor(root: HTMLElement, private readonly onChange: (status: AudioModelStatus) => void) {
    this.select = root.querySelector<HTMLSelectElement>('[data-audio-model-select]')!;
    this.button = root.querySelector<HTMLButtonElement>('[data-audio-model-apply]')!;
    this.status = root.querySelector<HTMLElement>('[data-audio-model-status]')!;
    this.summary = root.querySelector('[data-audio-summary]');
    this.timings = root.querySelector('[data-audio-timings]');
    this.button.addEventListener('click', this.switchModel);
    if (isNativeAudio()) {
      void this.refresh();
      this.timer = setInterval(() => void this.refresh(), 1500);
    } else {
      this.select.disabled = true; this.button.disabled = true;
      this.status.textContent = '浏览器预览使用 Web Speech；模型管理请在桌宠中打开。';
    }
  }

  private async refresh(): Promise<void> {
    if (this.polling || this.switching || this.disposed) return;
    this.polling = true;
    const version = this.version;
    try {
      const status = await getAudioModels();
      if (!this.disposed && version === this.version) this.render(status);
      const timings = await getAudioTimings();
      if (!this.disposed && version === this.version && this.timings) this.timings.textContent = formatAudioTimings(timings);
    } catch (error) {
      if (!this.disposed && version === this.version) this.status.textContent = audioError(error).message;
    } finally { this.polling = false; }
  }

  private render(status: AudioModelStatus): void {
    const changed = this.current?.selectedModel !== status.selectedModel || this.current?.phase !== status.phase;
    if (!this.current || this.current.selectedModel !== status.selectedModel) {
      this.select.replaceChildren();
      for (const model of [{ id: 'windows', name: 'Windows 系统语音' }, ...status.models]) {
        const option = document.createElement('option');
        option.value = model.id; option.textContent = model.name;
        this.select.append(option);
      }
      this.select.value = status.selectedModel;
    }
    this.current = status;
    const phase = { loading: '加载中', ready: '已就绪', failed: '启动失败' }[status.phase];
    this.status.textContent = [status.name + ' · ' + phase, status.device,
      status.speakers.length ? status.speakers.length + ' 个可用音色' : '', status.detail].filter(Boolean).join(' · ');
    this.status.dataset.kind = status.phase;
    if (this.summary) this.summary.textContent = '语音：' + status.name + ' · ' + phase;
    this.button.textContent = status.phase === 'failed' ? '切换 / 重试启动' : '启动并切换';
    if (changed) this.onChange(status);
  }

  private readonly switchModel = async (): Promise<void> => {
    if (this.switching || this.disposed) return;
    this.switching = true; this.version++;
    this.button.disabled = true; this.select.disabled = true;
    this.status.textContent = '正在停止旧语音并切换模型…';
    try {
      const status = await selectAudioModel(this.select.value);
      if (!this.disposed) this.render(status);
    } catch (error) {
      if (!this.disposed) this.status.textContent = audioError(error).message;
    } finally {
      this.switching = false;
      if (!this.disposed) { this.button.disabled = false; this.select.disabled = false; }
    }
  };

  dispose(): void {
    this.disposed = true; this.version++;
    if (this.timer) clearInterval(this.timer);
    this.button.removeEventListener('click', this.switchModel);
  }
}
