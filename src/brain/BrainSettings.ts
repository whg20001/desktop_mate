import type { CharacterSettings } from '../config/CharacterSettings';
import type { BrainSettings } from './BrainTypes';

export function brainSettingsFromCharacter(settings: CharacterSettings): BrainSettings {
  return {
    llmBaseUrl: settings.llmBaseUrl,
    llmModel: settings.llmModel,
    embeddingBaseUrl: settings.embeddingBaseUrl,
    embeddingModel: settings.embeddingModel,
    embeddingDimensions: settings.embeddingDimensions,
    memoryEnabled: settings.memoryEnabled,
    recallEnabled: settings.memoryRecallEnabled,
    memoryWriteEnabled: settings.memoryWriteEnabled,
    recallLimit: settings.memoryRecallLimit,
    requestTimeoutSeconds: 30,
  };
}
