export class SpeechBubble {
  private hideTimer?: number;

  constructor(private readonly element: HTMLDivElement) {}

  show(text: string, duration = 2200): void {
    if (this.hideTimer !== undefined) window.clearTimeout(this.hideTimer);
    this.element.textContent = text;
    this.element.hidden = false;
    this.hideTimer = window.setTimeout(() => this.hide(), duration);
  }

  hide(): void {
    this.element.hidden = true;
    this.hideTimer = undefined;
  }
}

