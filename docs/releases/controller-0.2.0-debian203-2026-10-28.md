# Controller .203 обновлён — 2026-10-28

Пользователь сообщил об установленном Windows worker0.2.0 и поручил обновить
controller и продолжать updater. Pull main591b471 не дал новых commits;
проверена и fast-forward включена remote ветка `refactor/multi-controller-review`
f069697 (включая release4bc1710/deployment f4c53f5). Production изменение — общий
readonly diagnostic sender/Info helper, сохраняющий одну сессию и прежние guards;
прочие Rust изменения относятся к tests. Нового worker для них не требуется.

## Обновление и подтверждение

Controller lib17 + real CLI/MCP diagnostics1 прошли, Python input6/0.
Новый controller0.2.0 собран из clean f0696978e2a91c7db5ff30f656d5443c1916929e,
build1793171869603595972. Старый user process PID5103/source768422d/0.1.0
скопирован из `/proc/5103/exe` и штатно остановлен SIGINT с подтверждением exit.
Новый PID131887 запускается из независимого retained image, не из target cache:

```text
/home/user1/winbridge/.winbridge/controller-runtime-updates/20261028T071841Z-203/winbridge-controller-0.2.0
```

Config прежний `.winbridge/controller-snapshot.local.json`, SHA
`0d6d7ace18b8fd40e3345675875a370a28071e24d0859c041b84aefbe37768b9`, state/instance
прежние. Firewall/systemd/services и controller.204 не менялись. Из fresh Info
дождались actual projection с новым controller build, а не повторяли startup при
первой отсутствующей projection. Controller generation58e55f0e… .203:1143.

Readonly connections через новый CLI подтвердили одновременно:

- debian-203/.203:1143 connected, session75bce726…, generation58e55f0e…;
- debian-204/.204:1143 connected, прежняя generationc7b5de88… .

Worker0.2.0/source48c834c/build1793126009319459400 остался прежним:
generation946f1128-2c30-42b6-b369-8acda852a834,
incarnation5a8eb173-4193-4332-b00f-67e3850b2c97, active0/recovery errors=null.
Все10 Task IDs из среза до restart отдельно прочитаны через get_task и сверены
по ID/status/project/acceptedAt. Исторический reset в lastError ожидаем при
restart; текущие connection.error=null, connected. Перед restart завершена одна
readonly Windows задача измерения диска/reference: d06f3b88… succeeded/0.
Windows K free24,554,651,648, retained released worker17 SHA536d8637… совпал.
Debian до build13,921,480,704 bytes; очистка не требовалась.

Evidence: каталог runtime-updates выше (preparation/old-exit/new-image/owner/
before/after/status/connections/verified и оба images), а также
`.winbridge/review-2026-10-28/`. Runtime/config/evidence не коммитятся.

## Запуск и откат

После проверки отсутствия другого процесса/свободного1143 запуск:

```bash
.winbridge/controller-runtime-updates/20261028T071841Z-203/winbridge-controller-0.2.0 \
  --config .winbridge/controller-snapshot.local.json serve
```

Для rollback сначала сверить PID/start/executable с owner.json, штатно завершить
новый controller и подтвердить exit. Затем вместо него:

```bash
.winbridge/controller-runtime-updates/20261028T071841Z-203/previous-winbridge-controller \
  --config .winbridge/controller-snapshot.local.json serve
```

Old image SHA `f69cdcc61d53759e7b3899b2272d877136eb5fd1d4c05d7e4fba7ac94c0b28e1`.
Новые readonly wrappers старому0.1.0 недоступны. Worker/config/journal откат этим
не выполняется. Обновление controller завершено; installed worker оставить
запущенным, desktop не нужен. Далее updater controller-session/lifetime/commit.
