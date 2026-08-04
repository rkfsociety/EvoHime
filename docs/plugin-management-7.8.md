# Plugin management

Плагины Евы устанавливаются как локальные расширения с явным trust/risk assessment. Core проверяет manifest, integrity hash и разрешения до активации.

## Правила

- плагин не получает доступ к workspace без permission scope;
- dangerous actions проходят approval;
- lock-файл фиксирует выбранную версию и hash;
- quarantine и uninstall должны быть обратимыми;
- audit trail хранится в локальной SQLite event journal;
- UI показывает только состояние, полученное от Core.

Плагинная система остаётся отдельным этапом после стабилизации Files, Editor, Git и Terminal.
