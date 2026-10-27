# Резервирование постоянного activation test runtime

2026-10-26. Продолжение [Actor25](local-service-actor-2026-10-26.md), commit5a0f83d.
Это test infrastructure, не public installation/recovery API. Full same-child
Main25 пока не запущен. Installed worker0.1.1/source59249e6 не менялся.

## Реализовано и проверено

`scripts/lib/test-activation-runtime.ps1` использует существующий
`Open-TestWorkerRuntime`: прежние schema1 marker, exclusive lease, source SHA,
production separation, отказ неизвестным файлам и работающему worker сохранены.
Добавлен immutable marker `winbridge-activation-runtime.json`: reservation ID,
точный runtime/run/production path, source commit/manifest SHA, initial image SHA
и timestamp. Это evidence, не native process/Job custody.

Сначала удерживаются image pin, marker pin и runtime lease. Отдельный
`Release-TestWorkerActivationImagePin` один раз снимает только image pin **до**
normal Main/preparation/admission; lease и marker pin остаются. Script не мешает
последующей native replacement. Замена требует исходный updater guard.
Close идемпотентен, освобождает handles и сохраняет marker/config/image. Оба helper
отказываются заново использовать такой slot. Автоматического archive/cleanup/
принятия старой custody нет. Main fixture пока не подключён; slot ещё не полностью
переиспользуемый между activation runs. Production `runtime` и network slot
`runtime-test` этим прогоном не открывались для записи.

Native Windows run **20261026-210824-6ce141b3**, task
**354d9f95-6d9c-4066-ad0f-612c9ce7e17e**, succeeded/0:

- прежний `test-worker-runtime.ps1`: **6 checks**;
- новый `test-activation-runtime.ps1`: **11 checks**: pins/paths/overlap/unknown files,
  contenders, реальные image/marker write locks, single pin release с сохранением
  lease, cold no-reuse с сохранением config;
- native AST parser разобрал все6 PowerShell files; source hashes проверены до
  запуска и независимо дважды прочитаны из shared после результата;
- Debian verifier проверил exact check names/counts/boolean flags и source manifest
  по сохранённому stdout byte stream, а не только exit0;
- старый policy test запускает только свой fixture-long; новый не запускает
  процессов/сети. UI окон нет.

Producer — pinned worktree files на основе5a0f83d, **не clean source export и не
скомпилированный Main25**. Source manifest SHA:
`b8d18b8034c85f368ef514b1d136ff50e33b5f9c6d0434a41080354ce1dbea8b`.
Chunks SHA:
`88a9d4c6c084be7cf3cc0df9fa990a160ad87d9e36986a4e19e923b9eaf91de0`.
Evidence: `.winbridge/update-preparation/activation-slot-policy-20261026-210824-6ce141b3/`
(`request`, `accepted`, `terminal`, `chunks`, `info-before/after`, `source`, `verified.json`);
observer log `activation-slot-policy-repeat.log`. Shared source bundle:
`K:\VMs\shared\WinBridge\workflow-validation\20261026-210824-6ce141b3`.
Test files сохранены внутри registered project:
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\activation-slot-policy\<run>`.

## Исправленная ошибка первого прогона

Run20261026-210733-be6eb24b/task31e73cb3-4efc-4f98-bee6-11d38838ed13 failed/1:
условие `-or`/`-and` в новом path validator без скобок приняло `K:relative`.
Добавлены явные группы. Negative paths проверяются чистым validator до файловых
действий; unexpected successful reservation закрывается test helper перед ошибкой.
Первый failed run создал test-only slot
`K:\_proj-26\winbridge\workspaces\snapshot-fixture\relative` с копией fixture,
двумя test markers и lease file. Он сохранён, не является reference; скопированный
exe не запускался. Повтор использовал новые каталоги. Рабочий worker не сбоил.
Не удалять Windows test directories по этому документу.

## Storage и рабочие процессы

Успешный run: K:33,389,379,584 →33,387,823,104 bytes available;
Debian после проверки16,098,131,968 bytes. Сборки/cleanup не выполнялись.
Installed worker SHA:
`536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712`,
generation936909c6-d148-40cd-abf6-a0d996935994,
incarnation5a8eb173-4193-4332-b00f-67e3850b2c97 прежние.
После task active0, maintenance/recovery errors=null. Controller5103 оставлен
работать192.168.44.203:1143; test processes завершены, desktop не нужен.

## Продолжить отсюда

1. Explicit fixture runtime layout с default прежним root/runtime, фактический
   stable path объявляется до setup/manifest/baseline/admission. Нужны изменения
   update_shutdown helpers и feature-only update_control/validation.rs, который
   пока требует root/runtime и controller=None. JSON не заменяет original guard/lease.
2. Архивирование только собственного inactive slot после actual native child/Job
   exit и process/lock/marker/file checks. Сохранять journals/configs/paths; crash
   archive не даёт нового permit. Текущий helper намеренно отказывает повтору.
3. Отдельный controller1144, сеть точного stable exe проверяется до original startup
   window240s; first firewall wait900s отдельно. Rules не менять.
4. Main25 branch сразу после writer confirmation, до тяжёлых audits: сохранённый
   controller URL без соединения local service, те же PID/lock/generation, один
   epoch+1, history/output/pause, root AwaitingController, lost ACK/cold no-replay,
   actual Job exit. Независимый verifier/negative mutations и actual19→20/refusal.
5. Затем controller/lifetime/commit/release, remaining U2/U3/U4 и A4.3. Прежние
   gates не переименовывать. Ручное обновление worker сейчас не требуется.
