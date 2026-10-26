<# Read-only use of frozen pilot binaries; new diagnostic states/reports, no Cargo or UI. #>
param(
    [Parameter(Mandatory)][string]$PilotDirectory,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{64}$')][string]$ReportSha256,
    [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'session-runtime.ps1')
$pilot = [IO.Path]::GetFullPath($PilotDirectory)
$output = [IO.Path]::GetFullPath($ReportDirectory)
Assert-SessionDirectory $pilot
Assert-SessionDirectory $output
if ((Split-Path $pilot -Parent) -ine (Split-Path $output -Parent) -or
    (Split-Path $pilot -Leaf) -notmatch '^profile-v1-\d{8}-\d{6}-[a-f0-9]{8}$' -or
    (Split-Path $output -Leaf) -notmatch '^hotspots-v1-\d{8}-\d{6}-[a-f0-9]{8}$' -or
    (Test-Path -LiteralPath $output)) { throw 'Expected NEW sibling diagnostic directory' }
$inputReport = Join-Path $pilot 'report.json'
if ((Get-FileHash -LiteralPath $inputReport).Hash.ToLowerInvariant() -cne $ReportSha256) { throw 'Pilot report changed' }
$previous = Get-Content -Raw -LiteralPath $inputReport | ConvertFrom-Json
if ($previous.kind -cne 'update_profile_pilot' -or $previous.passed -ne $true -or $previous.installedRuntimeChanged -ne $false) { throw 'Requires passed pilot' }
$names = @($previous.testNames)
$report = [ordered]@{ schemaVersion=1; kind='update_profile_hotspots'; passed=$false; diagnosticOnly=$true;
    installedRuntimeChanged=$false; pilotReportSha256=$ReportSha256; sourceCommit=$previous.sourceCommit;
    binaries=$previous.binaries; testNames=$previous.testNames; commands=@(); error=$null }
$pins = @()
$savedLocation = Get-Location
$savedTemp = $env:TEMP
$savedTmp = $env:TMP
try {
    foreach ($binary in $previous.binaries) {
        if ($binary.file -cne "tests-$($binary.variant).exe") { throw 'Unsafe binary path' }
        $path = Join-Path $pilot $binary.file
        Assert-SessionFile $path
        $pins += [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        if ((Get-Item -LiteralPath $path).Length -ne $binary.bytes -or (Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -cne $binary.sha256) { throw 'Frozen binary changed' }
    }
    New-Item -ItemType Directory -Path $output | Out-Null
    $env:TEMP = Join-Path $output 'test-temp'
    $env:TMP = $env:TEMP
    New-Item -ItemType Directory -Path $env:TEMP | Out-Null
    Set-Location (Join-Path (Split-Path $pilot -Parent) 'winbridge/apps/update')
    for ($i=0; $i -lt $names.Count; $i++) {
        $testName = $names[$i]
        $group = $testName -replace '::[^:]+$', '::'
        foreach ($variant in @('A','B')) {
            $name = "case-$i-$variant"
            $log = "$name.log"
            $args = @($testName, '--exact', '--test-threads=1', '--nocapture', '--color=never')
            Write-Host "WinBridge profile diagnostic: $testName ($variant)"
            $watch = [Diagnostics.Stopwatch]::StartNew()
            & (Join-Path $pilot "tests-$variant.exe") @args 2>&1 | ForEach-Object { $_.ToString() } | Set-Content -LiteralPath (Join-Path $output $log) -Encoding utf8
            $code = $LASTEXITCODE
            $watch.Stop()
            $text = Get-Content -Raw -LiteralPath (Join-Path $output $log)
            if ($code -ne 0 -or $text -notmatch 'test result: ok\. (\d+) passed; 0 failed; 0 ignored;.*?finished in ([0-9.]+)s' -or [int]$Matches[1] -ne 1) { throw "Incomplete diagnostic: $name" }
            $report.commands += @{ testName=$testName; group=$group; variant=$variant; count=1; log=$log; arguments=$args; exitCode=$code;
                elapsedMs=$watch.ElapsedMilliseconds; libtestMs=[double]::Parse($Matches[2],[Globalization.CultureInfo]::InvariantCulture)*1000 }
        }
    }
    if ((Get-FileHash -LiteralPath $inputReport).Hash.ToLowerInvariant() -cne $ReportSha256) { throw 'Pilot report changed during diagnostic' }
    $report.passed = $true
} catch {
    $report.error = $_.ToString()
    Write-Warning $report.error
} finally {
    Set-Location $savedLocation
    $env:TEMP = $savedTemp
    $env:TMP = $savedTmp
    foreach ($pin in $pins) { $pin.Dispose() }
    if (Test-Path -LiteralPath $output -PathType Container) {
        Write-NewJson (Join-Path $output 'report.json') $report
        $files = @(Get-ChildItem -LiteralPath $output -File | ForEach-Object { @{path=$_.Name;bytes=$_.Length;sha256=(Get-FileHash -LiteralPath $_.FullName).Hash.ToLowerInvariant()} })
        Write-NewJson (Join-Path $output 'report.ready.json') @{schemaVersion=1;files=$files}
    }
}
if (-not $report.passed) { throw 'Diagnostic failed; preserve report and reconcile' }
