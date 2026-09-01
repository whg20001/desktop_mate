import {
  characterSettingsSchema,
  DEFAULT_COLOR_SETTINGS,
  DEFAULT_SPEECH_SETTINGS,
  type CharacterSettings,
} from './CharacterSettings';

interface SettingsPanelOptions {
  initialSettings: CharacterSettings;
  onPreview: (settings: CharacterSettings) => void | Promise<void>;
  onVoicePreview: (settings: CharacterSettings) => void | Promise<void>;
  onSave: (settings: CharacterSettings) => void | Promise<void>;
  onClose: () => void | Promise<void>;
}

function requiredElement<T extends Element>(root: ParentNode, selector: string): T {
  const element = root.querySelector<T>(selector);
  if (!element) throw new Error(`缺少配置界面元素: ${selector}`);
  return element;
}

export class SettingsPanel {
  private readonly form: HTMLFormElement;
  private readonly feedback: HTMLParagraphElement;
  private readonly scaleOutput: HTMLOutputElement;
  private readonly voiceSelect: HTMLSelectElement;
  private readonly voicePreviewButton: HTMLButtonElement;
  private settings: CharacterSettings;
  private closing = false;

  constructor(
    private readonly element: HTMLElement,
    private readonly options: SettingsPanelOptions,
  ) {
    this.settings = options.initialSettings;
    this.form = requiredElement(element, '#settings-form');
    this.feedback = requiredElement(element, '[data-settings-feedback]');
    this.scaleOutput = requiredElement(element, '[data-scale-output]');
    this.voiceSelect = requiredElement(element, '[name="speechVoiceId"]');
    this.voicePreviewButton = requiredElement(element, '[data-preview-voice]');
    this.element.addEventListener('click', this.onPanelClick);
    this.form.addEventListener('submit', this.onSubmit);
    this.form.addEventListener('input', this.onInput);
    window.addEventListener('keydown', this.onKeyDown);
    if ('speechSynthesis' in window) {
      window.speechSynthesis.addEventListener('voiceschanged', this.onVoicesChanged);
    }
  }

  open(): void {
    this.populateVoices();
    this.writeForm();
    this.feedback.textContent = '';
    this.element.hidden = false;
    requiredElement<HTMLInputElement>(this.form, '[name="displayName"]').focus();
  }

  close(): void {
    void this.finishClose(true);
  }

  dispose(): void {
    this.element.removeEventListener('click', this.onPanelClick);
    this.form.removeEventListener('submit', this.onSubmit);
    this.form.removeEventListener('input', this.onInput);
    window.removeEventListener('keydown', this.onKeyDown);
    if ('speechSynthesis' in window) {
      window.speechSynthesis.removeEventListener('voiceschanged', this.onVoicesChanged);
    }
  }

  private writeForm(): void {
    requiredElement<HTMLInputElement>(this.form, '[name="displayName"]').value =
      this.settings.displayName;
    requiredElement<HTMLInputElement>(this.form, '[name="scale"]').value =
      String(this.settings.scale);
    requiredElement<HTMLInputElement>(this.form, '[name="followCursor"]').checked =
      this.settings.followCursor;
    requiredElement<HTMLInputElement>(this.form, '[name="breathing"]').checked =
      this.settings.breathing;
    requiredElement<HTMLInputElement>(this.form, '[name="materialAmbientScale"]').value =
      String(this.settings.materialAmbientScale);
    requiredElement<HTMLInputElement>(this.form, '[name="hemisphereLightIntensity"]').value =
      String(this.settings.hemisphereLightIntensity);
    requiredElement<HTMLInputElement>(this.form, '[name="directionalLightIntensity"]').value =
      String(this.settings.directionalLightIntensity);
    requiredElement<HTMLInputElement>(this.form, '[name="speechEnabled"]').checked =
      this.settings.speechEnabled;
    this.voiceSelect.value = this.settings.speechVoiceId;
    requiredElement<HTMLInputElement>(this.form, '[name="speechLanguage"]').value =
      this.settings.speechLanguage;
    requiredElement<HTMLInputElement>(this.form, '[name="speechRate"]').value =
      String(this.settings.speechRate);
    requiredElement<HTMLInputElement>(this.form, '[name="speechPitch"]').value =
      String(this.settings.speechPitch);
    requiredElement<HTMLInputElement>(this.form, '[name="speechVolume"]').value =
      String(this.settings.speechVolume);
    requiredElement<HTMLInputElement>(this.form, '[name="apiBaseUrl"]').value =
      this.settings.apiBaseUrl;
    requiredElement<HTMLInputElement>(this.form, '[name="apiModel"]').value =
      this.settings.apiModel;
    this.scaleOutput.value = this.settings.scale.toFixed(2);
    this.updateColorOutputs();
    this.updateSpeechControls();
  }

  private parseForm() {
    const data = new FormData(this.form);
    return characterSettingsSchema.safeParse({
      displayName: String(data.get('displayName') ?? ''),
      scale: Number(data.get('scale')),
      followCursor: data.get('followCursor') === 'on',
      breathing: data.get('breathing') === 'on',
      materialAmbientScale: Number(data.get('materialAmbientScale')),
      hemisphereLightIntensity: Number(data.get('hemisphereLightIntensity')),
      directionalLightIntensity: Number(data.get('directionalLightIntensity')),
      speechEnabled: data.get('speechEnabled') === 'on',
      speechVoiceId: this.voiceSelect.value,
      speechLanguage: requiredElement<HTMLInputElement>(
        this.form,
        '[name="speechLanguage"]',
      ).value,
      speechRate: Number(
        requiredElement<HTMLInputElement>(this.form, '[name="speechRate"]').value,
      ),
      speechPitch: Number(
        requiredElement<HTMLInputElement>(this.form, '[name="speechPitch"]').value,
      ),
      speechVolume: Number(
        requiredElement<HTMLInputElement>(this.form, '[name="speechVolume"]').value,
      ),
      apiBaseUrl: String(data.get('apiBaseUrl') ?? ''),
      apiModel: String(data.get('apiModel') ?? ''),
    });
  }

  private readonly onSubmit = (event: SubmitEvent): void => {
    event.preventDefault();
    void this.submit();
  };

  private readonly onVoicesChanged = (): void => {
    this.populateVoices(this.voiceSelect.value);
  };

  private async submit(): Promise<void> {
    const parsed = this.parseForm();
    if (!parsed.success) {
      this.feedback.textContent = '配置格式无效，请检查基础、声音和 API 参数。';
      return;
    }

    try {
      await this.options.onSave(parsed.data);
      this.settings = parsed.data;
      await this.finishClose(false);
    } catch {
      this.feedback.textContent = '配置保存失败，请稍后重试。';
    }
  }

  private readonly onInput = (event: Event): void => {
    const target = event.target;
    if (target instanceof HTMLInputElement && target.name === 'scale') {
      this.scaleOutput.value = Number(target.value).toFixed(2);
    }
    if (target instanceof HTMLInputElement && target.dataset.colorSetting !== undefined) {
      this.updateColorOutputs();
      const parsed = this.parseForm();
      if (parsed.success) void this.preview(parsed.data);
    }
    if (target instanceof HTMLInputElement && target.dataset.speechSetting !== undefined) {
      this.updateSpeechControls();
    }
    if (target instanceof HTMLSelectElement && target.name === 'speechVoiceId') {
      const language = target.selectedOptions[0]?.dataset.language;
      if (language) {
        requiredElement<HTMLInputElement>(this.form, '[name="speechLanguage"]').value = language;
      }
    }
  };

  private updateColorOutputs(): void {
    for (const name of [
      'materialAmbientScale',
      'hemisphereLightIntensity',
      'directionalLightIntensity',
    ] as const) {
      const input = requiredElement<HTMLInputElement>(
        this.form,
        '[name="' + name + '"]',
      );
      requiredElement<HTMLOutputElement>(
        this.form,
        '[data-value-for="' + name + '"]',
      ).value = Number(input.value).toFixed(2);
    }
  }

  private populateVoices(selectedVoiceId = this.settings.speechVoiceId): void {
    this.voiceSelect.replaceChildren();
    const defaultOption = document.createElement('option');
    defaultOption.value = '';
    defaultOption.textContent = '跟随系统默认';
    this.voiceSelect.append(defaultOption);

    const voices =
      'speechSynthesis' in window
        ? [...window.speechSynthesis.getVoices()].sort(
            (left, right) =>
              left.lang.localeCompare(right.lang) || left.name.localeCompare(right.name),
          )
        : [];
    const ids = new Set<string>();
    for (const voice of voices) {
      const id = voice.voiceURI || voice.name;
      if (ids.has(id)) continue;
      ids.add(id);
      const option = document.createElement('option');
      option.value = id;
      option.dataset.language = voice.lang;
      option.textContent =
        voice.name + ' · ' + voice.lang + (voice.default ? '（系统默认）' : '');
      this.voiceSelect.append(option);
    }

    if (selectedVoiceId && !ids.has(selectedVoiceId)) {
      const unavailable = document.createElement('option');
      unavailable.value = selectedVoiceId;
      unavailable.textContent = '已保存的音色（当前不可用）';
      this.voiceSelect.append(unavailable);
    }
    this.voiceSelect.value = selectedVoiceId;
  }

  private updateSpeechControls(): void {
    for (const name of ['speechRate', 'speechPitch', 'speechVolume'] as const) {
      const input = requiredElement<HTMLInputElement>(
        this.form,
        '[name="' + name + '"]',
      );
      requiredElement<HTMLOutputElement>(
        this.form,
        '[data-value-for="' + name + '"]',
      ).value = Number(input.value).toFixed(2);
    }

    const enabled = requiredElement<HTMLInputElement>(
      this.form,
      '[name="speechEnabled"]',
    ).checked;
    this.form
      .querySelectorAll<HTMLInputElement | HTMLSelectElement>('[data-speech-control]')
      .forEach((control) => {
        control.disabled = !enabled;
      });
    this.voicePreviewButton.disabled = !enabled;
  }

  private resetSpeechSettings(): void {
    requiredElement<HTMLInputElement>(this.form, '[name="speechEnabled"]').checked =
      DEFAULT_SPEECH_SETTINGS.speechEnabled;
    this.voiceSelect.value = DEFAULT_SPEECH_SETTINGS.speechVoiceId;
    for (const name of [
      'speechLanguage',
      'speechRate',
      'speechPitch',
      'speechVolume',
    ] as const) {
      requiredElement<HTMLInputElement>(this.form, '[name="' + name + '"]').value =
        String(DEFAULT_SPEECH_SETTINGS[name]);
    }
    this.updateSpeechControls();
  }

  private async previewVoice(): Promise<void> {
    const parsed = this.parseForm();
    if (!parsed.success) {
      this.feedback.textContent = '声音配置格式无效，请检查语言和调节范围。';
      return;
    }
    try {
      await this.options.onVoicePreview(parsed.data);
      this.feedback.textContent = '试听请求已发送，请在角色窗口确认声音和嘴型。';
    } catch {
      this.feedback.textContent = '无法发送试听请求，请检查角色窗口连接。';
    }
  }

  private resetColorSettings(): void {
    for (const [name, value] of Object.entries(DEFAULT_COLOR_SETTINGS)) {
      requiredElement<HTMLInputElement>(this.form, '[name="' + name + '"]').value =
        String(value);
    }
    this.updateColorOutputs();
    const parsed = this.parseForm();
    if (parsed.success) void this.preview(parsed.data);
  }

  private async preview(settings: CharacterSettings): Promise<void> {
    try {
      await this.options.onPreview(settings);
    } catch {
      this.feedback.textContent = '实时预览失败，请检查设置窗口连接。';
    }
  }

  private async finishClose(restoreSavedSettings: boolean): Promise<void> {
    if (this.closing) return;
    this.closing = true;
    try {
      if (restoreSavedSettings) {
        await this.preview(this.settings);
        this.writeForm();
      }
      await this.options.onClose();
    } finally {
      this.closing = false;
    }
  }

  private readonly onPanelClick = (event: MouseEvent): void => {
    const target = event.target;
    if (target instanceof Element && target.closest('[data-preview-voice]')) {
      void this.previewVoice();
      return;
    }
    if (target instanceof Element && target.closest('[data-reset-speech]')) {
      this.resetSpeechSettings();
      return;
    }
    if (target instanceof Element && target.closest('[data-reset-colors]')) {
      this.resetColorSettings();
      return;
    }
    if (target === this.element || (target instanceof Element && target.closest('[data-settings-close]'))) {
      this.close();
    }
  };

  private readonly onKeyDown = (event: KeyboardEvent): void => {
    if (event.key === 'Escape' && !this.element.hidden) this.close();
  };
}
