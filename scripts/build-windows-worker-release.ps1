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
 [Parameter(Mandatory)][string]$InstalledConfigSha256
)
$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue'
$PSNativeCommandUseErrorActionPreference=$false
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/validation-storage.ps1')
$work=[IO.Path]::GetFullPath($WorkDirectory)
$cache=[IO.Path]::GetFullPath($CacheDirectory)
foreach($path in @($work,$cache)){Assert-SessionDirectory $path;Assert-WorkflowCachePath $path;if([IO.DriveInfo]::new([IO.Path]::GetPathRoot($path)).DriveType -ne [IO.DriveType]::Fixed){throw 'Local fixed disk required'}}
if($env:OS -cne 'Windows_NT' -or (Test-Path -LiteralPath $work) -or
 (Split-Path $work -Leaf) -notmatch '^worker-0\.1\.1-\d{8}-\d{6}-[a-f0-9]{8}$' -or
 (Split-Path $cache -Leaf) -cne 'test-build-cache' -or -not (Test-Path -LiteralPath $cache -PathType Container)){throw 'New local release directory and existing compiler cache required'}
if(@(Get-Process cargo -ErrorAction SilentlyContinue).Count -or
 @(Get-ChildItem Env:|Where-Object {$_.Name -match '^(RUSTFLAGS|CARGO_ENCODED_RUSTFLAGS|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CARGO_PROFILE_.*|WINBRIDGE_.*)$'}).Count){throw 'Requires idle compiler and unmodified environment'}
foreach($pair in @(@($SourceArchive,$ArchiveSha256),@($VerifierExecutable,$VerifierSha256),@($BaselineWorker,$BaselineSha256),@($InstalledConfig,$InstalledConfigSha256))){
 Assert-SessionFile $pair[0]
 if((Get-FileHash -LiteralPath $pair[0]).Hash.ToLowerInvariant() -cne $pair[1]){throw 'Pinned input differs'}
}
$leasePath=Join-Path $cache '.winbridge-build-cache.lock';Assert-SessionFile $leasePath
$lease=[IO.File]::Open($leasePath,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
$source=Join-Path $work 'tmp-build/source/winbridge'
$report=[ordered]@{schemaVersion=1;kind='worker_patch_release';passed=$false;version='0.1.1';sourceCommit=$SourceCommit;
 manifestSha256=$ManifestSha256;archiveSha256=$ArchiveSha256;installedRuntimeChanged=$false;windowsOpened=$false;
 profile='dev; existing package optimizations; debug assertions and overflow checks enabled';commands=@();artifacts=@();cpu=@();error=$null}
$saved=@{};foreach($name in @('CARGO_INCREMENTAL','CARGO_TERM_COLOR','TEMP','TMP','WINBRIDGE_IDLE_BASELINE_WORKER')){$saved[$name]=[Environment]::GetEnvironmentVariable($name)}
$location=Get-Location;$child=$null;$out=$null;$err=$null
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
  if(-not $p.WaitForExit(15000)){$p.Kill();$p.WaitForExit();throw 'Worker client timeout; do not retry mutations'}
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
 $env:CARGO_INCREMENTAL='0';$env:CARGO_TERM_COLOR='never'
 $env:TEMP=Join-Path $work 'tmp-build/tmp-test-temp';$env:TMP=$env:TEMP;New-Item -ItemType Directory -Path $env:TEMP|Out-Null
 Set-Location $source
 $common=@('test','-p','winbridge-worker','--locked','--offline','--features','bundled-sqlite','--target-dir',$cache)
 # Network timing fixtures and real process ownership are validated serially,
 # matching the established native workflow harness; do not saturate their clocks.
 Invoke-Check 'worker-regressions' ($common+@('--lib','--test','idle_scheduler','--test','runtime','--test','leases','--test','queue_control','--test','resource_scheduling','--test','task_fsm','--test','work_scheduling','--test','workflows','--','--test-threads=1'))
 $env:WINBRIDGE_IDLE_BASELINE_WORKER=$BaselineWorker
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
 if($old.kind -cne 'info' -or $old.info.build.version -cne '0.1.0'){throw 'New CLI failed against old installed worker'}
 $report.installedGeneration=$old.info.generation
 $network=Join-Path $work 'tmp-build/network';$project=Join-Path $network 'project'
 New-Item -ItemType Directory -Path $project|Out-Null
 [IO.File]::Copy((Join-Path $cache 'debug/winbridge-fixture.exe'),(Join-Path $project 'winbridge-fixture.exe'),$false)
 $configPath=Join-Path $network 'worker.json'
 $config=@{schemaVersion=1;instanceId=$InstanceId;nodeId='release-worker';stateDirectory=(Join-Path $network 'state');worker=@{controllerUrl=$ControllerUrl;keepRunningOnUiExit=$true};
  projects=@(@{id='fixture';workspaceRoot=$project;allowDirectCommands=$true;maxConcurrentMutations=1;recipes=@(@{id='fixture-success';executable=(Join-Path $project 'winbridge-fixture.exe');arguments=@('success');workingDirectory='.';timeoutMs=5000})})}
 Write-NewJson $configPath $config
 $si=[Diagnostics.ProcessStartInfo]::new($image);$si.UseShellExecute=$false;$si.CreateNoWindow=$true;$si.RedirectStandardOutput=$true;$si.RedirectStandardError=$true
 $si.StandardOutputEncoding=[Text.UTF8Encoding]::new($false);$si.StandardErrorEncoding=[Text.UTF8Encoding]::new($false)
 foreach($arg in @('--config',$configPath,'serve')){$si.ArgumentList.Add($arg)}
 $si.Environment['WINBRIDGE_SCHEDULER_DIAGNOSTICS']='1'
 $child=[Diagnostics.Process]::new();$child.StartInfo=$si;[void]$child.Start()
 $out=$child.StandardOutput.ReadToEndAsync();$err=$child.StandardError.ReadToEndAsync()
 Write-Host 'WinBridge worker release: waiting for old-controller/new-worker RPC check'
 $deadline=[DateTime]::UtcNow.AddSeconds(90);$info=$null
 do{
  if($child.HasExited){throw 'Isolated worker exited before network validation'}
  try{$reply=Invoke-WorkerJson @('--config',$configPath,'info');$info=$reply.info}catch{Start-Sleep -Milliseconds 200;continue}
  if($info.controllerConnection.state -ceq 'connected'){break}
  Start-Sleep -Milliseconds 200
 }while([DateTime]::UtcNow -lt $deadline)
 if($null -eq $info -or $info.controllerConnection.state -cne 'connected'){throw 'Old controller connection failed; see isolated worker stderr; no firewall changes attempted'}
 Write-NewJson (Join-Path $SharedReportDirectory 'network-ready.json') @{version='0.1.1';sourceCommit=$SourceCommit;instanceId=$InstanceId;context=$info.fencedRequests.context}
 $donePath=Join-Path $SharedReportDirectory 'network-complete.json'
 while(-not (Test-Path -LiteralPath $donePath)){
  if($child.HasExited -or [DateTime]::UtcNow -ge $deadline){throw 'Cross-version RPC validation incomplete'}
  Start-Sleep -Milliseconds 200
 }
 $done=Get-Content -Raw -LiteralPath $donePath|ConvertFrom-Json
 if($done.passed -isnot [bool] -or -not $done.passed -or $done.context.journalIncarnation -cne $info.fencedRequests.context.journalIncarnation){throw 'Cross-version RPC proof differs'}
 $report.network=$done
 $last=Invoke-WorkerJson @('--config',$configPath,'info');if($last.info.activeTaskCount -ne 0){throw 'Isolated tasks still active'}
 $child.Kill();$child.WaitForExit();$report.isolatedWorkerExitConfirmed=$true
 [IO.File]::WriteAllText((Join-Path $work 'network-worker-stdout.log'),$out.GetAwaiter().GetResult());[IO.File]::WriteAllText((Join-Path $work 'network-worker-stderr.log'),$err.GetAwaiter().GetResult())
 $child.Dispose();$child=$null
 $readback=@(& $VerifierExecutable verify-source $source $ManifestSha256);if($LASTEXITCODE -ne 0){throw 'Source changed after build'}
 if((Get-FileHash -LiteralPath $InstalledConfig).Hash.ToLowerInvariant() -cne $InstalledConfigSha256){throw 'Installed config changed during validation'}
 if((Get-FileHash -LiteralPath $image).Hash.ToLowerInvariant() -cne $q.image.sha256){throw 'Frozen worker changed'}
 $report.artifacts=@(@{name='winbridge-worker.exe';bytes=(Get-Item -LiteralPath $image).Length;sha256=$q.image.sha256})
 $report.passed=$true
}catch{$report.error=$_.Exception.Message;Write-Host "Worker release failed: $($report.error)"}
finally{
 if($null -ne $child){if(-not $child.HasExited){$child.Kill();$child.WaitForExit()};[IO.File]::WriteAllText((Join-Path $work 'network-worker-stderr.log'),$err.GetAwaiter().GetResult());$child.Dispose()}
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
