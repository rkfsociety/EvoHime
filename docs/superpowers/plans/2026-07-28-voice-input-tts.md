# Voice input and TTS Implementation Plan

> **Для агентных исполнителей:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (рекомендуется) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить в browser-only чат EvoHime безопасный голосовой ввод на Web Speech API и ручное озвучивание завершённых ответов через `speechSynthesis`.

**Architecture:** STT будет изолирован в `useVoiceInput`, который переиспользует один экземпляр recognition, хранит transcript в ref/state и возвращает итог из `stop()` после `onend`. TTS будет одним `useSpeechSynthesis` на уровне списка сообщений с глобально единственным `speakingMessageId`; отдельные сообщения только сравнивают свой id с этим значением.

**Tech Stack:** React 18, TypeScript, Vite, Web Speech API, `speechSynthesis`; без backend/API/БД и без новых npm-зависимостей.

## Global Constraints

- Голосовой ввод работает только в безопасном контексте HTTPS или `localhost`.
- `window.isSecureContext === false` или отсутствие свойства даёт состояние `insecure-context`, а не `unsupported`.
- Отсутствие конструктора SpeechRecognition даёт `unsupported`; runtime-ошибка после успешного feature detection даёт `error` и `composerNotice`.
- Язык распознавания фиксирован как `ru-RU`.
- Во время диктовки composer `readOnly`; interim отображается отдельным полупрозрачным курсивным хвостом.
- Отправка во время записи делает `const { transcript } = await stop()` и отправляет именно возвращённое значение.
- Один TTS-хук управляет всеми сообщениями; стримящееся сообщение disabled, завершённые сообщения остаются доступны.
- Аудио не сохраняется и не отправляется на сервер.
- На мобильном layout кнопки имеют touch-target не менее 44px.

---

### Task 1: STT adapter and `useVoiceInput`

**Files:**
- Create: `frontend/web/src/hooks/useVoiceInput.ts`
- Create: `frontend/web/src/lib/voice-types.ts`
- Test/verification: `frontend/web` typecheck/build и browser mock matrix из Task 3

**Interfaces:**
- Produces `VoiceInputStatus = "idle" | "listening" | "unsupported" | "insecure-context" | "error"`.
- Produces:

```ts
useVoiceInput(): {
  isSupported: boolean;
  isListening: boolean;
  status: VoiceInputStatus;
  error: string | null;
  start: (baseText: string) => void;
  stop: () => Promise<{ transcript: string }>;
  transcript: string;
  interim: string;
  resetTranscript: () => void;
}
```

- `stop()` resolves only from `onend` and returns a ref-backed final transcript that includes the last `onresult` event.

- [ ] **Step 1: Define browser-safe types and constructor lookup**

  Declare minimal TypeScript interfaces for `SpeechRecognition`, its result/error events, and `window.SpeechRecognition` / `window.webkitSpeechRecognition`. Resolve constructors only inside effects/callbacks, never during module evaluation.

- [ ] **Step 2: Add secure-context and feature-detection states**

  On mount, return `insecure-context` when `window.isSecureContext` is absent or false. Return `unsupported` when neither recognition constructor exists. Keep the mic disabled for those states, with distinct messages.

- [ ] **Step 3: Implement one reusable recognition instance**

  Create the instance once in a ref. Set `lang = "ru-RU"`, `interimResults = true`, and `continuous = false`. On each start, capture `baseText`, clear only interim, and reset the current stop promise.

- [ ] **Step 4: Implement result, end, and error lifecycle**

  Append final results to the ref-backed transcript and expose interim separately. `onend` resolves the pending stop promise with `{ transcript }`. `no-speech` and `aborted` preserve confirmed text; `not-allowed`, `audio-capture`, `network`, `service-not-allowed`, `language-not-supported`, and unknown errors set `error`, finish the session, and preserve confirmed text.

- [ ] **Step 5: Make repeated start/stop deterministic**

  A `start()` while listening is a no-op at the hook boundary; the UI maps the active mic click to `stop()`. A second `stop()` reuses the pending promise. Cleanup removes `onresult`, `onend`, and `onerror`, stops recognition, resolves/rejects pending lifecycle safely, and releases the ref on unmount.

- [ ] **Step 6: Run frontend validation**

  Run `npm run typecheck` and `npm run build` from `frontend/web`. Expected: both commands succeed without new dependencies.

- [ ] **Step 7: Commit the isolated STT change**

  Run:

```powershell
git add frontend/web/src/hooks/useVoiceInput.ts frontend/web/src/lib/voice-types.ts
git commit -m "feat(web): add browser voice input hook"
```

---

### Task 2: Centralized `useSpeechSynthesis`

**Files:**
- Create: `frontend/web/src/hooks/useSpeechSynthesis.ts`
- Modify: `frontend/web/src/lib/voice-types.ts`
- Test/verification: `frontend/web` typecheck/build и TTS mock matrix из Task 3

**Interfaces:**
- Produces:

```ts
useSpeechSynthesis(): {
  speak: (messageId: string, text: string) => void;
  stop: () => void;
  speakingMessageId: string | null;
  isSupported: boolean;
  error: string | null;
}
```

- [ ] **Step 1: Add support detection and shared state**

  Detect `window.speechSynthesis` and `SpeechSynthesisUtterance` after mount. Keep one active utterance and one `speakingMessageId`; calling `speak` for another id first calls `cancel()`.

- [ ] **Step 2: Implement utterance lifecycle**

  Wire `onstart`, `onend`, and `onerror`. `end`, explicit `stop`, and replacement all clear `speakingMessageId`; `error` additionally sets a user-safe `error` string without breaking chat.

- [ ] **Step 3: Add list-level cleanup**

  On unmount of the owner/list hook, call `speechSynthesis.cancel()` and clear active state. Message components must not independently cancel global speech during their own unmount.

- [ ] **Step 4: Run frontend validation**

  Run `npm run typecheck` and `npm run build` from `frontend/web`. Expected: both commands succeed.

- [ ] **Step 5: Commit the isolated TTS change**

```powershell
git add frontend/web/src/hooks/useSpeechSynthesis.ts frontend/web/src/lib/voice-types.ts
git commit -m "feat(web): add centralized speech synthesis hook"
```

---

### Task 3: Composer, message actions, and browser verification

**Files:**
- Modify: `frontend/web/src/app.tsx`
- Modify: `frontend/web/src/styles/workspace.css`
- Modify: `frontend/web/src/styles/mobile-shell.css` if the mobile target needs an override
- Test/verification: `frontend/web` typecheck/build and manual browser matrix

**Interfaces:**
- Consumes the hooks from Tasks 1–2.
- Existing `sendMessage` remains the only path that emits `ClientCommand`.

- [ ] **Step 1: Integrate STT into composer**

  Add the mic button beside attachments. On start pass the current `input` as `baseText`; while listening render `baseText + finalized transcript + interim`, set the textarea `readOnly`, and style interim distinctly. On `Esc`, active mic click, or stop flow preserve finalized text and clear interim.

- [ ] **Step 2: Serialize submit with recognition shutdown**

  In `sendMessage`, if listening, await `stop()`, use its returned transcript, update the composer value, and continue existing validation/upload/socket logic with that exact text. Enter handling must not create a parallel submit while stop is pending.

- [ ] **Step 3: Integrate centralized TTS**

  Instantiate `useSpeechSynthesis()` once at the message-list owner. Add speak/stop action to assistant messages. Disable only the action for the currently streaming message; completed messages remain usable while a new response streams. Compare `line.id` with `speakingMessageId` for button state.

- [ ] **Step 4: Add keyboard, error, and fallback behavior**

  Handle `Escape` while listening, show `insecure-context`, `unsupported`, `not-allowed`, and other runtime errors through `composerNotice`, and leave manual input available whenever voice APIs fail. Do not auto-speak responses.

- [ ] **Step 5: Add responsive and accessible styles**

  Add mic and speak buttons with visible listening/speaking states, `aria-label`, `aria-pressed` where applicable, focus styles, and at least 44px touch targets. Keep the existing desktop composer grid intact.

- [ ] **Step 6: Run static validation**

  Run `npm run typecheck`, `npm run build`, and `git diff --check` from the repository/frontend directories. Expected: all succeed and no protocol/backend files change.

- [ ] **Step 7: Run manual browser matrix**

  In HTTPS or `localhost`, verify permission grant, `ru-RU` interim/final results, `onend` after speech pause, `Esc`, repeated mic click, send during listening without losing the last word, and editable composer after stop. Verify HTTP/insecure-context, missing constructor, Safari/iOS partial support, `not-allowed`, `audio-capture`, `network`, and `language-not-supported` fallbacks. For TTS verify start/stop, natural `end`, `error`, replacement by another message, stream-only button disablement, and list cleanup.

- [ ] **Step 8: Commit the UI integration**

```powershell
git add frontend/web/src/app.tsx frontend/web/src/styles/workspace.css frontend/web/src/styles/mobile-shell.css
git commit -m "feat(web): integrate voice controls in chat"
```

---

## Plan self-review

- Spec coverage: secure context, feature detection/runtime failure, STT errors, transcript race, readOnly UX, Esc/repeated stop, centralized TTS, streaming-specific disablement, cleanup, accessibility, responsive targets, and manual fallback matrix are covered above.
- Completeness scan: each task has concrete files, interfaces, commands, and expected outcomes.
- Type consistency: `useVoiceInput.stop()` returns `{ transcript: string }`; `useSpeechSynthesis.speak()` accepts `(messageId, text)` and exposes `speakingMessageId` used by Task 3.
