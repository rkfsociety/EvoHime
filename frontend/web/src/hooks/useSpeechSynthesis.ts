import { useCallback, useEffect, useRef, useState } from "react";
import { isActiveUtterance, isSpeechText } from "../lib/voice-runtime";

export function useSpeechSynthesis() {
  const [isSupported, setIsSupported] = useState(false);
  const [speakingMessageId, setSpeakingMessageId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const mountedRef = useRef(false);
  const synthesisRef = useRef<SpeechSynthesis | null>(null);
  const activeUtteranceRef = useRef<SpeechSynthesisUtterance | null>(null);

  useEffect(() => {
    mountedRef.current = true;
    const supported = typeof window !== "undefined"
      && typeof window.speechSynthesis !== "undefined"
      && typeof window.SpeechSynthesisUtterance !== "undefined";
    if (!supported) {
      return () => {
        mountedRef.current = false;
      };
    }
    synthesisRef.current = window.speechSynthesis;
    setIsSupported(true);

    return () => {
      mountedRef.current = false;
      synthesisRef.current?.cancel();
      activeUtteranceRef.current = null;
      synthesisRef.current = null;
    };
  }, []);

  const stop = useCallback(() => {
    synthesisRef.current?.cancel();
    activeUtteranceRef.current = null;
    if (mountedRef.current) {
      setSpeakingMessageId(null);
    }
  }, []);

  const speak = useCallback((messageId: string, text: string) => {
    const synthesis = synthesisRef.current;
    if (!synthesis || !isSpeechText(text)) return;

    if (activeUtteranceRef.current) {
      synthesis.cancel();
      activeUtteranceRef.current = null;
    }
    setError(null);
    const utterance = new SpeechSynthesisUtterance(text.trim());
    activeUtteranceRef.current = utterance;
    setSpeakingMessageId(messageId);
    utterance.onstart = () => {
      if (!mountedRef.current || !isActiveUtterance(utterance, activeUtteranceRef.current)) return;
      setSpeakingMessageId(messageId);
    };
    utterance.onend = () => {
      if (!mountedRef.current || !isActiveUtterance(utterance, activeUtteranceRef.current)) return;
      activeUtteranceRef.current = null;
      setSpeakingMessageId(null);
    };
    utterance.onerror = (event) => {
      if (!mountedRef.current || !isActiveUtterance(utterance, activeUtteranceRef.current)) return;
      activeUtteranceRef.current = null;
      setSpeakingMessageId(null);
      setError(event.error ? `Ошибка озвучивания: ${event.error}` : "Не удалось озвучить сообщение.");
    };
    synthesis.speak(utterance);
  }, []);

  return { speak, stop, speakingMessageId, isSupported, error };
}
