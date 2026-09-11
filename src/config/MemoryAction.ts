export async function runMemoryAction(
  buttons: readonly HTMLButtonElement[],
  feedback: HTMLElement,
  action: () => Promise<void>,
): Promise<void> {
  if (buttons.some((button) => button.disabled)) return;
  buttons.forEach((button) => { button.disabled = true; });
  feedback.textContent = '正在处理…';
  try {
    await action();
    feedback.textContent = '操作已完成';
  } catch (error: unknown) {
    const detail = error instanceof Error ? error.message : String(error);
    feedback.textContent = `操作失败，可重试：${detail}`;
  } finally {
    buttons.forEach((button) => { button.disabled = false; });
  }
}
