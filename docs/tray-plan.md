# Следующий сценарий этапа 5: tray и уведомления

План 2026-10-07 фиксируется после [ревью](review-attention-2026-10-07.md), до реализации.
Это продолжение незавершённого этапа 5; этап 6 с performance ещё не начинается.

## Поведение и зачем оно нужно

- Закрытие main window сначала дожидается сохранения всех форм и скрывает окно.
  Tray возвращает окно; отдельная команда завершает companion после того же save ACK.
  Worker и принятые команды продолжают работать. При save error окно остаётся открытым.
- Один companion для одного runtime directory, повторный запуск открывает существующее
  окно. Это предотвращает двойные сигналы и одновременную запись desktop state.
  Отдельный validation runtime получает собственную identity и не затрагивает рабочий UI.
- Rust background poll читает worker независимо от hidden WebView. Tray показывает
  worker freshness, точное pending attention count и доступный счётчик активных задач.
  Неизвестный/старый счётчик не показывается как ноль.
- Небольшой Tauri popup содержит проект, severity, причину, pending count и «Открыть
  задачу». Он не забирает focus автоматически. Открытие переводит к точной task page;
  acknowledgement остаётся отдельным действием пользователя в worker journal.
- Popup и встроенный короткий Windows звук по умолчанию включены для warning/error,
  volume 25%, минимальный интервал сигналов 30 секунд. События за интервал объединяются
  в один popup; все остаются в журнале. Quiet hours по умолчанию выключены, задаются
  местным временем Windows, поддерживают переход через полночь.
- Preferences и cursor сигналов сохраняются рядом с exe в desktop state, отдельно
  от frozen workflow и worker acceptance. Restart не повторяет уже показанные события;
  quiet hours/rate limit не подтверждают и не удаляют пропущенное внимание.
- Ошибка presentation/storage видна в main window. Без исправного tray окно не
  скрывается так, чтобы пользователь потерял возможность управлять интерфейсом.

## Реализация и проверка

DTO — protocol, чистая quiet/severity/rate policy — core, journal — worker,
Tauri tray/popup и Windows sound — desktop. Используется [Tauri System Tray](https://v2.tauri.app/learn/system-tray/).
Нет дополнительного сервиса, нового транспорта, глобальных hotkeys, firewall или
изменения production Netwatch.

Проверки включают persistence, dedup/restart, quiet hours с midnight boundary,
rate limit и acknowledgement во время отложенного сигнала; close/save/error/quit и
переход к задаче в UI. Автоматический native сценарий собирает отдельный desktop,
использует отдельные configs/state/pipe и публикует checksum-verified shared report.
Не выдавать compilation/browser mock за выполненное native popup/tray событие.
При вопросе или неожиданном поведении остановиться и уточнить у пользователя.

Далее в этапе 5 остаются workflow/app attention, history/audit/diagnostic export и MCP.
