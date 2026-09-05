import { CHARACTER_CATALOG, findCharacterCatalogEntry } from '../character/CharacterCatalog';
import {
  MOTION_CATALOG,
  type AiMotionId,
} from '../character/animation/MotionCatalog';
import {
  characterSettingsSchema,
  DEFAULT_BRAIN_SETTINGS,
  DEFAULT_COLOR_SETTINGS,
  DEFAULT_SPEECH_SETTINGS,
  type CharacterSettings,
} from './CharacterSettings';

type SettingsPage = 'character' | 'voice' | 'motion' | 'brain';

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

function parseSettingsPage(value: string | undefined): SettingsPage | undefined {
  if (value === 'character' || value === 'voice' || value === 'motion' || value === 'brain') {
    return value;
  }
  return undefined;
}

export class SettingsPanel {
  private readonly form: HTMLFormElement;
  private readonly feedback: HTMLParagraphElement;
  private readonly scaleOutput: HTMLOutputElement;
  private readonly characterModelSelect: HTMLSelectElement;
  private readonly characterModelSummary: HTMLElement;
  private readonly voiceSelect: HTMLSelectElement;
  private readonly voicePreviewButton: HTMLButtonElement;
  private readonly tabButtons: HTMLButtonElement[];
  private readonly tabPanels: HTMLElement[];
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
    this.characterModelSelect = requiredElement(element, '[data-character-model-select]');
    this.characterModelSummary = requiredElement(element, '[data-character-model-summary]');
    this.voiceSelect = requiredElement(element, '[name="speechVoiceId"]');
    this.voicePreviewButton = requiredElement(element, '[data-preview-voice]');
    this.tabButtons = [...element.querySelectorAll<HTMLButtonElement>('[data-settings-tab]')];
    this.tabPanels = [...element.querySelectorAll<HTMLElement>('[data-settings-page]')];

    this.populateCharacterModels();
    this.populateMotionCatalog();
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
    this.activatePage('character');
    this.tabButtons[0]?.focus();
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
    this.ensureSavedCharacterModel();
    this.characterModelSelect.value = this.settings.characterModelId;
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
    requiredElement<HTMLInputElement>(this.form, '[name="aiMotionEnabled"]').checked =
      this.settings.aiMotionEnabled;
    this.form
      .querySelectorAll<HTMLInputElement>('[name="enabledAiMotionIds"]')
      .forEach((input) => {
        input.checked = this.settings.enabledAiMotionIds.includes(input.value as AiMotionId);
      });
    requiredElement<HTMLInputElement>(this.form, '[name="llmBaseUrl"]').value =
      this.settings.llmBaseUrl;
    requiredElement<HTMLInputElement>(this.form, '[name="llmModel"]').value =
      this.settings.llmModel;
    requiredElement<HTMLInputElement>(this.form, '[name="embeddingBaseUrl"]').value =
      this.settings.embeddingBaseUrl;
    requiredElement<HTMLInputElement>(this.form, '[name="embeddingModel"]').value =
      this.settings.embeddingModel;
    requiredElement<HTMLInputElement>(this.form, '[name="embeddingDimensions"]').value =
      String(this.settings.embeddingDimensions);
    requiredElement<HTMLInputElement>(this.form, '[name="memoryEnabled"]').checked =
      this.settings.memoryEnabled;
    requiredElement<HTMLInputElement>(this.form, '[name="memoryRecallEnabled"]').checked =
      this.settings.memoryRecallEnabled;
    requiredElement<HTMLInputElement>(this.form, '[name="memoryWriteEnabled"]').checked =
      this.settings.memoryWriteEnabled;
    requiredElement<HTMLInputElement>(this.form, '[name="memoryRecallLimit"]').value =
      String(this.settings.memoryRecallLimit);
    requiredElement<HTMLInputElement>(this.form, '[name="memoryApprovalRequired"]').checked =
      this.settings.memoryApprovalRequired;
    requiredElement<HTMLInputElement>(this.form, '[name="memoryMinimumImportance"]').value =
      String(this.settings.memoryMinimumImportance);
    requiredElement<HTMLInputElement>(this.form, '[name="memoryRetentionDays"]').value =
      String(this.settings.memoryRetentionDays);
    requiredElement<HTMLInputElement>(this.form, '[name="graphitiEnabled"]').checked =
      this.settings.graphitiEnabled;
    requiredElement<HTMLInputElement>(this.form, '[name="graphitiUri"]').value =
      this.settings.graphitiUri;
    requiredElement<HTMLInputElement>(this.form, '[name="graphitiDatabase"]').value =
      this.settings.graphitiDatabase;
    requiredElement<HTMLInputElement>(this.form, '[name="graphitiUser"]').value =
      this.settings.graphitiUser;

    this.scaleOutput.value = this.settings.scale.toFixed(2);
    this.updateCharacterModelSummary();
    this.updateColorOutputs();
    this.updateSpeechControls();
    this.updateMotionControls();
    this.updateBrainControls();
  }

  private parseForm() {
    const data = new FormData(this.form);
    return characterSettingsSchema.safeParse({
      characterModelId: this.characterModelSelect.value,
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
      aiMotionEnabled: data.get('aiMotionEnabled') === 'on',
      enabledAiMotionIds: data.getAll('enabledAiMotionIds').map(String),
      llmBaseUrl: String(data.get('llmBaseUrl') ?? ''),
      llmModel: String(data.get('llmModel') ?? ''),
      embeddingBaseUrl: String(data.get('embeddingBaseUrl') ?? ''),
      embeddingModel: String(data.get('embeddingModel') ?? ''),
      embeddingDimensions: Number(data.get('embeddingDimensions')),
      memoryEnabled: data.get('memoryEnabled') === 'on',
      memoryRecallEnabled: requiredElement<HTMLInputElement>(
        this.form,
        '[name="memoryRecallEnabled"]',
      ).checked,
      memoryWriteEnabled: requiredElement<HTMLInputElement>(
        this.form,
        '[name="memoryWriteEnabled"]',
      ).checked,
      memoryRecallLimit: Number(
        requiredElement<HTMLInputElement>(this.form, '[name="memoryRecallLimit"]').value,
      ),
      memoryApprovalRequired: requiredElement<HTMLInputElement>(
        this.form,
        '[name="memoryApprovalRequired"]',
      ).checked,
      memoryMinimumImportance: Number(
        requiredElement<HTMLInputElement>(this.form, '[name="memoryMinimumImportance"]').value,
      ),
      memoryRetentionDays: Number(
        requiredElement<HTMLInputElement>(this.form, '[name="memoryRetentionDays"]').value,
      ),
      graphitiEnabled: requiredElement<HTMLInputElement>(
        this.form,
        '[name="graphitiEnabled"]',
      ).checked,
      graphitiUri: requiredElement<HTMLInputElement>(
        this.form,
        '[name="graphitiUri"]',
      ).value,
      graphitiDatabase: requiredElement<HTMLInputElement>(
        this.form,
        '[name="graphitiDatabase"]',
      ).value,
      graphitiUser: requiredElement<HTMLInputElement>(
        this.form,
        '[name="graphitiUser"]',
      ).value,
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
      this.feedback.textContent = '配置格式无效，请检查模型、声音、动作和 API 参数。';
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
    if (target instanceof HTMLInputElement && target.dataset.motionSetting !== undefined) {
      this.updateMotionControls();
    }
    if (target instanceof HTMLInputElement && target.dataset.brainSetting !== undefined) {
      this.updateBrainControls();
    }
    if (target instanceof HTMLSelectElement && target.name === 'characterModelId') {
      this.updateCharacterModelSummary();
    }
    if (target instanceof HTMLSelectElement && target.name === 'speechVoiceId') {
      const language = target.selectedOptions[0]?.dataset.language;
      if (language) {
        requiredElement<HTMLInputElement>(this.form, '[name="speechLanguage"]').value = language;
      }
    }
  };

  private populateCharacterModels(): void {
    this.characterModelSelect.replaceChildren();
    for (const character of CHARACTER_CATALOG) {
      const option = document.createElement('option');
      option.value = character.id;
      option.textContent = character.displayName;
      option.disabled = !character.available;
      this.characterModelSelect.append(option);
    }
  }

  private ensureSavedCharacterModel(): void {
    const savedModelExists = [...this.characterModelSelect.options].some(
      (option) => option.value === this.settings.characterModelId,
    );
    if (savedModelExists) return;

    const unavailable = document.createElement('option');
    unavailable.value = this.settings.characterModelId;
    unavailable.textContent = '已保存的模型（当前不可用）';
    this.characterModelSelect.append(unavailable);
  }

  private updateCharacterModelSummary(): void {
    const character = findCharacterCatalogEntry(this.characterModelSelect.value);
    const title = document.createElement('h3');
    const description = document.createElement('p');
    const status = document.createElement('span');
    status.className = 'model-status';

    if (character) {
      title.textContent = character.displayName;
      description.textContent = character.description;
      status.textContent = '已安装';
      status.dataset.available = 'true';
    } else {
      title.textContent = '模型当前不可用';
      description.textContent = '请检查角色清单和模型素材是否已正确安装。';
      status.textContent = '不可用';
      status.dataset.available = 'false';
    }

    const content = document.createElement('div');
    content.append(title, description);
    this.characterModelSummary.replaceChildren(content, status);
  }

  private populateMotionCatalog(): void {
    const aiList = requiredElement<HTMLElement>(this.form, '[data-ai-motion-list]');
    const systemList = requiredElement<HTMLElement>(this.form, '[data-system-motion-list]');
    aiList.replaceChildren();
    systemList.replaceChildren();

    for (const motion of MOTION_CATALOG) {
      const content = document.createElement('div');
      const title = document.createElement('strong');
      const description = document.createElement('p');
      const scenes = document.createElement('span');
      title.textContent = motion.displayName;
      description.textContent = motion.description;
      scenes.className = 'motion-scenes';
      scenes.textContent = motion.scenes.join(' · ');
      content.append(title, description, scenes);

      if (motion.owner === 'ai') {
        const item = document.createElement('label');
        item.className = 'motion-item';
        const input = document.createElement('input');
        input.type = 'checkbox';
        input.name = 'enabledAiMotionIds';
        input.value = motion.id;
        input.dataset.motionControl = '';
        item.append(input, content);
        aiList.append(item);
      } else {
        const item = document.createElement('article');
        item.className = 'motion-item motion-item--system';
        const badge = document.createElement('span');
        badge.className = 'motion-owner-badge';
        badge.textContent = '系统接管';
        item.append(content, badge);
        systemList.append(item);
      }
    }
  }

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

  private updateMotionControls(): void {
    const enabled = requiredElement<HTMLInputElement>(
      this.form,
      '[name="aiMotionEnabled"]',
    ).checked;
    this.form
      .querySelectorAll<HTMLInputElement>('[data-motion-control]')
      .forEach((control) => {
        control.disabled = !enabled;
      });
  }

  private updateBrainControls(): void {
    const enabled = requiredElement<HTMLInputElement>(
      this.form,
      '[name="memoryEnabled"]',
    ).checked;
    this.form
      .querySelectorAll<HTMLInputElement>('[data-memory-control]')
      .forEach((control) => {
        control.disabled = !enabled;
      });
    const graphitiEnabled = enabled && requiredElement<HTMLInputElement>(
      this.form,
      '[name="graphitiEnabled"]',
    ).checked;
    this.form
      .querySelectorAll<HTMLInputElement>('[data-graphiti-control]')
      .forEach((control) => {
        control.disabled = !graphitiEnabled;
      });
  }

  private resetBrainSettings(): void {
    for (const [name, value] of Object.entries(DEFAULT_BRAIN_SETTINGS)) {
      const input = requiredElement<HTMLInputElement>(this.form, '[name="' + name + '"]');
      if (typeof value === 'boolean') input.checked = value;
      else input.value = String(value);
    }
    this.updateBrainControls();
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

  private activatePage(page: SettingsPage, focusTab = false): void {
    let activeButton: HTMLButtonElement | undefined;
    for (const button of this.tabButtons) {
      const selected = button.dataset.settingsTab === page;
      button.classList.toggle('is-active', selected);
      button.setAttribute('aria-selected', String(selected));
      button.tabIndex = selected ? 0 : -1;
      if (selected) activeButton = button;
    }
    for (const panel of this.tabPanels) {
      panel.hidden = panel.dataset.settingsPage !== page;
    }
    if (focusTab) activeButton?.focus();
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
    if (target instanceof Element) {
      const tab = target.closest<HTMLButtonElement>('[data-settings-tab]');
      const page = parseSettingsPage(tab?.dataset.settingsTab);
      if (page) {
        this.activatePage(page);
        return;
      }
      if (target.closest('[data-preview-voice]')) {
        void this.previewVoice();
        return;
      }
      if (target.closest('[data-reset-speech]')) {
        this.resetSpeechSettings();
        return;
      }
      if (target.closest('[data-reset-colors]')) {
        this.resetColorSettings();
        return;
      }
      if (target.closest('[data-reset-brain]')) {
        this.resetBrainSettings();
        return;
      }
      if (target.closest('[data-settings-close]')) {
        this.close();
        return;
      }
    }
    if (target === this.element) this.close();
  };

  private readonly onKeyDown = (event: KeyboardEvent): void => {
    if (event.key === 'Escape' && !this.element.hidden) {
      this.close();
      return;
    }

    const target = event.target;
    if (!(target instanceof HTMLButtonElement) || !target.dataset.settingsTab) return;
    const currentIndex = this.tabButtons.indexOf(target);
    if (currentIndex < 0) return;

    let nextIndex: number | undefined;
    if (event.key === 'ArrowDown' || event.key === 'ArrowRight') {
      nextIndex = (currentIndex + 1) % this.tabButtons.length;
    }
    if (event.key === 'ArrowUp' || event.key === 'ArrowLeft') {
      nextIndex = (currentIndex - 1 + this.tabButtons.length) % this.tabButtons.length;
    }
    if (event.key === 'Home') nextIndex = 0;
    if (event.key === 'End') nextIndex = this.tabButtons.length - 1;
    if (nextIndex === undefined) return;

    const page = parseSettingsPage(this.tabButtons[nextIndex]?.dataset.settingsTab);
    if (!page) return;
    event.preventDefault();
    this.activatePage(page, true);
  };
}
