# Extended reasoning для Евы

Extended reasoning реализуется внутри Rust Core и model gateway. Electron UI получает только безопасные task events через named pipe и отображает их в desktop timeline.

## Правила

- thinking chunks не записываются в diagnostics без redaction;
- cancellation прерывает ожидание model response;
- tool calls проходят общий permission/approval gate;
- лимиты reasoning и tool iterations задаются Core;
- модель никогда не получает секреты из локальных настроек напрямую.

## Проверка

```powershell
cargo test --locked -p evohime-core -p evohime-model-gateway
```
