# Снимки перед переносом

Созданы штатной командой codex-backup и проверены cmp до изменений. Backups не редактировать. Команды ниже восстанавливают исходные файлы Netwatch до переноса; они не отменяют commits и не удаляют WinBridge.

## README.md

Исходный путь: `/home/user1/netwatch/README.md`.

Backup: `/home/user1/backup/codex/snapshots/20261005T000542.921614459Z-12943/home/user1/netwatch/README.md`.

Точное восстановление:

```bash
cp -a -- '/home/user1/backup/codex/snapshots/20261005T000542.921614459Z-12943/home/user1/netwatch/README.md' '/home/user1/netwatch/README.md'
```

## code-review-2026-10-04.md

Исходный путь: `/home/user1/netwatch/docs/code-review-2026-10-04.md`.

Backup: `/home/user1/backup/codex/snapshots/20261005T000543.301282719Z-12959/home/user1/netwatch/docs/code-review-2026-10-04.md`.

Точное восстановление:

```bash
cp -a -- '/home/user1/backup/codex/snapshots/20261005T000543.301282719Z-12959/home/user1/netwatch/docs/code-review-2026-10-04.md' '/home/user1/netwatch/docs/code-review-2026-10-04.md'
```

## windows-bridge-proposal.md

Исходный путь: `/home/user1/netwatch/docs/windows-bridge-proposal.md`.

Backup: `/home/user1/backup/codex/snapshots/20261005T000543.689026674Z-12975/home/user1/netwatch/docs/windows-bridge-proposal.md`.

Точное восстановление:

```bash
cp -a -- '/home/user1/backup/codex/snapshots/20261005T000543.689026674Z-12975/home/user1/netwatch/docs/windows-bridge-proposal.md' '/home/user1/netwatch/docs/windows-bridge-proposal.md'
```
