# Windows gate: runtime preimages actor / cooperative cancellation

2026-10-19, clean source `2cdc850598837b18600f6e5da33c982d87b4f9ae`.
Run `20261019-072235-273b010c`, Task `567fe768-5bf1-4005-9439-68a2eb1263e1`:
**succeeded / exit 0, 193 native tests / 19 groups**, full Rust/desktop build.
Updater schema 9, supervisor v8 / view 3; candidate worker schema 15.

Archive `winbridge-20261019T112211Z-2cdc850-0148b636.zip`, SHA
`3d2e433ac2b345c969f0e7f0ec334dfe34347afe09a00d1f5b98095140a5ec7a`.
All 43 files from report.ready.json checked for exact size/SHA. Report SHA
`8e9f8da9e3f5c1dcc2d87936ec06d800483efe70fe3691da902971094bb1dcbc`.
Windows free before/after 20,906,876,928 / 20,616,052,736 bytes.
Original cache owner `20261017-061527-f4639971` reused; tests ran anew.
No Windows validation directory was deleted.

Actual copied worker test process confirmed: intent persisted before ACK;
status and duplicate reads remain responsive during held runtime observation;
source work and Stop defer while copier owns its callback. Five independent
files match the exact baseline. A second operation accepts durable abort while
the callback is held; abort completes only after callback quiescence, without
late directory creation or a verified preimages receipt. Both operations retain
the same test worker PID; source remains unchanged, no lease/slot remains.
Other-role descriptors in this fixture are synthetic: this is actor/copy
coordination evidence, not candidate application health.

Previous schema-8 updater refusal, all four exe identity queries/exact payload
files and actual worker Main startup guards passed. Separate actual Main serve
retained five runtime pins, released them after forced owned idle stop and
preserved bytes/updater DB (PID 14180, elapsed 10,307 ms). This forced stop does
not prove graceful production shutdown or activation.

Installed worker remains source `1f3aa9f`, build `1792228466051801600`, generation
`994671b4-e10c-469c-8515-b2aa71a4e3dc`: fresh Info connected, idle0, maintenance and
recovery errors null. No UI windows; desktop unnecessary, leave worker running.
Next: candidate fresh-state health/rehearsal, durable health receipt/approval,
exact shutdown and install. U1 and public auto-update remain open.
