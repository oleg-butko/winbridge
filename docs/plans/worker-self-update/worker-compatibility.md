# Live-worker compatibility and readonly candidate config check

2026-10-19. Implemented; native gate pending. 530 workspace tests including
doctests, Linux/Windows Clippy, generated contracts and UI build/checks passed. Updater/worker schemas
remain 9/15. No durable health receipt, candidate approval or install is enabled.

Worker control v3 adds `ObserveCompatibility`, separately from v1 leases and v2
runtime identity. Supervisor v9 adds `ObserveWorkerCompatibility`; older actions,
reply versions and view schemas are unchanged. No v1 forwarding or fallback may
turn a compatibility observation into a lease, identity or installation grant.

The exact registered worker/supervisor PID and birth, installation scope,
owner/epoch, journal incarnation, generation and frozen config SHA are verified
before and after runtime hashing. No Store mutex is retained during hashes.
The response includes the loaded pinned worker identity, actual worker SQLite
schema (checked against its compiled schema), readonly updater schema, config
version/SHA and protocol version. Metadata comes from the running worker, not a
requester's expected numbers. Terminal operations, recovery, pending config restart,
unattached runtime guards and stale process/context/owner bindings refuse it.
V6 and v9 share one bounded query slot; the mailbox remains responsive and retires
the slot before publishing the response.

The separate `WorkerConfigCheck.contract-1` image marker enables
`--check-update-config ABSOLUTE_CONFIG SHA256` in candidate Main. It reads bounded
exact bytes, parses the full config with candidate policy and rechecks File ID/SHA.
It never constructs Worker or opens its journal; no config values are serialized
into the result or copied to another file. Errors do not echo config contents.
`winbridge-update check-worker-config IMAGE IMAGE_SHA CONFIG CONFIG_SHA` holds
both read pins, authenticates the candidate descriptor/process, checks the typed
result and confirms bounded capture/owned-tree cleanup. Unsupported images/wrong
SHA fail before launching the check. The first helper query is ArtifactQuery v1;
its frozen JSON shape and legacy worker RPC are unchanged.

Tests: unknown/mixed response kinds and versions; wrong context/generation/build/
PID/config/schema; permission-field injection; candidate accepts valid config with
a deliberately malformed SQLite canary unchanged; wrong config SHA/schema/extra
arguments and unsupported image reject before journal effects. The positive native
copied-process control fixture is extended with v3 compatibility and config-change
rejection after hashing. That fixture uses synthetic other-role descriptors and
is not candidate health evidence. A second script checks the actual full candidate
against a synthetic config and malformed journal; it does not copy production data.

Next: a durable one-use health launch intent binding candidate artifact receipt,
source lease, updater owner/epoch and authenticated current compatibility. Candidate
config check and fresh-state rehearsal must be launched under that intent, with
saved process identities, cleanup receipts and no pending-intent replay. Only a
fresh verified result may advance health/approval; these standalone diagnostics
cannot be imported as authority. Then complete drain/exact shutdown/install and UI.
