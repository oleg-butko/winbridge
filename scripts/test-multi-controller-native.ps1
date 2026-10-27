<# One native worker in an announced fixed EXE slot. No production changes. #>
param(
 [Parameter(Mandatory)][string]$WorkDirectory,
 [Parameter(Mandatory)][string]$CacheDirectory,
 [Parameter(Mandatory)][string]$SharedReportDirectory,
 [Parameter(Mandatory)][string]$TestRuntimeDirectory,
 [Parameter(Mandatory)][string]$SourceCommit,
 [Parameter(Mandatory)][string]$ManifestSha256,
 [ValidateRange(60,900)][int]$ConnectionWaitSeconds=240
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
 diagnosticWriteFailures=0;freeBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($work)).AvailableFreeSpace}
$slot=$null;$child=$null;$out=$null;$err=$null;$outFile=$null;$errFile=$null;$traceWriter=$null
$runClock=[Diagnostics.Stopwatch]::StartNew()
function Trace-Gate([string]$Event,$Data){
 $row=@{atUtc=[DateTime]::UtcNow.ToString('o');elapsedMs=$runClock.ElapsedMilliseconds;bootstrapPid=$PID;event=$Event;data=$Data}
 $line=$row|ConvertTo-Json -Depth 16 -Compress
 try{if($null -ne $traceWriter){$traceWriter.WriteLine($line)}}catch{$report.diagnosticWriteFailures++;$report.lastDiagnosticError=$_.Exception.Message}
 try{[Console]::WriteLine($line)}catch{}
}
$oldLocation=Get-Location;$oldIncremental=$env:CARGO_INCREMENTAL;$oldMigration=$env:WINBRIDGE_ATTRIBUTION_MIGRATION_INPUT
function Worker-Json([string[]]$Arguments){
 $si=[Diagnostics.ProcessStartInfo]::new($slot.executable)
 $si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $si.StandardOutputEncoding=[Text.UTF8Encoding]::new($false);$si.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
 foreach($a in $Arguments){$si.ArgumentList.Add($a)}
 $p=[Diagnostics.Process]::new();$p.StartInfo=$si
 try{
  [void]$p.Start();$stdout=$p.StandardOutput.ReadToEndAsync();$stderr=$p.StandardError.ReadToEndAsync()
  Trace-Gate 'readonly-cli-started' @{clientPid=$p.Id;operation=$Arguments[-1]}
  $clientClock=[Diagnostics.Stopwatch]::StartNew()
  if(-not $p.WaitForExit(15000)){Trace-Gate 'readonly-cli-timeout' @{clientPid=$p.Id;elapsedMs=$clientClock.ElapsedMilliseconds};$p.Kill();$stopped=$p.WaitForExit(5000);Trace-Gate 'readonly-cli-stop' @{clientPid=$p.Id;waitConfirmed=$stopped};throw 'Readonly client timeout'}
  Trace-Gate 'readonly-cli-finished' @{clientPid=$p.Id;elapsedMs=$clientClock.ElapsedMilliseconds;exitCode=$p.ExitCode}
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
 $traceStream=[IO.File]::Open((Join-Path $SharedReportDirectory 'diagnostics.jsonl'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
 $traceWriter=[IO.StreamWriter]::new($traceStream,[Text.UTF8Encoding]::new($false));$traceWriter.AutoFlush=$true
 Trace-Gate 'gate-start' @{sourceCommit=$SourceCommit;freeBytes=$report.freeBytesBefore;testExecutable=(Join-Path $TestRuntimeDirectory 'winbridge-worker.exe');powershellVersion=$PSVersionTable.PSVersion.ToString()}
 if($report.freeBytesBefore -lt 6GB){throw 'Less than 6 GiB; no build'}
 Trace-Gate 'source-verification-start' @{}
 Verify-Source 'source-before.log'
 Trace-Gate 'source-verification-complete' @{}
 $env:CARGO_INCREMENTAL='0';Set-Location -LiteralPath $source
 $cargoArguments=@('build','-p','winbridge-worker','--bins','--locked','--offline','--features','bundled-sqlite','--target-dir',$cache)
 Write-Output ('Native multi-controller: build normal worker; cache '+$cache)
 Trace-Gate 'build-start' @{arguments=$cargoArguments}
 $timer=[Diagnostics.Stopwatch]::StartNew()
 & cargo.exe @cargoArguments 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $SharedReportDirectory 'worker-build.log') -Encoding utf8
 $code=$LASTEXITCODE;$timer.Stop()
 $report.commands+=@(@{name='worker-build';arguments=$cargoArguments;exitCode=$code;elapsedMs=$timer.ElapsedMilliseconds})
 Trace-Gate 'build-complete' @{elapsedMs=$timer.ElapsedMilliseconds;exitCode=$code}
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
 Trace-Gate 'cold-migration-start' @{}
 $timer=[Diagnostics.Stopwatch]::StartNew()
 & cargo.exe @migrationArguments 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $SharedReportDirectory 'worker-migration.log') -Encoding utf8
 $code=$LASTEXITCODE;$timer.Stop()
 $report.commands+=@(@{name='worker17-to19';arguments=$migrationArguments;exitCode=$code;elapsedMs=$timer.ElapsedMilliseconds})
 Trace-Gate 'cold-migration-complete' @{elapsedMs=$timer.ElapsedMilliseconds;exitCode=$code}
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
 Trace-Gate 'test-worker-started' @{processId=$child.Id;executable=$slot.executable;workerSha256=$sha;configPath=$config}
 Write-NewJson (Join-Path $SharedReportDirectory 'worker-process.json') @{processId=$child.Id;startedAtUtc=$child.StartTime.ToUniversalTime().ToString('o');executable=$slot.executable;workerSha256=$sha}
 # Write bytes as they arrive. A stalled process/pipe must leave partial logs;
 # retaining an unfinished ReadToEnd task must never hide startup diagnostics.
 $outFile=[IO.FileStream]::new((Join-Path $SharedReportDirectory 'worker-stdout.log'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read,1,$true)
 $errFile=[IO.FileStream]::new((Join-Path $SharedReportDirectory 'worker-stderr.log'),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read,1,$true)
 $out=$child.StandardOutput.BaseStream.CopyToAsync($outFile);$err=$child.StandardError.BaseStream.CopyToAsync($errFile)
 $report.connectionWaitSeconds=$ConnectionWaitSeconds
 $until=[DateTime]::UtcNow.AddSeconds($ConnectionWaitSeconds);$info=$null;$lastInfoTrace=-1L
 do{
  if($child.HasExited){throw 'Test worker exited before readiness'}
  try{$info=(Worker-Json @('--config',$config,'info')).info}catch{Trace-Gate 'local-info-error' @{message=$_.Exception.Message};Start-Sleep -Seconds 1;continue}
  if($runClock.ElapsedMilliseconds - $lastInfoTrace -ge 3000){Trace-Gate 'local-info' @{processId=$child.Id;generation=$info.generation;primary=$info.controllerConnection};$lastInfoTrace=$runClock.ElapsedMilliseconds}
  if($info.controllerConnection.state -ceq 'connected'){break}
  Start-Sleep -Seconds 2
 }while([DateTime]::UtcNow -lt $until)
 if($null -eq $info -or $info.controllerConnection.state -cne 'connected'){
  if($null -ne $info){Write-NewJson (Join-Path $SharedReportDirectory 'network-last-info.json') $info}
  throw 'Fixed-path worker connection failed; inspect partial stderr and last Info; no automatic firewall changes'
 }
 Trace-Gate 'primary-connected' @{processId=$child.Id;connection=$info.controllerConnection}
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
 Trace-Gate 'test-worker-stop-request' @{processId=$child.Id;hasExited=$child.HasExited}
 $child.Kill();$stopped=$child.WaitForExit(15000);Trace-Gate 'test-worker-stop-result' @{processId=$child.Id;waitConfirmed=$stopped;hasExited=$child.HasExited}
 if(-not $stopped){throw 'Isolated worker exit unconfirmed'}
 $report.isolatedWorkerExitConfirmed=$true
 Verify-Source 'source-after.log'
 [IO.File]::Copy($image,(Join-Path $SharedReportDirectory 'winbridge-worker.exe'),$false)
 if((Get-FileHash -LiteralPath (Join-Path $SharedReportDirectory 'winbridge-worker.exe')).Hash.ToLowerInvariant() -cne $sha){throw 'Exported worker differs'}
 $report.passed=$true
}catch{$report.error=$_.Exception.Message;Trace-Gate 'gate-error' @{message=$report.error};Write-Output ('Multi-controller native gate failed: '+$report.error)}
finally{
 if($null -ne $child){
  try{Trace-Gate 'cleanup-worker' @{processId=$child.Id;hasExited=$child.HasExited};if(-not $child.HasExited){$child.Kill()};$stopped=$child.WaitForExit(15000);Trace-Gate 'cleanup-worker-wait' @{processId=$child.Id;waitConfirmed=$stopped};if($stopped){$report.isolatedWorkerExitConfirmed=$true}else{$report.passed=$false;$report.error+='; worker exit unconfirmed'}}catch{$report.passed=$false;$report.error+='; worker cleanup unconfirmed';Trace-Gate 'cleanup-worker-error' @{message=$_.Exception.Message}}
  $report.stdoutCaptureComplete=$false;$report.stderrCaptureComplete=$false
  foreach($pair in @(@($out,$outFile,'stdoutCaptureComplete'),@($err,$errFile,'stderrCaptureComplete'))){
   try{if($null -ne $pair[0] -and $pair[0].Wait(5000)){
    $pair[0].GetAwaiter().GetResult();$pair[1].Flush($true);$pair[1].Dispose();$report[$pair[2]]=$true
   }}catch{$report.passed=$false}
  }
  if(-not $report.stdoutCaptureComplete -or -not $report.stderrCaptureComplete){$report.passed=$false;$report.error+='; capture incomplete, partial logs preserved'}
  # A process with unconfirmed termination may retain unfinished pipe I/O.
  # Do not let synchronous disposal block publication of its owner/error report.
  if($report.isolatedWorkerExitConfirmed -and $report.stdoutCaptureComplete -and $report.stderrCaptureComplete){$child.Dispose()}
 }
 Close-TestWorkerRuntime $slot
 Set-Location -LiteralPath $oldLocation;$env:CARGO_INCREMENTAL=$oldIncremental;$env:WINBRIDGE_ATTRIBUTION_MIGRATION_INPUT=$oldMigration;$lease.Dispose()
 $report.freeBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($work)).AvailableFreeSpace
 Trace-Gate 'gate-finished' @{passed=$report.passed;error=$report.error;workerExitConfirmed=$report.isolatedWorkerExitConfirmed;freeBytes=$report.freeBytesAfter}
 if($report.diagnosticWriteFailures -gt 0){$report.passed=$false;$report.error+='; diagnostic log write failed'}
 if($null -ne $traceWriter){$traceWriter.Dispose()}
 Write-NewJson (Join-Path $SharedReportDirectory 'report.json') $report
 $files=@(Get-ChildItem -LiteralPath $SharedReportDirectory -File|Sort-Object Name|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}})
 Write-NewJson (Join-Path $SharedReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
if(-not $report.passed){exit 1}
