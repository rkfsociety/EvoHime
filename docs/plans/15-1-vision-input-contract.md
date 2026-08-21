# План 15.1. Контракт visual input и budget

## Цель

Определить versioned контракт входа в vision/document worker и гарантировать,
что worker получает только явно переданные artifacts и capability snapshot.

## Изменения

- Описать typed `VisionInput` с kind (image, video clip, document), artifact
  identity, MIME/size, source scope, locale и correlation id.
- Добавить явные selection параметры: страницы, диапазон кадров, максимальное
  разрешение, число результатов и общий visual budget.
- Валидировать размер, формат, число страниц/кадров, memory/time/cost budget до
  запуска worker; невалидный input не должен попадать во внешний backend.
- Описать typed output envelope с confidence, diagnostic/unknown state,
  capability snapshot и ссылками на evidence, но без action authority.
- Версионировать схему и добавить совместимые unknown/unsupported варианты.

## Проверки

- deterministic fixtures для одного изображения, видео и многостраничного
  документа;
- отказ на oversized, unsupported, пустом и budget-exhausted input;
- проверка canonical artifact identity, correlation id и redaction boundary;
- backward/forward schema tests и rejection неизвестных опасных полей.

## Готово, когда

До worker доходят только валидированные bounded inputs, output невозможно
трактовать как команду, а contract tests фиксируют размер, версии и ошибки.

