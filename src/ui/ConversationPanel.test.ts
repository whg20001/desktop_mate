import { describe, expect, it, vi } from 'vitest';
import { ConversationPanel } from './ConversationPanel';

function setup(onMessage: (text: string) => Promise<boolean>) {
  const input = { value: '  请记住这句话  ', disabled: false, focus: vi.fn() };
  const button = { disabled: false };
  const form = Object.assign(new EventTarget(), {
    querySelector: (selector: string) =>
      selector === '[data-conversation-input]' ? input : button,
  });
  const panel = new ConversationPanel(form as unknown as HTMLFormElement, onMessage);
  const submit = () => form.dispatchEvent(new Event('submit', { cancelable: true }));
  return { input, button, panel, submit };
}

describe('ConversationPanel', () => {
  it('preserves the original draft on failure, then clears it after a successful retry', async () => {
    const send = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    const { input, button, panel, submit } = setup(send);
    submit();
    expect(input.disabled).toBe(true);
    await vi.waitFor(() => expect(button.disabled).toBe(false));
    expect(input.value).toBe('  请记住这句话  ');
    expect(input.focus).toHaveBeenCalledOnce();
    submit();
    await vi.waitFor(() => expect(input.value).toBe(''));
    expect(send.mock.calls).toEqual([['请记住这句话'], ['请记住这句话']]);
    panel.dispose();
  });

  it('unlocks the controls and keeps the draft after an unexpected rejection', async () => {
    const { input, button, panel, submit } = setup(vi.fn().mockRejectedValue(new Error('offline')));
    submit();
    await vi.waitFor(() => expect(button.disabled).toBe(false));
    expect(input.disabled).toBe(false);
    expect(input.value).toBe('  请记住这句话  ');
    panel.dispose();
  });

  it('ignores duplicate submissions while a request is pending', async () => {
    let finish!: (success: boolean) => void;
    const send = vi.fn(() => new Promise<boolean>((resolve) => { finish = resolve; }));
    const { input, panel, submit } = setup(send);
    submit();
    submit();
    expect(send).toHaveBeenCalledOnce();
    expect(input.value).toBe('  请记住这句话  ');
    finish(true);
    await vi.waitFor(() => expect(input.disabled).toBe(false));
    expect(input.value).toBe('');
    panel.dispose();
    submit();
    expect(send).toHaveBeenCalledOnce();
  });
});
