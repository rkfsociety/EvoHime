# License и attribution inventory

Канонический inventory — [`manifest.json`](manifest.json) плюс locked package
metadata. `scripts/license-inventory.tests.ps1` проверяет, что все registry
crates и npm-пакеты имеют license metadata, что lockfiles не изменились без
обновления manifest hash, а listener-runtime остаётся отдельной областью
поставки. Секреты и private URLs здесь запрещены.

| Artifact | Version/commit | License | Source | Distributed? | Hash/evidence |
| --- | --- | --- | --- | --- | --- |
| EvoHime bundled Rust dependencies | `Cargo.lock` | per-package metadata | crates.io package metadata | yes, as bundled code | `manifest.json` + release manifest |
| EvoHime Electron production dependencies | `package-lock.json` | per-package metadata | npm package metadata | yes, as bundled code | `manifest.json` + release manifest |
| listener runtime models/DLLs | release manifest | upstream license | `listener` module release | optional | `listener-runtime.json` |
| llama.cpp CPU adapter | `b10981` | MIT; OpenMP runtime retains its package license | [upstream Windows x64 CPU release](https://github.com/ggml-org/llama.cpp/releases/tag/b10981) | on explicit user install; not in EvoHime installer | `llama.cpp-MIT.txt`; archive SHA-256 `ca53c86dba93aaa23a2b6bcc5bf5e19409de11c28dfebdd87c0148d687dead71` |

Перед installer release release manifest должен добавить точные artifact
SHA-256 и ссылки на upstream license texts для listener-runtime и любого нового
распространяемого файла. Таблица остаётся metadata-only; отдельный
`llama.cpp-MIT.txt` включается в data-каталог только вместе с пакетом, который
пользователь установил явно.
