# Kameo: пригодность, ограничения и проверка

Часть [оценки Actor Model](README.md), 2026-10-12.
Исследован опубликованный crate **0.22.2**; VCS commit из package metadata —
`90138758779d2260798c41cfaa47598db84f05b8`. Выводы о переносимости относятся к
проверенным features, а не ко всем возможным integrations библиотеки.

## Что подходит WinBridge

| Механизм | Как использовать | Что он не гарантирует |
| --- | --- | --- |
| Типизированные `Message` и replies | Внутренние команды state/project actors, проверка типов при компиляции | Совместимость network schema между независимо обновляемыми проектами. |
| Последовательный mutable handler | Один владелец state без внешнего `Arc<Mutex<...>>` для каждого поля | Атомарность нескольких actors, журналирование и справедливость проектов. |
| Bounded mailbox | Ограничение внутреннего ingress, явный backpressure | Долговечное принятие задачи и лимит уже работающих процессов. |
| Lifecycle и supervision | Изоляция отказов, восстановление технических actors | Сохранение состояния и безопасный повтор внешних эффектов. |
| Detached completion через message | Долгая работа вне основного message loop | Сохранение результата при смерти получателя и завершение OS process tree. |

Последовательность обработчиков описана в
[Message API/source](https://raw.githubusercontent.com/tqwewe/kameo/90138758779d2260798c41cfaa47598db84f05b8/src/message.rs).
Lifecycle и mailboxes — в
[Actor API](https://docs.rs/kameo/0.22.2/kameo/actor/trait.Actor.html).
Приведённые границы применения — вывод из этих механизмов и требований WinBridge.

По опубликованному manifest Kameo использует Tokio, требует Rust **1.88+**,
собрана в edition 2024. WinBridge может оставаться edition 2021: зависимости
допускают другую edition. Здесь использован Rust 1.98.0; Windows toolchain при
реальной интеграции надо сверить отдельно. Это не требует system changes сейчас.
[Manifest релиза](https://raw.githubusercontent.com/tqwewe/kameo/90138758779d2260798c41cfaa47598db84f05b8/Cargo.toml).

Kameo можно спрятать за внутренними interfaces/reducers. Тогда перенос worker
на другую actor library или обычные Tokio channels затронет runtime wiring,
а не протокол каждого проекта. При 2–3 проектах преимущество Kameo прежде всего
в ясном владении и lifecycle; прирост производительности без измерений не доказан.

## Важные правила при интеграции

**Mailbox delivery не равен commit до ACK.** `tell().send()` сообщает об отправке
сообщения, но accepted task должен подтверждаться только бизнес-ответом после
транзакции. Mailbox проверенного ядра находится в памяти; SQLite queue сохраняется.

**Reply timeout не отменяет доставленную команду.** В `ask` timeout ограничивает
ожидание ответа; операция может продолжить работу. Для мутаций нужен прежний key
и reconciliation. Явная отмена получает собственное подтверждение и реальный
итог дерева процессов.
[Реализация ask](https://raw.githubusercontent.com/tqwewe/kameo/90138758779d2260798c41cfaa47598db84f05b8/src/request/ask.rs).

**Async handler не означает обработку следующего сообщения во время await.**
Когда mutable handler ждёт сборку до конца, status/cancel того же актора ждут тоже.
Handler должен сохранять короткое решение и передавать работу отдельному
исполнителю. Результат возвращается новым сообщением с operation ID/generation.
Это подтверждено нашим прототипом ниже.

**Supervision не должна повторять команду неизвестного исхода.** В supervision
есть policies `Permanent`, `Transient`, `Never`; для supervised child default
`Permanent` допускает restart даже после нормального выхода. Это подходит не всем
одноразовым исполнителям. One-shot jobs не получают автоматический повтор
побочного эффекта; инфраструктурный actor восстанавливает journal, а не запускает
прежнюю команду в `on_start`.
[Restart policy в релизе](https://raw.githubusercontent.com/tqwewe/kameo/90138758779d2260798c41cfaa47598db84f05b8/src/supervision.rs).

**Detached future требует отдельного владения.** Для `Context::pipe` документация
исходника указывает: при остановке/restart получателя сообщение результата может
быть отброшено, а future не отменяется. Поэтому удобный helper не является
достаточным process owner или durable delivery mechanism. Нужны process registry,
журнал и подтверждение результата.
[Context::pipe](https://raw.githubusercontent.com/tqwewe/kameo/90138758779d2260798c41cfaa47598db84f05b8/src/message.rs).

**Actor fault isolation не является OS sandbox.** Она не изолирует shared mutable
state, которое разработчик сам вынес наружу, и не защищает весь процесс от abort,
OOM или native fault. Код внешнего проекта запускать отдельным процессом,
не загружать в WinBridge как Rust actor/plugin.

Владение сильными ActorRef должно принадлежать registry worker, а не только
сетевой сессии. `ActorRef` не становится долговечным task ID. Порядок разных
mailbox и запросов нескольких отправителей не заменяет resource scheduler.

## Почему сначала без distributed Kameo

`remote` использует libp2p, registry/discovery и remote messaging. Это отдельный
transport stack, а не adapter существующего WinBridge WebSocket.
[Remote API](https://docs.rs/kameo/0.22.2/kameo/remote/index.html).

Если сделать его публичным контрактом других проектов, придётся согласовывать
actor/message identities, сериализацию, версии и сетевой lifecycle; WinBridge
всё равно должен сохранить ownership/dedup/recovery и UI. Сейчас выгоднее оставить
stable wire envelope WinBridge, а локальную реализацию actor выбирать независимо.

Console/tracing/metrics Kameo могут помочь разработчику наблюдать mailbox и actors,
но не заменяют task/report journal, source pins и объяснения пользователю.
Новый monitoring listener/console по умолчанию здесь не вводится.
[Возможности библиотеки](https://docs.rs/kameo/0.22.2/kameo/).

## Изолированный прототип

Исходники опубликованного crate загружены с официального
[crates.io artifact endpoint](https://static.crates.io/crates/kameo/kameo-0.22.2.crate).
SHA-256 package:
`7cbab7323ed30490812f43ef6416a9e21bb150e9633e451ed124ca276b1ca82a`.
Основной `Cargo.toml`/`Cargo.lock` WinBridge не изменялись.

Prototype в отдельном workspace:
`.winbridge/research/actor-model/probe/`, локальный receipt —
`.winbridge/research/actor-model/proof.json`. Эти исследовательские файлы
не включаются в Git; выводы и команды сохранены здесь.

| Проверка на Linux | Результат |
| --- | --- |
| Три actors, 300 concurrent updates | Все обновления учтены, состояние каждого актора независимо. |
| Доставленная mutation, затем reply timeout | Caller получил timeout, mutation позднее завершилась; автоматической отмены нет. |
| Handler ждёт разрешения закончить, приходит status | Status не обслуживается до завершения handler. |
| Mailbox capacity 1 при занятом handler | Следующее сообщение получает `MailboxFull`; ранее отправленная работа выполняется. |

**4 tests passed** с отключёнными default features и ещё **4 passed** с включёнными
`macros`/`tracing`. Обе конфигурации прошли `cargo check --tests` под
`x86_64-pc-windows-msvc`; в варианте с macros также скомпилирован `derive(Actor)`.
`remote`, console и distributed compatibility не тестировались.

```bash
PATH=/home/user1/.local/bin:$PATH cargo test \
  --manifest-path .winbridge/research/actor-model/probe/Cargo.toml --locked

PATH=/home/user1/.local/bin:$PATH cargo check \
  --manifest-path .winbridge/research/actor-model/probe/Cargo.toml \
  --target x86_64-pc-windows-msvc --tests --locked
```

Это команды к локально сохранённому prototype, а не готовый repository test suite.
Windows cross-check подтверждает компиляцию локального ядра и нашего actor code;
не доказывает runtime работу Windows child transport, GUI, Job Object или проекта.
Рабочие Windows процессы не использовались и не изменялись.

Для принятия Kameo в основной код нужен следующий небольшой эксперимент:
state actor с настоящим Store, commit до ACK, restart из journal, контроль
process ownership и отсутствие дубля после lost reply. После него — one-shot
project executor/report round trip на Windows через одобренный сценарий.
