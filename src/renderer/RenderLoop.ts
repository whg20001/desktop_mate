export class RenderLoop {
  private requestId?: number;
  private lastFrame = 0;
  private previousTimestamp?: number;

  constructor(private readonly onFrame: (delta: number) => void) {}

  start(): void {
    if (this.requestId !== undefined) return;
    this.previousTimestamp = undefined;
    this.requestId = requestAnimationFrame(this.frame);
  }

  stop(): void {
    if (this.requestId !== undefined) cancelAnimationFrame(this.requestId);
    this.requestId = undefined;
    this.previousTimestamp = undefined;
  }

  private readonly frame = (timestamp: number): void => {
    const targetInterval = document.hidden ? 1000 / 30 : 1000 / 60;
    if (timestamp - this.lastFrame >= targetInterval - 1) {
      const delta = Math.min(
        this.previousTimestamp === undefined ? 0 : (timestamp - this.previousTimestamp) / 1000,
        1 / 20,
      );
      this.previousTimestamp = timestamp;
      this.lastFrame = timestamp;
      this.onFrame(delta);
    }
    this.requestId = requestAnimationFrame(this.frame);
  };
}
