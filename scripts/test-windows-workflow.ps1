param(
    [Parameter(Mandatory = $true)][string]$SourceDirectory,
    [Parameter(Mandatory = $true)][string]$ReportDirectory,
    [Parameter(Mandatory = $true)][string]$SourceArchiveSha256
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ($env:OS -ne 'Windows_NT') { throw 'This scenario requires actual Windows' }
$source = (Resolve-Path -LiteralPath $SourceDirectory).Path
$manifest = Get-Content -LiteralPath (Join-Path $source 'winbridge-source-manifest.json') -Raw | ConvertFrom-Json
$report = [ordered]@{ schemaVersion = 1; platform = 'windows'; passed = $false; sourceArchiveSha256 = $SourceArchiveSha256;
    sourceCommit = $manifest.sourceCommit; sourceDirty = $manifest.dirty; startedAt = [DateTime]::UtcNow.ToString('o'); commands = @(); error = $null }
New-Item -ItemType Directory -Path $ReportDirectory -ErrorAction Stop | Out-Null
$reportPath = Join-Path $ReportDirectory 'report.json'
$oldTarget = $env:CARGO_TARGET_DIR
$oldTemp = $env:TEMP
$oldTmp = $env:TMP
$oldLocation = Get-Location
try {
    foreach ($file in $manifest.files) {
        $filePath = Join-Path $source $file.path
        $item = Get-Item -LiteralPath $filePath
        if ($item.Length -ne $file.bytes -or (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) {
            throw "Source manifest mismatch: $($file.path)"
        }
    }
    $env:CARGO_TARGET_DIR = Join-Path (Split-Path $source -Parent) 'test-build-cache'
    $env:TEMP = Join-Path (Split-Path $source -Parent) 'test-temp'
    $env:TMP = $env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP -Force | Out-Null
    Set-Location -LiteralPath $source
    $cargo = (Get-Command cargo.exe -ErrorAction Stop).Source
    $groups = @(
        @{ name = 'core-and-log-reader'; arguments = @('test', '-p', 'winbridge-core', '-p', 'winbridge-log-reader', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') },
        @{ name = 'worker-runtime-snapshots-workflows'; arguments = @('test', '-p', 'winbridge-worker', '--features', 'bundled-sqlite', '--test', 'runtime', '--test', 'snapshots', '--test', 'workflows', '--locked', '--offline', '--', '--test-threads=1', '--nocapture') }
    )
    foreach ($group in $groups) {
        $log = Join-Path $ReportDirectory ($group.name + '.log')
        $commandArgs = $group.arguments
        & $cargo @commandArgs 2>&1 | Tee-Object -FilePath $log | ForEach-Object { Write-Output $_ }
        $code = $LASTEXITCODE
        $report.commands += [ordered]@{ name = $group.name; executable = $cargo; arguments = $commandArgs; exitCode = $code; log = (Split-Path $log -Leaf) }
        if ($code -ne 0) { throw "Native test group $($group.name) failed with exit $code; see $log" }
    }
    $report.passed = $true
} catch {
    $report.error = $_.Exception.Message
    Write-Output $report.error
} finally {
    Set-Location -LiteralPath $oldLocation.Path
    $env:CARGO_TARGET_DIR = $oldTarget
    $env:TEMP = $oldTemp
    $env:TMP = $oldTmp
    $report.finishedAt = [DateTime]::UtcNow.ToString('o')
    $report | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $reportPath -Encoding utf8
    $artifacts = @(Get-ChildItem -LiteralPath $ReportDirectory -File | ForEach-Object {
        [ordered]@{ path = $_.Name; bytes = $_.Length; sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
    })
    [ordered]@{ schemaVersion = 1; platform = 'windows'; passed = $report.passed; files = $artifacts } |
        ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $ReportDirectory 'report.ready.json') -Encoding utf8
}
if (-not $report.passed) { exit 1 }
