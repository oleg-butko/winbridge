# Physical process custody отделена от verification pins

2026-10-29. Source шаг после [Main28](main28-native-2026-10-29.md) и
[ревью finalization](keeper-finalization-review-2026-10-29.md). Это структурное
разделение original owner, **не** готовая permanent keeper conversion.

`runtime_replacement/activation_process_custody.rs` содержит private non-clone/
non-Serde `ProcessCustody`: тот же ManagedChild/Job, original channel, stdout/
stderr captures и reader JoinHandles. File/config/installed-set pins, activation
records, old released state и updater authority в этот объект не входят.

`ActivationProcessGuard` держит physical owner первым полем, отдельно от прежних
verification resources. Все launch/handoff/writer/service/controller/lifetime
операции обращаются к тем же handles; новый процесс/Store/lock не создаются.
Public factories, DTO, supported versions, schemas и wire shapes прежние:
worker19/updater22/private6/actor27/view19, public install off.

## Cleanup и ограничения

Kill-on-close переехал из outer Drop в `ProcessCustody::drop`, вызывающий тот же
`ManagedChild::close_unconfirmed`. Rust declaration-order drop сначала запрашивает
cleanup original child/Job, затем освобождает outer verification pins. Это
сохраняет прежний nonblocking cleanup и не обещает positive Job.empty. Ошибка
и unwind по-прежнему владеют тем же boxed guard. `mem::forget`, клонирования Job
handles или изменения kernel limits не добавлено.

Outer guard больше не имеет собственного Drop; будущая trusted consuming factory
сможет переместить physical owner целиком без unsafe field extraction. **Сейчас
такой factory нет**, pins не отпускаются на основании JSON/receipt, private channel
не retire-ится, root commit/release не вызываются. Доступ к physical fields остаётся
внутри native owner module; внешнего Rust/JSON конструктора handles нет.

Output drain перенесён без изменения алгоритма: сохраняются до64KiB каждого pipe,
остальное продолжают читать, read failure отмечается incomplete. Slow diagnostics
не удерживают pipe read до обработки UI. Readers/captures перемещаются вместе
с владельцем, а не заменяются новыми потоками или metadata.

## Проверки

Три новых portable tests исполняют тот же output source: два reader сначала
блокируются, owner перемещается, затем оба полностью прочитывают по1MiB при
64KiB capture limit; IO error сохраняет уже полученный prefix с incomplete;
чистый EOF сохраняет Unicode bytes без ложного truncation. Это IO/resource tests,
не Windows authentication/Job handoff proof.

Full workspace/update-validation:972 passed/17 ignored/126 groups. Linux и Windows
MSVC workspace/all-target Clippy с update-validation/`-D warnings`, fmt/diff и
scaffold прошли. Protocol/UI не менялись. Source logs:
`.winbridge/review-2026-10-29/process-custody-*`;
summary `process-custody-workspace-summary.json`.
Перед full build Debian10,214,027,264/shared21,680,336,896 bytes; final
Debian10,212,265,984/shared21,493,178,368 bytes. Cleanup и builds below6GiB не было.

**Нового Windows execution gate на этот refactor не запускали.** Main28 относится
к producer0426a13 до разделения. Следующий full original-owner/finalization gate
должен проверить новую структуру вместе с реальной consuming conversion; нельзя
выдавать старый Main28 за это доказательство. Installed worker/production
controller не менялись; desktop не нужен, working worker оставить.

## Следующее

Typed resident control с actual OS peer, current link/projection proof и durable
finalization intent; старый V6 stream с unread body не использовать повторно.
Затем trusted root decision/scoped release/Active на том же Worker/Store,
consuming physical role с отпусканием pins и всех updater-authority references,
positive native evidence и следующий update/max2 keepers/two generations/U2–U4.
Детальный порядок и failure boundaries — в linked review. Cold records не создают
resident owner, новая control операция не продлевает original startup window.
