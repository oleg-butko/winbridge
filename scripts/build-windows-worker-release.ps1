<# Build and validate a worker-only local patch. Never replace installed runtime. #>
param(
 [Parameter(Mandatory)][string]$SourceArchive,
 [Parameter(Mandatory)][string]$ArchiveSha256,
 [Parameter(Mandatory)][string]$ManifestSha256,
 [Parameter(Mandatory)][string]$SourceCommit,
 [Parameter(Mandatory)][string]$WorkDirectory,
 [Parameter(Mandatory)][string]$CacheDirectory,
 [Parameter(Mandatory)][string]$VerifierExecutable,
 [Parameter(Mandatory)][string]$VerifierSha256,
 [Parameter(Mandatory)][string]$BaselineWorker,
 [Parameter(Mandatory)][string]$BaselineSha256,
 [Parameter(Mandatory)][string]$ControllerUrl,
 [Parameter(Mandatory)][string]$InstanceId,
 [Parameter(Mandatory)][string]$SharedReportDirectory,
 [Parameter(Mandatory)][string]$InstalledConfig,
 [Parameter(Mandatory)][string]$InstalledConfigSha256,
 [Parameter(Mandatory)][string]$InstalledWorker,
 [Parameter(Mandatory)][string]$InstalledWorkerSha256,
 [Parameter(Mandatory)][string]$InstalledGeneration,
 [Parameter(Mandatory)][string]$TestRuntimeDirectory,
 [ValidateRange(60,3600)][int]$FirewallWaitSeconds=900
)
$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
$PSNativeCommandUseErrorActionPreference=$false
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/validation-storage.ps1')
. (Join-Path $PSScriptRoot 'lib/test-worker-runtime.ps1')
$work=[IO.Path]::GetFullPath($WorkDirectory)
$cache=[IO.Path]::GetFullPath($CacheDirectory)
foreach($path in @($work,$cache)){Assert-SessionDirectory $path;Assert-WorkflowCachePath $path;if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($path)).DriveType -ne [IO.DriveType]::Fixed){throw 'Local fixed disk required'}}
if($env:OS -cne 'Windows_NT' -or (Test-Path -LiteralPath $work) -or
 (Split-Path $work -Leaf) -notmatch '^worker-0\.1\.1-\d{8}-\d{6}-[a-f0-9]{8}$' -or
 (Split-Path $cache -Leaf) -cne 'test-build-cache' -or -not (Test-Path -LiteralPath $cache -PathType Container)){throw 'New local release directory and existing compiler cache required'}
if(@(Get-Process cargo -ErrorAction SilentlyContinue).Count -or
 @(Get-ChildItem Env:|Where-Object {$_.Name -match '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_PROFILE_.*|WINBRIDGE_.*)$'}).Count){throw 'Requires idle compiler and unmodified environment'}
foreach($pair in @(@($SourceArchive,$ArchiveSha256),@($VerifierExecutable,$VerifierSha256),@($BaselineWorker,$BaselineSha256),@($InstalledConfig,$InstalledConfigSha256),@($InstalledWorker,$InstalledWorkerSha256))){
 Assert-SessionFile $pair[0]
 if((Get-FileHash -LiteralPath $pair[0]).Hash.ToLowerInvariant() -cne $pair[1]){throw 'Pinned input differs'}
}
$leasePath=Join-Path $cache '.winbridge-build-cache.lock';Assert-SessionFile $leasePath
$lease=[IO.File]::Open($leasePath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$installedManifest=Join-Path (Split-Path $InstalledWorker -Parent) 'winbridge-installed-set.json'
if(Test-Path -LiteralPath $installedManifest){throw 'Installed-set manifest requires a coordinated release, not one worker exe'}
$source=Join-Path $work 'tmp-build/source/winbridge'
$report=[ordered]@{schemaVersion=1;kind='worker_patch_release';passed=$false;version='0.1.1';sourceCommit=$SourceCommit;
 manifestSha256=$ManifestSha256;archiveSha256=$ArchiveSha256;installedRuntimeChanged=$false;windowsOpened=$false;
 profile='dev; existing package optimizations; debug assertions and overflow checks enabled';commands=@();artifacts=@();cpu=@();error=$null}
$saved=@{};foreach($name in @('CARGO_INCREMENTAL','CARGO_TERM_COLOR','TEMP','TMP','WINBRIDGE_IDLE_BASELINE_WORKER')){$saved[$name]=[Environment]::GetEnvironmentVariable($name)}
$testSlot=$null
$location=Get-Location;$childStarted=$false;$childStopAttempted=$false;$child=$null;$out=$null;$err=$null
function Save-ProcessCapture($Capture,[string]$Path){
 # EOF is not guaranteed after termination is requested (inherited handles or
 # pending OS I/O). A failed gate must still publish its diagnostic report.
 try{
  if($null -ne $Capture -and $Capture.Wait(5000)){
   [IO.File]::WriteAllText($Path,$Capture.GetAwaiter().GetResult());return
  }
 }catch{}
 [IO.File]::WriteAllText($Path,'Capture incomplete after bounded wait; preserve the task/process identity; no successful cleanup claimed.')
}
function Invoke-Check([string]$Name,[string[]]$Arguments){
 $free=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($cache)).AvailableFreeSpace
 if($free -lt 6GB){throw 'Less than 6GiB free; no build started'}
 Write-Host "WinBridge worker release: $Name"
 $watch=[Diagnostics.Stopwatch]::StartNew()
 & cargo.exe @Arguments 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $work "$Name.log") -Encoding utf8
 $code=$LASTEXITCODE;$watch.Stop()
 $report.commands+=@(@{name=$Name;log="$Name.log";arguments=$Arguments;exitCode=$code;elapsedMs=$watch.ElapsedMilliseconds;freeBytesBefore=$free})
 if($code -ne 0){throw "Release check failed: $Name ($code)"}
}
function Invoke-WorkerJson([string[]]$Arguments){
 $si=[Diagnostics.ProcessStartInfo]::new((Join-Path $work 'winbridge-worker.exe'))
 $si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $si.StandardOutputEncoding=[Text.UTF8Encoding]::new($false);$si.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
 foreach($arg in $Arguments){$si.ArgumentList.Add($arg)}
 $p=[Diagnostics.Process]::new();$p.StartInfo=$si
 try{
  [void]$p.Start();$stdout=$p.StandardOutput.ReadToEndAsync();$stderr=$p.StandardError.ReadToEndAsync()
  if(-not $p.WaitForExit(15000)){$p.Kill();[void]$p.WaitForExit(5000);throw 'Worker client timeout; exit may be unconfirmed; do not retry mutations'}
  $text=$stdout.GetAwaiter().GetResult();$errorText=$stderr.GetAwaiter().GetResult()
  if($p.ExitCode -ne 0){throw "Worker client error: $errorText"}
  return ($text|ConvertFrom-Json -Depth 60)
 }finally{$p.Dispose()}
}
try{
 New-Item -ItemType Directory -Path (Join-Path $work 'tmp-build')|Out-Null
 Write-WorkflowStorageGuide -RunDirectory $work
 $zip=Join-Path $work 'tmp-build/source.zip';[IO.File]::Copy($SourceArchive,$zip,$false)
 if((Get-FileHash -LiteralPath $zip).Hash.ToLowerInvariant() -cne $ArchiveSha256){throw 'Copied ZIP differs'}
 Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $work 'tmp-build/source')
 $verify=@(& $VerifierExecutable verify-source $source $ManifestSha256)
 if($LASTEXITCODE -ne 0){throw 'Source verification failed'}
 $verified=($verify -join "`n")|ConvertFrom-Json
 if($verified.kind -cne 'source_verified' -or $verified.sourceCommit -cne $SourceCommit -or $verified.manifestSha256 -cne $ManifestSha256){throw 'Source pin differs'}
 foreach($relative in @('scripts/build-windows-worker-release.ps1','scripts/lib/test-worker-runtime.ps1','scripts/test-worker-runtime.ps1')){
  $tokens=$null;$errors=$null
  [Management.Automation.Language.Parser]::ParseFile((Join-Path $source $relative),[ref]$tokens,[ref]$errors)|Out-Null
  if($errors.Count){throw ('PowerShell syntax check failed: '+$relative+' '+($errors|Out-String))}
 }
 $report.syntaxVerified=$true
 $env:CARGO_INCREMENTAL='0';$env:CARGO_TERM_COLOR='never'
 $env:TEMP=Join-Path $work 'tmp-build/tmp-test-temp';$env:TMP=$env:TEMP;New-Item -ItemType Directory -Path $env:TEMP|Out-Null
 Set-Location $source
 $common=@('test','-p','winbridge-worker','--locked','--offline','--features','bundled-sqlite','--target-dir',$cache)
 # Network timing fixtures and real process ownership are validated serially,
 # matching the established native workflow harness; do not saturate their clocks.
 Invoke-Check 'worker-regressions' ($common+@('--lib','--test','idle_scheduler','--test','runtime','--test','leases','--test','queue_control','--test','resource_scheduling','--test','task_fsm','--test','work_scheduling','--test','workflows','--','--test-threads=1'))
 # Retained runtime files have adjacent update guards. Preserve that reference;
 # run only an exact, independent copy with no installed-set/config neighbours.
 $baselineDirectory=Join-Path $work 'tmp-build/baseline'
 New-Item -ItemType Directory -Path $baselineDirectory|Out-Null
 $baselineCopy=Join-Path $baselineDirectory 'winbridge-worker.exe'
 [IO.File]::Copy($BaselineWorker,$baselineCopy,$false)
 if((Get-FileHash -LiteralPath $baselineCopy).Hash.ToLowerInvariant() -cne $BaselineSha256){throw 'Independent baseline copy differs'}
 $report.baselineWorkerSha256=$BaselineSha256
 $env:WINBRIDGE_IDLE_BASELINE_WORKER=$baselineCopy
 Invoke-Check 'idle-cpu' ($common+@('--test','idle_scheduler','native_idle_cpu_baseline_and_candidate','--','--ignored','--nocapture'))
 $report.cpu=@(Get-Content -LiteralPath (Join-Path $work 'idle-cpu.log')|Where-Object {$_.StartsWith('IDLE_CPU ')}|ForEach-Object {$_.Substring(9)|ConvertFrom-Json})
 if($report.cpu.Count -ne 2 -or $report.cpu[1].percentOfOneCore -ge $report.cpu[0].percentOfOneCore*0.5){throw 'Idle CPU acceptance failed'}
 $image=Join-Path $work 'winbridge-worker.exe';[IO.File]::Copy((Join-Path $cache 'debug/winbridge-worker.exe'),$image,$false)
 $query=@(& $VerifierExecutable artifact-query $image worker)
 if($LASTEXITCODE -ne 0){throw 'Worker identity query failed'}
 $q=($query -join "`n")|ConvertFrom-Json -Depth 60
 if(-not $q.cleanupVerified -or $q.descriptor.build.version -cne '0.1.1' -or $q.descriptor.build.sourceCommit -cne $SourceCommit -or
  $q.descriptor.build.sourceManifestSha256 -cne $ManifestSha256 -or $q.descriptor.build.sourceDirty){throw 'Worker artifact identity differs'}
 $report.artifactQuery=$q
 [IO.File]::WriteAllText((Join-Path $work 'build-identity.json'),($q.descriptor|ConvertTo-Json -Depth 40),[Text.UTF8Encoding]::new($false))
 $report.versionText=(& $image --version|Out-String).Trim();if($LASTEXITCODE -ne 0){throw 'Version query failed'}
 & $image --config $InstalledConfig validate|Set-Content -LiteralPath (Join-Path $work 'installed-config-validation.log') -Encoding utf8
 if($LASTEXITCODE -ne 0){throw 'Existing configuration is not accepted'}
 $old=Invoke-WorkerJson @('--config',$InstalledConfig,'info')
 if($old.kind -cne 'info' -or $old.info.build.version -cne '0.1.0' -or $old.info.generation -cne $InstalledGeneration){throw 'Installed worker changed or new CLI failed against old worker'}
 $report.installedGeneration=$old.info.generation
 $network=Join-Path $work 'tmp-build/network';$project=Join-Path $network 'project'
 New-Item -ItemType Directory -Path $project|Out-Null
 [IO.File]::Copy((Join-Path $cache 'debug/winbridge-fixture.exe'),(Join-Path $project 'winbridge-fixture.exe'),$false)
 $configPath=Join-Path $network 'worker.json'
 $config=@{schemaVersion=1;instanceId=$InstanceId;nodeId='release-worker';stateDirectory=(Join-Path $network 'state');worker=@{controllerUrl=$ControllerUrl;keepRunningOnUiExit=$true};
  projects=@(@{id='fixture';workspaceRoot=$project;allowDirectCommands=$true;maxConcurrentMutations=1;recipes=@(@{id='fixture-success';executable=(Join-Path $project 'winbridge-fixture.exe');arguments=@('success');workingDirectory='.';timeoutMs=5000})})}
 Write-NewJson $configPath $config
 Write-Host 'WinBridge worker release: test-runtime-policy'
 $policyArguments=@('-NoProfile','-NonInteractive','-File',(Join-Path $source 'scripts/test-worker-runtime.ps1'),'-WorkDirectory',(Join-Path $work 'tmp-build/runtime-policy'),'-Fixture',(Join-Path $project 'winbridge-fixture.exe'))
 $watch=[Diagnostics.Stopwatch]::StartNew()
 & pwsh.exe @policyArguments 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $work 'test-runtime-policy.log') -Encoding utf8
 $code=$LASTEXITCODE;$watch.Stop()
 $report.commands+=@(@{name='test-runtime-policy';log='test-runtime-policy.log';arguments=$policyArguments;exitCode=$code;elapsedMs=$watch.ElapsedMilliseconds;freeBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($cache)).AvailableFreeSpace})
 if($code -ne 0){throw 'Test runtime safety checks failed'}
 $testSlot=Open-TestWorkerRuntime -Directory $TestRuntimeDirectory -InstalledWorker $InstalledWorker -Image $image -Sha256 $q.image.sha256
 $report.testRuntimeDirectory=$testSlot.directory;$report.testExecutable=$testSlot.executable;$report.testExecutableSha256=$testSlot.sha256
 $report.firewallWaitSeconds=$FirewallWaitSeconds
 Write-Host ("Test worker network executable (fixed path): "+$testSlot.executable)
 $si=[Diagnostics.ProcessStartInfo]::new($testSlot.executable);$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $si.StandardOutputEncoding=[Text.UTF8Encoding]::new($false);$si.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
 foreach($arg in @('--config',$configPath,'serve')){$si.ArgumentList.Add($arg)}
 $si.Environment['WINBRIDGE_SCHEDULER_DIAGNOSTICS']='1'
 $child=[Diagnostics.Process]::new();$child.StartInfo=$si;[void]$child.Start();$childStarted=$true
 $out=$child.StandardOutput.ReadToEndAsync();$err=$child.StandardError.ReadToEndAsync()
 Write-Host 'WinBridge worker release: waiting for old-controller/new-worker RPC check'
 Write-Host ("Waiting up to $FirewallWaitSeconds seconds for firewall approval/connection; updater startup deadlines are unchanged")
 $connectionWait=[Diagnostics.Stopwatch]::StartNew();$info=$null;$lastProgress=-15
 do{
  if($child.HasExited){throw 'Isolated worker exited before network validation'}
  try{$reply=Invoke-WorkerJson @('--config',$configPath,'info');$info=$reply.info}catch{Start-Sleep -Milliseconds 200;continue}
  if($info.controllerConnection.state -ceq 'connected'){break}
  if($connectionWait.Elapsed.TotalSeconds -ge $lastProgress+15){
   $lastProgress=$connectionWait.Elapsed.TotalSeconds
   Write-Host ("Waiting for test worker connection: "+($info.controllerConnection|ConvertTo-Json -Compress))
  }
  Start-Sleep -Milliseconds 200
 }while($connectionWait.Elapsed.TotalSeconds -lt $FirewallWaitSeconds)
 if($null -eq $info -or $info.controllerConnection.state -cne 'connected'){
  if($null -ne $info){
   Write-NewJson (Join-Path $SharedReportDirectory 'network-last-connection.json') $info.controllerConnection
   Write-Host ("Isolated worker connection: "+($info.controllerConnection|ConvertTo-Json -Compress))
  }
  throw 'Old controller connection failed; see isolated worker stderr; no firewall changes attempted'
 }
 Write-NewJson (Join-Path $SharedReportDirectory 'network-ready.json') @{version='0.1.1';sourceCommit=$SourceCommit;instanceId=$InstanceId;context=$info.fencedRequests.context}
 $donePath=Join-Path $SharedReportDirectory 'network-complete.json'
 $rpcWait=[Diagnostics.Stopwatch]::StartNew()
 while(-not (Test-Path -LiteralPath $donePath)){
  if($child.HasExited -or $rpcWait.Elapsed.TotalSeconds -ge 120){throw 'Cross-version RPC validation incomplete'}
  Start-Sleep -Milliseconds 200
 }
 $done=Get-Content -Raw -LiteralPath $donePath|ConvertFrom-Json
 if($done.passed -isnot [bool] -or -not $done.passed -or $done.context.journalIncarnation -cne $info.fencedRequests.context.journalIncarnation){throw 'Cross-version RPC proof differs'}
 $report.network=$done
 $last=Invoke-WorkerJson @('--config',$configPath,'info');if($last.info.activeTaskCount -ne 0){throw 'Isolated tasks still active'}
 $childStopAttempted=$true;$child.Kill()
 if(-not $child.WaitForExit(15000)){throw 'Isolated worker exit unconfirmed; preserve process identity, no retry'}
 $report.isolatedWorkerExitConfirmed=$true
 Save-ProcessCapture $out (Join-Path $work 'network-worker-stdout.log')
 Save-ProcessCapture $err (Join-Path $work 'network-worker-stderr.log')
 $child.Dispose();$child=$null;$childStarted=$false
 & $VerifierExecutable verify-source $source $ManifestSha256 2>&1|ForEach-Object {$_.ToString()}|Set-Content -LiteralPath (Join-Path $work 'source-final-validation.log') -Encoding utf8
 if($LASTEXITCODE -ne 0){throw 'Source changed after build; see source-final-validation.log'}
 if(Test-Path -LiteralPath $installedManifest){throw 'Installed runtime now requires a coordinated release'}
 if((Get-FileHash -LiteralPath $InstalledConfig).Hash.ToLowerInvariant() -cne $InstalledConfigSha256){throw 'Installed config changed during validation'}
 if((Get-FileHash -LiteralPath $InstalledWorker).Hash.ToLowerInvariant() -cne $InstalledWorkerSha256){throw 'Installed worker bytes changed during validation'}
 if((Get-FileHash -LiteralPath $image).Hash.ToLowerInvariant() -cne $q.image.sha256){throw 'Frozen worker changed'}
 $report.artifacts=@(@{name='winbridge-worker.exe';bytes=(Get-Item -LiteralPath $image).Length;sha256=$q.image.sha256})
 $report.passed=$true
}catch{$report.error=$_.Exception.Message;Write-Host "Worker release failed: $($report.error)"}
finally{
 if($null -ne $child -and $childStarted){
  try{
   if(-not $child.HasExited -and -not $childStopAttempted){$childStopAttempted=$true;$child.Kill()}
   if(-not $child.WaitForExit(15000)){$report.error+='; isolated process exit unconfirmed';$report.passed=$false}
   else{$report.isolatedWorkerExitConfirmed=$true}
  }catch{$report.error+='; isolated process cleanup unconfirmed';$report.passed=$false}
  Save-ProcessCapture $out (Join-Path $work 'network-worker-stdout.log')
  Save-ProcessCapture $err (Join-Path $work 'network-worker-stderr.log')
  $child.Dispose()
 }
 Close-TestWorkerRuntime $testSlot
 Set-Location $location;foreach($name in $saved.Keys){[Environment]::SetEnvironmentVariable($name,$saved[$name])};$lease.Dispose()
 if(Test-Path -LiteralPath $work){
  $report.freeBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($cache)).AvailableFreeSpace
  Write-NewJson (Join-Path $work 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $work -File|Where-Object {$_.Name -in @('winbridge-worker.exe','build-identity.json','report.json') -or $_.Extension -eq '.log'}|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $work 'report.ready.json') @{schemaVersion=1;files=$files}
  foreach($file in $files){[IO.File]::Copy((Join-Path $work $file.path),(Join-Path $SharedReportDirectory $file.path),$false)}
  [IO.File]::Copy((Join-Path $work 'report.ready.json'),(Join-Path $SharedReportDirectory 'report.ready.json'),$false)
 }
}
if(-not $report.passed){exit 1}
