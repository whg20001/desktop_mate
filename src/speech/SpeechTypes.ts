export type SpeechSource = 'interaction' | 'ai' | 'system';
export type SpeechViseme = 'a' | 'i' | 'u' | 'e' | 'o';

export interface SpeechRequest {
  text: string;
  source: SpeechSource;
  voiceId?: string;
  language?: string;
  rate?: number;
  pitch?: number;
  volume?: number;
}

export interface SpeechUtterance extends SpeechRequest {
  id: string;
}

export interface SpeechEngineFrame {
  level: number;
  viseme?: SpeechViseme;
}

export interface SpeechMotionFrame extends SpeechEngineFrame {
  active: boolean;
}

export interface SpeechEngineObserver {
  onStarted(): void;
  onFrame(frame: SpeechEngineFrame): void;
}

export interface SpeechEngine {
  dispose?(): void;
  speak(
    utterance: SpeechUtterance,
    observer: SpeechEngineObserver,
    signal: AbortSignal,
  ): Promise<void>;
}

export interface SpeechMotionTarget {
  setSpeechFrame(frame: SpeechMotionFrame): void;
}

export interface SpeechRecognitionRequest {
  language?: string;
  interimResults?: boolean;
}

export interface SpeechRecognitionResult {
  text: string;
  isFinal: boolean;
  confidence?: number;
}

export interface SpeechRecognitionObserver {
  onResult(result: SpeechRecognitionResult): void;
}

export interface SpeechRecognizer {
  listen(
    request: SpeechRecognitionRequest,
    observer: SpeechRecognitionObserver,
    signal: AbortSignal,
  ): Promise<void>;
}

export type SpeechControllerEvent =
  | { type: 'preparing'; utterance: SpeechUtterance }
  | { type: 'started'; utterance: SpeechUtterance }
  | { type: 'completed'; utterance: SpeechUtterance }
  | { type: 'cancelled'; utterance: SpeechUtterance }
  | { type: 'failed'; utterance: SpeechUtterance; error: Error };

export type SpeechControllerListener = (event: SpeechControllerEvent) => void;
