# Candidate worker: fresh-state rehearsal

2026-10-19. Implemented standalone diagnostic; native gate pending.
This is the first health block, not a durable health receipt or installation approval.
Updater/worker journal schemas remain 9/15. No installed runtime is changed.

`winbridge-update rehearse-worker IMAGE SHA256 NEW_ABSOLUTE_DIRECTORY WORKER_SCHEMA UPDATER_SCHEMA`
pins the exact candidate image, detects the separate
`WinBridge.WorkerRehearsal.contract-1` marker before execution, queries the frozen
ArtifactQuery v1 descriptor and runs `--rehearsal` in an owned managed process tree.
The marker is capability detection, not a signature or source trust. The SHA must
come from the verified candidate artifact set; standalone callers still own that
policy. Unsupported older images and hash mismatches are rejected before launch.
Normal ArtifactQuery v1/legacy RPC have no new fields or changed JSON shape.

The candidate accepts only a new local absolute directory, not a config path.
It creates an independent empty journal and project with unique instance ID;
controller/delivery/recipes/direct commands are disabled. Existing directories,
even empty ones, are refused. Neither production config nor journal is copied.
It initializes the real worker, starts the normal local IPC server/scheduler,
observes Info over IPC and checks empty/healthy state, then requests normal stop
of this fresh instance and awaits server/control teardown. The outer updater
confirms process exit, empty owned tree, drained bounded stdout/stderr and unchanged
image identity. All partial scratch files remain available for diagnosis.

The typed report binds the compiled worker descriptor, exact process PID/birth,
new directory/File ID, instance/generation/journal incarnation, protocol/config
versions and supported worker/updater journal schemas. `sameSchema` means equality
with the explicitly supplied expectations. It does not prove the current live
production schema or compatibility with a particular user's full configuration.
`journalBound`, `candidateVerified`, `runtimeApplied` remain false.

Never start recovery/scheduling over a copied live worker journal: its stored
process/job identities could refer to real production tasks. Future migration
rehearsal must have a separate effect-free contract; this smoke does not authorize
that. Existing normal Main known-runtime/pin tests remain separate evidence:
rehearsal does not pretend to validate production startup/activation.

522 workspace tests, Linux/Windows Clippy all-targets, contracts/fmt and runner
checks passed. Four local actual-process tests cover fresh IPC readiness/normal server stop/owned tree
cleanup, untouched unrelated canary, existing journal refusal, extra production
config arguments, unsupported image/wrong SHA and exact-schema mismatch. Cargo
outputs are copied before testing because their hard links are deliberately
rejected by the artifact reader. The Windows pipeline runs those tests and the
same diagnostic against the complete actual candidate build, verifying a new
`worker-rehearsal-proof.json` in both PowerShell and Debian report readers.

Next: bind a one-use rehearsal launch intent and result to updater owner/epoch,
current artifact receipt/source lease and authenticated current-worker schemas;
expose consistent status/abort before accepting a durable health receipt. No
filesystem/process effect may be replayed from a pending intent after a crash.
Then complete approval/drain/exact shutdown/install. U1/U2/U3 remain open.
