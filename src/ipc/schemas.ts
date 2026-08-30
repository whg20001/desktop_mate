import { z } from 'zod';

export const hitRegionSchema = z.object({
  shape: z.enum(['rect', 'ellipse']),
  x: z.number(),
  y: z.number(),
  width: z.number().nonnegative(),
  height: z.number().nonnegative(),
});

export const hitRegionPayloadSchema = z.object({
  regions: z.array(hitRegionSchema).max(16),
  footX: z.number(),
  footY: z.number(),
  scaleFactor: z.number().positive(),
});

export const cursorPositionSchema = z.object({
  x: z.number(),
  y: z.number(),
  insideCharacter: z.boolean(),
});

export const characterModeSchema = z.enum([
  'idle',
  'dragged',
  'falling',
  'landing',
]);

export const characterStateSchema = z.object({
  x: z.number(),
  y: z.number(),
  vx: z.number(),
  vy: z.number(),
  mode: characterModeSchema,
  grabbed: z.boolean(),
  supportSurface: z
    .discriminatedUnion('type', [
      z.object({ type: z.literal('monitorFloor'), monitorId: z.string() }),
      z.object({ type: z.literal('windowTop'), hwnd: z.number(), title: z.string() }),
    ])
    .nullable(),
});

export const monitorInfoSchema = z.object({
  id: z.string(),
  left: z.number(),
  top: z.number(),
  right: z.number(),
  bottom: z.number(),
  workLeft: z.number(),
  workTop: z.number(),
  workRight: z.number(),
  workBottom: z.number(),
  dpi: z.number(),
  primary: z.boolean(),
});

export const desktopWindowSchema = z.object({
  hwnd: z.number(),
  processId: z.number(),
  title: z.string(),
  left: z.number(),
  top: z.number(),
  right: z.number(),
  bottom: z.number(),
  visible: z.boolean(),
  minimized: z.boolean(),
});

export const desktopWorldSchema = z.object({
  monitors: z.array(monitorInfoSchema),
  windows: z.array(desktopWindowSchema),
  foregroundWindow: z.number().nullable(),
  cursor: z.object({ x: z.number(), y: z.number() }),
});

export type HitRegion = z.infer<typeof hitRegionSchema>;
export type HitRegionPayload = z.infer<typeof hitRegionPayloadSchema>;
export type CursorPosition = z.infer<typeof cursorPositionSchema>;
export type CharacterState = z.infer<typeof characterStateSchema>;
export type DesktopWorld = z.infer<typeof desktopWorldSchema>;
