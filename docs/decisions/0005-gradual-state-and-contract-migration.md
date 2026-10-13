# ADR 0005 — постепенная миграция состояния и контрактов

Accepted: 2026-10-13, в рамках порученного пользователем архитектурного перехода.
Определяет порядок и ограничения реализации; новая runtime архитектура ещё не готова.

## Контекст

Этапы 0–7 дали рабочие commands, delivery, workflows, attention и MCP.
Растут количество проектов и число обработчиков. Текущие typed DTO и SQLite
dedup сохраняются, но владение состоянием/FSM/ресурсами распределено между
обработчиками. Actor runtime сам не решает durable acceptance, OS recovery и
совместимость старых consumers. Порядок принят в [плане A0–A9](../plans/architecture-transition/README.md)
после [повторного ревью](../plans/architecture-transition/review.md).

## Решение

1. Сначала frozen baseline; затем pure typed FSM в core и application service
   атомарных переходов у worker. Только после durable ownership/recovery переносить
   state owners в локальные actors. Library скрывается за interfaces; Kameo
   остаётся кандидатом до пилота, не external wire dependency.
2. Worker — единственная execution authority. SQLite — источник истины;
   commit состояния/события предшествует ACK. Actor mailbox, session generation,
   UI state и controller cache не дают durable acceptance.
3. Существующие IDs и dedup payload не пересчитываются при миграции. Для новой
   операции ключ scoped по stable operation family/project, contract version
   входит в fingerprint. Старые семейства tasks/imports/workflows сохраняют
   свои namespace/adapters, одинаковый ключ разных семей не объединяется задним числом.
4. Внутренний FSM state, wire version, contract version, DB schema и build identity
   различаются. Новый internal state проецируется в v1 DTO, пока контракт старого
   reader не допускает его напрямую. Принятая версия операции закреплена до ACK.
5. Revision проверяет актуальность изменения; ownership epoch — право текущего
   исполнителя. Grant durable и scoped по ресурсу/владельцу в journal incarnation.
   Connection UUID не epoch. Защита записей не прекращает I/O старого процесса:
   новый владелец физических ресурсов требует остановки или изоляции.
6. OS effect отделён от transaction: durable intent → effect → observation.
   Unknown process outcome остаётся indeterminate, recovery/restart не повторяет
   spawn. Cancel не равен факту остановки, закрытие UI не отменяет работу.
7. Общая ресурсная очередь вводится до полной actor миграции. Все mutating writers,
   включая legacy adapters и recovery/retention, учитывают общий guard;
   parent/child и lifetime app resources не создают обход лимитов.
8. Проектные executors — отдельные управляемые процессы с portable contract и
   native OS adapters. Сначала queryable report; outbox/inbox вводятся вместе
   для конкретного durable receiving flow. Нет обещания exactly-once OS execution.
9. Consumer test pack → provider safety tests → candidate pair → native/mixed
   gate → enable одного профиля. Existing v1 API продолжает обслуживать старые
   операции. Rollback не означает downgrade БД или потерю dedup keys.

## Последствия

Нет одновременной переписи всех компонентов и новой distributed infrastructure.
Сначала растёт тестовая матрица и строгость Store interfaces, затем меняются
owners и внешние возможности. Изменения protocol/DB/process semantics стоят
дороже добавления recipe, поэтому версии вводятся только на соответствующей
границе. Kafka, remote actor protocol, general DAG, hot project config reload
и удаление v1 требуют отдельной доказанной потребности.

В A0 добавлены только fixtures/tests/docs; CAS, ownership epochs, discovery,
scheduler и actor runtime пока являются следующими пакетами плана.
