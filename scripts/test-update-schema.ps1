$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/update-schema.ps1')
$root = Join-Path ([IO.Path]::GetTempPath()) ('WinBridge-schema-' + [guid]::NewGuid().ToString('N'))
$directory = Join-Path $root 'apps/update/src'
New-Item -ItemType Directory -Path $directory -Force | Out-Null
$path = Join-Path $directory 'journal.rs'
try {
    foreach ($version in @(7,8,9)) {
        Set-Content -LiteralPath $path -Value "pub const UPDATE_SCHEMA_VERSION: u32 = $version;"
        Assert-WinBridgeUpdaterSchema -SourceDirectory $root -ReportedSchema $version
        foreach ($wrong in @(($version - 1), ($version + 1), "$version", $null)) {
            $rejected = $false
            try { Assert-WinBridgeUpdaterSchema -SourceDirectory $root -ReportedSchema $wrong }
            catch { $rejected = $true }
            if (-not $rejected) { throw "Incorrect schema proof accepted: $wrong" }
        }
    }
    Set-Content -LiteralPath $path -Value "pub const UPDATE_SCHEMA_VERSION: u32 = 8;`npub const UPDATE_SCHEMA_VERSION: u32 = 9;"
    $rejected = $false
    try { Assert-WinBridgeUpdaterSchema -SourceDirectory $root -ReportedSchema 8 }
    catch { $rejected = $true }
    if (-not $rejected) { throw 'Ambiguous source schema accepted' }
    Write-Output 'Updater schema proof matches exact source; stale/future/string/null/ambiguous versions rejected'
} finally { Remove-Item -LiteralPath $root -Recurse -Force }
