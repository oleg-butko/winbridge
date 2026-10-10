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
  Native результат на момент подготовки коммита ещё не получен.

Этапы 0–7 остаются завершёнными. Продолжение этой проверки — автоматический Windows
gate с exact archive SHA-256 и сохранённым request key; исходы мутаций не повторять
автоматически при неизвестном результате. Новое продуктовое расширение выбирается
отдельно: пользователю предложены dirty snapshots, следующий реальный проект или
просмотр/управление source cache. Runtime dirty snapshot в этой правке не добавлен.

Рабочие configs/журналы/runtime и original Netwatch не менялись; important user-dirty
files отсутствовали, backups не требовались. После прогона указать, что desktop
можно открыть/закрыть и какой worker оставить работающим.
