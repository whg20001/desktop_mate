import type { CharacterSettings } from '../config/CharacterSettings';
import type { DesktopBridge } from '../desktop/DesktopBridge';
import type { CharacterRenderer } from '../renderer/CharacterRenderer';
import { BlinkController } from './animation/BlinkController';
import { LookAtController } from './animation/LookAtController';
import { MotionController } from './animation/MotionController';
import { HitRegionController } from './interaction/HitRegionController';
import { MoeruMmdRuntime } from './mmd/MoeruMmdRuntime';
import type { PMXCompatibilityReport } from './mmd/MmdRuntime';
import type { ModelManifest } from './mmd/ModelManifest';

export class CharacterRuntime {
  private readonly mmd: MoeruMmdRuntime;
  private readonly blink: BlinkController;
  private readonly motion: MotionController;
  private readonly lookAt: LookAtController;
  private readonly hitRegions: HitRegionController;
  private lastHitRegionUpdate = Number.NEGATIVE_INFINITY;

  constructor(
    private readonly renderer: CharacterRenderer,
    private readonly bridge: DesktopBridge,
    manifest: ModelManifest,
  ) {
    this.mmd = new MoeruMmdRuntime(renderer.scene, manifest);
    this.blink = new BlinkController(this.mmd);
    this.motion = new MotionController(this.mmd);
    this.lookAt = new LookAtController(this.mmd);
    this.hitRegions = new HitRegionController(this.mmd, renderer);
    this.bridge.onCursor((cursor) => {
      this.lookAt.setTarget({
        x: cursor.x,
        y: cursor.y,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
      });
    });
    this.bridge.onCharacterState((state) => this.motion.setDesktopState(state));
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
    this.mmd.update(delta);
    this.motion.update(delta);
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

  reactToClick(): void {
    this.motion.greet();
    this.motion.talk(1.6);
  }

  talk(durationSeconds = 2.2): void {
    this.motion.talk(durationSeconds);
  }

  resize(): void {
    this.renderer.resize();
    const bounds = this.mmd.getBounds();
    if (bounds) this.renderer.fit(bounds);
    void this.publishHitRegions();
  }

  dispose(): void {
    this.motion.detach();
    this.mmd.dispose();
  }

  private async publishHitRegions(): Promise<void> {
    const payload = this.hitRegions.measure();
    if (payload) await this.bridge.updateHitRegions(payload);
  }
}
