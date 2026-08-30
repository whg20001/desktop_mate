import { describe, expect, it } from 'vitest';
import { modelManifestSchema } from './ModelManifest';

describe('modelManifestSchema', () => {
  it('accepts the checked-in LingGuang manifest', () => {
    const manifest = modelManifestSchema.parse({
      id: 'linguang',
      name: '陵光',
      model: '陵光-珠绣联动3.0.pmx',
      bones: { head: '頭' },
      morphs: { blink: 'まばたき' },
    });
    expect(manifest.scale).toBe(1);
    expect(manifest.groundOffset).toBe(0);
  });

  it('rejects unsafe empty asset names and non-positive scale', () => {
    expect(() => modelManifestSchema.parse({ id: 'x', name: 'x', model: '', scale: 0 })).toThrow();
  });
});

