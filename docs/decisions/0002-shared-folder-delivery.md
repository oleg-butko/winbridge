# ADR 0002 — доставка через VMware Shared Folders

Accepted: 2026-10-04, прямой ответ пользователя.

Mapping: `/mnt/hgfs/shared` в Debian ↔ `K:\VMs\shared` в Windows. Выбран режим «Общая папка для доставки, отдельная папка Windows для сборки».

Controller публикует immutable snapshot/package и manifest. Worker проверяет и импортирует его в выбранный локальный Windows workspace. Runtime databases и build caches находятся вне shared папки. Control requests/events идут по отдельной сессии.

Это позволяет использовать уже доступный общий каталог и стабильные исходники сборки. Скорость подтверждается benchmark этой VM; нельзя обещать превосходство HGFS по всем workloads. Direct build/edit общей папки можно рассмотреть только как последующее изменение scope.
