# Security Policy

**EvoHime** — локальный single-tenant инструмент для AI-агентов. Документация по безопасности находится в [`docs/security/threat-model.md`](docs/security/threat-model.md).

## Краткое резюме

### Что защищено
- ✅ Локальная аутентификация (token-based HTTP + WebSocket)
- ✅ SSRF-блокировка для `browser.open` и `mcp.call`
- ✅ Path traversal protection в файловых инструментах
- ✅ Shell injection mitigation через env scrubbing
- ✅ Secret encryption в `app_settings` (AES-256-GCM)
- ✅ Plugin quarantine с risk-scan гейтом
- ✅ Permission audit trail (PostgreSQL)
- ✅ Secure default (localhost-only, CORS allowlist, no auto-resume mutating tasks)

### Что не защищено
- ❌ Compromised local machine (вне scope)
- ❌ LLM prompt injection (inherent risk)
- ❌ Supply chain vulnerabilities (dependencies update, but no guarantee)
- ❌ Multi-tenant scenarios (не поддерживается, не рекомендуется)

---

## Threat Model

Для полного анализа угроз, миграций и рекомендаций смотрите [`docs/security/threat-model.md`](docs/security/threat-model.md).

Основной принцип: **trust boundary между оператором и агентом не пересекается**. Агент — инструмент, не враг.

---

## Reporting Security Issues

Если вы обнаружили уязвимость:

1. **Не создавайте public issue** в GitHub
2. **Пишите на email:** romankuzminvital@gmail.com
3. **Укажите:** описание, сценарий, если возможно — POC

Мы проверим и выпустим patch, спасибо за ответственное раскрытие.

---

## Deployment Recommendations

### Локальный режим (по умолчанию)
```bash
# Безопасно — слушает только localhost
./start-dev.ps1
```

### Если нужен сетевой доступ
```bash
# ⚠️ Требует явного auth token
BIND_ADDR=0.0.0.0:3000 EVOHIME_API_TOKEN=very-strong-random-token ./target/release/evohime-server
```

### Рекомендации
- Используйте `EVOHIME_API_TOKEN` (сложный random string, ≥32 chars)
- Включите HTTPS в production (stage 8.E: `--tls` опция)
- Ограничьте доступ на firewall level
- Регулярно обновляйте dependencies
- Читайте threat model при архитектурных изменениях

---

## Known Issues & Roadmap

- **Keychain integration (7.7 TODO):** Secrets хранятся в памяти; production может использовать OS keychain (Stage 8 upgrade)
- **Project index persistence (7.57):** Пока нет on-disk кэша; медленный поиск на больших проектах (Stage 8.E)
- **TLS support (8.E):** Coming soon для non-localhost deployments

---

## References

- [Threat Model](docs/security/threat-model.md) — Полный анализ
- [Roadmap § Stage 7](docs/roadmap.md#этап-7--hardening-product--scale) — Security & Reliability пункты
- [Development Plan](docs/development-plan.md) — Архитектура
