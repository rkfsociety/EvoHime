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
- Во время диктовки textarea `readOnly`; подтверждённый полный `transcript` находится в textarea, а interim отображается отдельным полупрозрачным курсивным элементом рядом с composer и не входит в значение textarea до final.
- Публичный `transcript` и результат `stop()` содержат полный composer text: `baseText + finalized dictated text`; `interim` содержит только неподтверждённый диктуемый хвост.
- Отправка во время записи делает `const { transcript } = await stop()` и отправляет именно полный возвращённый текст.
- `stop()` идемпотентен: после автоматического `onend` немедленно возвращает текущий итог и никогда не оставляет pending promise.
- Между `recognition.stop()` и `onend` статус равен `stopping`; новые `start()` и `stop()` в этот период не создают сессию или новый promise.
- Каждая STT-сессия получает monotonically increasing id; поздние события старой сессии не меняют состояние новой.
- Один TTS-хук управляет всеми сообщениями; стримящееся сообщение disabled, завершённые сообщения остаются доступны.
- TTS callbacks изменяют состояние только для текущего active utterance; события отменённого utterance игнорируются.
- Аудио не сохраняется и не отправляется на сервер.
- На мобильном layout кнопки имеют touch-target не менее 44px.
- В `frontend/web` сейчас нет существующего test runner; MVP не добавляет новые npm-зависимости и не обещает автоматические unit-тесты. Lifecycle проверяется typecheck/build и ручным browser matrix с DevTools/mocks.
- Voice hooks не изменяют существующий message transport, WebSocket protocol, upload flow или backend contracts; они только подготавливают текст composer перед существующим `sendMessage`.

---

### Task 1: STT adapter and `useVoiceInput`

**Files:**
- Create: `frontend/web/src/hooks/useVoiceInput.ts`
- Create: `frontend/web/src/lib/voice-types.ts`
- Verification: `frontend/web` typecheck/build и ручная STT browser matrix из Task 3

**Interfaces:**
- Produces `VoiceInputStatus = "idle" | "listening" | "stopping" | "unsupported" | "insecure-context" | "error"`.
- Produces:

```ts
useVoiceInput(): {
  canStart: boolean;
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

- `canStart` равно `true` только при `status === "idle"`; при `listening` кнопка выполняет stop, а при `stopping` остаётся заблокированной.
- `canStart` является производным состоянием и не хранится отдельно; `isListening` также вычисляется как `status === "listening"`, чтобы `status` оставался единственным источником истины.

- После успешной отправки сообщения или когда приложение намеренно сбрасывает composer вызывается `resetTranscript()`, чтобы следующий запуск не переиспользовал старый базовый снимок или финальный хвост.

- В обычном жизненном цикле `stop()` resolves after `onend` and returns a ref-backed full composer text that includes the last `onresult` event. При unmount pending promise принудительно завершается текущим ref-backed текстом, поскольку обработчики recognition удаляются и ожидание `onend` больше невозможно.

- [ ] **Step 1: Define browser-safe types and constructor lookup**

  Declare minimal TypeScript interfaces for `SpeechRecognition`, its result/error events, and `window.SpeechRecognition` / `window.webkitSpeechRecognition`. Resolve constructors only inside effects/callbacks, never during module evaluation.

- [ ] **Step 2: Add secure-context and feature-detection states**

  On mount, return `insecure-context` when `window.isSecureContext` is absent or false. Return `unsupported` when neither recognition constructor exists. Keep the mic disabled for those states, with distinct messages.

- [ ] **Step 3: Implement one reusable recognition instance**

  Create the instance once in a ref. Set `lang = "ru-RU"`, `interimResults = true`, and request `continuous = true` where supported so natural pauses do not intentionally end the MVP dictation. On each start, capture `baseText`, clear only interim, increment `sessionIdRef`, and reset the current stop promise. The browser may still force `onend`; the user can start again without losing confirmed text.

- [ ] **Step 4: Implement result, end, and error lifecycle**

  Append final results to the ref-backed full composer text and expose interim separately. `onend` resolves the pending stop promise with `{ transcript }`. `no-speech` preserves confirmed text and shows a soft `composerNotice`; `aborted` caused by explicit stop/cleanup is normal and has no notice, while unexpected `aborted` only ends the session and preserves text. `not-allowed`, `audio-capture`, `network`, `service-not-allowed`, `language-not-supported`, and unknown errors set `error`, finish the session, and preserve confirmed text. Every callback checks the captured session id before mutating state or resolving a promise; the id protects React state and promises, not browser event identity.

- [ ] **Step 5: Make repeated start/stop deterministic**

  A `start()` while listening or stopping is a no-op at the hook boundary; the UI maps the active mic click to `stop()`. A second `stop()` reuses the pending promise. If recognition is already inactive after automatic `onend`, `stop()` returns `Promise.resolve({ transcript: currentFullText })` and does not call `recognition.stop()`. A new recognition session never starts before the previous `onend`. On unmount, resolve pending `stop()` with the current ref-backed full text, remove `onresult`, `onend`, and `onerror`, call `recognition.abort()`, clear refs, and do not update React state. All recognition events delivered after cleanup are ignored.

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
- Verification: `frontend/web` typecheck/build и ручная TTS browser matrix из Task 4

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

  Wire `onstart`, `onend`, and `onerror`. Store the utterance in `activeUtteranceRef`; each callback first checks `activeUtteranceRef.current === utterance`, so late events from a cancelled/replaced utterance cannot clear state for the new one. `end`, explicit `stop`, and replacement all clear `speakingMessageId`; `error` additionally sets a user-safe `error` string without breaking chat. Empty or whitespace-only text is a no-op.

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

### Task 3: Voice composer integration

**Files:**
- Modify: `frontend/web/src/app.tsx`
- Modify: `frontend/web/src/styles/workspace.css`
- Modify: `frontend/web/src/styles/mobile-shell.css` if the mobile target needs an override
- Verification: `frontend/web` typecheck/build and manual STT browser matrix

**Interfaces:**
- Consumes `useVoiceInput()` from Task 1.
- Existing `sendMessage` remains the only path that emits `ClientCommand`.

- [ ] **Step 1: Integrate STT into composer**

  Add the mic button beside attachments. Disable it when `canStart` is false and it is not currently listening; while listening, an active click calls `stop()`, while `stopping` prevents a new start. Pass the current `input` as `baseText`; the textarea displays the `transcript` value, which already contains `baseText + finalized dictated text`, and remains `readOnly`. Render `interim` in a separate element beside the composer; do not concatenate `baseText` into `transcript` a second time. On `Esc` or stop flow preserve finalized text and clear interim.

- [ ] **Step 2: Serialize submit with recognition shutdown**

  In `sendMessage`, if listening or stopping, await `stop()`, use its returned full transcript, update the composer value, and continue existing validation/upload/socket logic with that exact text. Guard the flow with `const submitPendingRef = useRef(false)` (or equivalent state): while `await stop()` is pending, repeated Enter and send-button clicks are ignored, and the flag resets in `finally`. After successful send, and whenever the composer is explicitly cleared, call `resetTranscript()`.

- [ ] **Step 3: Add keyboard, error, and fallback behavior**

  Handle `Escape` while listening, show all user-facing voice API errors exclusively through the existing `composerNotice` (no new error components), and leave manual input available whenever voice APIs fail. Do not auto-speak responses.

- [ ] **Step 4: Add responsive and accessible styles**

  Add the mic button with visible listening state, `aria-label`, `aria-pressed`, focus styles, and at least 44px touch targets. Keep the existing desktop composer grid intact; TTS button styles belong to Task 4.

- [ ] **Step 5: Run static validation**

  Run `npm run typecheck`, `npm run build`, `git diff --check`, and `git diff --name-only`. Expected: all succeed and the changed paths are only `frontend/web/src/app.tsx`, the listed frontend styles, and the already committed hook/type files.

- [ ] **Step 6: Run manual browser matrix**

  In HTTPS or `localhost`, verify permission grant, `ru-RU` interim/final results, and that supported Chromium generally does not terminate dictation immediately on ordinary pauses when `continuous = true` is requested, while the browser may still force `onend`. Verify `stopping`, `Esc`, repeated mic click, idempotent stop after automatic `onend`, send during listening without losing the last word or baseText, double-submit suppression, `resetTranscript()` after send/application reset, and editable composer after stop. Verify HTTP/insecure-context, missing constructor, Safari/iOS partial support, `not-allowed`, `no-speech`, `audio-capture`, `network`, and `language-not-supported` fallbacks.

- [ ] **Step 7: Commit the UI integration**

```powershell
git add frontend/web/src/app.tsx frontend/web/src/styles/workspace.css frontend/web/src/styles/mobile-shell.css
git commit -m "feat(web): integrate voice input in composer"
```

---

### Task 4: Assistant speech controls and final browser verification

**Files:**
- Modify: `frontend/web/src/app.tsx`
- Modify: `frontend/web/src/styles/workspace.css`
- Modify: `frontend/web/src/styles/mobile-shell.css` if the mobile target needs an override
- Verification: `frontend/web` typecheck/build and manual TTS browser matrix

**Interfaces:**
- Consumes `useSpeechSynthesis()` from Task 2.
- `speak(line.id, line.text)` starts a message; if `speakingMessageId === line.id`, the same button calls `stop()`.

- [ ] **Step 1: Integrate the centralized TTS hook**

  Instantiate `useSpeechSynthesis()` once at the message-list owner. Add an action to completed assistant messages. Disable the action only for the currently streaming message; completed messages remain usable while a new response streams. Compare `line.id` with `speakingMessageId` for button state.

- [ ] **Step 2: Add TTS errors and empty-text behavior**

  Render the hook error through the existing safe notice path. Do not render a speaking action for empty text, and make `speak()` a no-op for whitespace-only text.

- [ ] **Step 3: Add TTS responsive and accessible styles**

  Add visible speaking/stopped states, `aria-label`, focus styles, and at least 44px touch targets without changing the existing desktop chat layout.

- [ ] **Step 4: Run static validation**

  Run `npm run typecheck`, `npm run build`, `git diff --check`, and `git diff --name-only`. Expected: all succeed and the diff contains only the listed frontend files plus hook/type files.

- [ ] **Step 5: Run manual TTS browser matrix**

  Verify start/stop, natural `end`, synthesis `error`, replacement by another message, late events from the cancelled utterance not clearing the new message state, only the streaming message being disabled, completed messages remaining available during a new stream, and list cleanup calling `cancel()`.

- [ ] **Step 6: Commit the UI integration**

```powershell
git add frontend/web/src/app.tsx frontend/web/src/styles/workspace.css frontend/web/src/styles/mobile-shell.css
git commit -m "feat(web): add speech controls to assistant messages"
```

---

## Plan self-review

- Spec coverage: secure context, feature detection/runtime failure, STT errors, transcript race, readOnly UX, Esc/repeated stop, centralized TTS, streaming-specific disablement, cleanup, accessibility, responsive targets, and manual fallback matrix are covered above.
- Completeness scan: each task has concrete files, interfaces, commands, and expected outcomes.
- Type consistency: `useVoiceInput.stop()` returns `{ transcript: string }`; `useSpeechSynthesis.speak()` accepts `(messageId, text)` and exposes `speakingMessageId` used by Task 4.
