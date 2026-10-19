# Windows candidate rehearsal gate: initial run and sidecar fix

2026-10-19. Clean `cd85cf9b653356196c83879d0d3b141c162f4c49`, run
`20261019-074259-5c52f602`, Task `87eac941-f37c-4fbe-aa9c-b84846cb84e0`:
**failed / exit 1**. 196 tests passed; the final runtime-control fixture failed.
The four new real candidate fresh-state rehearsal tests passed on Windows, but
complete candidate build / packaged smoke were not reached. Do not call the
combined health gate passed.

Archive `winbridge-20261019T114251Z-cd85cf9-0ef995cf.zip`, SHA
`55c01a95938060c28033451dd8a32774e600d5949d228481741a12b4b7c6cfbc`.
35 ready files checked for exact size/SHA; report SHA
`117e085550c5888298cf4dd96e9cfa7db37fea146c3d0f5e6fb5330dd729bf66`.
Windows free before/after 20,483,092,480 / 20,370,821,120 bytes. Original cache
owner reused; validation directories preserved.

The unchanged cancellation helper failed at `cancel.await`: path validation of
its own `runtime/update-state/updates.sqlite-wal` returned Access denied (os error 5).
It was not the outer 180 s process deadline: helper failed after 64.42 s and parent
after 67.31 s. No new process-health/candidate activation failure was demonstrated.

Review found that metadata errors lost their io::ErrorKind through contextual
wrapping. Windows delete-pending sidecars can also reject opens with Access denied;
that is a plausible explanation, not a proven trace of the original filesystem
operation. Microsoft documents the pending-delete/open-denial behavior in
[DeleteFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-deletefilew). The fix preserves the I/O kind and retries only full readonly validation
of the two exact optional WAL/SHM names, at most three attempts / two 10 ms waits.
NotFound retains its single full-validation retry. Main DB, owner lock and rollback
journal denials do not retry. Persistent errors and unsafe hardlink/symlink/reparse
replacements fail closed. No database transaction or filesystem effect is retried.
Seven local path tests and 530 workspace tests, Linux/Windows Clippy passed;
native retry also includes an actual DeleteFileW
pending-delete test. Combined repeat remains required.

Fresh installed-worker Info after failure: source 1f3aa9f/build 1792228466051801600,
generation `994671b4-e10c-469c-8515-b2aa71a4e3dc`, connected/idle0, no maintenance or
recovery error. No production runtime replaced; no UI windows. Leave worker running,
desktop unnecessary. The authorized automatic correction/retry needs no manual update.
