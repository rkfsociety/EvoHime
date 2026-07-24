---
name: project_overview
description: "EvoHime — платформа для AI-агентов, назначение и функциональность"
metadata: 
  node_type: memory
  type: project
  originSessionId: adc991a0-0ba6-4d74-ace4-d6cf01d7403e
  modified: 2026-07-24T16:13:26.452Z
---

## Что такое EvoHime

**EvoHime** — это веб-первая платформа для AI-агентов с браузером, функционирующая как среда разработки для autonomous agents. Нет Electron/desktop/mobile клиентов, только браузер.

**Основная функциональность:**
- Native ReAct loop (tool → observation → next action)
- Персистентная история сессий в PostgreSQL
- Инструменты: файловая система, shell, Git, браузер (с CDP), MCP интеграция
- Структурированная память с дедупликацией и конфликт-разрешением
- Python-воркеры для тяжёлых вычислений (summarize, chunk, similarity, entities, diff, classify, language, redact)
- Permission-система с approval-flows и аудит
- LLM-маршрутизация через LiteRouter (OpenAI-compatible API)
- UI-панели: Chat, Settings, Tasks, Actions, Terminal, Files, Editor, Git, Plugins, Pull Requests, Sites, Scheduled, Memory

**Стадия разработки:** 7 (Hardening + Product; стадии 1–6 завершены)

**Status:** Production-ready, активно развивается (последний pull: 2026-07-24)

**Главная цель:** Быть IDE для autonomous agents, с feature-parity классическим IDE (VS Code, JetBrains), но ориентированная на AI
