import { z } from 'zod';

export const brainPhaseSchema = z.enum([
  'stopped',
  'starting',
  'ready',
  'degraded',
  'restarting',
  'failed',
]);

export const brainStatusSchema = z.object({
  phase: brainPhaseSchema,
  ready: z.boolean(),
  pid: z.number().int().positive().nullable(),
  restartCount: z.number().int().nonnegative(),
  detail: z.string(),
});

export const brainSettingsSchema = z.object({
  llmBaseUrl: z.string(),
  llmModel: z.string(),
  embeddingBaseUrl: z.string(),
  embeddingModel: z.string(),
  embeddingDimensions: z.number().int().min(64).max(8192),
  memoryEnabled: z.boolean(),
  recallEnabled: z.boolean(),
  memoryWriteEnabled: z.boolean(),
  recallLimit: z.number().int().min(1).max(20),
  requestTimeoutSeconds: z.number().int().min(2).max(120),
});

export const conversationScopeSchema = z.object({
  userId: z.string().min(1).max(128),
  characterId: z.string().min(1).max(128),
  sessionId: z.string().min(1).max(128),
});

export const characterResponseSchema = z.object({
  turnId: z.string().min(1).max(128),
  text: z.string().min(1).max(8000),
  emotion: z
    .object({
      type: z.enum(['neutral', 'happy', 'concerned', 'curious', 'surprised', 'sad']),
      intensity: z.number().min(0).max(1),
      durationMs: z.number().int().min(250).max(10_000),
    })
    .optional(),
  actionIntent: z
    .object({
      id: z.string().min(1).max(128),
      intensity: z.number().min(0).max(1),
    })
    .optional(),
  speech: z.object({ text: z.string().min(1).max(8000) }).optional(),
  degradedReasons: z.array(z.string()).default([]),
});

export const brainMemorySchema = z.object({
  id: z.string().min(1).max(256),
  content: z.string().min(1).max(8000),
  score: z.number(),
  metadata: z.unknown().default({}),
  createdAt: z.string().nullable().optional(),
  updatedAt: z.string().nullable().optional(),
});

export type BrainStatus = z.infer<typeof brainStatusSchema>;
export type BrainSettings = z.infer<typeof brainSettingsSchema>;
export type ConversationScope = z.infer<typeof conversationScopeSchema>;
export type CharacterResponse = z.infer<typeof characterResponseSchema>;
export type BrainMemory = z.infer<typeof brainMemorySchema>;
