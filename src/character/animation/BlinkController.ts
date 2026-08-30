import type { MmdRuntime } from '../mmd/MmdRuntime';

type BlinkPhase = 'waiting' | 'closing' | 'opening';

export class BlinkController {
  private phase: BlinkPhase = 'waiting';
  private elapsed = 0;
  private nextBlink = 0;
  private doubleBlinkPending = false;

  constructor(
    private readonly runtime: MmdRuntime,
    private readonly random: () => number = Math.random,
  ) {
    this.schedule();
  }

  update(delta: number): void {
    this.elapsed += delta;

    if (this.phase === 'waiting') {
      if (this.elapsed >= this.nextBlink) this.beginBlink();
      return;
    }

    const duration = this.phase === 'closing' ? 0.065 : 0.095;
    const progress = Math.min(this.elapsed / duration, 1);
    this.runtime.setMorph('blink', this.phase === 'closing' ? progress : 1 - progress);

    if (progress < 1) return;
    if (this.phase === 'closing') {
      this.phase = 'opening';
      this.elapsed = 0;
      return;
    }

    this.runtime.setMorph('blink', 0);
    if (this.doubleBlinkPending) {
      this.doubleBlinkPending = false;
      this.phase = 'waiting';
      this.elapsed = 0;
      this.nextBlink = 0.12;
    } else {
      this.schedule();
    }
  }

  private beginBlink(): void {
    this.phase = 'closing';
    this.elapsed = 0;
    this.doubleBlinkPending = this.random() < 0.16;
  }

  private schedule(): void {
    this.phase = 'waiting';
    this.elapsed = 0;
    this.nextBlink = 2.5 + this.random() * 3.5;
  }
}

