import assert from "node:assert/strict";
import test from "node:test";
import {
  composeTranscript,
  canStartVoice,
  isListeningVoice,
  isCurrentVoiceSession,
  isSpeechText,
  isActiveUtterance,
} from "./voice-runtime.ts";

test("composeTranscript keeps manual text and appends finalized dictation", () => {
  assert.equal(composeTranscript("  ручной текст ", "продиктовано"), "ручной текст продиктовано");
});

test("voice status exposes only idle as startable", () => {
  assert.equal(canStartVoice("idle"), true);
  assert.equal(canStartVoice("listening"), false);
  assert.equal(canStartVoice("stopping"), false);
  assert.equal(canStartVoice("error"), false);
});

test("voice status derives listening from status", () => {
  assert.equal(isListeningVoice("listening"), true);
  assert.equal(isListeningVoice("stopping"), false);
});

test("late recognition callbacks are ignored after a newer session starts", () => {
  assert.equal(isCurrentVoiceSession(4, 4), true);
  assert.equal(isCurrentVoiceSession(4, 5), false);
});

test("speech no-ops for empty or whitespace-only text", () => {
  assert.equal(isSpeechText(""), false);
  assert.equal(isSpeechText("   "), false);
  assert.equal(isSpeechText("озвучь это"), true);
});

test("speech callbacks only affect the active utterance", () => {
  const active = {};
  const stale = {};
  assert.equal(isActiveUtterance(active, active), true);
  assert.equal(isActiveUtterance(stale, active), false);
});
