param([string]$ScratchRoot = '', [string]$ReportPath = '')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/update-stage.ps1')
$generatedScratch = -not $ScratchRoot
if (-not $ScratchRoot) { $ScratchRoot = Join-Path ([IO.Path]::GetTempPath()) ('winbridge-candidate-finalization-'+[Guid]::NewGuid().ToString('N')) }
if (Test-Path -LiteralPath $ScratchRoot) { throw 'Scratch root already exists; preserved' }
[IO.Directory]::CreateDirectory($ScratchRoot) | Out-Null
# Execute the actual final build-script statement with a bounded metadata mock.
# This catches unbound caller variables/argv wiring; Rust/native checks cover files.
$errors=$null; $tokens=$null
$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'build-windows-update.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw 'Build script syntax invalid' }
$last=$ast.EndBlock.Statements[-1]
if ($last -isnot [Management.Automation.Language.TryStatementAst]) { throw 'Expected isolated finalization try/finally statement' }
$finalize=[scriptblock]::Create($last.Extent.Text)
Write-Output 'WinBridge script-wiring fixture: metadata mock only; no product build or install'
$queryHelper=Join-Path $ScratchRoot 'metadata-helper.ps1'
$source=Join-Path $ScratchRoot 'src'
[IO.Directory]::CreateDirectory($source) | Out-Null
$ManifestSha256='a'*64
$oldUpdateConsoleEncoding=[Console]::OutputEncoding
$oldUpdatePipelineEncoding=$OutputEncoding
foreach ($bad in @($false,$true)) {
  $stage=Join-Path $ScratchRoot $(if ($bad) {'rejected'} else {'accepted'})
  [IO.Directory]::CreateDirectory($stage) | Out-Null
  [IO.File]::WriteAllText((Join-Path $stage 'candidate-report.json'),'compile fixture')
  $reportBefore=(Get-FileHash -LiteralPath (Join-Path $stage 'candidate-report.json')).Hash
  $kind=if ($bad) {'wrong_contract'} else {'candidate_file_inspection'}
  $helper=@'
param($Operation,$Source,$Manifest,$Stage,$ReportSha)
if ($Operation -cne 'inspect-candidate-files') { throw 'Wrong operation forwarded' }
$global:LASTEXITCODE=0
@{kind='KIND';readOnly=$true;journalBound=$false;runtimeApplied=$false;candidateVerified=$false;source=@{manifestSha256=$Manifest};report=@{sha256=$ReportSha};artifacts=@(1,2,3,4)} | ConvertTo-Json -Depth 8 -Compress
'@
  [IO.File]::WriteAllText($queryHelper,$helper.Replace('KIND',$kind))
  $failure=$null
  try { & $finalize } catch { $failure=$_.Exception.Message }
  if ($bad) {
    if (-not $failure -or $failure -notlike '*exact-file proof incomplete*' -or (Test-Path -LiteralPath (Join-Path $stage 'candidate-files-proof.json'))) { throw 'Malformed metadata was not rejected' }
  } elseif ($failure) { throw "Final build-script statement failed: $failure" }
  $command=Get-Content -LiteralPath (Join-Path $stage 'candidate-files-command.json') -Raw | ConvertFrom-Json
  if ($command.executable -cne $queryHelper -or $command.exitCode -ne 0 -or $command.arguments.Count -ne 5 -or $command.arguments[1] -cne $source -or $command.arguments[2] -cne $ManifestSha256 -or $command.arguments[3] -cne $stage -or $command.arguments[4] -cne $reportBefore.ToLowerInvariant()) { throw 'Finalization helper/argv mismatch' }
  if ((Get-FileHash -LiteralPath (Join-Path $stage 'candidate-report.json')).Hash -cne $reportBefore) { throw 'Published compiler report was changed' }
}
if ($ReportPath) { Write-NewJson $ReportPath @{schemaVersion=1;kind='candidate_finalization_wiring_proof';passed=$true;platform=$(if ($env:OS -eq 'Windows_NT') {'windows'} else {'linux'});actualFinalStatement=$true;helperBound=$true;malformedRejected=$true;reportPreserved=$true;metadataMocked=$true;runtimeApplied=$false;windowsOpened=$false} }
if ($generatedScratch) { Remove-Item -LiteralPath $ScratchRoot -Recurse -Force }
Write-Output 'Candidate finalization: caller variables/argv, malformed response and immutable report checked'
