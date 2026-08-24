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
| listener runtime models/DLLs | release manifest | upstream license | `listener-runtime` release | optional | `listener-runtime.json` |

Перед installer release release manifest должен добавить точные artifact
SHA-256 и ссылки на upstream license texts для listener-runtime и любого нового
распространяемого файла. Этот каталог — metadata-only и не является runtime
input.
