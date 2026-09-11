import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
import { MOTION_CATALOG } from '../character/animation/MotionCatalog';
import {
  brainSettingsSchema,
  brainStatusSchema,
  brainMemorySchema,
  memoryManagerStatusSchema,
  characterResponseSchema,
  type BrainSettings,
  type BrainMemory,
  type BrainStatus,
  type CharacterResponse,
  type ConversationScope,
  type MemoryManagerStatus,
} from './BrainTypes';

function isTauri(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

export class BrainBridge {
  private pendingTurn?: { key: string; turnId: string };

  async status(): Promise<BrainStatus> {
    if (!isTauri()) {
      return {
        phase: 'failed',
        ready: false,
        pid: null,
        restartCount: 0,
        detail: '浏览器预览模式不会启动 Python Brain Sidecar',
      };
    }
    return brainStatusSchema.parse(await invoke('get_brain_status'));
  }

  async settings(): Promise<BrainSettings> {
    return brainSettingsSchema.parse(await invoke('get_brain_settings'));
  }

  async configure(settings: BrainSettings): Promise<BrainSettings> {
    return brainSettingsSchema.parse(await invoke('configure_brain', { settings }));
  }

  async converse(
    userInput: string,
    scope: ConversationScope,
    enabledMotionIds: readonly string[],
  ): Promise<CharacterResponse> {
    const key = JSON.stringify([scope.userId, scope.characterId, scope.sessionId, userInput]);
    if (this.pendingTurn?.key !== key) {
      this.pendingTurn = { key, turnId: crypto.randomUUID() };
    }
    const turn = this.pendingTurn;
    const availableActions = MOTION_CATALOG.filter(
      (motion) => motion.owner === 'ai' && enabledMotionIds.includes(motion.id),
    ).map(({ id, description, scenes }) => ({ id, description, scenes: [...scenes] }));
    const response = await invoke('converse', {
      request: {
        turnId: turn.turnId,
        scope,
        userInput,
        availableActions,
        desktopContext: null,
      },
    });
    const parsed = characterResponseSchema.parse(response);
    if (this.pendingTurn === turn) this.pendingTurn = undefined;
    return parsed;
  }

  async listMemories(scope: ConversationScope): Promise<BrainMemory[]> {
    const response = await invoke('list_brain_memories', { scope });
    return z.array(brainMemorySchema).parse(response);
  }

  async updateMemory(
    scope: ConversationScope,
    memoryId: string,
    content: string,
  ): Promise<void> {
    await invoke('update_brain_memory', { scope, memoryId, content });
  }

  async deleteMemory(scope: ConversationScope, memoryId: string): Promise<void> {
    await invoke('delete_brain_memory', { scope, memoryId });
  }

  async memoryStatus(): Promise<MemoryManagerStatus> {
    return memoryManagerStatusSchema.parse(await invoke('get_memory_status'));
  }

  async retryMemory(scope: ConversationScope): Promise<void> {
    await invoke('retry_brain_memory', { scope });
  }

  async approveMemory(scope: ConversationScope, memoryId: string): Promise<void> {
    await invoke('approve_brain_memory', { scope, memoryId });
  }

  async rejectMemory(scope: ConversationScope, memoryId: string): Promise<void> {
    await invoke('reject_brain_memory', { scope, memoryId });
  }

  async rebuildMemory(scope: ConversationScope, provider: string): Promise<number> {
    return z.number().int().nonnegative().parse(
      await invoke('rebuild_brain_memory', { scope, provider }),
    );
  }
}
