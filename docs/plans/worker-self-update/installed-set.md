# U1: известный набор установленного runtime

2026-10-18. Trusted readonly library, без publication/install.

`winbridge-installed-set.json`, format 1, описывает set ID, installation ID,
scope и четыре фиксированных exe: размеры, SHA-256 и verified artifact descriptors.
Manifest должен появиться при проверенном bootstrap/install. Reader не создаёт
его автоматически и не считает текущие найденные файлы известной установкой.
Отсутствующий manifest означает Bootstrap required; другой installation/scope,
dirty/mixed source, неверные роли, hard links и изменённые bytes отклоняются.

`pin_installed_set` открывает manifest и четыре изображения, сверяет hashes/marker
и удерживает Windows read handles с запретом write/delete. Observation сохраняет
runtime directory identity и File IDs. Перед использованием `revalidate` повторяет
проверку. Caller обязан удерживать guard, а перед заменой отдельно подтвердить
shutdown и безопасно передать владение file handles следующему driver.
Linux development adapter обнаруживает изменения при revalidate; он не заявляет
Windows semantics запрета записи/удаления.

`bind_running_worker` получает descriptor из кода текущего процесса, сравнивает его
с worker entry, проверяет executable path/File ID/bytes и связывает с PID/birth/
host/session. Этот constructor предназначен для раннего startup worker с guard
на всю жизнь процесса. Произвольный caller descriptor не принимается. Сейчас
normal worker Main подключён следующим [startup guard](runtime-startup.md);
authenticated live-worker control contract и positive actual-main native gate
остаются следующей задачей. Observation/DTO сами
по себе не являются разрешением остановки, замены или activation.

Source commit/manifest в installed set относятся к binaries. Их нельзя безусловно
сравнивать с текущим src manifest: при первом bootstrap binaries C могут работать
с сохранённым src A, а во время обновления src B уже применён, пока старый runtime C
ещё обслуживает другие проекты. Source transaction сохраняет собственные pins.

Существующие updater schema 7, worker schema 15, legacy RPC и supervisor v1–v5
не меняются. Следующий coordinator должен закреплять проверенный baseline до
SourceApply; publication current-set metadata, live identity/acceptance binding,
exact shutdown, runtime transaction и activation ещё не реализованы.

Четыре tests проверяют readonly/no-adoption, missing/foreign manifest, dirty/mixed/
duplicate/changed/hard-linked files и удержание Windows pins. Проверка bind текущего
постороннего процесса отрицательная; positive worker startup пока не подтверждён.
Native readiness — в handoff, весь U1 остаётся открыт.
