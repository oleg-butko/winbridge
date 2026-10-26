<# Read-only logical sizes. Never follow junctions; names do not authorize deletion. #>
param([Parameter(Mandatory)][string]$Root)
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$rootPath=[IO.Path]::GetFullPath($Root)
$ancestor=[IO.DirectoryInfo]::new($rootPath)
while ($null -ne $ancestor) {
    if (-not $ancestor.Exists -or ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint)) {throw 'Root must have real directory ancestors'}
    $ancestor=$ancestor.Parent
}
function Get-Size([IO.FileSystemInfo]$Item) {
    $pending=[Collections.Generic.Stack[IO.FileSystemInfo]]::new()
    $pending.Push($Item)
    [long]$bytes=0;[long]$files=0;[long]$links=0;[long]$errors=0
    while ($pending.Count) {
        $entry=$pending.Pop()
        try {
            if ($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) {$links++;continue}
            if ($entry -is [IO.DirectoryInfo]) {
                foreach ($child in $entry.EnumerateFileSystemInfos()) {$pending.Push($child)}
            } else {$bytes+=$entry.Length;$files++}
        } catch {$errors++}
    }
    return [ordered]@{bytes=$bytes;files=$files;skippedLinks=$links;errors=$errors;complete=($links -eq 0 -and $errors -eq 0)}
}
function Get-Role([string]$Name) {
    if ($Name -eq 'test-build-cache') {return 'compiler-cache'}
    if ($Name -eq 'winbridge') {return 'source-and-possible-sdk-cache'}
    if ($Name -in @('test-temp','tmp-test-temp')) {return 'temporary-test-files'}
    if ($Name -eq 'shutdown-main-cases') {return 'retained-updater-reference'}
    if ($Name -match '^tmp-') {return 'temporary-by-name-needs-terminal-and-reference-check'}
    return 'unclassified-preserve'
}
$runs=@()
foreach ($run in ([IO.DirectoryInfo]::new($rootPath)).EnumerateFileSystemInfos()) {
    $groups=@()
    if ($run -is [IO.DirectoryInfo] -and -not ($run.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        try {
            foreach ($item in $run.EnumerateFileSystemInfos()) {
                $size=Get-Size $item
                $groups+=@([ordered]@{name=$item.Name;role=(Get-Role $item.Name);measurement=$size})
            }
        } catch {$groups+=@([ordered]@{name='[enumeration-error]';role='unclassified-preserve';measurement=@{bytes=0;files=0;skippedLinks=0;errors=1;complete=$false}})}
    } else {
        $groups+=@([ordered]@{name=$run.Name;role='unclassified-preserve';measurement=(Get-Size $run)})
    }
    [long]$bytes=0;[long]$files=0;[long]$errors=0;[long]$links=0
    foreach ($group in $groups) {$bytes+=$group.measurement.bytes;$files+=$group.measurement.files;$errors+=$group.measurement.errors;$links+=$group.measurement.skippedLinks}
    $runs+=@([ordered]@{name=$run.Name;bytes=$bytes;files=$files;errors=$errors;skippedLinks=$links;groups=@($groups|Sort-Object {$_.measurement.bytes} -Descending)})
}
[long]$total=0
foreach ($run in $runs) {$total+=$run.bytes}
[ordered]@{schemaVersion=1;root=$rootPath;measuredUtc=[DateTime]::UtcNow.ToString('o');
    logicalBytes=$total;readOnly=$true;deletionAuthorized=$false;
    caveat='Logical sizes; hard links counted per path. Files may change. Skipped links/errors mean incomplete measurement. Roles are hints, not a cleanup approval.';
    freeBytes=[IO.DriveInfo]::new([IO.Path]::GetPathRoot($rootPath)).AvailableFreeSpace;
    runs=@($runs|Sort-Object {$_.bytes} -Descending)}|ConvertTo-Json -Depth 9 -Compress
