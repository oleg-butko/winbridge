# Ревью lifetime runtime из Debian .204

2026-10-28, Debian .203. Ветка `feat/updater-lifetime-runtime-204`, base94629b2,
commits90fc5b4/2a0bee6/49b6d7b. Последний WIP commit пользователя включён в ревью;
его файлы восстановимы из Git. Runtime installed worker0.2.0/source48c834c
и production controller131887 не заменяются.

## Обнаруженная ошибка и проверенный подход к исправлению

Сохранённый Main27 `20261028-224638-3e951af3` завершился неуспешно.
В `shutdown-main-pipeline.log` отказ: `Controller delivery is not original Issued`.
Supervisor trace подтверждает отдельные `private_ready_verified`,
`normal_mode_verified` и `lifetime_fact_confirmed`: entry занял15,264ms,
оставалось37,073ms исходного окна. Это полезная диагностика, но не passed gate.
Report/failed logs и runtime archive сохранены; успешный новый результат должен
получить собственный run ID, без изменения прежнего отчёта.

Причина — `observe_lifetime` вызывал `inspect_activation_controller_receiver`,
предназначенный только для первоначальной V5 delivery. Controller в этой точке
уже Confirmed. Проверку Issued ослаблять нельзя: это сломало бы no-replay fence.
В49b6d7b добавлен отдельный `activation_lifetime_owned_current`: один SQLite snapshot
проверяет current authority/supervisor/root/local7/nonce и полный immutable graph,
точный lifetime command key. Он вызывается до и после normal IPC; история
Issued/Indeterminate/Confirmed читается как данные, а не как повторный send permit.
Регрессионный тест проверяет все три состояния, чужой ключ и изменённый prefix.

## Остальной код и границы

- V6 продолжает тот же authenticated stream и RunningLocalService. Initial receiver
  принимает только Issued lifetime, проверяет original parent/window до sealing.
- Verification → Supervised — typed resident transition внутри исходного окна.
  Entry receipt immutable; время позднего readonly наблюдения хранится отдельно.
  Запрос не создаёт новый mode, Store/authority, instance lock или startup budget.
- Job membership запрашивается у существующей Job с query rights; временный handle
  закрывается, limits не меняются. Keeper-loss watchdog дешёвый, без idle hashing.
- Parent потребляет original preparation один раз, независимо проверяет actual
  normal IPC/PID/readonly Info. После неизвестного ACK можно только наблюдать
  тот же command; cold journal read не восстанавливает custody.
- Supervisor27/view19 явно отделяют saved receipt, observedAt, process liveness
  и original custody. Views1–18 сохраняют прежнюю форму. Public install off.
- Main27 использует stable runtime-test-204 и отдельный controller1144, не legacy
  .203 slot. Distinct clean target/source pins, persisted request до submit,
  native exact-test selection, causal hashes и negative audits сохранены.

Дополнительных критичных дефектов в этих границах при source review не выявлено.
Permanent keeper role, root commit/scoped release, private Ready loss fault и
проверка двух поколений остаются отдельной работой; dispatch-ACK loss в Main27
не называется private-Ready-loss coverage. Пока новый native run не прошёл,
исправленный Windows путь не объявляется подтверждённым.

## Воспроизводимые проверки

Логи этой сессии: `.winbridge/review-2026-10-28/lifetime-204-*`.
Workspace/update-validation:964 passed,17 ignored,126 groups; регрессия
`lifetime_observer_requires_confirmed_controller_and_same_key_in_all_outcomes`
прошла. Python regressions:45 tests passed (lifetime4/controller5/service9/
lifetime migration8/multi-controller6/controller migration5/OS spike8).
TypeScript strict/UI build/fmt/diff/scaffold323 passed. Linux/MSVC workspace
all-target Clippy с update-validation/`-D warnings` и generated Rust/TS contracts
прошли. Ниже описан preflight,
не успешный Main27.

Readonly Windows preflight task5a84140a succeeded0: K:24,060,624,896 bytes,
installed worker PID2532, test-slot processes0, slot worker EXE отсутствует,
worker17 reference SHA536d8637 совпадает. Runtime/state/config не менялись.
Перед source checks Debian10,324,602,880 и shared24,148,213,760 bytes свободно;
сборки ниже6GiB не запускались, очистка не выполнялась.

Следом — слияние reviewed source, clean export и один новый Main27 через worker,
independent reread; затем remaining lifetime fault/keeper/commit/release/U2–U4.
Рабочий worker оставить запущенным, desktop и тестовые окна не нужны.
