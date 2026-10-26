param([string]$SourceDirectory,[string]$ManifestSha256,[string]$CacheDirectory,[Parameter(Mandatory)][string]$WorkDirectory)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
. (Join-Path $PSScriptRoot 'lib/update-build-profile.ps1')
Assert-SessionDirectory $WorkDirectory
if (Test-Path -LiteralPath $WorkDirectory) { throw 'New profile-check work directory required' }
if ((Split-Path $WorkDirectory -Parent) -ine (Split-Path $SourceDirectory -Parent) -or
    (Split-Path $WorkDirectory -Leaf) -notmatch '^build-profile-check-\d{8}-\d{6}-[a-f0-9]{8}$') { throw 'Expected isolated sibling work directory' }
$target='K:\WinBridge compiler cache\Проверка\target'
$expected=@('build','--manifest-path','apps/desktop/src-tauri/Cargo.toml','--features','custom-protocol','--release','--locked','--target-dir',$target)
$normal=@(Get-UpdateDesktopBuildArguments -TargetDirectory $target)
if (($normal|ConvertTo-Json -Compress) -cne ($expected|ConvertTo-Json -Compress)) { throw 'Production release arguments changed' }
$explicitNormal=@(Get-UpdateDesktopBuildArguments -TargetDirectory $target -NativeValidation:$false)
if (($normal|ConvertTo-Json -Compress) -cne ($explicitNormal|ConvertTo-Json -Compress)) { throw 'False native switch changed production mode' }
$native=@(Get-UpdateDesktopBuildArguments -TargetDirectory $target -NativeValidation)
$packages=@('winbridge-desktop','winbridge-core','winbridge-protocol','winbridge-platform','winbridge-transport')
if ($native.Count -ne $expected.Count+2*$packages.Count) { throw 'Unexpected native options' }
for ($i=0; $i -lt $packages.Count; $i++) {
    if ($native[2*$i] -cne '--config' -or $native[2*$i+1] -cne "profile.release.package.$($packages[$i]).opt-level=0") { throw 'Native profile scope changed' }
}
if (($native[(2*$packages.Count)..($native.Count-1)]|ConvertTo-Json -Compress) -cne ($expected|ConvertTo-Json -Compress)) { throw 'Native profile changed release/custom-protocol/target options' }
New-Item -ItemType Directory -Path $WorkDirectory|Out-Null
Write-NewJson (Join-Path $WorkDirectory 'report.json') @{schemaVersion=1;kind='update_build_profile_checks';passed=$true;
    cargoInvoked=$false;windowsOpened=$false;productionArguments=$normal;nativeArguments=$native;
    freeBytes=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($WorkDirectory)).AvailableFreeSpace}
$item=Get-Item -LiteralPath (Join-Path $WorkDirectory 'report.json')
Write-NewJson (Join-Path $WorkDirectory 'report.ready.json') @{schemaVersion=1;files=@(@{path=$item.Name;bytes=$item.Length;sha256=(Get-FileHash -LiteralPath $item.FullName).Hash.ToLowerInvariant()})}
Write-Output 'Production argv unchanged; native argv changes only local desktop optimization; no Cargo/UI executed.'
