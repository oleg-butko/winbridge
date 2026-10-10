# The wrapper and its worker are in the same managed Windows Job Object.
# The actual worker's readiness line is relayed to stdout and a registered log.
param([Parameter(Mandatory)][string]$RuntimeDirectory,
      [Parameter(Mandatory)][string]$LogDirectory,
      [Parameter(Mandatory)][string]$TargetDirectory)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$worker = Join-Path $TargetDirectory 'debug/winbridge-worker.exe'
$config = Join-Path $RuntimeDirectory 'worker.local.json'
$log = Join-Path $LogDirectory 'worker.log'
if (Test-Path -LiteralPath $log) { throw 'Refusing to reuse an earlier application log' }
$writer = [IO.StreamWriter]::new($log, $false, [Text.UTF8Encoding]::new($false))
$writer.AutoFlush = $true
try {
    & $worker --config $config serve 2>&1 | ForEach-Object {
        $line = $_.ToString()
        $writer.WriteLine($line)
        Write-Output $line
    }
    $code = $LASTEXITCODE
} finally { $writer.Dispose() }
exit $code
