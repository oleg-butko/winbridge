# Supplemental exact-shutdown gate — пройден

2026-10-21. Task `99937c64-7cb7-42f7-9ef0-81e3fb19bdec`,
run `20261021-094402-73c4f112`, succeeded / exit0. Один integration test,
два полных случая за 648.51s; три команды runner завершились exit0.
Первые [четыре случая](windows-shutdown-main-gate-2026-10-21.md) не повторялись.

Отчёт в `K:\VMs\shared\WinBridge\workflow-validation\20261021-094402-73c4f112`.
Все 13 ready files сверены независимо дважды. SHA-256:

- `report.json`: `8f5cc63f8fdb600c07216440342c4599ac85579774df218715967f06bb97f03c`.
- `candidate-health-actor-proof.json`: `575df2a192b42f45046bcc2997ac79561e6cf22071a34e03334b8b050f990d7c`.

Source `9ca503964e99c5c2991d8bc2133cc6d3049bbcee`, clean build
`1792575854121152900`, manifest SHA
`02613ada5ef5f2c0b3c479cb0e38a999b72adda837d34aeddcfe5d030c4b559e`.
ZIP `winbridge-20261021T094333Z-9ca5039-8e3e52d5.zip`, SHA
`96c1962cdb92ee841ad986b6a34822ea915ec1bb4d5e26bc4905c0f403d67a3b`.
Target — прежний verified A294, worker16/updater12, исходники заранее проверены.
Source/apply здесь имеет ноль изменений; candidate и четыре old-role exe настоящие.
Normal Main использует только явную fixture feature; production Job guards сохранены.

| Case | Main / helper PID | Итог |
| --- | --- | --- |
| Delayed instance lock, update `ec3e33fa-c69b-4090-b113-36e93d2ae07c` |5600 /2440| Exited rev3, root Replacing после release |
| Supervisor crash before exit, update `a1ba8cc0-92cb-4bbd-8b12-c062da18b385` |16424| Acknowledged rev2, root Stopping после crash |

В первом случае отдельный принадлежащий parent Job процесс получает исходную
worker.lock после завершения serve, пока Main ещё жив. Проверены PID/birth/host/
session, attempt ID и исходные File IDs. Main затем выходит; в течение двух секунд
root остаётся Stopping/Acknowledged при занятой lock. Только после release helper
supervisor получает guard и атомарно записывает Exited/ Replacing. Далее проверяются
cold receipt, обычный startup refusal, неизменность runtime и отсутствие custody
после завершения своего supervisor Job.

Во втором случае Main удерживается после acceptance. Parent проверяет живые Main
и supervisor, затем завершает только свой validation Job. Сохранённые root и receipt
остаются без изменений; cold reader возвращает ту же acceptance, startup блокируется,
историческая проекция не создаёт Exited/custody. Удаления guards и повторного stop нет.

Verifier дополнительно отклонил девять искажений реального proof: missing case,
неверный lock File ID/attempt, отсутствие проверки lock, fabricated exit/commit,
выдуманная custody, второй stop и writable cold reader. Локальная запись —
`.winbridge/update-preparation/shutdown-supplemental-native-verified.json`.

После native run дополнительно укреплено ожидание lock в fixture: Weak::upgrade
может вернуть None до окончания destructor последнего Arc. Helper теперь ограниченно
ждёт настоящую byte lock, сохраняя тот же открытый original file и проверяя File ID.
Эта небольшая fixture правка прошла Windows Clippy; native producer выше — 9ca5039.
Production shutdown driver не менялся.

Windows available 47171813376→46212440064 bytes; очистка не выполнялась. Installed
worker d052, generation `ee48ba22-7a9e-4ce3-8ed1-5080a90806a3`, healthy/idle 0,
coverage complete, ошибок нет. Процессы/окна теста закрыты, worker оставить работать.

Это завершает внутренний U1 preparation/shutdown цикл вместе с прежними gates.
Следом U2: final consistent backup под живым guard, затем replacement/activation.
Public update, desktop coordination, rollback и установленное самообновление ещё впереди.
