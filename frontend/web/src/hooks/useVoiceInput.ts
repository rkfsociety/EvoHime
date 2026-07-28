import { useCallback, useEffect, useRef, useState } from "react";
import {
  canStartVoice,
  composeTranscript,
  isCurrentVoiceSession,
  isListeningVoice,
} from "../lib/voice-runtime";
import type {
  SpeechRecognitionConstructorLike,
  SpeechRecognitionEventLike,
  SpeechRecognitionLike,
  VoiceInputStatus,
} from "../lib/voice-types";

type SpeechWindow = Window & {
  SpeechRecognition?: SpeechRecognitionConstructorLike;
  webkitSpeechRecognition?: SpeechRecognitionConstructorLike;
};

type StopResult = { transcript: string };
type PendingStop = { resolve: (result: StopResult) => void };

function recognitionConstructor(): SpeechRecognitionConstructorLike | undefined {
  const speechWindow = window as SpeechWindow;
  return speechWindow.SpeechRecognition ?? speechWindow.webkitSpeechRecognition;
}

function collectResult(event: SpeechRecognitionEventLike): { finalText: string; interimText: string } {
  let finalText = "";
  let interimText = "";
  for (let index = event.resultIndex; index < event.results.length; index += 1) {
    const result = event.results[index];
    const text = result?.[0]?.transcript ?? "";
    if (result?.isFinal) {
      finalText += text;
    } else {
      interimText += text;
    }
  }
  return { finalText, interimText };
}

export function useVoiceInput() {
  const [status, setStatus] = useState<VoiceInputStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [transcript, setTranscript] = useState("");
  const [interim, setInterim] = useState("");
  const recognitionRef = useRef<SpeechRecognitionLike | null>(null);
  const mountedRef = useRef(false);
  const statusRef = useRef<VoiceInputStatus>("idle");
  const baseTextRef = useRef("");
  const dictatedTextRef = useRef("");
  const transcriptRef = useRef("");
  const interimRef = useRef("");
  const sessionIdRef = useRef(0);
  const activeSessionIdRef = useRef(0);
  const activeRecognitionRef = useRef(false);
  const stopRequestedRef = useRef(false);
  const pendingStopRef = useRef<PendingStop | null>(null);

  const updateStatus = useCallback((next: VoiceInputStatus) => {
    statusRef.current = next;
    if (mountedRef.current) {
      setStatus(next);
    }
  }, []);

  const resolveStop = useCallback(() => {
    const pending = pendingStopRef.current;
    pendingStopRef.current = null;
    pending?.resolve({ transcript: transcriptRef.current });
  }, []);

  const setTranscriptValue = useCallback((value: string) => {
    transcriptRef.current = value;
    if (mountedRef.current) {
      setTranscript(value);
    }
  }, []);

  const setInterimValue = useCallback((value: string) => {
    interimRef.current = value;
    if (mountedRef.current) {
      setInterim(value);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    if (typeof window === "undefined") {
      return () => {
        mountedRef.current = false;
      };
    }

    const speechWindow = window as SpeechWindow;
    if (!("isSecureContext" in speechWindow) || speechWindow.isSecureContext === false) {
      updateStatus("insecure-context");
      return () => {
        mountedRef.current = false;
      };
    }

    const Constructor = recognitionConstructor();
    if (!Constructor) {
      updateStatus("unsupported");
      return () => {
        mountedRef.current = false;
      };
    }

    const recognition = new Constructor();
    recognition.lang = "ru-RU";
    recognition.interimResults = true;
    recognition.continuous = true;
    recognitionRef.current = recognition;

    recognition.onresult = (event) => {
      if (!mountedRef.current || !activeRecognitionRef.current) return;
      if (!isCurrentVoiceSession(activeSessionIdRef.current, sessionIdRef.current)) return;
      const result = collectResult(event);
      if (result.finalText) {
        dictatedTextRef.current += result.finalText;
        setTranscriptValue(composeTranscript(baseTextRef.current, dictatedTextRef.current));
      }
      setInterimValue(result.interimText);
    };

    recognition.onend = () => {
      if (!mountedRef.current || !activeRecognitionRef.current) return;
      if (!isCurrentVoiceSession(activeSessionIdRef.current, sessionIdRef.current)) return;
      activeRecognitionRef.current = false;
      stopRequestedRef.current = false;
      setInterimValue("");
      updateStatus("idle");
      resolveStop();
    };

    recognition.onerror = (event) => {
      if (!mountedRef.current || !activeRecognitionRef.current) return;
      if (!isCurrentVoiceSession(activeSessionIdRef.current, sessionIdRef.current)) return;
      const code = event.error;
      const wasExplicitlyStopped = stopRequestedRef.current;
      activeRecognitionRef.current = false;
      stopRequestedRef.current = false;
      setInterimValue("");
      if (code === "aborted" && wasExplicitlyStopped) {
        updateStatus("idle");
        resolveStop();
        return;
      }
      if (code === "no-speech") {
        if (mountedRef.current) setError("Речь не распознана. Попробуйте ещё раз.");
        updateStatus("idle");
        resolveStop();
        return;
      }
      if (mountedRef.current) {
        setError(event.message || `Ошибка голосового ввода: ${code}`);
      }
      updateStatus("error");
      resolveStop();
    };

    return () => {
      mountedRef.current = false;
      activeRecognitionRef.current = false;
      resolveStop();
      recognition.onresult = null;
      recognition.onend = null;
      recognition.onerror = null;
      recognition.abort();
      recognitionRef.current = null;
    };
  }, [resolveStop, setInterimValue, setTranscriptValue, updateStatus]);

  const start = useCallback((baseText: string) => {
    const recognition = recognitionRef.current;
    if (!recognition || !canStartVoice(statusRef.current) || !mountedRef.current) return;
    baseTextRef.current = baseText;
    dictatedTextRef.current = "";
    setTranscriptValue(baseText);
    setInterimValue("");
    setError(null);
    sessionIdRef.current += 1;
    activeSessionIdRef.current = sessionIdRef.current;
    activeRecognitionRef.current = true;
    stopRequestedRef.current = false;
    updateStatus("listening");
    try {
      recognition.start();
    } catch (startError) {
      activeRecognitionRef.current = false;
      updateStatus("error");
      setError(startError instanceof Error ? startError.message : "Не удалось запустить микрофон.");
    }
  }, [setInterimValue, setTranscriptValue, updateStatus]);

  const stop = useCallback((): Promise<StopResult> => {
    const recognition = recognitionRef.current;
    if (!recognition || !activeRecognitionRef.current || statusRef.current === "idle") {
      return Promise.resolve({ transcript: transcriptRef.current });
    }
    if (statusRef.current === "stopping" && pendingStopRef.current) {
      return new Promise((resolve) => {
        const previous = pendingStopRef.current;
        pendingStopRef.current = {
          resolve: (result) => {
            previous?.resolve(result);
            resolve(result);
          },
        };
      });
    }
    updateStatus("stopping");
    stopRequestedRef.current = true;
    const promise = new Promise<StopResult>((resolve) => {
      pendingStopRef.current = { resolve };
    });
    try {
      recognition.stop();
    } catch {
      activeRecognitionRef.current = false;
      stopRequestedRef.current = false;
      updateStatus("idle");
      resolveStop();
    }
    return promise;
  }, [resolveStop, updateStatus]);

  const resetTranscript = useCallback(() => {
    baseTextRef.current = "";
    dictatedTextRef.current = "";
    setTranscriptValue("");
    setInterimValue("");
  }, [setInterimValue, setTranscriptValue]);

  return {
    canStart: canStartVoice(status),
    isListening: isListeningVoice(status),
    status,
    error,
    start,
    stop,
    transcript,
    interim,
    resetTranscript,
  };
}
