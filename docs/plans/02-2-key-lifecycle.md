# Этап 02.2: Key lifecycle

Этап плана [02 Подписанные hash-chain receipts](02-0-signed-hash-chain-receipts.md).

## Зависимости

Блокирующие: этап 02.1 — подписывается именно canonical payload.

Разблокирует: 02.3 (подпись действий) и 02.4 (offline verification).

## Что этап отдаёт наружу

Key pair с защищённым private key, key id, rotation и команду публичной
проверки, не требующую Core или сети.

## Содержание

- Генерировать key pair при первом запуске через supervisor.
- Защитить private key DPAPI/ACL, поддержать key id и rotation.
- Не менять ключ молча: rotation создаёт chain metadata и audit event.
- Public verification command не требует Core или сети.

## Проверки

- key rotation and offline verification after Core shutdown;
- private key не попадает в source, renderer и обычные logs;
- rotation оставляет chain metadata и audit event, а не молчаливую замену.

## Критерии готовности

- verifier работает без сети и без private key;
- смена ключа видна в chain metadata и audit.
