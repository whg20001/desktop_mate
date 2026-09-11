import { CharacterRuntime } from '../character/CharacterRuntime';
import { BehaviorPlanner } from '../behavior/BehaviorPlanner';
import { BrainBridge } from '../brain/BrainBridge';
import { brainSettingsFromCharacter } from '../brain/BrainSettings';
import { getCharacterCatalogEntry } from '../character/CharacterCatalog';
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
import { ConversationPanel } from '../ui/ConversationPanel';

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
  const brain = new BrainBridge();
  const behavior = new BehaviorPlanner();
  let settings = loadCharacterSettings();
  const characterModel = getCharacterCatalogEntry(settings.characterModelId);
  const manifestUrl = characterModel.manifestUrl;
  const bridge = new DesktopBridge();
  const renderer = new CharacterRenderer(canvas);

  statusText.textContent = '正在连接 Windows 桌面…';
  await bridge.connect();
  void brain.configure(brainSettingsFromCharacter(settings)).catch((error: unknown) => {
    console.warn('[brain configuration]', error instanceof Error ? error.message : '配置失败');
  });

  statusText.textContent = `正在加载${characterModel.displayName}模型…`;
  const manifest = await loadModelManifest(manifestUrl);
  const character = new CharacterRuntime(renderer, bridge, manifest);
  const report = await character.load(resolveAssetUrl(manifestUrl, manifest.model));
  character.applySettings(settings);
  const voice = new SpeechController(new WebSpeechEngine(), character);
  const sessionId =
    window.sessionStorage.getItem('desktop-companion.conversation-session') ??
    crypto.randomUUID();
  window.sessionStorage.setItem('desktop-companion.conversation-session', sessionId);
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
    statusText.textContent = `${characterModel.displayName}已就绪 · ${report.materialCount} 个材质`;
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
      const modelChanged = nextSettings.characterModelId !== characterModel.id;
      settings = nextSettings;
      if (!settings.speechEnabled) voice.cancel();
      saveCharacterSettings(settings);
      character.applySettings(settings);
      character.applyBehavior(behavior.audio(1_200));
      const restartNote = modelChanged ? ' 模型将在下次启动时切换。' : '';
      speech.show(`${settings.displayName}：配置已保存。${restartNote}`);
    },
  });
  const pointer = new PointerController(
    canvas,
    bridge,
    () => {
      const message = '嗯？我在这里。';
      character.applyBehavior(behavior.interaction('greeting'));
      speech.show(settings.displayName + '：' + message);
      conversation.focus();
      if (settings.speechEnabled) {
        void speakWithSettings(message, 'interaction').catch((error: unknown) => {
          character.applyBehavior(behavior.audio(1_600));
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
  const conversation = new ConversationPanel(
    requiredElement<HTMLFormElement>('#conversation-panel'),
    async (userInput) => {
      speech.show(settings.displayName + '：正在思考…', 30_000);
      try {
        const response = await brain.converse(
          userInput,
          {
            userId: 'local-user',
            characterId: settings.characterModelId,
            sessionId,
          },
          settings.aiMotionEnabled ? settings.enabledAiMotionIds : [],
        );
        speech.show(settings.displayName + '：' + response.text, 6_000);
        character.applyBehavior(behavior.resolve(response, settings));
        if (settings.speechEnabled && response.speech?.text) {
          void speakWithSettings(response.speech.text, 'ai').catch((error: unknown) => {
            console.warn('[speech]', error instanceof Error ? error.message : '播放失败');
          });
        }
        return true;
      } catch (error: unknown) {
        const message = error instanceof Error ? error.message : 'Brain 当前不可用';
        speech.show(settings.displayName + '：' + message, 5_000);
        return false;
      }
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
    conversation.dispose();
    stopSettingsSync();
    voice.dispose();
    loop.stop();
    character.dispose();
    renderer.dispose();
    bridge.dispose();
  };
}
