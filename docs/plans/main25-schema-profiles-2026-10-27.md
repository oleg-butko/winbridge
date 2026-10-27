# Main25: явные schema profiles и cold worker migration

2026-10-27, feature.multi-controller на Debian.204. Пользователь разрешил выполнить
план и вручную подключил installed worker0.1.1 к.204. После последнего Windows
прогона сообщить, что worker можно переключить обратно на.203.

## Реализация

- Mainba9557b переносится в feature с ранее проверенной startup/Links адаптацией.
  Временный LocalService не запускает network/scheduler/mutations; Main25 preflight
  по-прежнему требует явный single-route fixture, не выбирает primary из topology.
- `update-activation-service-proof.py`: profiles `worker17-updater20` (исторический
  default) и `worker19-updater20`. Worker schema проверяется в context, writer
  binding и service compatibility. Updater20, view17 и startup240s не меняются.
  Unknown profile/schema, bool/float/string coercion и смешанные17/19 отклоняются.
- Независимый verifier связывает выбранный profile с константами из обоих verified
  ZIP/source manifests и затем проверяет proof. Ожидаемая версия не берётся из proof.
- Новый create-only runner сохраняет original fenced context/key/request до submit,
  не replay-ит неизвестный исход, использует отдельный controller1144 и stable
  `K:\_proj-26\winbridge\runtime-test-204`. Чужой runtime-test и pending
  conservation plan не используются и не перемещаются. Controller остаётся при
  indeterminate task, закрывается после terminal. Windows desktop не нужен.
- Отдельный ignored native test создаёт реальную историю released worker17 на
  новом state, затем запускает worker19: same task/output/dedup, verified SQLite
  backup17, legacy unknown creator, новые creator/cancellation после restart,
  old17 refusal без изменения main/WAL frames и два marker executions без replay.
  Reference EXE копируется по явному SHA, installed state/config не открываются.
  Это cold17→19, не migration authority для active updater handoff.

## Проверки и execution pins

До build Debian13 GiB. Readonly Windows preflight task
`a0c6924a-360e-4e38-98d4-0993fcee6857` succeeded/0:
free31,514,517,504 bytes; runtime-test занят прежним reservation, runtime-test-204
не существует. Installed worker source59249e6/generation772e0679/journal incarnation
5a8eb173 остаётся прежним. Никаких production replacements/cleanup/firewall changes.

Linux workspace894 passed/17 ignored; Python profile/evidence boundary tests6/0.
MSVC workspace/all-targets/update-validation check и Clippy `-D warnings`, UI/typecheck,
fmt/scaffold303 прошли. Source/target pins и native результаты записываются по завершении.
Не считать подготовленные profiles или synthetic boundary tests native proof.
