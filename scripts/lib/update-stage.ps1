# Create-only payload preparation. This file never copies to installed runtime paths.
. (Join-Path $PSScriptRoot '../session-runtime.ps1')
function New-UpdatePayload {
  param([Parameter(Mandatory)]$Sources, [Parameter(Mandatory)][string]$Destination)
  $names = @('winbridge-worker.exe','winbridge-fixture.exe','winbridge-desktop.exe','winbridge-update.exe')
  if (@($Sources.Keys).Count -ne $names.Count -or @($Sources.Keys | Where-Object { $_ -notin $names }).Count) { throw 'Candidate requires exactly the worker, fixture, desktop and updater artifacts' }
  Assert-SessionDirectory $Destination
  if (Test-Path -LiteralPath $Destination) { throw "Candidate destination already exists; preserved: $Destination" }
  $plan = @()
  foreach ($name in $names) {
    $path = [IO.Path]::GetFullPath($Sources[$name])
    Assert-SessionDirectory ([IO.Path]::GetDirectoryName($path))
    Assert-SessionFile $path
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Missing candidate artifact: $name" }
    $plan += @{name=$name;source=$path;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
  }
  [IO.Directory]::CreateDirectory($Destination) | Out-Null
  $result = @()
  foreach ($entry in $plan) {
    $target = Join-Path $Destination $entry.name
    $inputStream = [IO.File]::OpenRead($entry.source)
    try {
      $outputStream = [IO.File]::Open($target,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
      try { $inputStream.CopyTo($outputStream); $outputStream.Flush($true) } finally { $outputStream.Dispose() }
    } finally { $inputStream.Dispose() }
    if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.sha256 -or (Get-FileHash -LiteralPath $entry.source -Algorithm SHA256).Hash.ToLowerInvariant() -cne $entry.sha256) { throw "Candidate artifact changed during staging: $($entry.name)" }
    $result += @{name=$entry.name;bytes=(Get-Item -LiteralPath $target).Length;sha256=$entry.sha256}
  }
  return $result
}
