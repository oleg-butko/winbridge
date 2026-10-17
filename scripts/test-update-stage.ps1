param([string]$ScratchRoot = '')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/update-stage.ps1')
if (-not $ScratchRoot) { $ScratchRoot = Join-Path ([IO.Path]::GetTempPath()) ('winbridge-update-stage-'+[Guid]::NewGuid().ToString('N')) }
if (Test-Path -LiteralPath $ScratchRoot) { throw 'Scratch root already exists; preserved' }
[IO.Directory]::CreateDirectory($ScratchRoot) | Out-Null
function Assert-True([bool]$Value,[string]$Message) { if (-not $Value) { throw $Message } }
function Assert-Rejected([scriptblock]$Operation,[string]$Message) {
  $rejected = $false
  try { & $Operation | Out-Null } catch { $rejected = $true }
  Assert-True $rejected $Message
}
$build = Join-Path $ScratchRoot 'build'
$runtime = Join-Path $ScratchRoot 'runtime'
[IO.Directory]::CreateDirectory($build) | Out-Null
[IO.Directory]::CreateDirectory($runtime) | Out-Null
$sources = [ordered]@{}
foreach ($name in @('winbridge-worker.exe','winbridge-fixture.exe','winbridge-desktop.exe','winbridge-update.exe')) {
  [IO.File]::WriteAllText((Join-Path $runtime $name),"old runtime $name")
  $path = Join-Path $build $name
  [IO.File]::WriteAllText($path,"new candidate $name")
  $sources[$name] = $path
}
$before = @{}
foreach ($name in $sources.Keys) { $before[$name] = (Get-FileHash -LiteralPath (Join-Path $runtime $name)).Hash }
$payload = Join-Path $ScratchRoot 'payload'
$result = @(New-UpdatePayload $sources $payload)
Assert-True ($result.Count -eq 4) 'Candidate incomplete'
foreach ($entry in $result) {
  Assert-True ((Get-FileHash -LiteralPath (Join-Path $payload $entry.name)).Hash.ToLowerInvariant() -ceq $entry.sha256) 'Payload hash mismatch'
}
Assert-Rejected { New-UpdatePayload $sources $runtime } 'Existing runtime destination accepted'
Assert-Rejected { New-UpdatePayload $sources $payload } 'Existing candidate overwritten'
$missingStage = Join-Path $ScratchRoot 'missing-artifact'
$sources['winbridge-desktop.exe'] = Join-Path $build 'absent.exe'
Assert-Rejected { New-UpdatePayload $sources $missingStage } 'Missing artifact accepted'
Assert-True (-not (Test-Path -LiteralPath $missingStage)) 'Partial candidate created before artifact preflight'
foreach ($name in $sources.Keys) { Assert-True ((Get-FileHash -LiteralPath (Join-Path $runtime $name)).Hash -ceq $before[$name]) 'Installed runtime changed' }
Write-Output 'Update staging checks passed: complete payload, hashes, create-only paths, missing artifact, unchanged runtime'
