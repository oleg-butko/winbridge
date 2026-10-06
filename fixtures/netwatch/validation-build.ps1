param([Parameter(Mandatory)][string]$RuntimeDirectory, [Parameter(Mandatory)][string]$LogDirectory,
      [Parameter(Mandatory)][string]$TargetDirectory)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ((Test-Path -LiteralPath $RuntimeDirectory) -or (Test-Path -LiteralPath $LogDirectory)) {
    throw 'Validation runtime already exists; prepare a fresh workflow'
}
New-Item -ItemType Directory -Path $RuntimeDirectory | Out-Null
New-Item -ItemType Directory -Path $LogDirectory | Out-Null
Copy-Item -LiteralPath './winbridge-validation.json' -Destination (Join-Path $RuntimeDirectory 'config.json')
& cargo.exe build -p netwatch-agent --locked --offline --target-dir $TargetDirectory
exit $LASTEXITCODE
