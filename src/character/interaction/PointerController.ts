import type { DesktopBridge } from '../../desktop/DesktopBridge';

export class PointerController {
  private pointerId?: number;
  private start?: { x: number; y: number; time: number };
  private clickTimer?: number;

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly bridge: DesktopBridge,
    private readonly onClick: () => void,
    private readonly onDoubleClick: () => void,
  ) {}

  attach(): void {
    this.canvas.addEventListener('pointerdown', this.onPointerDown);
    this.canvas.addEventListener('pointerup', this.onPointerUp);
    this.canvas.addEventListener('pointercancel', this.onPointerCancel);
    this.canvas.addEventListener('contextmenu', this.preventContextMenu);
  }

  detach(): void {
    this.canvas.removeEventListener('pointerdown', this.onPointerDown);
    this.canvas.removeEventListener('pointerup', this.onPointerUp);
    this.canvas.removeEventListener('pointercancel', this.onPointerCancel);
    this.canvas.removeEventListener('contextmenu', this.preventContextMenu);
    if (this.clickTimer !== undefined) window.clearTimeout(this.clickTimer);
  }

  private readonly onPointerDown = (event: PointerEvent): void => {
    if (event.button !== 0 || this.pointerId !== undefined) return;
    event.preventDefault();
    this.pointerId = event.pointerId;
    this.start = { x: event.clientX, y: event.clientY, time: performance.now() };
    this.canvas.setPointerCapture(event.pointerId);
    void this.bridge.beginDrag();
  };

  private readonly onPointerUp = (event: PointerEvent): void => {
    if (event.pointerId !== this.pointerId) return;
    event.preventDefault();
    const start = this.start;
    const distance = start ? Math.hypot(event.clientX - start.x, event.clientY - start.y) : Infinity;
    const elapsed = start ? performance.now() - start.time : Infinity;
    this.release(event.pointerId);
    void this.bridge.endDrag();
    if (distance < 8 && elapsed < 450) this.handleClick();
  };

  private readonly onPointerCancel = (event: PointerEvent): void => {
    if (event.pointerId !== this.pointerId) return;
    this.release(event.pointerId);
    void this.bridge.endDrag();
  };

  private handleClick(): void {
    if (this.clickTimer !== undefined) {
      window.clearTimeout(this.clickTimer);
      this.clickTimer = undefined;
      this.onDoubleClick();
      return;
    }

    this.clickTimer = window.setTimeout(() => {
      this.clickTimer = undefined;
      this.onClick();
    }, 250);
  }

  private release(pointerId: number): void {
    if (this.canvas.hasPointerCapture(pointerId)) this.canvas.releasePointerCapture(pointerId);
    this.pointerId = undefined;
    this.start = undefined;
  }

  private readonly preventContextMenu = (event: MouseEvent): void => event.preventDefault();
}

