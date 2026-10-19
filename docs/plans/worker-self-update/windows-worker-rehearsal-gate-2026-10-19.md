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


## First compatibility retry: correct the test boundary

Clean 29b4401, run `20261019-112352-a49287c3`, Task
`75f1319b-556b-48c5-9b28-a92814d31c95` failed in the second group.
The new Windows-only test called DeleteFileW before entering path validation;
Rust then treated the optional directory entry as absent and validation correctly
returned Ok. The assertion expecting PermissionDenied was wrong. This run did
not demonstrate failure of the new production retry logic or of compatibility.

All five ready files verified. Report SHA
`6a1c45b59301dd882acb14f85d713acfafcac9fff9d81eaf57f861fc24ade1fe`.
Windows free 21,412,536,320 → 21,492,977,664 bytes. Installed worker remains
connected/idle0, generation unchanged, no recovery/maintenance fault.

The corrected test reads metadata first, calls real DeleteFileW while holding a
share-delete handle, then attempts the guard's real hard-link handle open. It
requires PermissionDenied from that open, closes the last handle and requires
the subsequent complete validation to succeed with the file absent. This matches
the actual metadata/open race; it neither accepts arbitrary outcomes nor changes
production validation. Windows all-target Clippy passed; automatic native repeat
remains required. Working runtime and validation reports preserved, no UI.


## Corrected boundary: host reports NotFound

Clean 0327204, run `20261019-112845-985c1652`, Task
`10c55041-01fc-41b1-9426-30449c5d3d41` terminal failed1 in updater tests.
The metadata/open race was reached, but the host's open returned NotFound rather
than PermissionDenied. Five ready files verified; report SHA
`1b2df0adc2441e04caf293795d2f6ec43055ee622035922641fc8b940476fc62`.
Windows free 21,431,177,216 → 21,434,384,384 bytes. Installed worker healthy/idle0.

The test now permits only the two removal outcomes already handled by the guard:
NotFound or PermissionDenied. It still requires a failed actual open, exactly one
subsequent full validation and an absent file after closing the retained handle.
The observed native kind/raw code is logged. This host's removal test must not be
reported as reproducing Access denied: that branch has separate controlled fault
coverage, including persistent denial and unsafe replacement. Production policy
is unchanged. Windows all-target Clippy passed; combined repeat is still needed.


## Portable boundary: all native tests passed; missing helper import

Source 7ed3900, run `20261019-113708-5fe0af67`, Task
`96f5965b-5a8e-49ba-ac90-3f9d3d84c01c`: all **206 tests / 19 groups passed**.
Real removal test logged NotFound / OS Some(2). Authenticated v3 compatibility,
post-hash config rejection and original coordinated abort all passed.
The combined gate still failed before candidate build because the new early
Assert-WinBridgeUpdaterSchema call preceded the helper's later dot-source.
The helper is now imported at script startup. No runtime behavior is changed.

36 ready files checked, report SHA
`3a26c51feda0d16d8fc38989d24330a92bd5827350955b54c009f1cbff550fa8`.
Windows free 21,421,084,672 → 21,586,993,152 bytes. Installed worker healthy/idle0,
generation unchanged. Full candidate/config/rehearsal/Main/pins still require the
corrected script; do not mark the complete gate passed from these tests alone.
