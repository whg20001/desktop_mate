import { CharacterRuntime } from '../character/CharacterRuntime';
import {
  loadCharacterSettings,
  saveCharacterSettings,
} from '../config/CharacterSettings';
import { listenForSettingsChanges, openSettingsWindow } from '../config/SettingsWindow';
import { PointerController } from '../character/interaction/PointerController';
import { loadModelManifest, resolveAssetUrl } from '../character/mmd/ModelManifest';
import { DesktopBridge } from '../desktop/DesktopBridge';
import { CharacterRenderer } from '../renderer/CharacterRenderer';
import { RenderLoop } from '../renderer/RenderLoop';
import { SpeechController } from '../speech/SpeechController';
import { WebSpeechEngine } from '../speech/WebSpeechEngine';
import type { SpeechSource } from '../speech/SpeechTypes';
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
  const voice = new SpeechController(new WebSpeechEngine(), character);
  const speakWithSettings = (
    text: string,
    source: SpeechSource,
    speechSettings = settings,
  ): Promise<void> =>
    voice.speak({
      text,
      source,
      voiceId: speechSettings.speechVoiceId || undefined,
      language: speechSettings.speechLanguage,
      rate: speechSettings.speechRate,
      pitch: speechSettings.speechPitch,
      volume: speechSettings.speechVolume,
    });

  const missingCore = !report.bones.head || !report.morphs.blink;
  if (missingCore || report.warnings.length > 0) {
    status.dataset.kind = missingCore ? 'error' : 'warning';
    statusText.textContent = report.warnings.join(' ') || '模型已加载，但缺少部分生命感映射。';
  } else {
    statusText.textContent = `陵光已就绪 · ${report.materialCount} 个材质`;
    window.setTimeout(() => status.classList.add('is-hidden'), 1600);
  }

  const stopSettingsSync = await listenForSettingsChanges({
    onPreview(nextSettings) {
      character.applyColorSettings(nextSettings);
    },
    onVoicePreview(nextSettings) {
      const message = '你好，这是当前的声音配置。';
      if (!nextSettings.speechEnabled) {
        speech.show(nextSettings.displayName + '：角色语音当前已关闭。');
        return;
      }
      speech.show(nextSettings.displayName + '：' + message);
      void speakWithSettings(message, 'system', nextSettings).catch((error: unknown) => {
        const detail = error instanceof Error ? error.message : '未知错误';
        speech.show(nextSettings.displayName + '：语音试听失败。');
        console.warn('[speech preview]', detail);
      });
    },
    onSave(nextSettings) {
      settings = nextSettings;
      if (!settings.speechEnabled) voice.cancel();
      saveCharacterSettings(settings);
      character.applySettings(settings);
      character.talk(1.2);
      speech.show(`${settings.displayName}：配置已保存。`);
    },
  });
  const pointer = new PointerController(
    canvas,
    bridge,
    () => {
      const message = '嗯？我在这里。';
      character.reactToClick();
      speech.show(settings.displayName + '：' + message);
      if (settings.speechEnabled) {
        void speakWithSettings(message, 'interaction').catch((error: unknown) => {
          character.talk(1.6);
          console.warn('[speech]', error);
        });
      }
    },
    () => {
      void openSettingsWindow().catch((error: unknown) => {
        const message = error instanceof Error ? error.message : '无法打开设置窗口';
        speech.show(`${settings.displayName}：${message}`);
      });
    },
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
    stopSettingsSync();
    voice.dispose();
    loop.stop();
    character.dispose();
    renderer.dispose();
    bridge.dispose();
  };
}
