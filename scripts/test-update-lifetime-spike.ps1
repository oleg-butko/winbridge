<# Two local-process OS experiments. No networking, install or Job limit changes. #>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory,
  [switch]$BootstrapExclusion,
  [switch]$KeeperProbe,
  [switch]$BootstrapFiles,
  [string]$SourceArchiveName
)
$ErrorActionPreference='Stop';Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
if(([int][bool]$BootstrapExclusion+[int][bool]$KeeperProbe+[int][bool]$BootstrapFiles) -gt 1){throw 'Choose one native profile'}
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($path in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $path}
if($SourceArchiveSha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Pinned source archive SHA required'}
$source=[IO.Path]::GetFullPath($SourceDirectory);$work=Split-Path $source -Parent
if(Test-Path -LiteralPath $ReportDirectory){throw 'Existing lifetime report preserved'}
New-Item -ItemType Directory -Path $ReportDirectory|Out-Null
$spikeReport=[ordered]@{schemaVersion=1;kind='activation_lifetime_keeper_native_gate';platform='windows';passed=$false;sourceArchiveSha256=$SourceArchiveSha256;runtimeApplied=$false;installedRuntimeApplied=$false;windowsOpened=$false;networkStarted=$false;error=$null;commands=@();storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
if($BootstrapExclusion){$spikeReport.kind='activation_bootstrap_exclusion_native_gate'}
if($KeeperProbe){$spikeReport.kind='keeper_probe_native_gate'}
if($BootstrapFiles){$spikeReport.kind='bootstrap_files_native_gate'}
$lease=$null;$location=Get-Location
$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL;$oldInput=$env:WINBRIDGE_LIFETIME_SPIKE_INPUT
$oldProbeProof=$env:WINBRIDGE_KEEPER_PROBE_NATIVE_PROOF;$oldFilesInput=$env:WINBRIDGE_BOOTSTRAP_FILES_INPUT
$oldTemp=$env:TEMP;$oldTmp=$env:TMP
$oldEncoding=[Console]::OutputEncoding;$oldPipelineEncoding=$OutputEncoding
$casesRoot=Join-Path $work 'cases'
try {
  if($env:OS -cne 'Windows_NT' -or $spikeReport.storage.availableBytesBefore -lt 6GB){throw 'Windows and 6 GiB free required'}
  $manifestPath=Join-Path $source 'winbridge-source-manifest.json';Assert-SessionFile $manifestPath
  $manifest=Get-Content -LiteralPath $manifestPath -Raw|ConvertFrom-Json
  if($manifest.schemaVersion -ne 1 -or $manifest.dirty){throw 'Clean source manifest required'}
  foreach($entry in $manifest.files){
    $path=Join-Path $source $entry.path;Assert-SessionFile $path
    if((Get-Item -LiteralPath $path).Length -ne $entry.bytes -or (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.sha256){throw "Source mismatch: $($entry.path)"}
  }
  $spikeReport.sourceCommit=$manifest.sourceCommit
  $spikeReport.sourceManifestSha256=(Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
  if(Test-Path -LiteralPath $casesRoot){throw 'Existing lifetime cases preserved'}
  New-Item -ItemType Directory -Path $casesRoot|Out-Null
  $inputPath=Join-Path $work 'lifetime-input.json';Write-NewJson $inputPath @{schemaVersion=1;root=$casesRoot}
  $env:WINBRIDGE_LIFETIME_SPIKE_INPUT=$inputPath
  $env:CARGO_TARGET_DIR=Join-Path $ValidationCacheDirectory 'test-build-cache';Assert-WorkflowCachePath $env:CARGO_TARGET_DIR
  if(-not(Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){throw 'Explicit retained build cache required'}
  $lease=[IO.File]::Open((Join-Path $env:CARGO_TARGET_DIR '.winbridge-build-cache.lock'),[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  $env:CARGO_INCREMENTAL='0';$env:TEMP=Join-Path $work 'lifetime-temp';$env:TMP=$env:TEMP
  New-Item -ItemType Directory -Path $env:TEMP|Out-Null
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  Set-Location -LiteralPath $source
  $spikeCases=@(
    @{name='two-generation';test='resident_keeper_launches_next_updater_outside_old_worker_job_without_breakaway'},
    @{name='keeper-loss';test='keeper_process_loss_kills_waiting_worker_without_recovery_or_limit_changes'}
  )
  if($BootstrapExclusion){$spikeCases=@(@{name='pinned-process-exclusion';exact='infrastructure::tests::pinned_process_requires_exact_live_identity_and_observes_owned_exit'})+$spikeCases}
  if($KeeperProbe){
    $spikeCases=@(@{name='keeper-probe';probe='supervisor::keeper_probe::tests::native::original_child_probe_uses_real_peer_and_exact_handles'})
    $env:WINBRIDGE_KEEPER_PROBE_NATIVE_PROOF=Join-Path $ReportDirectory 'keeper-probe-proof.json'
  }
  if($BootstrapFiles){
    if([string]::IsNullOrEmpty($SourceArchiveName) -or $SourceArchiveName.Contains('/') -or $SourceArchiveName.Contains('\') -or $SourceArchiveName.Contains(':')){throw 'Exact source archive basename required'}
    $filesInput=Join-Path $work 'bootstrap-files-input.json'
    Write-NewJson $filesInput @{source=@{archiveName=$SourceArchiveName;archiveSha256=$SourceArchiveSha256;manifestSha256=$spikeReport.sourceManifestSha256;sourceCommit=$manifest.sourceCommit};candidate=(Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-update.exe');proof=(Join-Path $ReportDirectory 'bootstrap-files-proof.json')}
    $env:WINBRIDGE_BOOTSTRAP_FILES_INPUT=$filesInput
    $spikeCases=@(@{name='bootstrap-files-build';build=$true},@{name='bootstrap-files';files='bootstrap::files::native::fresh_helper_and_config_pins_allow_only_the_intended_file_access'})
  }
  foreach($spikeCase in $spikeCases){
    $arguments=@()
    if(-not $spikeCase.ContainsKey('exact') -and -not $spikeCase.ContainsKey('probe') -and -not $BootstrapFiles){$arguments=@('test','-p','winbridge-platform','--lib','--locked','--offline','--','--exact',('lifetime_spike::'+$spikeCase.test),'--ignored','--test-threads=1','--nocapture')}
    if($spikeCase.ContainsKey('exact')){$arguments=@('test','-p','winbridge-platform','--lib','--locked','--offline','--','--exact',$spikeCase.exact,'--test-threads=1','--nocapture')}
    if($spikeCase.ContainsKey('probe')){$arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','--locked','--offline','--','--exact',$spikeCase.probe,'--ignored','--test-threads=1','--nocapture')}
    if($spikeCase.ContainsKey('build')){$arguments=@('build','-p','winbridge-update','--features','bundled-sqlite','--bin','winbridge-update','--locked','--offline')}
    if($spikeCase.ContainsKey('files')){$arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','--locked','--offline','--','--exact',$spikeCase.files,'--ignored','--test-threads=1','--nocapture')}
    $started=[DateTimeOffset]::UtcNow;$log=$spikeCase.name+'.log'
    Write-Output ("Lifetime OS experiment: "+$spikeCase.name+"; installed worker stays active; no windows or networking")
    & cargo @arguments 2>&1|Tee-Object -FilePath (Join-Path $ReportDirectory $log)|Out-Host
    $code=$LASTEXITCODE
    $spikeReport.commands+=@{executable='cargo';name=$spikeCase.name;arguments=$arguments;exitCode=$code;startedAt=$started.ToString('o');elapsedMs=[long]([DateTimeOffset]::UtcNow-$started).TotalMilliseconds;log=$log}
    if($code -ne 0){throw ('Lifetime experiment failed: '+$spikeCase.name+'; preserve original cases')}
    $text=Get-Content -LiteralPath (Join-Path $ReportDirectory $log) -Raw
    if(-not $spikeCase.ContainsKey('build') -and $text -notmatch 'test result: ok\. 1 passed; 0 failed; 0 ignored;'){throw 'Exact native experiment was not selected'}
    if($BootstrapFiles -and $spikeCase.ContainsKey('files')){Assert-SessionFile (Join-Path $ReportDirectory 'bootstrap-files-proof.json')}
    if($KeeperProbe){Assert-SessionFile $env:WINBRIDGE_KEEPER_PROBE_NATIVE_PROOF}
    if(-not $spikeCase.ContainsKey('exact') -and -not $KeeperProbe -and -not $BootstrapFiles){Assert-SessionFile (Join-Path $casesRoot ($spikeCase.name+'/proof.json'))}
  }
  $spikeReport.passed=$true
} catch {$spikeReport.error=$_.Exception.Message}
finally {
  Set-Location -LiteralPath $location
  if($lease){$lease.Dispose()}
  $env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental;$env:WINBRIDGE_LIFETIME_SPIKE_INPUT=$oldInput
  $env:TEMP=$oldTemp;$env:TMP=$oldTmp;$env:WINBRIDGE_KEEPER_PROBE_NATIVE_PROOF=$oldProbeProof;$env:WINBRIDGE_BOOTSTRAP_FILES_INPUT=$oldFilesInput
  [Console]::OutputEncoding=$oldEncoding;$OutputEncoding=$oldPipelineEncoding
  try { foreach($mode in @('two-generation','keeper-loss')){
    $directory=Join-Path $casesRoot $mode
    if(Test-Path -LiteralPath $directory -PathType Container){
      foreach($file in (Get-ChildItem -LiteralPath $directory -File)){
        $knownJson=@('metadata','task-domain','task-leaf','task','cancelled-task','old-worker','old-keeper','start-next','request-next','start-next-keeper','new-worker','new-keeper','survived','stop-old-worker','old-keeper-done','stop-new-worker','new-keeper-done','crash-old-keeper','proof')
        $knownNames=@($knownJson|ForEach-Object {$_.ToString()+'.json';$_.ToString()+'.json.ready'})
        $knownNames+=@('old-keeper','old-worker','new-keeper','new-worker','task')|ForEach-Object {$_.ToString()+'.stdout';$_.ToString()+'.stderr'}
        if($knownNames -cnotcontains $file.Name -or $file.Length -gt 65536){throw 'Unexpected diagnostic preserved locally'}
        Assert-SessionFile $file.FullName
        [IO.File]::Copy($file.FullName,(Join-Path $ReportDirectory ($mode+'-'+$file.Name)),$false)
      }
    }
  }} catch {$spikeReport.passed=$false;$spikeReport.error=([string]$spikeReport.error+'; diagnostic capture: '+$_.Exception.Message);Write-Output $spikeReport.error}
  $spikeReport.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $spikeReport
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File|Sort-Object Name|ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
if(-not $spikeReport.passed){exit 1}
