import type { VoiceInputStatus } from "./voice-types";

export function composeTranscript(baseText: string, dictatedText: string): string {
  const base = baseText.trim();
  const dictated = dictatedText.trim();
  return [base, dictated].filter(Boolean).join(" ");
}

export function canStartVoice(status: VoiceInputStatus): boolean {
  return status === "idle";
}

export function isListeningVoice(status: VoiceInputStatus): boolean {
  return status === "listening";
}

export function isCurrentVoiceSession(sessionId: number, currentSessionId: number): boolean {
  return sessionId === currentSessionId;
}

export function isSpeechText(text: string): boolean {
  return text.trim().length > 0;
}

export function isActiveUtterance<T>(utterance: T, activeUtterance: T | null): boolean {
  return activeUtterance === utterance;
}
