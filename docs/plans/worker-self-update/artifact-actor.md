# U1: supervisor v5 — проверка artifact set

2026-10-18. Новый internal action `verify_candidate_artifacts`, contract 5.
Legacy/supervisor v1–v4 response shapes и updater schema 7 остаются прежними.
Это trusted preparation API; public self-update/install не включены.

Action требует точные Active worker-bound supervisor, nonce/endpoint/revision,
current source authority и успешный Completed compiler. Затем responsive mailbox
отдаёт blocking driver проверку файлов и atomic artifact receipt/FSM transition.
Во время чтения journal остаётся у той же authority/epoch; status обслуживается
независимо. Duplicate активной artifact-проверки читает текущий status; повтор
после commit возвращает сохранённый receipt. Другой active driver не выдаёт ACK
за ещё не начавшуюся artifact-проверку. No-effect readonly dispatch не создаёт
внешнего процесса и не требует отдельного execution intent.

Building → ValidatingCandidate означает только подтверждённые artifact bytes,
roles/source и завершённые metadata queries. SourceHeld и slot удерживаются;
raw release запрещён. CandidateAccepted/health/install остаются недоступны.
При неверном report driver_error сохраняется, parent Building не превращается
в успех, повтор dispatch отклоняется до inspect/abort. Safe abort подтверждает
source/compiler cleanup и, если receipt существует, его digest перед release.

Positive fixture запускает настоящий PowerShell compiler actor. Rustc собирает
четыре маленьких fixture-exe с contract-1 metadata, updater реально запускает
bounded queries и сохраняет exact report. Supervisor проверяет его, записывает
receipt, остаётся отзывчивым и затем завершает safe abort. Это real process /
contract interoperability; fixture-exe **не доказывают здоровье WinBridge worker**.
Negative fixture оставляет неполный report/payload: receipt не появляется, source
barrier сохраняется, retry запрещён и отдельный safe abort освобождает slot.

Local: оба explicit actor tests passed, 477 workspace tests, Linux/Windows Clippy
all-targets, generated contracts, fmt, typecheck/build:ui/scaffold, export/runner
tests и portable PowerShell finalization regression passed. Native rollup впереди.
[Предыдущий run](windows-artifact-set-diagnostic-2026-10-18.md) прошёл 149 tests и
actual build/query; overall failed из-за helper variable в PowerShell. Отдельная
actual-file диагностика passed. Ошибка исправлена, portable test исполняет реальный
final statement до полной native сборки; следующий gate объединяет оба блока.

После native gate: old runtime baseline/loaded image guards до полного pipeline,
product source layout check до SourceApply, exact shutdown, candidate journal /
health rehearsal, activation и U2–U3. Full source/build/artifact/health cycle
на actual WinBridge остаётся обязательным до bootstrap; отдельные fixtures и
build-only диагностика его не заменяют.
