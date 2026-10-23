<#
.SYNOPSIS
Test exact process/state handles and old-reader refusal. No installed update or UI.
#>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory,
  [Parameter(Mandatory)][string]$ReferenceReceipt,
  [switch]$ColdReaderOnly,
  [switch]$BackupMigrationOnly,
  [switch]$RuntimeFileMovesOnly,
  [switch]$RuntimePreparationOnly,
  [switch]$RuntimeReplacementOnly,
  [switch]$PreparationMigrationOnly
)
$ErrorActionPreference='Stop'
if(@($ColdReaderOnly,$BackupMigrationOnly,$RuntimeFileMovesOnly,$RuntimePreparationOnly,$RuntimeReplacementOnly,$PreparationMigrationOnly).Where({[bool]$_}).Count -gt 1){throw 'Select one gate mode'}
Set-StrictMode -Version Latest
$PSNativeCommandUseErrorActionPreference=$false
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
foreach($p in @($SourceDirectory,$ValidationCacheDirectory,$ReportDirectory)){Assert-SessionDirectory $p}
Assert-SessionFile $ReferenceReceipt
if($SourceArchiveSha256 -cnotmatch '^[0-9a-f]{64}$'){throw 'Pinned archive SHA required'}
$source=[IO.Path]::GetFullPath($SourceDirectory)
$cache=Join-Path ([IO.Path]::GetFullPath($ValidationCacheDirectory)) 'test-build-cache'
Assert-WorkflowCachePath $cache
if(-not (Test-Path -LiteralPath $cache -PathType Container)){throw 'Existing approved test cache required'}
$manifest=Get-Content -LiteralPath (Join-Path $source 'winbridge-source-manifest.json') -Raw | ConvertFrom-Json
if($manifest.dirty){throw 'Clean exported source required'}
$kind=if($RuntimeReplacementOnly){'runtime_replacement_model_native_gate'}elseif($PreparationMigrationOnly){'runtime_preparation_migration_native_gate'}elseif($RuntimePreparationOnly){'runtime_preparation_native_gate'}elseif($RuntimeFileMovesOnly){'runtime_file_moves_native_gate'}elseif($BackupMigrationOnly){'state_backup_migration_native_gate'}elseif($ColdReaderOnly){'shutdown_reader_native_gate'}else{'shutdown_primitives_native_gate'}
$report=[ordered]@{schemaVersion=1;kind=$kind;platform='windows';passed=$false;sourceCommit=$manifest.sourceCommit;sourceArchiveSha256=$SourceArchiveSha256;runtimeApplied=$false;windowsOpened=$false;commands=@();error=$null;storage=@{availableBytesBefore=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace}}
$variables=@('CARGO_TARGET_DIR','CARGO_INCREMENTAL','TEMP','TMP','WINBRIDGE_SHUTDOWN_REFERENCE_WORKER','WINBRIDGE_UPDATE_REFERENCE_UPDATER','WINBRIDGE_STATE_BACKUP_MIGRATION_INPUT','WINBRIDGE_RUNTIME_PREPARATION_MIGRATION_INPUT')
$old=@{};foreach($name in $variables){$old[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
$oldLocation=Get-Location;$oldEncoding=[Console]::OutputEncoding;$oldOutputEncoding=$OutputEncoding;$lease=$null
try {
  if($env:OS -cne 'Windows_NT' -or $report.storage.availableBytesBefore -lt 6GB){throw 'Windows local disk with >=6 GiB required; no cleanup'}
  [Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);$OutputEncoding=[Text.UTF8Encoding]::new($false)
  foreach($f in $manifest.files){
    $p=Join-Path $source $f.path;Assert-SessionFile $p
    if((Get-Item -LiteralPath $p).Length -ne $f.bytes -or (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() -cne $f.sha256){throw "Source mismatch: $($f.path)"}
  }
  $refs=Get-Content -LiteralPath $ReferenceReceipt -Raw | ConvertFrom-Json
  if($PreparationMigrationOnly){
    if($refs.kind -cne 'preparation_schema13_reference'){throw 'Proven schema13 final backup reference required'}
    Assert-SessionDirectory $refs.runtimeDirectory
    $refManifest=Join-Path $refs.runtimeDirectory 'winbridge-installed-set.json';Assert-SessionFile $refManifest
    if((Get-FileHash -LiteralPath $refManifest -Algorithm SHA256).Hash.ToLowerInvariant() -cne $refs.manifestSha256){throw 'Retained schema13 manifest changed'}
    $installed=Get-Content -LiteralPath $refManifest -Raw | ConvertFrom-Json
    $updaters=@($installed.artifacts | Where-Object {$_.name -ceq 'winbridge-update.exe'})
    if($updaters.Count -ne 1){throw 'Exact retained updater role required'}
    $refUpdater=Join-Path $refs.runtimeDirectory 'winbridge-update.exe';Assert-SessionFile $refUpdater
    if((Get-FileHash -LiteralPath $refUpdater -Algorithm SHA256).Hash.ToLowerInvariant() -cne $updaters[0].sha256){throw 'Retained schema13 updater changed'}
    $migrationRoot=Join-Path (Split-Path $source -Parent) 'preparation-migration-case'
    if(Test-Path -LiteralPath $migrationRoot){throw 'Existing migration preserved'}
    $inputPath=Join-Path (Split-Path $source -Parent) 'preparation-migration-input.json'
    Write-NewJson $inputPath @{root=$migrationRoot;reference=$refs}
    $env:WINBRIDGE_RUNTIME_PREPARATION_MIGRATION_INPUT=$inputPath
  } elseif($RuntimeReplacementOnly){
    if($refs.kind -cne 'runtime_replacement_validation'){throw 'Fixed replacement model fixture declaration required'}
  } elseif($RuntimePreparationOnly){
    if($refs.kind -cne 'runtime_preparation_validation'){throw 'Fixed preparation fixture declaration required'}
  } elseif($RuntimeFileMovesOnly){
    if($refs.kind -cne 'runtime_file_move_validation'){throw 'Fixed file-move fixture declaration required'}
  } elseif($BackupMigrationOnly){
    if($refs.kind -cne 'state_backup_schema12_reference'){throw 'Proven schema12 full history reference required'}
    Assert-SessionDirectory $refs.runtimeDirectory
    $refManifest=Join-Path $refs.runtimeDirectory 'winbridge-installed-set.json'
    $refUpdater=Join-Path $refs.runtimeDirectory 'winbridge-update.exe'
    foreach($p in @($refManifest,$refUpdater)){Assert-SessionFile $p}
    if((Get-FileHash -LiteralPath $refManifest -Algorithm SHA256).Hash.ToLowerInvariant() -cne $refs.manifestSha256 -or (Get-FileHash -LiteralPath $refUpdater -Algorithm SHA256).Hash.ToLowerInvariant() -cne $refs.updaterSha256){throw 'Retained schema12 reference changed'}
    $migrationRoot=Join-Path (Split-Path $source -Parent) 'migration-case'
    if(Test-Path -LiteralPath $migrationRoot){throw 'Existing migration case preserved'}
    $inputPath=Join-Path (Split-Path $source -Parent) 'migration-input.json'
    Write-NewJson $inputPath @{root=$migrationRoot;reference=$refs}
    $env:WINBRIDGE_STATE_BACKUP_MIGRATION_INPUT=$inputPath
  } else {
  if($refs.kind -cne 'shutdown_base_reference' -or $refs.runtimeApplied -ne $false -or $refs.files.Count -ne 2){throw 'Retained schema15/11 receipt required'}
  $env:WINBRIDGE_SHUTDOWN_REFERENCE_WORKER=$null;$env:WINBRIDGE_UPDATE_REFERENCE_UPDATER=$null
  foreach($r in $refs.files){
    Assert-SessionFile $r.path
    if($r.sha256 -cnotmatch '^[0-9a-f]{64}$' -or (Get-FileHash -LiteralPath $r.path -Algorithm SHA256).Hash.ToLowerInvariant() -cne $r.sha256){throw 'Reference bytes changed'}
    $query=& $r.path --artifact-info
    if($LASTEXITCODE -ne 0){throw 'Reference artifact query failed'}
    $actual=$query | ConvertFrom-Json
    foreach($field in @('role','targetOs','targetArch','queryContract','schemaVersion','kind')){if($actual.$field -cne $r.descriptor.$field){throw "Reference descriptor differs: $field"}}
    foreach($field in @('sourceCommit','sourceManifestSha256','buildId','sourceDirty')){if($actual.build.$field -cne $r.descriptor.build.$field){throw "Reference build differs: $field"}}
    if($actual.role -ceq 'worker' -and $r.schemaFromReviewedSource -eq 15){$env:WINBRIDGE_SHUTDOWN_REFERENCE_WORKER=$r.path}
    elseif($actual.role -ceq 'updater' -and $r.schemaFromReviewedSource -eq 11){$env:WINBRIDGE_UPDATE_REFERENCE_UPDATER=$r.path}
    else {throw 'Unexpected reference schema/role'}
  }
  if(-not $env:WINBRIDGE_SHUTDOWN_REFERENCE_WORKER -or -not $env:WINBRIDGE_UPDATE_REFERENCE_UPDATER){throw 'Both exact references required'}
  }
  $lock=Join-Path $cache '.winbridge-build-cache.lock';Assert-SessionFile $lock
  $lease=[IO.File]::Open($lock,[IO.FileMode]::OpenOrCreate,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
  $env:CARGO_TARGET_DIR=$cache;$env:CARGO_INCREMENTAL='0'
  $env:TEMP=Join-Path (Split-Path $source -Parent) 'test-temp';$env:TMP=$env:TEMP
  New-Item -ItemType Directory -Path $env:TEMP | Out-Null
  Set-Location -LiteralPath $source
  $groups=@(
    @{name='shutdown-process-pin';minimum=1;args=@('test','-p','winbridge-platform','--lib','pinned_process_requires_exact_live_identity','--locked','--offline')},
    @{name='shutdown-data-state-journal';minimum=12;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','shutdown','--locked','--offline')},
    @{name='shutdown-worker-acceptance';minimum=4;args=@('test','-p','winbridge-worker','--features','bundled-sqlite','--lib','store::shutdown::tests','--locked','--offline')},
    @{name='shutdown-worker15-refuses16';minimum=1;args=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','update_coordination','previous_worker_fifteen_refuses_current_schema','--locked','--offline','--','--ignored')},
    @{name='shutdown-updater11-refuses12';minimum=1;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--test','source_transaction','previous_updater_refuses_current_schema_without_writes','--locked','--offline','--','--ignored')}
  )
  if($ColdReaderOnly){
    $groups=@(
      @{name='shutdown-reader-cli';minimum=1;args=@('test','-p','winbridge-worker','--features','bundled-sqlite','--test','shutdown_inspection','--locked','--offline')},
      @{name='shutdown-reader-report';minimum=1;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','cold_report_is_exact_readonly_history','--locked','--offline')},
      @{name='shutdown-reader-v6-contract';minimum=2;args=@('test','-p','winbridge-protocol','--features','jsonschema','--test','worker_shutdown_receipt','--locked','--offline')}
    )
  }
  if($BackupMigrationOnly){
    $groups=@(@{name='backup-schema12-history-migration';minimum=1;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--test','state_backup_migration','archived_schema_twelve_shutdown_migrates_without_adopting_custody_and_old_twelve_refuses','--locked','--offline','--','--exact','--ignored')})
  }
  if($RuntimeFileMovesOnly){
    $groups=@(
      @{name='runtime-owned-file-moves';minimum=3;args=@('test','-p','winbridge-platform','--lib','runtime_files::tests','--locked','--offline')},
      @{name='runtime-file-exchange-data';minimum=6;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','runtime_exchange','--locked','--offline')}
    )
  }
  if($RuntimePreparationOnly){
    $groups=@(@{name='runtime-preparation-data-ledger';minimum=10;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','runtime_preparation','--locked','--offline')})
  }
  if($RuntimeReplacementOnly){
    $groups=@(
      @{name='runtime-file-exchange-data';minimum=6;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','runtime_exchange','--locked','--offline')},
      @{name='runtime-replacement-transaction-data';minimum=6;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--lib','runtime_replacement','--locked','--offline')}
    )
  }
  if($PreparationMigrationOnly){
    $groups=@(@{name='preparation-schema13-history-migration';minimum=1;args=@('test','-p','winbridge-update','--features','bundled-sqlite','--test','runtime_preparation_migration','actual_thirteen_backup_history_survives_fourteen_and_old_thirteen_refuses','--locked','--offline','--','--exact','--ignored')})
  }
  foreach($g in $groups){
    $log=Join-Path $ReportDirectory ($g.name+'.log')
    $arguments=@($g.args)
    if($arguments -notcontains '--'){$arguments+=@('--')}
    $arguments+=@('--test-threads=1','--nocapture')
    Write-Output "Isolated shutdown check: $($g.name); installed worker stays running"
    & cargo.exe @arguments 2>&1 | Tee-Object -FilePath $log
    $code=$LASTEXITCODE
    $matches=[regex]::Matches((Get-Content -LiteralPath $log -Raw),'test result: ok\. ([0-9]+) passed; 0 failed;')
    $passed=0;foreach($m in $matches){$passed+=[int]$m.Groups[1].Value}
    $report.commands+=@{name=$g.name;exitCode=$code;passed=$passed;minimum=$g.minimum;log=[IO.Path]::GetFileName($log)}
    if($code -ne 0 -or $passed -lt $g.minimum){throw "Native group failed or selected no expected tests: $($g.name)"}
  }
  if($BackupMigrationOnly){
    [IO.File]::Copy((Join-Path $migrationRoot 'migration-proof.json'),(Join-Path $ReportDirectory 'state-backup-migration-proof.json'),$false)
  }
  if($PreparationMigrationOnly){
    [IO.File]::Copy((Join-Path $migrationRoot 'migration-proof.json'),(Join-Path $ReportDirectory 'runtime-preparation-migration-proof.json'),$false)
  }
  $report.passed=$true
} catch {$report.error=$_.Exception.Message;Write-Output ("Shutdown primitives failed: "+$report.error)} finally {
  $report.storage.availableBytesAfter=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($source)).AvailableFreeSpace
  if($lease){$lease.Dispose()}
  foreach($name in $variables){[Environment]::SetEnvironmentVariable($name,$old[$name],'Process')}
  Set-Location -LiteralPath $oldLocation;[Console]::OutputEncoding=$oldEncoding;$OutputEncoding=$oldOutputEncoding
  Write-NewJson (Join-Path $ReportDirectory 'report.json') $report
  $files=@(Get-ChildItem -LiteralPath $ReportDirectory -File | Where-Object {$_.Name -cne 'report.ready.json'} | Sort-Object Name | ForEach-Object {@{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()}})
  Write-NewJson (Join-Path $ReportDirectory 'report.ready.json') @{schemaVersion=1;files=$files}
}
if(-not $report.passed){exit 1}
