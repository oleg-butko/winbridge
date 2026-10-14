# A3.2c — readonly readiness в desktop

Дата: 2026-10-14. После actual [producer gate](paired-runtime.md).

Desktop получает одну проекцию через существующий local worker IPC. Новый
outbound desktop connection, firewall path и HGFS polling не нужны. Controller
посылает bounded readonly frame только после того, как actual worker Info
advertises projection contract 1. Старому worker новые frames не передаются.
Worker cache хранит session/controller generation, собственные build/scope,
монотонный sequence и локальное время получения. Disconnect/reconnect и TTL
исключают применение прежнего `enabled`; cache не является execution authority.

Внутри opaque `contractCatalog` добавляется optional opaque projection, поэтому
worker RPC 1/2 frame/schema сохраняются. Known reader отдельно валидирует новый
payload; unknown version не включает readiness. Каталог всё ещё описывает
support, а проекция — evidence/current freshness. Малый bounded recent list
показывает состояние проверок и ссылки на задачи; полная paged history остаётся A3.3.

UI показывает четыре отдельных факта, profile/scope/build, причину несовпадения,
историческую квитанцию и статус проверки. Readiness не переименовывает existing
policy permission. Closing UI ничего не запускает. Устаревшие данные явно помечены.

Gate: legacy wire/schema, future/foreign/reordered payload rejection, disconnect
cache clearing, actual Unix WS→worker IPC→UI projection, shared Rust/TS reader
cases; isolated Windows tests/desktop compile через installed worker. Actual
installed new/new route требует последующего обновления worker. Full A3.2 не
закрывать до него. Ревью плана: no new mutation API, no mailboxes-as-journal,
no historical receipt auto-enable, capped metadata independent of process output.

Реализовано локально: negotiated latest-value frame, disposable worker cache
(TTL 10 секунд), обновление каждые 3 секунды, four-fact UI и task links.
Последние десять проверок — ограниченная сводка, не paged operation history.
Incomplete unpublished plan directory не подавляет остальные данные;
corrupt published record не интерпретируется как evidence.
307 workspace tests, Linux/Windows-target Clippy, TypeScript/UI build прошли.
29 shared Rust/TS cases и actual Unix WS→IPC→browser rendering/disconnect прошли.
Frozen worker RPC-2 schema SHA-256 прежний `dd86d35b8b9ed59aca0bb438491e906b9c25c69970f0121226fdee99b98e9a12`.
Native candidate gate готовится. Installed `b2aeb9f` не объявляет projection 1;
actual new/new Windows presentation требует следующего обновления.
