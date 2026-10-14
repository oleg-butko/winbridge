# A3.2c — Windows candidate gate

2026-10-14. Code `7e4bb74b22cda0c0d3995961e2926d25bdec7ea2`, clean source ZIP
`winbridge-20261014T233029Z-7e4bb74-e2f8710c.zip`, SHA-256
`0d1f3285e2ec6e6baede7e491bb0b687aeae90d055b7f8f9b93a937bd6af0f79`.

Run `20261014-193052-b3bb11cf`, task `07d199a6-de90-4b8f-a9fb-e3c564387340`,
succeeded/exit 0. Shared report
`K:\VMs\shared\WinBridge\workflow-validation\20261014-193052-b3bb11cf`.
Report SHA-256 `f3790e831f5f8990ad6b414e93b5396fefbb56a5258861e3494c315059813cdd`;
семь artifacts verified bytes/SHA-256.

41 tests: five pure readiness/policy, seventeen protocol/legacy/projection,
one native cache test и eighteen worker Info/scope/maintenance tests.
Desktop compiled on actual Windows. Candidate build `1792020710405165700`,
manifest SHA `78927319007fca4d6f68775d0928e1097486631b161ea017048822a1a4527630`.
RPC-2 schema SHA-256 совпал с Debian и frozen baseline:
`dd86d35b8b9ed59aca0bb438491e906b9c25c69970f0121226fdee99b98e9a12`.

Compiler cache owner `20261014-054619-d5d99c71` reused; new source/state/report.
Windows free before `23346552832`, after `23441309696` bytes (~21.8 GiB).
Debian ~31 GiB, CARGO_INCREMENTAL=0. Working runtime/config/journal не заменялись.
UI окон и outbound соединения candidate worker не было.

Debian verifier initially expected four groups instead of the new five. The
Windows run had already passed. Fixed group selection is pinned by archive
content; `--verify-readiness-run` rechecked original accepted submission,
terminal Task ID, all hashes/groups, actual native policy/schema and health,
without submit/rebuild or replacing the original report. Read-only observation
is saved separately. No Windows repeat or manual checklist was needed.

Installed worker remains clean `b2aeb9f`, build `1792012729498364300`, generation
`3b5139d6-7d98-4abf-8bca-481512a9abbb`, healthy/idle, but it advertises only
worker RPC 1/2. Projection 1 needs the next runtime update. **Actual installed
new/new projection and full A3.2/A3 remain pending.** Candidate evidence is not
substituted for the actual user-built executable or its paired receipt.
