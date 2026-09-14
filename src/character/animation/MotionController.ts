import * as THREE from 'three';
import type { CharacterState } from '../../ipc/schemas';
import type { EmotionKind } from '../../behavior/BehaviorTypes';
import type { SpeechMotionFrame } from '../../speech/SpeechTypes';
import type { CharacterBoneRole } from '../mmd/BoneMap';
import type { CharacterMorphRole } from '../mmd/MorphMap';
import type { MmdRuntime } from '../mmd/MmdRuntime';

const GREETING_SECONDS = 1.6;
const LANDING_SECONDS = 0.52;
const RESTING_ARM_DEGREES = 68;
const MOUTH_MORPHS = ['a', 'i', 'u', 'e', 'o'] as const satisfies readonly CharacterMorphRole[];

interface BonePose {
  bone: THREE.Bone;
  position: THREE.Vector3;
  rotation: THREE.Euler;
}

export class MotionController {
  private elapsed = 0;
  private mode: CharacterState['mode'] = 'idle';
  private previousState?: Pick<CharacterState, 'mode' | 'x' | 'y'>;
  private greetingElapsed = GREETING_SECONDS;
  private landingElapsed = LANDING_SECONDS;
  private talkingRemaining = 0;
  private speechFrame: SpeechMotionFrame = { active: false, level: 0 };
  private mouthWeight = 0;
  private smileWeight = 0;
  private emotionKind: EmotionKind = 'neutral';
  private emotionIntensity = 0;
  private emotionRemaining = 0;
  private breathingEnabled = false;
  private dragX = 0;
  private dragY = 0;
  private idleHeadTilt = 0;
  private idleHeadTarget = 0;
  private idleHeadHold = 2.4;
  private center?: BonePose;
  private upperBody?: BonePose;
  private neck?: BonePose;
  private leftArm?: BonePose;
  private rightArm?: BonePose;
  private rightHand?: BonePose;

  constructor(
    private readonly runtime: MmdRuntime,
    private readonly random: () => number = Math.random,
  ) {}

  attach(): void {
    this.center = this.capture('center');
    this.upperBody = this.capture('upperBody');
    this.neck = this.capture('neck');
    this.leftArm = this.capture('leftArm');
    this.rightArm = this.capture('rightArm');
    this.rightHand = this.capture('rightHand');
    this.elapsed = 0;
    this.mode = 'idle';
    this.previousState = undefined;
    this.dragX = 0;
    this.dragY = 0;
    this.greetingElapsed = GREETING_SECONDS;
    this.landingElapsed = LANDING_SECONDS;
    this.talkingRemaining = 0;
    this.speechFrame = { active: false, level: 0 };
    this.mouthWeight = 0;
    this.smileWeight = 0;
    this.emotionKind = 'neutral';
    this.emotionIntensity = 0;
    this.emotionRemaining = 0;
    this.idleHeadTilt = 0;
    this.idleHeadTarget = 0;
    this.idleHeadHold = 2.4;
    this.resetMotionMorphs();
  }

  setDesktopState(state: CharacterState): void {
    if (state.mode === 'landing' && this.mode !== 'landing') {
      this.landingElapsed = 0;
    }

    if (state.mode === 'dragged' && this.previousState?.mode === 'dragged') {
      this.dragX = THREE.MathUtils.clamp((state.x - this.previousState.x) / 80, -1, 1);
      this.dragY = THREE.MathUtils.clamp((state.y - this.previousState.y) / 80, -1, 1);
    }

    this.mode = state.mode;
    this.previousState = { mode: state.mode, x: state.x, y: state.y };
  }

  isLandingActive(): boolean {
    return this.landingElapsed < LANDING_SECONDS;
  }

  greet(): void {
    this.greetingElapsed = 0;
  }

  talk(durationSeconds = 2.2): void {
    this.talkingRemaining = Math.max(
      this.talkingRemaining,
      THREE.MathUtils.clamp(durationSeconds, 0.1, 30),
    );
  }

  setSpeechFrame(frame: SpeechMotionFrame): void {
    if (!frame.active) {
      this.talkingRemaining = 0;
      this.mouthWeight = 0;
      MOUTH_MORPHS.forEach((morph) => this.runtime.setMorph(morph, 0));
    }
    this.speechFrame = {
      active: frame.active,
      level: THREE.MathUtils.clamp(frame.level, 0, 1),
      ...(frame.viseme ? { viseme: frame.viseme } : {}),
    };
  }

  setBreathingEnabled(enabled: boolean): void {
    this.breathingEnabled = enabled;
  }

  setEmotion(kind: EmotionKind, intensity: number, durationSeconds: number): void {
    this.emotionKind = kind;
    this.emotionIntensity = THREE.MathUtils.clamp(intensity, 0, 1);
    this.emotionRemaining = THREE.MathUtils.clamp(durationSeconds, 0.25, 10);
  }

  update(delta: number): void {
    const step = THREE.MathUtils.clamp(delta, 0, 0.1);
    this.elapsed += step;
    this.greetingElapsed = Math.min(this.greetingElapsed + step, GREETING_SECONDS);
    this.landingElapsed = Math.min(this.landingElapsed + step, LANDING_SECONDS);
    this.talkingRemaining = Math.max(this.talkingRemaining - step, 0);
    this.emotionRemaining = Math.max(this.emotionRemaining - step, 0);
    if (this.emotionRemaining === 0) {
      this.emotionKind = 'neutral';
      this.emotionIntensity = 0;
    }

    this.updateIdleHead(step);
    if (this.mode !== 'dragged') {
      const dragRecovery = 1 - Math.exp(-step * 7);
      this.dragX = THREE.MathUtils.lerp(this.dragX, 0, dragRecovery);
      this.dragY = THREE.MathUtils.lerp(this.dragY, 0, dragRecovery);
    }

    let centerX = 0;
    let centerY = 0;
    let upperBodyX = 0;
    let upperBodyZ = 0;
    let neckX = 0;
    let neckZ = 0;
    let leftArmZ = THREE.MathUtils.degToRad(-RESTING_ARM_DEGREES);
    let rightArmZ = THREE.MathUtils.degToRad(RESTING_ARM_DEGREES);
    let rightHandZ = 0;

    if (this.mode === 'idle' || this.mode === 'landing') {
      const idlePhase = (this.elapsed * Math.PI * 2) / 6.8;
      centerX = Math.sin(idlePhase) * 0.025;
      upperBodyZ = THREE.MathUtils.degToRad(Math.sin(idlePhase) * 0.24);
      neckZ = this.idleHeadTilt;

      if (this.breathingEnabled) {
        const breath = Math.sin((this.elapsed * Math.PI * 2) / 4.2);
        upperBodyX += THREE.MathUtils.degToRad(0.35) * breath;
        upperBodyZ += THREE.MathUtils.degToRad(0.08) * breath;
      }
    } else if (this.mode === 'dragged') {
      upperBodyX = THREE.MathUtils.degToRad(this.dragY * 2.5);
      upperBodyZ = THREE.MathUtils.degToRad(this.dragX * -4.5);
      neckZ = THREE.MathUtils.degToRad(this.dragX * -1.2);
    } else if (this.mode === 'falling') {
      upperBodyX = THREE.MathUtils.degToRad(-2.5);
      leftArmZ = THREE.MathUtils.degToRad(-36);
      rightArmZ = THREE.MathUtils.degToRad(36);
    }

    const greetingWeight =
      this.mode === 'idle'
        ? Math.sin(
            Math.PI * THREE.MathUtils.clamp(this.greetingElapsed / GREETING_SECONDS, 0, 1),
          )
        : 0;
    if (greetingWeight > 0) {
      rightArmZ = THREE.MathUtils.lerp(
        rightArmZ,
        THREE.MathUtils.degToRad(18),
        greetingWeight,
      );
      rightHandZ =
        THREE.MathUtils.degToRad(12) *
        Math.sin(this.greetingElapsed * Math.PI * 5) *
        greetingWeight;
      neckX +=
        THREE.MathUtils.degToRad(1.8) *
        Math.sin(this.greetingElapsed * Math.PI * 3) *
        greetingWeight;
    }

    const landingWeight =
      this.mode === 'idle' || this.mode === 'landing'
        ? Math.sin(
            Math.PI * THREE.MathUtils.clamp(this.landingElapsed / LANDING_SECONDS, 0, 1),
          )
        : 0;
    if (landingWeight > 0) {
      centerY -= 0.14 * landingWeight;
      upperBodyX += THREE.MathUtils.degToRad(3.2) * landingWeight;
      leftArmZ += THREE.MathUtils.degToRad(8) * landingWeight;
      rightArmZ -= THREE.MathUtils.degToRad(8) * landingWeight;
    }

    const talking = this.talkingRemaining > 0 || this.speechFrame.active;
    if (talking) {
      const syllable = 0.5 + Math.sin(this.elapsed * Math.PI * 9) * 0.5;
      const mouthTarget = this.speechFrame.active
        ? this.speechFrame.level * 0.65
        : 0.08 + syllable * 0.3;
      neckX += THREE.MathUtils.degToRad(0.65) * Math.sin(this.elapsed * Math.PI * 2.6);
      this.mouthWeight = THREE.MathUtils.lerp(
        this.mouthWeight,
        mouthTarget,
        1 - Math.exp(-step * 18),
      );
    } else {
      this.mouthWeight = THREE.MathUtils.lerp(
        this.mouthWeight,
        0,
        1 - Math.exp(-step * 18),
      );
    }

    const emotionWeight = this.emotionRemaining > 0 ? this.emotionIntensity : 0;
    if (this.emotionKind === 'curious') {
      neckZ += THREE.MathUtils.degToRad(2.2) * emotionWeight;
    } else if (this.emotionKind === 'concerned' || this.emotionKind === 'sad') {
      neckX += THREE.MathUtils.degToRad(1.4) * emotionWeight;
    }

    const smileTarget = Math.max(
      greetingWeight * 0.5,
      talking ? 0.13 + Math.sin(this.elapsed * Math.PI * 1.7) * 0.04 : 0,
      this.emotionKind === 'happy' ? emotionWeight * 0.55 : 0,
    );
    this.smileWeight = THREE.MathUtils.lerp(
      this.smileWeight,
      smileTarget,
      1 - Math.exp(-step * 9),
    );

    const smoothing = 1 - Math.exp(-step * 9);
    this.applyPosition(this.center, 'x', centerX, smoothing);
    this.applyPosition(this.center, 'y', centerY, smoothing);
    this.applyRotation(this.upperBody, 'x', upperBodyX, smoothing);
    this.applyRotation(this.upperBody, 'z', upperBodyZ, smoothing);
    this.applyRotation(this.neck, 'x', neckX, smoothing);
    this.applyRotation(this.neck, 'z', neckZ, smoothing);
    this.applyRotation(this.leftArm, 'z', leftArmZ, smoothing);
    this.applyRotation(this.rightArm, 'z', rightArmZ, smoothing);
    this.applyRotation(this.rightHand, 'z', rightHandZ, smoothing);

    const activeMouth = this.speechFrame.active
      ? MOUTH_MORPHS.indexOf(this.speechFrame.viseme ?? 'a')
      : Math.floor(this.elapsed * 7) % MOUTH_MORPHS.length;
    MOUTH_MORPHS.forEach((morph, index) => {
      this.runtime.setMorph(morph, talking && index === activeMouth ? this.mouthWeight : 0);
    });
    this.runtime.setMorph('smile', this.smileWeight);
  }

  detach(): void {
    for (const pose of [
      this.center,
      this.upperBody,
      this.neck,
      this.leftArm,
      this.rightArm,
      this.rightHand,
    ]) {
      if (!pose) continue;
      pose.bone.position.copy(pose.position);
      pose.bone.rotation.copy(pose.rotation);
    }
    this.resetMotionMorphs();
  }

  private capture(role: CharacterBoneRole): BonePose | undefined {
    const bone = this.runtime.getBone(role);
    return bone
      ? { bone, position: bone.position.clone(), rotation: bone.rotation.clone() }
      : undefined;
  }

  private updateIdleHead(delta: number): void {
    if (this.mode !== 'idle' && this.mode !== 'landing') {
      this.idleHeadTarget = 0;
    } else {
      this.idleHeadHold -= delta;
      if (this.idleHeadHold <= 0) {
        const rest = this.random() < 0.36;
        this.idleHeadTarget = rest
          ? 0
          : THREE.MathUtils.degToRad((this.random() * 2 - 1) * 2.2);
        this.idleHeadHold = 2.8 + this.random() * 3.8;
      }
    }
    this.idleHeadTilt = THREE.MathUtils.lerp(
      this.idleHeadTilt,
      this.idleHeadTarget,
      1 - Math.exp(-delta * 2.5),
    );
  }

  private resetMotionMorphs(): void {
    MOUTH_MORPHS.forEach((morph) => this.runtime.setMorph(morph, 0));
    this.runtime.setMorph('smile', 0);
  }

  private applyPosition(
    pose: BonePose | undefined,
    axis: 'x' | 'y',
    offset: number,
    smoothing: number,
  ): void {
    if (!pose) return;
    pose.bone.position[axis] = THREE.MathUtils.lerp(
      pose.bone.position[axis],
      pose.position[axis] + offset,
      smoothing,
    );
  }

  private applyRotation(
    pose: BonePose | undefined,
    axis: 'x' | 'z',
    offset: number,
    smoothing: number,
  ): void {
    if (!pose) return;
    pose.bone.rotation[axis] = THREE.MathUtils.lerp(
      pose.bone.rotation[axis],
      pose.rotation[axis] + offset,
      smoothing,
    );
  }
}
