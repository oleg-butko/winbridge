# Контекст переноса из Netwatch

[Первоначальное предложение](original-proposal-2026-10-04.md) перенесено из `/home/user1/netwatch/docs/windows-bridge-proposal.md` без изменения bytes. Оно сохраняет рассуждения, ссылки и происхождение идеи.

Оно было написано до последних решений пользователя. Три положения заменены: TLS исключён, общий repository/workspace с Netwatch заменён отдельным `~/winbridge`, основной delivery adapter — VMware Shared Folders с отдельной Windows build папкой. Имя WinBridge принято окончательно. Текущие требования: `../architecture.md`, `../decisions/`, `../handoff.md` и корневой AGENTS.md.

Netwatch commit `39409e5` содержит пять исправлений после code review; рабочее дерево очищено от WinBridge proposal и его ссылок. В Netwatch остался собственный review report. Поведение мониторинга и внешних VPS не менялось ради нового проекта.

Полезные места Netwatch для изучения, а не runtime dependencies: agent IPC, launcher supervisor/runtime, source unpacker и их tests. Перед переносом учитывать busy pipe retry, общий heartbeat deadline, bounded pong writes, junction rejection и ownership backup при rollback. Windows runtime проверка этих веток всё ещё нужна.

[Снимки перед переносом и команды восстановления](migration-recovery.md) записаны отдельно. Backups не изменялись после создания.
