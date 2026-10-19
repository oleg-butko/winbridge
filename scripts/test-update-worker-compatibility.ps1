param(
  [Parameter(Mandatory)][string]$CandidateDirectory,
  [Parameter(Mandatory)][string]$ScratchDirectory,
  [Parameter(Mandatory)][string]$ReportPath
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/update-stage.ps1')
foreach ($path in @($CandidateDirectory,$ScratchDirectory)) { Assert-SessionDirectory $path }
if (Test-Path -LiteralPath $ScratchDirectory) { throw 'Config-check scratch already exists; preserved' }
[IO.Directory]::CreateDirectory($ScratchDirectory) | Out-Null
$state = Join-Path $ScratchDirectory 'state'
$project = Join-Path $ScratchDirectory 'project'
[IO.Directory]::CreateDirectory($state) | Out-Null
[IO.Directory]::CreateDirectory($project) | Out-Null
$canary = Join-Path $state 'journal.sqlite'
[IO.File]::WriteAllText($canary,'canary: intentionally not a SQLite database')
$canarySha = (Get-FileHash -LiteralPath $canary).Hash
$config = Join-Path $ScratchDirectory 'worker.json'
Write-NewJson $config @{schemaVersion=1;instanceId='candidate-config-check';nodeId='test';stateDirectory=$state;worker=@{keepRunningOnUiExit=$true};projects=@(@{id='test';workspaceRoot=$project;allowDirectCommands=$false;maxConcurrentMutations=1;recipes=@()})}
$configSha = (Get-FileHash -LiteralPath $config).Hash.ToLowerInvariant()
$candidate = Get-Content -LiteralPath (Join-Path $CandidateDirectory 'candidate-report.json') -Raw | ConvertFrom-Json
$worker = @($candidate.artifactIdentities | Where-Object { $_.name -ceq 'winbridge-worker.exe' })
if ($worker.Count -ne 1) { throw 'Expected one queried candidate worker' }
$updater = Join-Path $CandidateDirectory 'payload/winbridge-update.exe'
$workerPath = Join-Path $CandidateDirectory 'payload/winbridge-worker.exe'
$output = @(& $updater 'check-worker-config' $workerPath $worker[0].query.image.sha256 $config $configSha 2> ($ReportPath+'.stderr.log'))
if ($LASTEXITCODE -ne 0) { throw "Candidate config check failed; see $ReportPath.stderr.log" }
$inspection = ($output -join "`n") | ConvertFrom-Json
if ($inspection.kind -cne 'worker_config_inspection' -or $inspection.readOnly -ne $true -or $inspection.cleanupVerified -ne $true -or $inspection.journalBound -ne $false -or $inspection.candidateVerified -ne $false -or $inspection.runtimeApplied -ne $false -or $inspection.image.sha256 -cne $worker[0].query.image.sha256 -or $inspection.check.config.sha256 -cne $configSha -or $inspection.check.workerConfigSchema -ne 1 -or ($inspection.check.descriptor | ConvertTo-Json -Depth 15 -Compress) -cne ($worker[0].query.descriptor | ConvertTo-Json -Depth 15 -Compress)) { throw 'Config-check image/process/config evidence incomplete' }
foreach ($name in @('workerJournalOpened','schedulerStarted','candidateVerified','runtimeApplied')) {
  if ($inspection.check.$name -ne $false) { throw "Config check unexpectedly enabled $name" }
}
if ((Get-FileHash -LiteralPath $config).Hash.ToLowerInvariant() -cne $configSha -or (Get-FileHash -LiteralPath $canary).Hash -cne $canarySha -or @(Get-ChildItem -LiteralPath $state -Force).Count -ne 1) { throw 'Readonly config check modified config/state' }
Write-NewJson $ReportPath @{schemaVersion=1;kind='worker_config_compatibility_proof';passed=$true;platform='windows';malformedJournalUnopened=$true;configUnchanged=$true;productionConfigCopied=$false;runtimeApplied=$false;candidateVerified=$false;windowsOpened=$false;inspection=$inspection}
Write-Output 'Candidate full config accepted without journal/scheduler or config copies; malformed journal canary unchanged'
