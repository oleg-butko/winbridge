param([string]$ScratchRoot = '')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
if (-not $ScratchRoot) { $ScratchRoot = Join-Path ([IO.Path]::GetTempPath()) ('winbridge-runtime-check-' + [Guid]::NewGuid().ToString('N')) }
if (Test-Path -LiteralPath $ScratchRoot) { throw "Scratch directory already exists; preserved: $ScratchRoot" }
[IO.Directory]::CreateDirectory($ScratchRoot) | Out-Null
function Assert-True([bool]$Value, [string]$Message) { if (-not $Value) { throw $Message } }
function Assert-Rejected([scriptblock]$Operation, [string]$Message) {
  $rejected = $false
  try { & $Operation | Out-Null } catch { $rejected = $true }
  Assert-True $rejected $Message
}
$runtime = Join-Path $ScratchRoot 'fixed-runtime'
$first = Get-SessionLayout (Join-Path $ScratchRoot 'source-one') $runtime 'first'
$second = Get-SessionLayout (Join-Path $ScratchRoot 'source-two') $runtime 'second'
foreach ($field in @('runtimeDirectory','projectDirectory','stateDirectory','workerConfigPath','desktopConfigPath')) {
  Assert-True ($first[$field] -eq $second[$field]) "Runtime path changed with source/run: $field"
}
Assert-True ($first.runRoot -ne $second.runRoot) 'Report run must remain unique'
Assert-Rejected { Get-SessionLayout $ScratchRoot $runtime '..-bad/escape' } 'Traversal run ID accepted'
$fileParent = Join-Path $ScratchRoot 'not-a-directory'
[IO.File]::WriteAllText($fileParent, 'preserve existing file')
Assert-Rejected { Get-SessionLayout $ScratchRoot (Join-Path $fileParent 'runtime') 'file-parent' } 'File used as runtime parent'
if ($env:OS -ne 'Windows_NT') {
  $dangling = Join-Path $ScratchRoot 'dangling-runtime'
  New-Item -ItemType SymbolicLink -Path $dangling -Target (Join-Path $ScratchRoot 'absent-target') | Out-Null
  Assert-Rejected { Get-SessionLayout $ScratchRoot $dangling 'dangling' } 'Dangling runtime symlink accepted'
}
$settings = @{ instanceId = 'local-development'; nodeId = 'windows-host'; controllerUrl = 'ws://192.168.44.203:1143' }
$created = Initialize-SessionConfiguration $first $settings
Assert-True ($created.stateDirectory -eq $first.stateDirectory) 'State directory differs'
Assert-True ($created.projects[0].workspaceRoot -eq $first.projectDirectory) 'Project directory differs'
$workerHash = (Get-FileHash -LiteralPath $first.workerConfigPath).Hash
$desktopHash = (Get-FileHash -LiteralPath $first.desktopConfigPath).Hash
$draft = Join-Path $runtime 'winbridge-desktop-state.local.json'
[IO.File]::WriteAllText($draft, '{"manualDraft":"Привет ✓"}')
[IO.Directory]::CreateDirectory($first.stateDirectory) | Out-Null
$stateMarker = Join-Path $first.stateDirectory 'preserved-marker.txt'
[IO.File]::WriteAllText($stateMarker, 'existing state')
$otherDefaults = @{ instanceId = 'other'; nodeId = 'different'; controllerUrl = 'ws://127.0.0.1:1143' }
$reused = Initialize-SessionConfiguration $second $otherDefaults
Assert-True ($reused.instanceId -eq 'local-development') 'Existing configuration replaced by creation defaults'
Assert-True ((Get-FileHash -LiteralPath $first.workerConfigPath).Hash -eq $workerHash) 'Worker config modified'
Assert-True ((Get-FileHash -LiteralPath $first.desktopConfigPath).Hash -eq $desktopHash) 'Desktop config modified'
Assert-True ([IO.File]::ReadAllText($stateMarker) -eq 'existing state') 'State marker modified'
Assert-True ([IO.File]::ReadAllText($draft).Contains('Привет ✓')) 'Draft modified'

$source = Join-Path $ScratchRoot 'build'
[IO.Directory]::CreateDirectory($source) | Out-Null
$sources = [ordered]@{}
foreach ($name in @('winbridge-worker.exe','winbridge-fixture.exe','winbridge-desktop.exe')) {
  $path = Join-Path $source $name
  [IO.File]::WriteAllText($path, "fixture bytes $name")
  $sources[$name] = $path
}
$initial = @(Copy-SessionBinaries $sources $runtime (Join-Path $ScratchRoot 'stage-first'))
Assert-True ($initial.Count -eq 3) 'Missing runtime binaries'
$workerExe = Join-Path $runtime 'winbridge-worker.exe'
[IO.File]::SetLastWriteTimeUtc($workerExe, [DateTime]::Parse('2001-01-01T00:00:00Z').ToUniversalTime())
$oldWriteTime = [IO.File]::GetLastWriteTimeUtc($workerExe)
$same = @(Copy-SessionBinaries $sources $runtime (Join-Path $ScratchRoot 'stage-second') @($workerExe))
Assert-True (@($same | Where-Object { $_.updated }).Count -eq 0) 'Identical binaries rewritten'
Assert-True ([IO.File]::GetLastWriteTimeUtc($workerExe) -eq $oldWriteTime) 'Identical active executable touched'
$oldHashes = @{}
foreach ($entry in $initial) { $oldHashes[$entry.path] = (Get-FileHash -LiteralPath $entry.path).Hash }
[IO.File]::WriteAllText($sources['winbridge-worker.exe'], 'updated worker')
[IO.File]::WriteAllText($sources['winbridge-desktop.exe'], 'updated desktop')
Assert-Rejected { Copy-SessionBinaries $sources $runtime (Join-Path $ScratchRoot 'stage-blocked') @((Join-Path $runtime 'winbridge-desktop.exe')) } 'Active runtime update was accepted'
foreach ($path in $oldHashes.Keys) { Assert-True ((Get-FileHash -LiteralPath $path).Hash -eq $oldHashes[$path]) 'Part of runtime changed before active-process refusal' }
$updated = @(Copy-SessionBinaries $sources $runtime (Join-Path $ScratchRoot 'stage-updated'))
Assert-True (@($updated | Where-Object { $_.updated }).Count -eq 2) 'Changed binaries not updated'
Assert-True ((Get-FileHash -LiteralPath $first.workerConfigPath).Hash -eq $workerHash) 'Config changed during binary update'
Assert-True ([IO.File]::ReadAllText($stateMarker) -eq 'existing state') 'State changed during binary update'

$mismatched = Get-SessionLayout $ScratchRoot (Join-Path $ScratchRoot 'mismatched') 'mismatch'
[IO.Directory]::CreateDirectory($mismatched.runtimeDirectory) | Out-Null
Write-NewJson $mismatched.desktopConfigPath @{ schemaVersion = 1; workerConfigPath = 'different-worker.local.json' }
$mismatchHash = (Get-FileHash -LiteralPath $mismatched.desktopConfigPath).Hash
Assert-Rejected { Initialize-SessionConfiguration $mismatched $settings } 'Mismatched desktop target accepted'
Assert-True ((Get-FileHash -LiteralPath $mismatched.desktopConfigPath).Hash -eq $mismatchHash) 'Mismatched config overwritten'
Write-Output "Session runtime file checks passed: stable paths, config/draft/state preservation, unchanged binaries, active-update refusal, hash verification, desktop target mismatch. Scratch: $ScratchRoot"
