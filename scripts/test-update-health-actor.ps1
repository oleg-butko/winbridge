<#
.SYNOPSIS
Run the isolated full-candidate health actor gate; never install or open desktop.
#>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$TargetArchivePath,
  [Parameter(Mandatory)][string]$TargetArchiveSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory,
  [switch]$DrainGate,
  [switch]$ShutdownMainGate,
  [switch]$ShutdownMainSupplemental,
  [switch]$FinalStateBackupGate,
  [switch]$FinalStateBackupFaults,
  [switch]$RuntimePreparationGate,
  [switch]$RuntimePreparationFaults,
  [switch]$RuntimeReplacementGate,
  [string]$PreviousUpdaterExecutable,
  [string]$PreviousUpdaterSha256
)
$ErrorActionPreference='Stop'
if($RuntimeReplacementGate){if($RuntimePreparationGate -or $RuntimePreparationFaults -or $DrainGate -or $ShutdownMainGate -or $ShutdownMainSupplemental -or $FinalStateBackupGate -or $FinalStateBackupFaults){throw 'Select one native gate mode'};$RuntimePreparationGate=$true}
if($RuntimePreparationFaults){if($RuntimePreparationGate){throw 'Select one native gate mode'};$RuntimePreparationGate=$true}
if($RuntimePreparationGate){if($DrainGate -or $ShutdownMainSupplemental -or $FinalStateBackupGate -or $FinalStateBackupFaults){throw 'Select one native gate mode'};$FinalStateBackupGate=$true}
if($FinalStateBackupFaults){if($DrainGate -or $ShutdownMainSupplemental -or $FinalStateBackupGate){throw 'Select one native gate mode'};$FinalStateBackupGate=$true}
if($ShutdownMainSupplemental){$ShutdownMainGate=$true}
if($FinalStateBackupGate){if($DrainGate -or $ShutdownMainSupplemental){throw 'Select one native gate mode'};$ShutdownMainGate=$true}
if($DrainGate -and $ShutdownMainGate){throw 'Select one native gate mode'}
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($path in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $path}
foreach($sha in @($SourceArchiveSha256,$TargetArchiveSha256)){if($sha -cnotmatch '^[0-9a-f]{64}$'){throw 'Exact source archive SHA required'}}
$source=[IO.Path]::GetFullPath($SourceDirectory)
$cache=[IO.Path]::GetFullPath($ValidationCacheDirectory)
$manifestPath=Join-Path $source 'winbridge-source-manifest.json'
$manifest=Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if($manifest.dirty -or -not $manifest.includesBuiltUi){throw 'Clean exported source with built UI required'}
New-Item -ItemType Directory -Path $ReportDirectory -Force | Out-Null
$report=[ordered]@{schemaVersion=1;kind='candidate_health_native_gate';platform='windows';passed=$false;sourceCommit=$manifest.sourceCommit;sourceArchiveSha256=$SourceArchiveSha256;commands=@();runtimeApplied=$(if($RuntimeReplacementGate){$null}else{$false});installedRuntimeApplied=$false;windowsOpened=$false;error=$null;storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
$oldShutdownInput=$env:WINBRIDGE_SHUTDOWN_GATE_INPUT
$oldPreviousUpdater=$env:WINBRIDGE_PREVIOUS_UPDATER10
$oldTarget=$env:CARGO_TARGET_DIR;$oldIncremental=$env:CARGO_INCREMENTAL;$oldInput=$env:WINBRIDGE_HEALTH_GATE_INPUT;$oldProof=$env:WINBRIDGE_HEALTH_GATE_PROOF
$oldConsoleEncoding=[Console]::OutputEncoding;$oldPipelineEncoding=$OutputEncoding
$oldTemp=$env:TEMP;$oldTmp=$env:TMP;$oldLocation=Get-Location
$lease=$null
try {
  if($env:OS -cne 'Windows_NT' -or $report.storage.availableBytesBefore -lt 6GB){throw 'Windows local disk with at least 6 GiB required; no cleanup'}
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  foreach($f in $manifest.files){$p=Join-Path $source $f.path;Assert-SessionFile $p;if((Get-Item -LiteralPath $p).Length -ne $f.bytes -or (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() -cne $f.sha256){throw "Source manifest mismatch: $($f.path)"}}
  Assert-SessionFile $TargetArchivePath
  if((Get-FileHash -LiteralPath $TargetArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() -cne $TargetArchiveSha256){throw 'Target archive SHA mismatch'}
  $zip=[IO.Compression.ZipFile]::OpenRead($TargetArchivePath)
  try {
    $entry=$zip.GetEntry('winbridge/winbridge-source-manifest.json');if(-not $entry -or $entry.Length -gt 4MB){throw 'Bounded target source manifest required'}
    $stream=$entry.Open();$memory=[IO.MemoryStream]::new()
    try {$stream.CopyTo($memory);$bytes=$memory.ToArray()} finally {$stream.Dispose();$memory.Dispose()}
  } finally {$zip.Dispose()}
  $targetManifest=[Text.Encoding]::UTF8.GetString($bytes) | ConvertFrom-Json
  if($targetManifest.dirty -or -not $targetManifest.includesBuiltUi -or $targetManifest.sourceCommit -ceq $manifest.sourceCommit){throw 'Distinct clean target generation with built UI required'}
  $pin=@{archiveName=[IO.Path]::GetFileName($TargetArchivePath);archiveSha256=$TargetArchiveSha256;manifestSha256=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant();sourceCommit=$targetManifest.sourceCommit}
  $pwsh=(Get-Command pwsh.exe -ErrorAction Stop).Source
  $inputPath=Join-Path (Split-Path $source -Parent) 'health-input.json'
  if($DrainGate){
    Assert-SessionFile $PreviousUpdaterExecutable
    if($PreviousUpdaterSha256 -cnotmatch '^[0-9a-f]{64}$' -or (Get-FileHash -LiteralPath $PreviousUpdaterExecutable -Algorithm SHA256).Hash.ToLowerInvariant() -cne $PreviousUpdaterSha256){throw 'Pinned schema10 updater reference required'}
    $env:WINBRIDGE_PREVIOUS_UPDATER10=[IO.Path]::GetFullPath($PreviousUpdaterExecutable)
    $report.kind='candidate_drain_native_gate'
  }
  Write-NewJson $inputPath @{schemaVersion=1;target=$pin;archive=[IO.Path]::GetFullPath($TargetArchivePath);powershell=$pwsh;validationCache=$cache;drainGate=[bool]$DrainGate}
  $report.target=$pin
  $env:CARGO_TARGET_DIR=Join-Path $cache 'test-build-cache'
  Assert-WorkflowCachePath $env:CARGO_TARGET_DIR
  if(-not (Test-Path -LiteralPath $env:CARGO_TARGET_DIR -PathType Container)){throw 'Retained native test cache required'}
  foreach($p in @((Join-Path $cache 'winbridge/target'),(Join-Path $cache 'winbridge/apps/desktop/src-tauri/target'))){Assert-SessionDirectory $p;if(-not (Test-Path -LiteralPath $p -PathType Container)){throw 'Retained candidate compiler caches required'}}
  $lock=Join-Path $env:CARGO_TARGET_DIR '.winbridge-build-cache.lock';Assert-SessionFile $lock
  $lease=[IO.File]::Open($lock,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  $env:CARGO_INCREMENTAL='0';$env:WINBRIDGE_HEALTH_GATE_INPUT=$inputPath;$env:WINBRIDGE_HEALTH_GATE_PROOF=Join-Path $ReportDirectory 'candidate-health-actor-proof.json'
  $env:TEMP=Join-Path (Split-Path $source -Parent) 'test-temp';$env:TMP=$env:TEMP;New-Item -ItemType Directory -Path $env:TEMP | Out-Null
  Set-Location -LiteralPath $source
  $commands=@(
    @{name='health-rehearsal-entry';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','update_rehearsal','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='health-transition-fsm';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','driver_transition_proof','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='health-image-hash';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','marker_detection_never_shortcuts','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='health-atomic-parent';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','health_acceptance_hook','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='health-receipt-contract';arguments=@('test','-p','winbridge-protocol','--test','worker_update_receipt','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='health-supervisor-build';arguments=@('build','-p','winbridge-update','--features','bundled-sqlite','--locked','--offline')},
    @{name='health-worker-receipt';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','command_receipt_lookup','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='health-drain-capacity';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','drain_lets_accepted','--locked','--offline','--','--test-threads=1','--nocapture')},
    @{name='health-full-pipeline';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','update_control::tests::health::health_actor_full_candidate_roundtrip','--locked','--offline','--','--exact','--ignored','--test-threads=1','--nocapture')}
  )
  if($DrainGate){
    $commands=@(
      @{name='drain-data-and-journal';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','drain','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='drain-v13-admission';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','candidate_drain_requires_health','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='drain-v12-worker-observation';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','drain_observation_preserves','--locked','--offline','--','--test-threads=1','--nocapture')}
    )+$commands
  }
  if($ShutdownMainGate){
    $report.kind=if($RuntimeReplacementGate){'runtime_replacement_main_native_gate'}elseif($RuntimePreparationFaults){'runtime_preparation_faults_native_gate'}elseif($RuntimePreparationGate){'runtime_preparation_main_native_gate'}elseif($FinalStateBackupFaults){'state_backup_faults_native_gate'}elseif($FinalStateBackupGate){'state_backup_main_native_gate'}elseif($ShutdownMainSupplemental){'shutdown_main_supplemental_native_gate'}else{'shutdown_main_native_gate'}
    $caseRoot=Join-Path (Split-Path $source -Parent) 'shutdown-main-cases'
    New-Item -ItemType Directory -Path $caseRoot | Out-Null
    $oldStage=Join-Path (Split-Path $source -Parent) 'shutdown-old-set'
    $shutdownInput=Join-Path (Split-Path $source -Parent) 'shutdown-main-input.json'
    $shutdownArguments=@{root=$caseRoot;target=$pin;archive=[IO.Path]::GetFullPath($TargetArchivePath);oldImages=(Join-Path $oldStage 'payload');powershell=$pwsh;validationCache=$cache;supplemental=[bool]$ShutdownMainSupplemental;finalBackup=[bool]$FinalStateBackupGate;backupFaults=[bool]$FinalStateBackupFaults}
    if($RuntimePreparationGate){$shutdownArguments.runtimePreparation=$true}
    if($RuntimeReplacementGate){$shutdownArguments.runtimeReplacement=$true}
    if($RuntimePreparationFaults){$shutdownArguments.preparationFaults=$true;$shutdownArguments.validationUpdater=(Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-update.exe')}
    Write-NewJson $shutdownInput $shutdownArguments
    $env:WINBRIDGE_SHUTDOWN_GATE_INPUT=$shutdownInput
    $mainTest=if($RuntimeReplacementGate){'actual_main_and_supervisor_replace_runtime_without_activation'}else{'actual_main_and_supervisor_confirm_exact_shutdown_without_runtime_replacement'}
    $commands=@(
      @{name='health-supervisor-build';arguments=@('build','-p','winbridge-update','--features','bundled-sqlite','--locked','--offline')},
      @{name='shutdown-old-real-set';executable=$pwsh;arguments=@('-NoProfile','-NonInteractive','-File',(Join-Path $source 'scripts/build-windows-update.ps1'),'-SourceDirectory',$source,'-ManifestSha256',(Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant(),'-StagingDirectory',$oldStage,'-VerifierExecutable',(Join-Path $env:CARGO_TARGET_DIR 'debug/winbridge-update.exe'),'-ValidationCacheDirectory',$cache)},
      @{name='shutdown-main-pipeline';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown','--locked','--offline','--','--exact',$mainTest,'--ignored','--test-threads=1','--nocapture')}
    )
  }
  if($FinalStateBackupGate){
    $commands=@(
      @{name='backup-empty-wal-fingerprint';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown','journal_fingerprint_allows_only_empty_wal_creation_and_detects_data_changes','--locked','--offline','--','--exact','--test-threads=1','--nocapture')},
      @{name='backup-crash-observation';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite,update-validation','--test','update_shutdown','post_crash_observation_retries_only_transient_read_io','--locked','--offline','--','--exact','--test-threads=1','--nocapture')},
      @{name='backup-data-and-ledger';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','state_backup','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='backup-sqlite-wal-and-pins';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','store::archive','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='backup-exact-entry';arguments=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','state_backup_export','--locked','--offline','--','--test-threads=1','--nocapture')}
    )+$commands
  }
  if($RuntimePreparationGate){
    $commands=@(
      @{name='runtime-preparation-driver-and-ledger';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','runtime_preparation','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='runtime-preparation-directory-pins';arguments=@('test','-p','winbridge-platform','--lib','source_files::tests::directory_pin','--locked','--offline','--','--test-threads=1','--nocapture')}
    )+$commands
  }
  if($RuntimeReplacementGate){
    $commands=@(
      @{name='runtime-replacement-model-ledger-and-directory';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','runtime_replacement','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='runtime-replacement-original-custody';arguments=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','custody_busy_restores_same_arc','--locked','--offline','--','--test-threads=1','--nocapture')},
      @{name='runtime-replacement-contract';arguments=@('test','-p','winbridge-protocol','--test','update_health','--locked','--offline','--','--test-threads=1','--nocapture')}
    )+$commands
  }
  # Worker IPC fixtures launch this executable. Reused caches may still hold an
  # older updater that correctly refuses the freshly migrated journal.
  $commands=@($commands | Where-Object {$_.name -ceq 'health-supervisor-build'})+@($commands | Where-Object {$_.name -cne 'health-supervisor-build'})
  if($RuntimePreparationFaults){
    $commands[0].arguments=@('build','-p','winbridge-update','--features','bundled-sqlite,update-validation','--locked','--offline')
  }
  foreach($command in $commands){
    $log=$command.name+'.log';$arguments=$command.arguments
    Write-Output "Health native gate: $($command.name); installed runtime remains active"
    $executable=if($command.ContainsKey('executable')){$command.executable}else{'cargo.exe'}
    & $executable @arguments 2>&1 | Tee-Object -FilePath (Join-Path $ReportDirectory $log)
    $code=$LASTEXITCODE;$report.commands+=@{name=$command.name;arguments=$arguments;exitCode=$code;log=$log}
    if($code -ne 0){throw "Health native command failed: $($command.name); see $log"}
    if($RuntimePreparationGate -and $command.name -in @('runtime-preparation-driver-and-ledger','runtime-preparation-directory-pins')){
      $minimum=if($command.name -ceq 'runtime-preparation-driver-and-ledger'){16}else{1}
      $counts=[regex]::Matches((Get-Content -LiteralPath (Join-Path $ReportDirectory $log) -Raw),'test result: ok\. ([0-9]+) passed; 0 failed;')
      $passed=0;foreach($count in $counts){$passed+=[int]$count.Groups[1].Value}
      if($passed -lt $minimum){throw "Expected preparation tests were not selected: $($command.name)"}
    }
    if($RuntimeReplacementGate -and $command.name -like 'runtime-replacement-*'){
      $minimum=if($command.name -ceq 'runtime-replacement-model-ledger-and-directory'){15}elseif($command.name -ceq 'runtime-replacement-contract'){3}else{1}
      $counts=[regex]::Matches((Get-Content -LiteralPath (Join-Path $ReportDirectory $log) -Raw),'test result: ok\. ([0-9]+) passed; 0 failed;')
      $passed=0;foreach($count in $counts){$passed+=[int]$count.Groups[1].Value}
      if($passed -lt $minimum){throw "Expected replacement tests were not selected: $($command.name)"}
    }
  }
  if($ShutdownMainGate){[IO.File]::Copy((Join-Path $caseRoot 'proof.json'),$env:WINBRIDGE_HEALTH_GATE_PROOF,$false)}
  $proof=Get-Content -LiteralPath $env:WINBRIDGE_HEALTH_GATE_PROOF -Raw | ConvertFrom-Json
  $expectedKind=if($RuntimeReplacementGate){'runtime_replacement_main_native_proof'}elseif($RuntimePreparationFaults){'runtime_preparation_faults_native_proof'}elseif($RuntimePreparationGate){'runtime_preparation_main_native_proof'}elseif($FinalStateBackupFaults){'state_backup_faults_native_proof'}elseif($FinalStateBackupGate){'state_backup_main_native_proof'}elseif($ShutdownMainSupplemental){'shutdown_main_supplemental_proof'}elseif($ShutdownMainGate){'shutdown_main_native_proof'}elseif($DrainGate){'candidate_drain_actor_proof'}else{'candidate_health_actor_proof'}
  $expectedCases=if($RuntimePreparationFaults){3}elseif($FinalStateBackupFaults){4}elseif($FinalStateBackupGate){1}elseif($ShutdownMainSupplemental){2}else{4}
  if($proof.kind -cne $expectedKind -or $proof.passed -ne $true -or $proof.runtimeApplied -ne [bool]$RuntimeReplacementGate -or $proof.windowsOpened -ne $false -or $proof.cases.Count -ne $expectedCases){throw 'Actual health actor proof incomplete'}
  if($RuntimeReplacementGate){if($proof.installedRuntimeApplied -ne $false){throw 'Installed runtime must remain unchanged'};$report.runtimeApplied=$proof.runtimeApplied}
  $report.passed=$true
} catch {$report.error=$_.Exception.Message;Write-Output ("Health gate failed: "+$report.error)} finally {
  try { if($ShutdownMainGate -and (Get-Variable caseRoot -ErrorAction SilentlyContinue)){
    foreach($caseName in @('None','LostAck','DelayedReply','ReceiptWriteDenied','DelayedInstanceLock','SupervisorCrashBeforeExit','None-ExistingDestination','None-ExitAfterCopy','None-WrongInputReport','None-HoldBeforeCopy','None-PreparationExistingDirectory','None-PreparationChangedCandidate','None-PreparationHoldBeforeReceipt')){
      $caseDirectory=Join-Path $caseRoot $caseName
      foreach($logName in @('stdout.log','stderr.log','validation-error.json','lock-helper-stdout.log','lock-helper-stderr.log','backup-helper-fault.json','backup-helper-held.json','pre-crash-proof.json')){
        $path=Join-Path $caseDirectory $logName
        if(Test-Path -LiteralPath $path -PathType Leaf){[IO.File]::Copy($path,(Join-Path $ReportDirectory ($caseName+'-'+$logName)),$false)}
      }
      $saved=Join-Path $caseDirectory 'supervisor.json'
      if(Test-Path -LiteralPath $saved -PathType Leaf){
        $sup=Get-Content -LiteralPath $saved -Raw | ConvertFrom-Json
        $updateId=$sup.spec.updateId
        if($updateId -cmatch '^[0-9a-f-]{36}$'){
          $path=Join-Path $caseDirectory ('runtime/update/'+$updateId+'/supervisor/stderr.log')
          if(Test-Path -LiteralPath $path -PathType Leaf){[IO.File]::Copy($path,(Join-Path $ReportDirectory ($caseName+'-supervisor-stderr.log')),$false)}
        }
      }
    }
  }
  } catch {Write-Output ('Optional case diagnostic export failed: '+$_.Exception.Message)}
  $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  if($lease){$lease.Dispose()}
  $env:WINBRIDGE_SHUTDOWN_GATE_INPUT=$oldShutdownInput
  $env:WINBRIDGE_PREVIOUS_UPDATER10=$oldPreviousUpdater;$env:CARGO_TARGET_DIR=$oldTarget;$env:CARGO_INCREMENTAL=$oldIncremental;$env:WINBRIDGE_HEALTH_GATE_INPUT=$oldInput;$env:WINBRIDGE_HEALTH_GATE_PROOF=$oldProof;$env:TEMP=$oldTemp;$env:TMP=$oldTmp
  [Console]::OutputEncoding=$oldConsoleEncoding;$OutputEncoding=$oldPipelineEncoding
  Set-Location -LiteralPath $oldLocation
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File | Sort-Object Name | ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
if(-not $report.passed){exit 1}
