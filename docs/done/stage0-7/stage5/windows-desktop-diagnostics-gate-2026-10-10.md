# Закрытие desktop и диагностика: Windows gate, 2026-10-10

Дополнение после завершения roadmap. Пользователь сообщил о неработающих close/hide/quit
после добавления и удаления проекта и о внутренней кнопке «Скрыть» в popup.
[Инструкция пользователя](../../../guides/desktop-diagnostics.md).

## Исправления

- Project undo восстанавливает saved JSON bytes: форматирование больше не оставляет
  ложную dirty configuration. Отказ закрытия перенесён к tray.
- Save/native ACK ограничены 10 секундами, кнопки становятся доступны после timeout;
  поздний ACK не скрывает окно. State I/O выполняется в spawn_blocking.
- Popup Win32 SW_SHOWNOACTIVATE оставлял Tao visibility cache=false, поэтому Tauri
  hide возвращал успех без изменения HWND. Теперь show/hide используют парные Win32
  операции; hide проверяет IsWindowVisible. Popup крестик и навигация используют
  общий helper. Popup commands асинхронны.
- Неблокирующий desktop JSONL: 1 MiB + 5 архивов, bounded queue, single writer,
  crash tail repair, UI события и native dispatch watchdog. Поля/commands/output не пишутся.

## Итоговый автоматический прогон

- Clean source: `3d759b3f84260652197e8c6aed9b25494b083ad0`.
- Source ZIP: `winbridge-20261011T012033Z-3d759b3-7f793870.zip`; SHA-256
  `beae72a29a48c4cb11d15c9d0fbb12e978cae25226fe7836c3cb05eece525acc`.
- Run `20261010-212047-c983fced`, task `9755a19a-8a79-490b-869d-d9ba739c7641`:
  succeeded / exit 0. Shared: `K:\VMs\shared\WinBridge\workflow-validation\20261010-212047-c983fced`.
- `report.json` SHA-256: `defa5e348fb31f071289e7ae985035720d774751e3af2e71e1b3f47a1f6cd4d9`.
- 42 Windows Rust tests: core 25, platform 7, worker attention 10. Offline worker/desktop
  build; два native UI запуска exit 0; 17 checksum-verified artifacts.
- Desktop log: 86 records, две sessions, popup.dismiss.end ok=true; close.timeout elapsedMs=10009.

Native UI подтвердил undo saved bytes, удержание ACK после настоящей записи формы,
возврат кнопок после timeout, поздний ACK без скрытия, titlebar guard/hide, single-instance
reopen, фактическое исчезновение popup без attention ACK и точную task navigation.
No-focus-steal, aggregation/deferred ACK, pending count 3, сохранённые preferences и
отсутствие replay после restart прошли. Sound API успешен; слышимость человеком не заявляется.
Оба UI quit оставили тестовый worker доступным. Cleanup errors отсутствуют.

## Сохранённые неудачные прогоны

1. `20261010-210454-db98e4e5`, task `a02b610e-1bdb-42f7-bcb7-62aae7145960`:
   tests/build прошли, `save-deferred` не прошёл. Подмена WebView invoke/NumberInput
   не обеспечила потерю ACK; native/UI heartbeats продолжались. Заменено на one-shot
   validation-only barrier после настоящей записи textarea. Normal launch его не включает.
2. `20261010-211212-f58ced85`, task `aac03761-4d41-48bf-85dc-edbf25b91430`:
   undo/timeout/late ACK/hide/reopen прошли, `popup dismissed` не прошёл. Лог показал
   popup.dismiss.begin/end и ui.popup.done при оставшемся видимом окне — воспроизведён
   пользовательский popup bug, исправленный выше.

Оба reports/logs/ready сохранены в своих shared каталогах. Повторы использовали новые run ID
только после terminal result; команды с неизвестным исходом не повторялись.

## Рабочий runtime и локальные проверки

Рабочий worker clean fa7e4d7, generation `372266b6-3dee-4d87-9de2-ccab896dbcf8`,
healthy/idle; session `6011e826-d858-4f9a-97ee-2d7b694586b6` connected. Его exe/config/journal
не заменялись; остановлены только тестовые процессы.

В Debian прошли 157 workspace tests, real Unix worker UI integration с save/native deadlines,
workspace/desktop Clippy, desktop check, Windows platform all-targets cross-check, fmt,
TypeScript/build, PowerShell parser и links/scaffold. Последующий receipt commit меняет docs.
Test windows закрыты; рабочий desktop можно открыть/закрыть, worker оставить работать.
Для применения исправлений нужен стандартный ручной update runtime.
