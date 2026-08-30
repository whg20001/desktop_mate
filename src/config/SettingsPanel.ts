import {
  characterSettingsSchema,
  DEFAULT_COLOR_SETTINGS,
  saveCharacterSettings,
  type CharacterSettings,
} from './CharacterSettings';

interface SettingsPanelOptions {
  initialSettings: CharacterSettings;
  onPreview: (settings: CharacterSettings) => void;
  onSave: (settings: CharacterSettings) => void;
  onVisibilityChange: (visible: boolean) => void;
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
  private settings: CharacterSettings;

  constructor(
    private readonly element: HTMLElement,
    private readonly options: SettingsPanelOptions,
  ) {
    this.settings = options.initialSettings;
    this.form = requiredElement(element, '#settings-form');
    this.feedback = requiredElement(element, '[data-settings-feedback]');
    this.scaleOutput = requiredElement(element, '[data-scale-output]');
    this.element.addEventListener('click', this.onPanelClick);
    this.form.addEventListener('submit', this.onSubmit);
    this.form.addEventListener('input', this.onInput);
    window.addEventListener('keydown', this.onKeyDown);
  }

  open(): void {
    this.writeForm();
    this.feedback.textContent = '';
    this.element.hidden = false;
    this.options.onVisibilityChange(true);
    requiredElement<HTMLInputElement>(this.form, '[name="displayName"]').focus();
  }

  close(): void {
    this.hide(true);
  }

  private hide(restoreSavedSettings: boolean): void {
    if (this.element.hidden) return;
    if (restoreSavedSettings) this.options.onPreview(this.settings);
    this.element.hidden = true;
    this.options.onVisibilityChange(false);
  }

  dispose(): void {
    this.close();
    this.element.removeEventListener('click', this.onPanelClick);
    this.form.removeEventListener('submit', this.onSubmit);
    this.form.removeEventListener('input', this.onInput);
    window.removeEventListener('keydown', this.onKeyDown);
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
    requiredElement<HTMLInputElement>(this.form, '[name="apiBaseUrl"]').value =
      this.settings.apiBaseUrl;
    requiredElement<HTMLInputElement>(this.form, '[name="apiModel"]').value =
      this.settings.apiModel;
    this.scaleOutput.value = this.settings.scale.toFixed(2);
    this.updateColorOutputs();
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
      apiBaseUrl: String(data.get('apiBaseUrl') ?? ''),
      apiModel: String(data.get('apiModel') ?? ''),
    });
  }

  private readonly onSubmit = (event: SubmitEvent): void => {
    event.preventDefault();
    const parsed = this.parseForm();
    if (!parsed.success) {
      this.feedback.textContent = '配置格式无效，请检查名称、比例和 API 参数。';
      return;
    }

    this.settings = parsed.data;
    saveCharacterSettings(this.settings);
    this.options.onSave(this.settings);
    this.hide(false);
  };

  private readonly onInput = (event: Event): void => {
    const target = event.target;
    if (target instanceof HTMLInputElement && target.name === 'scale') {
      this.scaleOutput.value = Number(target.value).toFixed(2);
    }
    if (target instanceof HTMLInputElement && target.dataset.colorSetting !== undefined) {
      this.updateColorOutputs();
      const parsed = this.parseForm();
      if (parsed.success) this.options.onPreview(parsed.data);
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

  private resetColorSettings(): void {
    for (const [name, value] of Object.entries(DEFAULT_COLOR_SETTINGS)) {
      requiredElement<HTMLInputElement>(this.form, '[name="' + name + '"]').value =
        String(value);
    }
    this.updateColorOutputs();
    const parsed = this.parseForm();
    if (parsed.success) this.options.onPreview(parsed.data);
  }

  private readonly onPanelClick = (event: MouseEvent): void => {
    const target = event.target;
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
