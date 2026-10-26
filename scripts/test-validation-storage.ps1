$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
. (Join-Path $PSScriptRoot 'lib/validation-storage.ps1')
$root=Join-Path ([IO.Path]::GetTempPath()) ('winbridge-storage-fixture-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root|Out-Null
try {
    $fresh=Join-Path $root 'fresh'
    New-Item -ItemType Directory -Path $fresh|Out-Null
    if ((Get-WorkflowTemporaryDirectory $fresh) -cne (Join-Path $fresh 'tmp-test-temp')) {throw 'New temporary name missing'}
    if (Test-Path -LiteralPath (Join-Path $fresh 'tmp-test-temp')) {throw 'Selection must not create directories'}
    $old=Join-Path $root 'old'
    New-Item -ItemType Directory -Path (Join-Path $old 'test-temp') -Force|Out-Null
    if ((Get-WorkflowTemporaryDirectory $old) -cne (Join-Path $old 'test-temp')) {throw 'Legacy path changed'}
    Write-WorkflowStorageGuide $fresh
    $guide=Join-Path $fresh 'README.storage.txt'
    if (-not (Test-Path -LiteralPath $guide) -or [IO.File]::ReadAllText($guide) -notmatch 'shutdown-main-cases') {throw 'Guide missing reference warning'}
    [IO.File]::WriteAllText($guide,'manual annotation')
    Write-WorkflowStorageGuide $fresh
    if ([IO.File]::ReadAllText($guide) -cne 'manual annotation') {throw 'Manual annotation overwritten'}
    [IO.File]::WriteAllText((Join-Path $fresh 'tmp-test-temp'),'not a directory')
    $denied=$false
    try {Get-WorkflowTemporaryDirectory $fresh|Out-Null} catch {$denied=$_.Exception.Message -like '*not a directory*'}
    if (-not $denied) {throw 'File accepted as temporary directory'}
    $inventoryRoot=Join-Path $root 'workflow-validation'
    $run=Join-Path $inventoryRoot 'test-run'
    foreach ($name in @('test-build-cache','tmp-test-temp','shutdown-main-cases','unknown')) {New-Item -ItemType Directory -Path (Join-Path $run $name) -Force|Out-Null}
    foreach ($item in @(@('test-build-cache',2000),@('tmp-test-temp',3000),@('shutdown-main-cases',4000),@('unknown',5))) {
        [IO.File]::WriteAllBytes((Join-Path $run ($item[0]+'/fixture.bin')),[byte[]]::new($item[1]))
    }
    $inventory=& (Join-Path $PSScriptRoot 'inspect-windows-validation-storage.ps1') -Root $inventoryRoot|ConvertFrom-Json
    if ($inventory.logicalBytes -ne 9005 -or -not $inventory.readOnly -or $inventory.deletionAuthorized -or $inventory.runs.Count -ne 1) {throw 'Incorrect readonly inventory'}
    $unknown=@($inventory.runs[0].groups|Where-Object {$_.name -eq 'unknown'})
    if ($unknown.Count -ne 1 -or $unknown[0].role -cne 'unclassified-preserve') {throw 'Unknown data was not protected'}
    if ($IsWindows) {
        $linked=Join-Path $root 'linked'
        New-Item -ItemType Junction -Path $linked -Target $old|Out-Null
        try {
            $denied=$false
            try {Get-WorkflowTemporaryDirectory $linked|Out-Null} catch {$denied=$_.Exception.Message -like '*linked*'}
            if (-not $denied) {throw 'Junction accepted'}
        } finally {[IO.Directory]::Delete($linked)}
        $outside=Join-Path $root 'outside'
        New-Item -ItemType Directory -Path $outside|Out-Null
        [IO.File]::WriteAllText((Join-Path $outside 'canary'),'preserve')
        $linked=Join-Path $run 'linked-outside'
        New-Item -ItemType Junction -Path $linked -Target $outside|Out-Null
        try {
            $inventory=& (Join-Path $PSScriptRoot 'inspect-windows-validation-storage.ps1') -Root $inventoryRoot|ConvertFrom-Json
            if ($inventory.logicalBytes -ne 9005 -or $inventory.runs[0].skippedLinks -ne 1 -or
                [IO.File]::ReadAllText((Join-Path $outside 'canary')) -cne 'preserve') {throw 'Inventory followed/changed junction target'}
        } finally {[IO.Directory]::Delete($linked)}
    }
    Write-Output 'Storage roles passed: new tmp name; legacy/manual guide preserved; file/junction refused; exact readonly sizes; unknown preserved; inventory skips junction targets.'
} finally {Remove-Item -LiteralPath $root -Recurse -Force}
