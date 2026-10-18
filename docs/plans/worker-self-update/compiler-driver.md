# U1: managed compiler attempt

2026-10-17–18. Продолжение [fresh SourceControl](fresh-source-control.md).
Реализуется trusted library для одного сохранённого compiler attempt на update.
Автоматический source/build supervisor и публичный update API пока не подключены;
полученный exit 0 не является разрешением на cutover или установку.

## Полномочия и пути

Caller берёт команду из trusted local build policy; agent JSON не выбирает executable,
cwd, runtime или Job. Будущий source/build driver сформирует фиксированный вызов
проверенного build script. Перед новым attempt нужны parent Building, Verified source
transaction, свежая SourceHeld с прежним owner/epoch/revision, config SHA, installation
scope и directory identity. Cwd — точный frozen source root, executable — local file.
Source manifest проверяется до запуска, перед Windows resume и после завершения.
Pure compiler FSM находится в core; readback проверяет все bounded v1 receipts.

До сборки измеряется свободное место source/runtime; менее 6 GiB откладывает запуск
без очистки. Compiler fixtures используют локальный Cargo volume: `/tmp` может быть
маленьким tmpfs. Локальная настройка temp не ослабляет production storage guard.
SourceHeld уже удерживает work slot. Named-resource update plan пока не поддержан;
worker не выдаёт новый grant при такой policy.

## Durable intent и дерево процессов

Updater journal schema 4 добавляет immutable build spec, отдельный attempt ID
(он отличается от binary BuildIdentity), CAS revisions и отдельные
receipts каждого этапа. Migration 1/2/3 → 4 требует owner lock и verified consistent
backup. Worker journal остаётся schema 15. Schema-3 updater должен отказаться открыть
schema 4 до любых WAL/migration writes. Старые source status остаются readonly.

Последовательность: Intent → создать suspended Windows process/назначить отдельный
compiler Job → сохранить exact PID/birth/host/session + Job receipt → сохранить
ResumeGranted → свежие source/authority проверки → resume → Running receipt.
ResumeGranted после сбоя обозначает неизвестный исход запуска; успех требует
сохранённого actual-start receipt. Production driver вызывается из infrastructure supervisor, проверенного на
исключение из worker domain. Library сохраняет обычное наследование Jobs. В native fixtures сохраняется внешний validation Job, breakaway нет.
Unix adapter проверяет development lifecycle, но не доказывает suspended launch
или восстановление Windows Job.

Повтор исходного key/command возвращает сохранённый record, в том числе Intent,
Running или Indeterminate. Новый payload/key того же update не создаёт второй
attempt. Новая owner epoch тоже лишь читает прежний record, не перепривязывает
старую worker lease и не продолжает процесс.

## Output и завершение

Два reader читают stdout/stderr независимо от UI, сохраняют до 16 MiB на поток и
продолжают drain после квоты или ошибки записи. Отдельно учитываются observed и
retained bytes, SHA и ошибки capture. Readback сверяет реальные файлы; partial
write не выдаётся за сохранённый полный блок.

Root exit не доказывает завершение helpers. Driver завершает свой compiler domain,
проверяет root exit, пустоту дерева и окончание обоих readers. Timeout возвращает
известный TimedOut только после этих доказательств. Ошибка источников/config/output
после процесса оставляет Indeterminate. При неизвестном cleanup отдельный platform
path закрывает только свои handles без unbounded wait; он не утверждает exit/empty.
Барьер и журнал сохраняются, повтор процесса запрещён.

Readonly `journal build-status UPDATE_ID_OR_ATTEMPT_ID` показывает saved record,
отдельное текущее OS observation и запрет replay. Empty Job не меняет сохранённый
Running/Indeterminate на успех. SourceHeld/startup guard остаются. Raw lifecycle
Fail/Defer/BuildSucceeded не обходят compiler receipts: до release нужен следующий
координированный artifact/abort driver. Пользовательская queue pause не снимается.

## Проверки и следующие зависимости

Проверяются success/dedup/key conflict, nonzero exit, timeout дочернего дерева,
output quota, ignored intent/PID/resume receipt writes, source/config changes,
ownership restart и consistent schema-3 backup. Native owner-crash test сохраняет
Running attempt и startup guard без replay; suspended-launch assertions выполняются
только на Windows. Этот gate проверяет fixture compiler lifecycle. Полный candidate
build выполняется отдельно прежним build-only сценарием; это разные доказательства.

429 workspace tests прошли; после итогового ревью прошли 10 целевых compiler checks.
Ревью добавило проверку всей bounded receipt history и отдельный attempt ID;
Clippy, Windows/tests cross-check, fmt/contracts, typecheck/build:ui/scaffold
и Python/PowerShell syntax passed. [Native gate schema 4](windows-compiler-gate-2026-10-18.md)
прошёл: 95 tests/13 groups, 22 artifacts и полный candidate build; installed worker
не изменён. Далее:
автоматическое source/build выполнение в supervisor, artifact verification и
координированный abort/release, exact shutdown, затем U2–U3 и один bootstrap.

Dev/test profile оптимизирует стандартную sha2 library, не меняя алгоритм/хеши.
Один и тот же verified ZIP (4,589,622 bytes) на Debian: 2.277 → 0.620 s wall,
2.20 → 0.56 s CPU. Это локальное измерение одного пакета, не Windows baseline.

До полного production pipeline требуется закрыть crash gap CreateProcess → Job
assignment: Intent без PID receipt всегда требует manual inspection и не replay.
Следующий platform block проверит атомарное назначение Jobs при CreateProcess
через [PROC_THREAD_ATTRIBUTE_JOB_LIST](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute).
[spawn_with_attributes в Rust std](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html#tymethod.spawn_with_attributes)
пока nightly-only, поэтому требуется отдельный stable Win32 adapter с проверкой
argv/handles и crash faults. Текущий library gate не доказывает отсутствие orphan
в этом crash window.
