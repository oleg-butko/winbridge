param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$CandidateDirectory,
  [Parameter(Mandatory)][string]$ScratchDirectory,
  [Parameter(Mandatory)][string]$ReportPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/update-stage.ps1')
. (Join-Path $PSScriptRoot 'lib/update-schema.ps1')
foreach ($path in @($SourceDirectory,$CandidateDirectory,$ScratchDirectory)) { Assert-SessionDirectory $path }
if (Test-Path -LiteralPath $ScratchDirectory) { throw 'Rehearsal scratch already exists; retained without replay' }
$candidate = Get-Content -LiteralPath (Join-Path $CandidateDirectory 'candidate-report.json') -Raw | ConvertFrom-Json
$worker = @($candidate.artifactIdentities | Where-Object { $_.name -ceq 'winbridge-worker.exe' })
if ($worker.Count -ne 1) { throw 'Expected one actual candidate worker query' }
$workerDefinitions = [regex]::Matches((Get-Content -LiteralPath (Join-Path $SourceDirectory 'apps/worker/src/store.rs') -Raw), '(?m)^pub const JOURNAL_SCHEMA_VERSION: u32 = ([1-9][0-9]*);\r?$')
$updaterDefinitions = [regex]::Matches((Get-Content -LiteralPath (Join-Path $SourceDirectory 'apps/update/src/journal.rs') -Raw), '(?m)^pub const UPDATE_SCHEMA_VERSION: u32 = ([1-9][0-9]*);\r?$')
if ($workerDefinitions.Count -ne 1 -or $updaterDefinitions.Count -ne 1) { throw 'Expected unique exact schema constants in verified source' }
$workerSchema = [uint32]$workerDefinitions[0].Groups[1].Value
$updaterSchema = [uint32]$updaterDefinitions[0].Groups[1].Value
$updater = Join-Path $CandidateDirectory 'payload/winbridge-update.exe'
$workerPath = Join-Path $CandidateDirectory 'payload/winbridge-worker.exe'
$output = @(& $updater 'rehearse-worker' $workerPath $worker[0].query.image.sha256 $ScratchDirectory $workerSchema $updaterSchema 2> ($ReportPath+'.stderr.log'))
if ($LASTEXITCODE -ne 0) { throw "Candidate worker rehearsal failed; see $ReportPath.stderr.log" }
$proof = ($output -join "`n") | ConvertFrom-Json
if ($proof.kind -cne 'worker_rehearsal_inspection' -or $proof.sameSchema -ne $true -or $proof.cleanupVerified -ne $true -or $proof.journalBound -ne $false -or $proof.candidateVerified -ne $false -or $proof.runtimeApplied -ne $false -or $proof.image.sha256 -cne $worker[0].query.image.sha256 -or $proof.rehearsal.workerJournalSchema -ne $workerSchema -or ($proof.rehearsal.descriptor | ConvertTo-Json -Depth 15 -Compress) -cne ($worker[0].query.descriptor | ConvertTo-Json -Depth 15 -Compress)) { throw 'Candidate rehearsal image/schema proof incomplete' }
Assert-WinBridgeUpdaterSchema -SourceDirectory $SourceDirectory -ReportedSchema $proof.rehearsal.updaterJournalSchema
foreach ($name in @('freshJournal','ipcReady','serverStopped')) {
  if ($proof.rehearsal.$name -ne $true) { throw "Candidate rehearsal missing $name" }
}
foreach ($name in @('controllerEnabled','directCommandsEnabled','productionStateOpened','candidateVerified','runtimeApplied')) {
  if ($proof.rehearsal.$name -ne $false) { throw "Candidate rehearsal unexpectedly enabled $name" }
}
Write-NewJson $ReportPath $proof
Write-Output 'Candidate worker rehearsal: fresh journal, IPC ready, normal isolated server shutdown and owned tree cleanup; no production data or install approval'
