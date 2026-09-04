import { describe, expect, it } from 'vitest';
import { chooseSettingsWindowPosition } from './SettingsWindow';

describe('chooseSettingsWindowPosition', () => {
  const monitor = { x: 0, y: 0, width: 1920, height: 1080 };
  const settings = { width: 960, height: 720 };

  it('places settings to the right when there is enough space', () => {
    expect(
      chooseSettingsWindowPosition(
        { x: 200, y: 120, width: 600, height: 900 },
        settings,
        monitor,
      ),
    ).toEqual({ x: 820, y: 120 });
  });

  it('places settings to the left near the right screen edge', () => {
    expect(
      chooseSettingsWindowPosition(
        { x: 1250, y: 700, width: 600, height: 900 },
        settings,
        monitor,
      ),
    ).toEqual({ x: 270, y: 360 });
  });
});
