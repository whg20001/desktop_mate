import { describe, expect, it, vi } from 'vitest';
import { runMemoryAction } from './MemoryAction';

describe('memory management actions', () => {
  it('restores all controls and reports a failed action so it can be retried', async () => {
    const buttons = [{ disabled: false }, { disabled: false }] as HTMLButtonElement[];
    const feedback = { textContent: '' } as HTMLElement;
    await runMemoryAction(buttons, feedback, async () => { throw new Error('offline'); });
    expect(buttons.every((button) => !button.disabled)).toBe(true);
    expect(feedback.textContent).toContain('offline');
    await runMemoryAction(buttons, feedback, async () => {});
    expect(feedback.textContent).toBe('操作已完成');
  });

  it('prevents conflicting mutations of the same memory while an action is pending', async () => {
    const buttons = [{ disabled: false }, { disabled: false }] as HTMLButtonElement[];
    const feedback = { textContent: '' } as HTMLElement;
    let finish!: () => void;
    const pending = runMemoryAction(buttons, feedback, () => new Promise<void>((resolve) => { finish = resolve; }));
    const duplicate = vi.fn(async () => {});
    await runMemoryAction(buttons, feedback, duplicate);
    expect(duplicate).not.toHaveBeenCalled();
    finish();
    await pending;
    expect(buttons.every((button) => !button.disabled)).toBe(true);
  });
});
