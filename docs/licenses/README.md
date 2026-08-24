# License и attribution inventory

В inventory добавляется одна строка на каждый third-party artifact, который
попадает в release package или listener-runtime release. Секреты и private
URLs здесь запрещены.

| Artifact | Version/commit | License | Source | Distributed? | Hash/evidence |
| --- | --- | --- | --- | --- | --- |
| EvoHime bundled dependencies | см. `Cargo.lock` и `package-lock.json` | per-package | lockfiles / upstream metadata | yes, as bundled code | release manifest |
| listener runtime models/DLLs | release manifest | upstream license | `listener-runtime` release | optional | `listener-runtime.json` |

Перед installer release эту таблицу нужно дополнить точными upstream license
текстами и SHA-256 manifest evidence. Этот файл — metadata-only и не является
runtime input.
