<# One native worker in an announced fixed EXE slot. No production changes. #>
param(
 [Parameter(Mandatory)][string]$WorkDirectory,
 [Parameter(Mandatory)][string]$CacheDirectory,
 [Parameter(Mandatory)][string]$SharedReportDirectory,
 [Parameter(Mandatory)][string]$TestRuntimeDirectory,
 [Parameter(Mandatory)][string]$SourceCommit,
 [Parameter(Mandatory)][string]$ManifestSha256
)
$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
$PSNativeCommandUseErrorActionPreference=$false
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/test-worker-runtime.ps1')
$work=[IO.Path]::GetFullPath($WorkDirectory);$cache=[IO.Path]::GetFullPath($CacheDirectory)
$source=Join-Path $work 'winbridge';$verifier=Join-Path $cache 'debug/winbridge-update.exe'
foreach($path in @($work,$cache)){Assert-SessionDirectory $path;Assert-WorkflowCachePath $path}
if((Split-Path $work -Leaf) -notmatch '^tmp-multi-204-\d{8}-\d{6}-[a-f0-9]{8}$' -or
 (Split-Path $cache -Leaf) -cne 'test-build-cache' -or -not (Test-Path -LiteralPath $verifier)){throw 'Owned experiment and retained verifier/cache required'}
$unexpected=@(Get-ChildItem Env:|Where-Object {$_.Name -match '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_PROFILE_.*|WINBRIDGE_.*)$' -and $_.Name -cne 'WINBRIDGE_SCHEDULER_DIAGNOSTICS'})
if($unexpected.Count){throw 'Unexpected build/validation environment; no build or runtime change'}
Assert-SessionFile (Join-Path $cache '.winbridge-build-cache.lock')
$lease=[IO.File]::Open((Join-Path $cache '.winbridge-build-cache.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$report=[ordered]@{schemaVersion=1;kind='multi_controller_native_windows';passed=$false;sourceCommit=$SourceCommit;manifestSha256=$ManifestSha256;
 installedRuntimeChanged=$false;windowsOpened=$false;commands=@();isolatedWorkerExitConfirmed=$false;remainingFixtureProcesses=$null;error=$null;
 freeBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($work)).AvailableFreeSpace}
$slot=$null;$child=$null;$out=$null;$err=$null
$oldLocation=Get-Location;$oldIncremental=$env:CARGO_INCREMENTAL;$oldMigration=$env:WINBRIDGE_ATTRIBUTION_MIGRATION_INPUT
function Worker-Json([string[]]$Arguments){
 $si=[Diagnostics.ProcessStartInfo]::new($slot.executable)
 $si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $si.StandardOutputEncoding=[Text.UTF8Encoding]::new($false);$si.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
 foreach($a in $Arguments){$si.ArgumentList.Add($a)}
 $p=[Diagnostics.Process]::new();$p.StartInfo=$si
 try{
  [void]$p.Start();$stdout=$p.StandardOutput.ReadToEndAsync();$stderr=$p.StandardError.ReadToEndAsync()
  if(-not $p.WaitForExit(15000)){$p.Kill();[void]$p.WaitForExit(5000);throw 'Readonly client timeout'}
  if($p.ExitCode -ne 0){throw $stderr.GetAwaiter().GetResult()}
  return ($stdout.GetAwaiter().GetResult()|ConvertFrom-Json -Depth 60)
 }finally{$p.Dispose()}
}
function Verify-Source([string]$Log){
 & $verifier verify-source $source $ManifestSha256 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $SharedReportDirectory $Log) -Encoding utf8
 if($LASTEXITCODE -ne 0){throw 'Source verification failed'}
 $v=Get-Content -LiteralPath (Join-Path $SharedReportDirectory $Log) -Raw|ConvertFrom-Json
 if($v.kind -cne 'source_verified' -or $v.sourceCommit -cne $SourceCommit){throw 'Source commit differs'}
}
try{
 if($report.freeBytesBefore -lt 6GB){throw 'Less than 6 GiB; no build'}
 Verify-Source 'source-before.log'
 $env:CARGO_INCREMENTAL='0';Set-Location -LiteralPath $source
 $cargoArguments=@('build','-p','winbridge-worker','--bins','--locked','--offline','--features','bundled-sqlite','--target-dir',$cache)
 Write-Output ('Native multi-controller: build normal worker; cache '+$cache)
 $timer=[Diagnostics.Stopwatch]::StartNew()
 & cargo.exe @cargoArguments 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $SharedReportDirectory 'worker-build.log') -Encoding utf8
 $code=$LASTEXITCODE;$timer.Stop()
 $report.commands+=@(@{name='worker-build';arguments=$cargoArguments;exitCode=$code;elapsedMs=$timer.ElapsedMilliseconds})
 if($code -ne 0){throw 'Native worker build failed'}
 $installed='K:\_proj-26\winbridge\runtime\winbridge-worker.exe'
 $migrationInput=Join-Path $work 'worker-migration-input.json'
 Write-NewJson $migrationInput @{root=(Join-Path $work 'worker-migration-case');reference=$installed;
  referenceSha256='536d86378b5c5adfc98b135c404e322d1dc9cc7dd19fb20941ca9b3f6686a712';
  powershell=(Get-Command pwsh.exe).Source;proof=(Join-Path $SharedReportDirectory 'worker-journal-attribution-migration-proof.json')}
 $env:WINBRIDGE_ATTRIBUTION_MIGRATION_INPUT=$migrationInput
 $migrationArguments=@('test','-p','winbridge-worker','--locked','--offline','--features','bundled-sqlite','--target-dir',$cache,
  '--test','attribution_migration','actual_seventeen_history_migrates_to_nineteen_without_replay_or_old_worker_write','--','--ignored','--exact','--nocapture')
 Write-Output 'Native multi-controller: cold journal17-to19 migration; local IPC only'
 $timer=[Diagnostics.Stopwatch]::StartNew()
 & cargo.exe @migrationArguments 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $SharedReportDirectory 'worker-migration.log') -Encoding utf8
 $code=$LASTEXITCODE;$timer.Stop()
 $report.commands+=@(@{name='worker17-to19';arguments=$migrationArguments;exitCode=$code;elapsedMs=$timer.ElapsedMilliseconds})
 if($code -ne 0){throw 'Released worker17 cold migration failed'}
 $env:WINBRIDGE_ATTRIBUTION_MIGRATION_INPUT=$oldMigration
 $image=Join-Path $cache 'debug/winbridge-worker.exe';$sha=(Get-FileHash -LiteralPath $image).Hash.ToLowerInvariant()
 $slot=Open-TestWorkerRuntime -Directory $TestRuntimeDirectory -InstalledWorker $installed -Image $image -Sha256 $sha
 $report.testExecutable=$slot.executable;$report.workerSha256=$sha
 $descriptor=Worker-Json @('--artifact-info')
 if($descriptor.build.sourceCommit -cne $SourceCommit -or $descriptor.build.sourceManifestSha256 -cne $ManifestSha256 -or $descriptor.build.sourceDirty){throw 'Native worker identity differs'}
 Write-NewJson (Join-Path $SharedReportDirectory 'worker-build-identity.json') $descriptor
 foreach($i in 0..3){
  $project=Join-Path $work ('p'+$i);New-Item -ItemType Directory -Path $project|Out-Null
  [IO.File]::Copy((Join-Path $cache 'debug/winbridge-fixture.exe'),(Join-Path $project 'winbridge-fixture.exe'),$false)
 }
 $config=Join-Path $work 'worker.json';[IO.File]::Copy((Join-Path $SharedReportDirectory 'worker.json'),$config,$false)
 $si=[Diagnostics.ProcessStartInfo]::new($slot.executable);$si.UseShellExecute=$false;$si.CreateNoWindow=$true
 $si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $si.StandardOutputEncoding=[Text.UTF8Encoding]::new($false);$si.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
 foreach($a in @('--config',$config,'serve')){$si.ArgumentList.Add($a)}
 Write-Output ('Native multi-controller: start fixed network EXE '+$slot.executable)
 $child=[Diagnostics.Process]::new();$child.StartInfo=$si;[void]$child.Start()
 $out=$child.StandardOutput.ReadToEndAsync();$err=$child.StandardError.ReadToEndAsync()
 $until=[DateTime]::UtcNow.AddSeconds(90);$info=$null
 do{
  if($child.HasExited){throw 'Test worker exited before readiness'}
  try{$info=(Worker-Json @('--config',$config,'info')).info}catch{Start-Sleep -Milliseconds 200;continue}
  if($info.controllerConnection.state -ceq 'connected'){break}
  Start-Sleep -Milliseconds 200
 }while([DateTime]::UtcNow -lt $until)
 if($null -eq $info -or $info.controllerConnection.state -cne 'connected'){throw 'Fixed-path worker connection failed; inspect stderr and firewall; no automatic firewall changes'}
 Write-NewJson (Join-Path $SharedReportDirectory 'network-ready.json') @{context=$info.fencedRequests.context;generation=$info.generation;processId=$child.Id;build=$info.build}
 $donePath=Join-Path $SharedReportDirectory 'network-complete.json';$until=[DateTime]::UtcNow.AddSeconds(240)
 while(-not (Test-Path -LiteralPath $donePath)){
  if($child.HasExited -or [DateTime]::UtcNow -gt $until){throw 'Network gate incomplete; original evidence preserved'}
  Start-Sleep -Milliseconds 200
 }
 $done=Get-Content -LiteralPath $donePath -Raw|ConvertFrom-Json -Depth 60
 if($done.passed -isnot [bool] -or -not $done.passed -or $done.context.journalIncarnation -cne $info.fencedRequests.context.journalIncarnation){throw 'Network proof scope differs'}
 $last=(Worker-Json @('--config',$config,'info')).info
 if($last.activeTaskCount -ne 0){throw 'Test tasks remain active'}
 $report.projectRuns=@()
 foreach($i in 0..3){
  $lines=@(Get-Content -LiteralPath (Join-Path $work ('p'+$i+'/runs.log')))
  $expected=if($i -eq 0){3}else{2}
  if($lines.Count -ne $expected){throw 'Duplicate or missing actual process execution'}
  $report.projectRuns+=@(@{project=('p'+$i);lines=$lines;expected=$expected})
 }
 $remaining=@(Get-CimInstance Win32_Process -Filter "Name='winbridge-fixture.exe'"|Where-Object {$_.ExecutablePath -and $_.ExecutablePath.StartsWith($work+'\',[StringComparison]::OrdinalIgnoreCase)})
 $report.remainingFixtureProcesses=$remaining.Count
 if($remaining.Count){throw 'Cancelled fixture descendant is still alive'}
 $child.Kill();if(-not $child.WaitForExit(15000)){throw 'Isolated worker exit unconfirmed'}
 $report.isolatedWorkerExitConfirmed=$true
 Verify-Source 'source-after.log'
 [IO.File]::Copy($image,(Join-Path $SharedReportDirectory 'winbridge-worker.exe'),$false)
 if((Get-FileHash -LiteralPath (Join-Path $SharedReportDirectory 'winbridge-worker.exe')).Hash.ToLowerInvariant() -cne $sha){throw 'Exported worker differs'}
 $report.passed=$true
}catch{$report.error=$_.Exception.Message;Write-Output ('Multi-controller native gate failed: '+$report.error)}
finally{
 if($null -ne $child){
  try{if(-not $child.HasExited){$child.Kill()};if($child.WaitForExit(15000)){$report.isolatedWorkerExitConfirmed=$true}else{$report.passed=$false;$report.error+='; worker exit unconfirmed'}}catch{$report.passed=$false;$report.error+='; worker cleanup unconfirmed'}
  foreach($pair in @(@($out,'worker-stdout.log'),@($err,'worker-stderr.log'))){
   $text='Capture did not finish within bounded wait'
   try{if($pair[0].Wait(5000)){$text=$pair[0].GetAwaiter().GetResult()}}catch{$report.passed=$false}
   [IO.File]::WriteAllText((Join-Path $SharedReportDirectory $pair[1]),$text,[Text.UTF8Encoding]::new($false))
  }
  $child.Dispose()
 }
 Close-TestWorkerRuntime $slot
 Set-Location -LiteralPath $oldLocation;$env:CARGO_INCREMENTAL=$oldIncremental;$env:WINBRIDGE_ATTRIBUTION_MIGRATION_INPUT=$oldMigration;$lease.Dispose()
 $report.freeBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($work)).AvailableFreeSpace
 Write-NewJson (Join-Path $SharedReportDirectory 'report.json') $report
 $files=@(Get-ChildItem -LiteralPath $SharedReportDirectory -File|Sort-Object Name|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}})
 Write-NewJson (Join-Path $SharedReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
if(-not $report.passed){exit 1}
