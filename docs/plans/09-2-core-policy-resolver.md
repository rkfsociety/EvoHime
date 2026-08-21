# 09-2 — Core resolver и operation checks

## Цель

Собрать все проверки filesystem, network, tool, timeout и cancellation в
едином Core-owned policy path.

## Изменения

1. Ввести canonical workspace anchor и path resolver для абсолютных и
   относительных путей; относительный путь всегда разрешать относительно
   выбранного workspace.
2. Проверять path traversal, symlink/reparse point, protected paths,
   workspace scope и operation type до запуска инструмента.
3. Проверять network route, redirect/egress policy, timeout, payload size,
   concurrency и provider budget до dispatch.
4. Повторять все hard checks непосредственно перед side effect, даже если UI
   или planner уже выполняли предварительную проверку.
5. Передавать worker/adapters только capability-scoped session и secret
   references; не передавать секреты через renderer, prompt, argv или лог.
6. Запретить hooks и tool adapters обходить supervisor, sandbox, approval или
   Core policy.

## Проверки

- path traversal, reparse/symlink и protected path fixtures;
- network/private/internal target, redirect и egress tests;
- timeout/cancellation до запуска, во время запуска и после результата;
- bounded input/output, concurrency и budget exhaustion;
- provider/worker boundary и secret reference tests.

## Готово, когда

Ни один внешний side effect не проходит мимо единого Core resolver, а policy
ошибки отличаются от unavailable и denied typed outcomes.
