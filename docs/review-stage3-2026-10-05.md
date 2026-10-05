# Ревью snapshot runtime, 2026-10-05

Проверены source capture, неполная публикация, ZIP/path policy, local filesystem границы, command/import lease, durable acceptance/recovery и native UI read model. Исправлены существенные случаи:

- Actual HGFS rename непустого staging directory возвращал ENOENT. Shared publisher теперь использует individual .part files, readback SHA-256 и ready last; original ready package не перезаписывается.
- Проверка только source hashes по изменяемому локальному manifest не связывала принятую команду с исходным snapshot. TaskRecord сохраняет accepted commit/manifest hash в том же acceptance transaction; перед launch сравнивает его и actual дерево. Тест заменяет marker/manifest вместе с файлами после acceptance и подтверждает failed без PID.
- Дополнительный файл в generation мог влиять на build. Verification проверяет exact source tree, metadata exception ограничен двумя файлами .winbridge; build cache/output находятся отдельно. Unbound command cwd в snapshots/import staging отклоняется, Windows scope comparison учитывает case.
- Git working-tree/replacement objects, archive extra/link entries, root-prefix/traversal/collision/device paths и symlink/junction/hardlink destinations исключены. ZIP central directory allocation ограничена до parser, sizes/hashes проверяются по actual bytes. Windows NTFS hard-link count читается через file handle.
- Журнал schema 1 мигрирует только после consistent SQLite VACUUM INTO backup. Import task/result не маскируется под executable command. Lease и durable progress принадлежат worker, не desktop.
- UI отражает confirmed import result и cancel checkpoint, сохраняет прежний read model при offline и позволяет повторить потерянный ACK с тем же payload/key. Preview не обозначается как verified payload.

44 Rust tests, native-source Unix build, 16 actual IPC/WebSocket/HGFS gate checks и actual owned worker kill during import подтверждены. После restart тот же import interrupted, неполный source не buildable, build не повторён. UI integration, 14 прежних session groups, contracts/TypeScript/UI build, Clippy/fmt, Windows compile и Linux desktop check проходят.

Оставшаяся проверка — [native Windows import/build/NTFS junction и UI gate](windows-snapshot-validation.md). Direct commands доверены зарегистрированному project/user; cwd и path guards не объявляются OS sandbox. Async filesystem cancellation достигается на checkpoints, не подменяется мгновенным succeeded/cancelled ACK. Dirty source, retention/resume, incremental sync и benchmarks не включены в реализацию.
