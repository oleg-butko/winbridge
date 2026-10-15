# A3.2c — actual installed pair и readonly presentation

2026-10-15. User installed `35b716a` from the published ZIP. Actual Windows
build `1792051201472964300`, manifest SHA
`a875e2627cd66a323c3e275131cf53fc8b40addb48c212b7b15aff0e00333b5c`, generation
`c4b847e1-b167-42d9-b3f5-d1faf5f94933`; journal scope preserved.
Controller actual tested `d817fe0`, build `1792021098069838891`.

Actual MCP producer gate `paired-20261015-040814-0c306612` passed:
provider task `6fed9b46-760c-4532-966c-734a3e488fbd`, pair task
`7c57b5be-9dfa-46e9-a035-0a6d35938ceb`, both succeeded/0.
Receipt `2179627b79e2974c43ca2d8d0e8f30e0c97d3834db69c0e85ab9e209040f24a1`.
The actual installed binary was tested in isolated state, then the real route:
duplicate key, output/exit, stale scope and original identity checks passed.

Local named-pipe gate `projection-20261015-041334-3fd56cd4`, task
`726dd188-9984-4014-a577-e9c42554b157`, PID 15608, succeeded/0.
Controller restart removed the worker's cached projection; the probe remained
alive and observed a new session with the same receipt and four true facts.
Worker build/generation/journal unchanged. Report SHA
`4080c14e4ad23ae7349caf9ed532d73289bee1ecd2a9f10bde7c4d8fd83fccf9`.

The actual ReadinessPanel rendered current Windows metadata in a sandboxed Debian
browser: all four facts, exact receipt, saved Task ID navigation, stale local
connection disables live readiness. This is a renderer test with actual Windows
data, not a claim of native window automation. Native shell had already passed
the [41-test candidate/desktop compile gate](windows-projection-gate-2026-10-14.md);
its window-management code did not change. No manual checklist or Windows UI
windows were required. Evidence lives under the two local gate directories in
`.winbridge/architecture-transition/`; Windows report in shared workflow-validation.

Post-gate review fixed arbitrary first-128 directory sampling: the disposable
index selects the newest ten admitted plans over the whole history, is rebuilt
after restart/admission and reads current terminal/status records each time.
Regression covers 140 admissions. Worker/protocol unchanged.

**A3.2 complete; full A3 remains open for A3.3 consistent operation pages.**
