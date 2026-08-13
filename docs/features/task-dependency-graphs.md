# Task Dependency Graphs

Task Dependency Graphs живут в Core и описывают зависимости между шагами задачи. Граф валидируется алгоритмом Kahn: отсутствующие зависимости и циклы отклоняются до запуска.

## Runtime

- независимые шаги объединяются в batch;
- batch выполняется с bounded concurrency;
- состояние каждого шага хранится в SQLite;
- failure strategy ограничивает суммарное число ошибок;
- desktop task timeline получает status events через named pipe.

## UI

Electron отображает граф и состояния как часть task workspace. UI не вычисляет зависимости и не запускает steps самостоятельно.
