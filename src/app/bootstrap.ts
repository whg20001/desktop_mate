import { CharacterRuntime } from '../character/CharacterRuntime';
import { loadCharacterSettings } from '../config/CharacterSettings';
import { SettingsPanel } from '../config/SettingsPanel';
import { PointerController } from '../character/interaction/PointerController';
import { loadModelManifest, resolveAssetUrl } from '../character/mmd/ModelManifest';
import { DesktopBridge } from '../desktop/DesktopBridge';
import { CharacterRenderer } from '../renderer/CharacterRenderer';
import { RenderLoop } from '../renderer/RenderLoop';
import { SpeechBubble } from '../ui/SpeechBubble';

const MANIFEST_URL = '/LinGuang/manifest.json';

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) throw new Error(`缺少页面元素: ${selector}`);
  return element;
}

export async function bootstrap(): Promise<() => void> {
  const canvas = requiredElement<HTMLCanvasElement>('#character-canvas');
  const status = requiredElement<HTMLDivElement>('#status');
  const statusText = requiredElement<HTMLSpanElement>('[data-status-text]');
  const speech = new SpeechBubble(requiredElement<HTMLDivElement>('#speech-bubble'));
  let settings = loadCharacterSettings();
  const bridge = new DesktopBridge();
  const renderer = new CharacterRenderer(canvas);

  statusText.textContent = '正在连接 Windows 桌面…';
  await bridge.connect();

  statusText.textContent = '正在加载陵光模型…';
  const manifest = await loadModelManifest(MANIFEST_URL);
  const character = new CharacterRuntime(renderer, bridge, manifest);
  const report = await character.load(resolveAssetUrl(MANIFEST_URL, manifest.model));
  character.applySettings(settings);

  const missingCore = !report.bones.head || !report.morphs.blink;
  if (missingCore || report.warnings.length > 0) {
    status.dataset.kind = missingCore ? 'error' : 'warning';
    statusText.textContent = report.warnings.join(' ') || '模型已加载，但缺少部分生命感映射。';
  } else {
    statusText.textContent = `陵光已就绪 · ${report.materialCount} 个材质`;
    window.setTimeout(() => status.classList.add('is-hidden'), 1600);
  }

  const settingsPanel = new SettingsPanel(requiredElement<HTMLElement>('#settings-panel'), {
    initialSettings: settings,
    onPreview(nextSettings) {
      character.applyColorSettings(nextSettings);
    },
    onSave(nextSettings) {
      settings = nextSettings;
      character.applySettings(settings);
      character.talk(1.2);
      speech.show(`${settings.displayName}：配置已保存。`);
    },
    onVisibilityChange(visible) {
      void bridge.setInteractionLocked(visible);
    },
  });
  const pointer = new PointerController(
    canvas,
    bridge,
    () => {
      character.reactToClick();
      speech.show(`${settings.displayName}：嗯？我在这里。`);
    },
    () => settingsPanel.open(),
  );
  pointer.attach();

  const loop = new RenderLoop((delta) => {
    character.update(delta, performance.now());
    renderer.render();
  });
  loop.start();

  const onResize = (): void => character.resize();
  window.addEventListener('resize', onResize);

  return () => {
    window.removeEventListener('resize', onResize);
    pointer.detach();
    settingsPanel.dispose();
    loop.stop();
    character.dispose();
    renderer.dispose();
    bridge.dispose();
  };
}

