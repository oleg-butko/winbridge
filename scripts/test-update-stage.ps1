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
# Cargo's final exe may alias its deps cache. Staging must copy bytes into a
# separate file, so later cache writes cannot modify the verified payload.
$cacheAlias = Join-Path $build 'worker-cache-alias.exe'
New-Item -ItemType HardLink -Path $cacheAlias -Target $sources['winbridge-worker.exe'] | Out-Null
$payload = Join-Path $ScratchRoot 'payload'
$result = @(New-UpdatePayload $sources $payload)
Assert-True ($result.Count -eq 4) 'Candidate incomplete'
foreach ($entry in $result) {
  Assert-True ((Get-FileHash -LiteralPath (Join-Path $payload $entry.name)).Hash.ToLowerInvariant() -ceq $entry.sha256) 'Payload hash mismatch'
}
$copiedWorkerHash = (Get-FileHash -LiteralPath (Join-Path $payload 'winbridge-worker.exe')).Hash
[IO.File]::WriteAllText($cacheAlias,'changed Cargo cache after staging')
Assert-True ((Get-FileHash -LiteralPath (Join-Path $payload 'winbridge-worker.exe')).Hash -ceq $copiedWorkerHash) 'Staged exe retained a cache hard link'
Assert-True ((Get-FileHash -LiteralPath $sources['winbridge-worker.exe']).Hash -cne $copiedWorkerHash) 'Hard-link regression fixture did not change its source alias'
Assert-Rejected { New-UpdatePayload $sources $runtime } 'Existing runtime destination accepted'
Assert-Rejected { New-UpdatePayload $sources $payload } 'Existing candidate overwritten'
$missingStage = Join-Path $ScratchRoot 'missing-artifact'
$sources['winbridge-desktop.exe'] = Join-Path $build 'absent.exe'
Assert-Rejected { New-UpdatePayload $sources $missingStage } 'Missing artifact accepted'
Assert-True (-not (Test-Path -LiteralPath $missingStage)) 'Partial candidate created before artifact preflight'
foreach ($name in $sources.Keys) { Assert-True ((Get-FileHash -LiteralPath (Join-Path $runtime $name)).Hash -ceq $before[$name]) 'Installed runtime changed' }
$sourceRoot = Join-Path $ScratchRoot 'source'
$generatedRoot = Join-Path $sourceRoot 'apps/desktop/src-tauri/gen'
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($generatedRoot)) | Out-Null
$outside = Join-Path $ScratchRoot 'outside-cache'
[IO.Directory]::CreateDirectory($outside) | Out-Null
$linkType = if ($env:OS -eq 'Windows_NT') { 'Junction' } else { 'SymbolicLink' }
New-Item -ItemType $linkType -Path $generatedRoot -Target $outside | Out-Null
Assert-Rejected { Assert-UpdateBuildPaths -Source $sourceRoot -WorkspaceTarget (Join-Path $sourceRoot 'target') -DesktopTarget (Join-Path $sourceRoot 'apps/desktop/src-tauri/target') } 'Generated build output follows a junction/symlink'
Assert-True (@(Get-ChildItem -LiteralPath $outside).Count -eq 0) 'Build preflight wrote outside source'
Write-Output 'Update staging checks passed: complete independent payload, Cargo hard-link isolation, hashes, create-only paths, missing artifact, unchanged runtime, generated-output link guard'
