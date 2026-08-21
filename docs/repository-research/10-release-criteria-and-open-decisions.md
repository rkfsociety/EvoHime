# 10. Общие release gates и нерешённые решения

Этот файл сопровождает остальные разделы и должен проверяться при подготовке
каждого полноценного плана.

## Общие критерии готовности

- Core/SQLite остаются единственным durable source of truth;
- все внешние действия проходят capability, scope, approval/policy и
  cancellation checks;
- схемы versioned и имеют contract tests;
- есть устойчивые IDs, atomic writes и sequence replay после reconnect;
- secrets, PII и чувствительный output redacted;
- действуют timeout, budget, bounded size и concurrency limits;
- есть deterministic fixture и replay из записанных входов;
- supervisor recovery не ломает состояние и аудит;
- renderer не может напрямую вызвать инструмент или изменить durable state;
- error, rejection, timeout и unknown model output представлены типизированно;
- новые внешние компоненты отдельно проверены по packaging, licensing,
  privacy, egress и maintenance cost.

## Не включать в базовый runtime

- сторонние Python/Node agent SDK и второй execution runtime;
- cloud control plane и обязательный внешний telemetry/export backend;
- Docker или host-full-access вместо Windows supervisor и Core policy;
- публичный HTTP API вместо authenticated local IPC;
- browser extension и unrestricted desktop control;
- автоматическое запоминание всего transcript;
- speaker cluster как доказанную identity;
- model reasoning/text как authority над filesystem, network или secrets;
- production side effects из benchmark/simulation окружений;
- неограниченную цепочку child agents и multi-agent autonomy.

## Нерешённые вопросы перед первым планом

- какой контракт из 01–03 становится первым вертикальным срезом;
- какие текущие IPC/SQLite schemas расширяются, а какие остаются без изменений;
- нужен ли отдельный worker process для browser/voice/vision;
- какие CPU, GPU, memory, disk, latency и retention limits приемлемы;
- какие features optional, а какие обязательны для поставки;
- какие fixtures и failure cases являются release gates;
- где хранится inventory лицензий и attribution для будущих компонентов.

## Шаблон полноценного плана на основе раздела

Каждый новый plan-файл должен явно содержать:

1. цель и пользовательский результат;
2. scope и non-goals;
3. блокирующие и optional зависимости;
4. текущий код/схемы, которые нужно проверить;
5. versioned contracts и migration strategy;
6. implementation slices с тестами;
7. security/privacy/egress review;
8. rollback и recovery behavior;
9. deterministic acceptance fixtures;
10. критерии готовности и доказательства результата.
