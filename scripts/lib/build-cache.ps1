# Cache selection is separate from source/test state. A removed compiler cache
# causes a fresh build; linked paths and non-directory replacements fail closed.
function Assert-WorkflowCachePath {
    param([Parameter(Mandatory)][string]$Path)
    $ancestor = [IO.Path]::GetFullPath($Path)
    while ($ancestor) {
        if (Test-Path -LiteralPath $ancestor) {
            if ((Get-Item -LiteralPath $ancestor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw 'Refusing linked build cache'
            }
        }
        $ancestor = Split-Path $ancestor -Parent
    }
}

function Get-WorkflowBuildCacheSelection {
    param(
        [Parameter(Mandatory)][string]$DefaultPath,
        [string]$PreviousPath = ''
    )
    Assert-WorkflowCachePath $DefaultPath
    if ($PreviousPath) {
        Assert-WorkflowCachePath $PreviousPath
        if (Test-Path -LiteralPath $PreviousPath) {
            if (-not (Test-Path -LiteralPath $PreviousPath -PathType Container)) {
                throw 'Prior build cache is not a directory'
            }
            return [ordered]@{ directory = [IO.Path]::GetFullPath($PreviousPath); reused = $true; skipped = $null }
        }
        return [ordered]@{ directory = [IO.Path]::GetFullPath($DefaultPath); reused = $false; skipped = 'cache_missing' }
    }
    return [ordered]@{ directory = [IO.Path]::GetFullPath($DefaultPath); reused = $false; skipped = $null }
}
