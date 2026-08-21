# План 15. Vision и document worker

## Цель

Добавить optional offline perception для явно переданных изображений, video
clips и документов. Возможность должна быть изолированной, ограниченной по
ресурсам и доказательной: visual output без page/frame provenance не считается
фактом и не получает права на host action.

## Что уже есть в checkout

В текущем checkout нет отдельного vision/document worker, OCR-контура или
мультимодального IPC-контракта. Поэтому план начинается с versioned input/output
contract и решения о процессе worker, а не с подключения конкретной модели.

## Границы

Входит bounded image/video/document input, visual budget, лимиты страниц,
кадров, разрешения, памяти, времени и стоимости, OCR, multilingual visual QA,
page-aware answers, provenance/evidence references, quality fallback,
изоляция worker, cleanup и release review.

Не входит continuous capture, unrestricted camera/screen/filesystem/network,
автоматические действия по одному visual output и обязательное включение
тяжёлого Python/CUDA/PyTorch runtime в Electron package или Rust Core.

## Зависимости

**Блокирующие:** планы 08–10 для execution/policy/IPC contracts и план 12 для
deterministic benchmark/evaluation; до реализации нужно зафиксировать решение
о worker process и упаковке.

**Опциональные:** план 13 может дать browser-produced artifacts, а план 14 —
voice-produced metadata. До их появления worker принимает только локальные
явно переданные artifacts с теми же проверками и предсказуемо сообщает
unsupported source.

## Этапы

1. Зафиксировать bounded input, selection budget, typed output и capability
   snapshot.
2. Добавить provenance для страниц/кадров, OCR evidence и cross-page answers.
3. Реализовать изолированный worker lifecycle с cancellation, cleanup и
   resource limits; выбрать optional backend без расширения базового runtime.
4. Провести benchmark, security/privacy/egress/licensing review и release gate.

## Готово, когда

Worker optional и изолирован; все inputs/outputs bounded и provenance-aware;
низкая уверенность и ошибки модели представлены typed unknown/diagnostic;
visual output не может напрямую вызвать host action; deterministic fixtures,
лимиты и packaging review проходят release gate.

