# Owned probe lifecycle hooks

2026-10-19. Internal foundation for the durable health driver; no health receipt,
CandidateAccepted or installation route is enabled. Journal schemas remain 9/15.

All existing artifact/config/rehearsal diagnostics use the same bounded process
runner. A private ProbeObserver now supplies mandatory hooks before creation,
before resume (exact PID/Job available), after the resume call, during execution
and after cleanup. Implementations cannot accidentally inherit success defaults.
The explicitly named UnjournaledProbe preserves standalone diagnostic behavior;
its JSON remains non-authoritative. Hooks themselves do not persist a journal.

Before-create rejection reports known absence of a created process. A platform
creation error with unknown outcome is distinct and cannot claim an empty tree.
Windows Job/PID/birth/session/host evidence is checked before the before-resume
hook. Only a successful hook permits the OS resume call. The timeout starts after
that hook, immediately before resume; metadata work is not charged as child runtime.
A failed resume is recorded as requested, not falsely described as never attempted.

Observer/cancellation/timeout errors terminate only the owned probe tree. The
runner observes exact exit/tree emptiness, drains bounded independent stdout and
stderr readers and invokes the cleanup hook on success and failures. A missing
capture/physical proof stays unconfirmed. PID receipt failure does not resume on
Windows, but still attempts cleanup. A failed final callback does not authorize
replay or approval. A future journal writer must retain that pending attempt.

533 workspace tests, Linux/Windows all-target Clippy, contracts/fmt and runner
checks passed. Three actual copied-process tests pass locally: denied creation, changed-owner
poll cancelling the private long process, and normal success with exact process/
exit/capture cleanup evidence. A fourth native Windows test rejects the PID receipt
and requires both no canary execution and verified cleanup. The owned test helper
is never a normal test command. New native verifiers require all four named tests.

Next: implement the health journal/actor vertical scenario. Persist a one-use
intent bound to source/artifact/current-worker metadata before ACK, save each
probe PID/Job and resume authorization atomically under owner/epoch before resume,
then verify candidate reports and atomically accept the candidate. Run only a
fresh isolated worker journal. Abort/release must prove health process/capture
quiescence as well as compiler cleanup; a pending attempt never creates a replay
permit. Reuse the retained artifact-set descriptor instead of extra query children.
