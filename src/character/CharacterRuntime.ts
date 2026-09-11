import type { CharacterSettings } from '../config/CharacterSettings';
import { BehaviorScheduler } from '../behavior/BehaviorScheduler';
import type { BehaviorIntent, MotionBehaviorIntent } from '../behavior/BehaviorTypes';
import type { DesktopBridge } from '../desktop/DesktopBridge';
import type { CharacterRenderer } from '../renderer/CharacterRenderer';
import type { SpeechMotionFrame, SpeechMotionTarget } from '../speech/SpeechTypes';
import { BlinkController } from './animation/BlinkController';
import { LookAtController } from './animation/LookAtController';
import { MotionController } from './animation/MotionController';
import { HitRegionController } from './interaction/HitRegionController';
import { MoeruMmdRuntime } from './mmd/MoeruMmdRuntime';
import type { PMXCompatibilityReport } from './mmd/MmdRuntime';
import type { ModelManifest } from './mmd/ModelManifest';

export class CharacterRuntime implements SpeechMotionTarget {
  private readonly mmd: MoeruMmdRuntime;
  private readonly blink: BlinkController;
  private readonly motion: MotionController;
  private readonly behavior: BehaviorScheduler;
  private readonly lookAt: LookAtController;
  private readonly hitRegions: HitRegionController;
  private readonly stopBridgeListeners: Array<() => void> = [];
  private lastHitRegionUpdate = Number.NEGATIVE_INFINITY;
  private disposed = false;

  constructor(
    private readonly renderer: CharacterRenderer,
    private readonly bridge: DesktopBridge,
    manifest: ModelManifest,
  ) {
    this.mmd = new MoeruMmdRuntime(renderer.scene, manifest);
    this.blink = new BlinkController(this.mmd);
    this.motion = new MotionController(this.mmd);
    this.behavior = new BehaviorScheduler({
      applyEmotion: (emotion) => {
        this.motion.setEmotion(emotion.kind, emotion.intensity, emotion.durationMs / 1000);
      },
      startMotion: (intent) => this.startMotion(intent),
    });
    this.lookAt = new LookAtController(this.mmd);
    this.hitRegions = new HitRegionController(this.mmd, renderer);
    this.stopBridgeListeners.push(
      this.bridge.onCursor((cursor) => {
        this.lookAt.setTarget({
          x: cursor.x,
          y: cursor.y,
          viewportWidth: window.innerWidth,
          viewportHeight: window.innerHeight,
        });
      }),
      this.bridge.onCharacterState((state) => {
        this.motion.setDesktopState(state);
        this.behavior.setPhysicalState(state.mode, performance.now());
      }),
    );
  }

  async load(modelUrl: string): Promise<PMXCompatibilityReport> {
    const report = await this.mmd.load(modelUrl);
    const bounds = this.mmd.getBounds();
    if (bounds) this.renderer.fit(bounds);
    this.motion.attach();
    this.lookAt.attach();
    await this.publishHitRegions();
    return report;
  }

  update(delta: number, timestamp: number): void {
    if (this.disposed) return;
    this.mmd.update(delta);
    this.motion.update(delta);
    if (!this.motion.isLandingActive()) {
      this.behavior.completeLanding(timestamp);
    }
    this.behavior.update(timestamp);
    this.lookAt.update(delta);
    this.blink.update(delta);

    if (timestamp - this.lastHitRegionUpdate >= 100) {
      this.lastHitRegionUpdate = timestamp;
      void this.publishHitRegions();
    }
  }

  applySettings(settings: CharacterSettings): void {
    this.motion.setBreathingEnabled(settings.breathing);
    this.lookAt.setEnabled(settings.followCursor);
    this.renderer.setDisplayScale(settings.scale);
    this.applyColorSettings(settings);
    void this.publishHitRegions();
  }

  applyColorSettings(settings: CharacterSettings): void {
    this.renderer.setLighting(
      settings.hemisphereLightIntensity,
      settings.directionalLightIntensity,
    );
    this.mmd.setMaterialAmbientScale(settings.materialAmbientScale);
  }

  applyBehavior(intent: BehaviorIntent): void {
    this.behavior.submit(intent);
  }

  private startMotion(intent: MotionBehaviorIntent): void {
    switch (intent.actionId) {
      case 'greeting':
        this.motion.greet();
        break;
      case 'talking':
        this.motion.talk(intent.durationMs / 1000);
        break;
      case 'idle':
      case 'dragged':
      case 'falling':
      case 'landing':
        break;
    }
  }

  setSpeechFrame(frame: SpeechMotionFrame): void {
    this.motion.setSpeechFrame(frame);
  }

  resize(): void {
    this.renderer.resize();
    const bounds = this.mmd.getBounds();
    if (bounds) this.renderer.fit(bounds);
    void this.publishHitRegions();
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.stopBridgeListeners.splice(0).forEach((stop) => stop());
    this.behavior.clear();
    this.motion.detach();
    this.mmd.dispose();
  }

  private async publishHitRegions(): Promise<void> {
    if (this.disposed) return;
    const payload = this.hitRegions.measure();
    if (!payload || this.disposed) return;
    try {
      await this.bridge.updateHitRegions(payload);
    } catch (error: unknown) {
      if (!this.disposed) {
        console.warn(
          '[hit regions]',
          error instanceof Error ? error.message : '更新角色点击区域失败',
        );
      }
    }
  }
}
