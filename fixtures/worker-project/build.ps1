# Validation-only adapter, copied into a separate committed WinBridge clone.
param([Parameter(Mandatory)][string]$RuntimeDirectory,
      [Parameter(Mandatory)][string]$LogDirectory,
      [Parameter(Mandatory)][string]$TargetDirectory)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if ((Test-Path -LiteralPath $RuntimeDirectory) -or (Test-Path -LiteralPath $LogDirectory)) {
    throw 'Validation runtime already exists; prepare a fresh workflow'
}
New-Item -ItemType Directory -Path $RuntimeDirectory | Out-Null
New-Item -ItemType Directory -Path $LogDirectory | Out-Null
$project = Join-Path $RuntimeDirectory 'project'
New-Item -ItemType Directory -Path $project | Out-Null
$fixture = Join-Path $TargetDirectory 'debug/winbridge-fixture.exe'
$config = [ordered]@{
    schemaVersion = 1; instanceId = 'winbridge-child-' + [guid]::NewGuid().ToString('N')
    nodeId = 'isolated-worker-application'; stateDirectory = (Join-Path $RuntimeDirectory 'state')
    worker = @{ controllerUrl = $null; keepRunningOnUiExit = $true }; delivery = $null
    projects = @(@{ id = 'fixture'; workspaceRoot = $project; allowDirectCommands = $false
        maxConcurrentMutations = 1; recipes = @(
            @{ id = 'success'; executable = $fixture; arguments = @('success'); workingDirectory = '.'; timeoutMs = 30000 },
            @{ id = 'tree'; executable = $fixture; arguments = @('tree'); workingDirectory = '.'; timeoutMs = 120000 }
        ) })
}
$config | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $RuntimeDirectory 'worker.local.json') -Encoding utf8
& cargo.exe build -p winbridge-worker --features bundled-sqlite --bins --locked --offline --target-dir $TargetDirectory
exit $LASTEXITCODE
