# Ревью после закрытия roadmap, 2026-10-10

Проверены последние изменения build identity/source export, shared recipe profiles,
workflow recipe pins и подготовка двух изолированных service projects. Repo был
clean. Controller и установленный Windows worker подключены, ошибок maintenance/
journal нет, activeTaskCount=0. Worker остаётся clean fa7e4d7 с generation
372266b6-3dee-4d87-9de2-ccab896dbcf8; desktop для backend проверки не нужен.

## Исправленная ошибка

`export-source.py` доверял `git status` до чтения файлов. В настоящем тестовом
репозитории `git update-index --assume-unchanged` скрывает изменённый source file:
status пустой, но старый exporter помещает изменённые байты в ZIP с `dirty=false`.
Изменение файла после первого status даёт тот же неверный результат.
Это нарушает связь показанного commit с фактическими исходниками сборки.

Exporter теперь закрепляет commit, читает его полный tree с NUL-separated paths
и сравнивает реально захваченные пути/байты с Git blob IDs этого commit. Поддержаны
Git SHA-1/SHA-256, Unicode/leading spaces/binary files. Index flags не влияют на
сверку; позднее восстановление файла не скрывает уже захваченные изменения.
После capture проверяются status и неизменность HEAD. Generated UI по-прежнему
явно включается отдельным параметром; эта проверка не пересобирает frontend.

Для Git-сборки Rust `sourceDirty=true` при assume-unchanged/skip-worktree entries:
status в этом случае не подтверждает фактические байты. Archive manifest mode
сохраняет прежнюю проверку SHA-256 всех перечисленных inputs.

Также exporter отклоняет parent checkout identity, symlink в любом компоненте
source path, symlinks/submodules в исходном tree и прежний export manifest, который
создал бы duplicate entry/самоссылку. Никакие пользовательские файлы не удаляются.

Новых важных ошибок в проверенных profile/workflow acceptance/recovery paths и
ownership двух прикладных сценариев не обнаружено. Это целевое ревью, а не аудит
всех функций/проектов.

## Проверки и продолжение

- 8 Python regressions с настоящими Git repositories и проверкой ZIP/manifest.
- 153 workspace Rust tests, включая расширенный build-script regression для двух
  index flags, Unicode paths, собственного/чужого checkout.
- Workspace Clippy/all-targets, protocol Windows all-targets cross-check, fmt,
  Python compile, PowerShell parser и Markdown/scaffold проходят.
- Подготовлен узкий `--identity-only` / `-IdentityOnly` через прежний approved native
  harness: только protocol tests на настоящей Windows/Git, без test worker/desktop.
  Native gate пройден: 5 protocol tests, включая actual build-script Git regression.

## Automatic Windows gate

Native-tested commit `ef4dd2621c47b0c137bcc1b46dfba9980d7e39fb`.
Archive `K:\VMs\shared\WinBridge\sources\winbridge-20261010T225241Z-ef4dd26-9cc9a527.zip`:
clean, 265 files, 3 654 320 bytes, SHA-256
`591b18c61016cd850ac635c1605c3699fcbb576cbbb85f6412381ba13bd0551c`.
ZIP и все source manifest files проверены readback, duplicate entries отсутствуют.

Run `20261010-185249-d913cb3f`, outer task
`ca1d5c9c-6ea0-4a80-a41e-c96aeee23f70`, succeeded/exit 0.
Shared report: `K:\VMs\shared\WinBridge\workflow-validation\20261010-185249-d913cb3f`.
Cargo group `source-build-identity-and-protocol` прошла offline на настоящей Windows:
1 build identity regression + 4 wire contract tests. Никакие тестовые worker/desktop
не запускались; проверены checkout/index semantics Windows Git, а не visual UI.

Readback SHA-256:

| Artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| report.json | 802 | 7ada477b60c10f70ddee31997f2d71700c4ef16e2e1998eeefdc22cb4a05d4e6 |
| source-build-identity-and-protocol.log | 2540 | b3e926a8d357bb2f6f2ea467a816b137015181dda3ebad356f452137257c8fd5 |
| report.ready.json | 420 | 23253ea5fe7e1f0c71b05fc463821c3ea3d0e4f2c407840595d75ebb19702d74 |

Installed worker до/после — прежняя generation
`372266b6-3dee-4d87-9de2-ccab896dbcf8`, maintenanceError/journalRecoveryRequired=null,
activeTaskCount=0. Request/task/worker proof сохранены локально в
`.winbridge/workflow-validation/20261010-185249-d913cb3f`; мутации не повторялись.

Этапы 0–7 остаются завершёнными. Автоматический Windows gate с exact archive SHA-256
и сохранённым request key завершён. Новое продуктовое расширение выбирается
отдельно: пользователю предложены dirty snapshots, следующий реальный проект или
просмотр/управление source cache. Runtime dirty snapshot в этой правке не добавлен.

Рабочие configs/журналы/runtime и original Netwatch не менялись; important user-dirty
files отсутствовали, backups не требовались. Пользователю сообщено: desktop можно
открыть/закрыть; установленный рабочий worker оставить работающим. Code archive
готов для обычного ручного обновления; повторного ручного gate не требуется.
