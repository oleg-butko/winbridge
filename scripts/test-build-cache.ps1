$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'lib/build-cache.ps1')
$root = Join-Path ([IO.Path]::GetTempPath()) ('winbridge-cache-selection-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
try {
    $fresh = Join-Path $root 'fresh/test-build-cache'
    $old = Join-Path $root 'previous/test-build-cache'
    $selection = Get-WorkflowBuildCacheSelection -DefaultPath $fresh -PreviousPath $old
    if ($selection.directory -ne $fresh -or $selection.reused -or $selection.skipped -ne 'cache_missing') { throw 'Missing cache did not select new cache' }
    if (Test-Path -LiteralPath $fresh) { throw 'Selection unexpectedly created files' }
    New-Item -ItemType Directory -Path $old -Force | Out-Null
    $selection = Get-WorkflowBuildCacheSelection -DefaultPath $fresh -PreviousPath $old
    if ($selection.directory -ne $old -or -not $selection.reused -or $selection.skipped) { throw 'Existing cache without old source was not reused' }
    $file = Join-Path $root 'file'
    [IO.File]::WriteAllText($file, 'fixture')
    $denied = $false
    try { Get-WorkflowBuildCacheSelection -DefaultPath $fresh -PreviousPath $file | Out-Null } catch { $denied = $_.Exception.Message -like '*not a directory*' }
    if (-not $denied) { throw 'Non-directory cache was accepted' }
    if (-not $IsWindows) {
        $linked = Join-Path $root 'linked'
        New-Item -ItemType SymbolicLink -Path $linked -Target (Split-Path $old -Parent) | Out-Null
        $denied = $false
        try { Get-WorkflowBuildCacheSelection -DefaultPath $fresh -PreviousPath (Join-Path $linked 'absent-cache') | Out-Null } catch { $denied = $_.Exception.Message -like '*linked*' }
        if (-not $denied) { throw 'Missing cache under linked ancestor was accepted' }
    }
    Write-Output 'Cache selection passed: missing -> fresh; existing without source -> reuse; file/link refused.'
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force
}
