# U1: атомарное создание compiler process и Job

2026-10-18. В trusted compiler driver используется отдельный platform adapter для
абсолютного `.exe`. Windows создаёт suspended процесс сразу с приватным Job через
`PROC_THREAD_ATTRIBUTE_JOB_LIST`. Crash до PID receipt больше не оставляет
неуправляемый compiler root. Durable intent всё равно предшествует CreateProcess;
неизвестный исход не разрешает повторный запуск.

## Контракт

`ManagedChild::prepare_atomic_executable` принимает executable, отдельные аргументы,
absolute cwd, приватный Job name и optional execution domain. Это library API,
не agent-selected launch mode. Compiler driver использует его; существующий adapter
для общих worker commands, PATH, `.bat` и `.cmd` сохраняется.

- Local/safe paths проверяются до создания процесса. Shell/PATH поиска нет.
- При наличии domain ordered Job list содержит domain и private Job. Наследуемые
  внешние Jobs сохраняются, breakaway не запрашивается.
- CRT quoting сохраняет пустые argv, Unicode, кавычки и завершающие backslashes.
  NUL и слишком длинная UTF-16 command line отклоняются до создания процесса.
- `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` разрешает наследовать только stdin NUL и
  stdout/stderr write ends. Read ends, Job и остальные родительские handles закрыты
  для наследования. Owned handles/attribute buffers живут до окончания CreateProcess.
- PID/birth и Job membership проверяются до возврата suspended child. Только после
  durable receipts вызывается ResumeThread с ожидаемым previous suspend count 1.
- Собственные process/thread handles используются для wait и resume. PID не
  переоткрывается для управления процессом. Ошибка после CreateProcess обозначается
  `no_process_created = false`; private kill-on-close Job остаётся владельцем дерева.
- Supervisor сам не помещается в новый kill-on-close Job: будущий runtime worker
  должен пережить штатное завершение updater. Compiler Job охватывает только сборку.

Rust stable пока не предоставляет стабильный `Command::spawn_with_attributes`,
поэтому adapter использует небольшой Win32 слой и private RootChild enum. Общая
policy, журнал и FSM остаются вне platform.
[Документация Rust](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html#tymethod.spawn_with_attributes),
[Microsoft: JOB_LIST и HANDLE_LIST](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute).
JOB_LIST требует Windows 10 / Server 2016 или новее. При невозможности атомарного
создания adapter возвращает ошибку; fallback к post-create assignment не выполняется.

## Проверки и готовность

Новые Windows tests проверяют argv/stdio, suspended membership, закрытие до resume,
одновременные domain/private Jobs, некорректные команды, дополнительный suspend
count, crash владельца сразу после CreateProcess до PID receipt и отсутствие
наследования постороннего inheritable handle. Crash hook включён только в test build.
Native harness собирает fixture заранее, исполняет tests последовательно и сохраняет
`atomic-launch-proof.json` с exact build identity; GUI/runtime install отключены.

431 workspace tests, Windows Clippy/tests cross-check, fmt, scaffold и script syntax
прошли; итоговые 10 compiler regressions также passed.
[Native gate clean 11566cb](windows-atomic-launch-gate-2026-10-18.md) passed:
102 tests/14 groups, 24 artifacts и полный candidate build. Exact root-exit
assertion добавляется в следующем source admission rollup.
U1 целиком открыт: source/build orchestration, artifact verification, coordinated
abort/release и exact shutdown ещё не подключены. Обновление установленного worker
для этого изолированного gate не требуется.
