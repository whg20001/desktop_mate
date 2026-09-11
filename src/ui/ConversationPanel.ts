export class ConversationPanel {
  private readonly input: HTMLInputElement;
  private readonly submit: HTMLButtonElement;
  private busy = false;

  constructor(
    private readonly form: HTMLFormElement,
    private readonly onMessage: (text: string) => Promise<boolean>,
  ) {
    const input = form.querySelector<HTMLInputElement>('[data-conversation-input]');
    const submit = form.querySelector<HTMLButtonElement>('[data-conversation-submit]');
    if (!input || !submit) throw new Error('缺少对话输入控件');
    this.input = input;
    this.submit = submit;
    form.addEventListener('submit', this.onSubmit);
  }

  focus(): void {
    this.form.hidden = false;
    this.input.focus();
  }

  dispose(): void {
    this.form.removeEventListener('submit', this.onSubmit);
  }

  private readonly onSubmit = (event: SubmitEvent): void => {
    event.preventDefault();
    const text = this.input.value.trim();
    if (!text || this.busy) return;
    this.busy = true;
    this.input.disabled = true;
    this.submit.disabled = true;
    void this.onMessage(text)
      .then((succeeded) => {
        if (succeeded) this.input.value = '';
      })
      .catch(() => {
        // An unexpected rejection must also preserve the draft for retry.
      })
      .finally(() => {
        this.busy = false;
        this.input.disabled = false;
        this.submit.disabled = false;
        this.input.focus();
      });
  };
}
