[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$SharedTree,
    [Parameter(Mandatory)][string]$RunId,
    [Parameter(Mandatory)][string]$Sample
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($RunId -notmatch '^[a-zA-Z0-9_-]{1,80}$' -or $Sample -notmatch '^[a-zA-Z0-9_-]{1,80}$') { throw 'Invalid measurement identity' }
$generation = (Get-Location).Path
if (-not (Test-Path -LiteralPath (Join-Path $generation '.winbridge/verified.json'))) { throw 'Run only inside a verified fixture snapshot' }
$project = Split-Path -Parent (Split-Path -Parent $generation)
$destination = Join-Path $project "performance/$RunId/$Sample/tree"
function Assert-NoLinks([string]$Path) {
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            if ((Get-Item -Force -LiteralPath $current).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Linked measurement path: $current" }
        }
        $parent = [IO.Path]::GetDirectoryName($current)
        if ($parent -eq $current) { break }; $current = $parent
    }
}
Assert-NoLinks $destination
Assert-NoLinks $SharedTree
if (Test-Path -LiteralPath $destination) { throw 'Refusing to overwrite an existing tree measurement' }
$manifestPath = Join-Path (Split-Path -Parent $SharedTree) 'tree-manifest.json'
Assert-NoLinks $manifestPath
if ((Get-Item -LiteralPath $manifestPath).Length -gt 2097152) { throw 'Tree manifest exceeds 2 MiB' }
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($manifest.schemaVersion -ne 1 -or $manifest.files.Count -gt 10000) { throw 'Unsupported tree manifest' }
$clock = [Diagnostics.Stopwatch]::StartNew()
[IO.Directory]::CreateDirectory($destination) | Out-Null
$bytes = [long]0
foreach ($file in $manifest.files) {
    if ([IO.Path]::IsPathRooted($file.path) -or $file.path -match '(^|/)\.\.?(/|$)|[\\:]' -or $file.bytes -lt 0 -or $file.bytes -gt 67108864) { throw 'Unsafe tree entry' }
    $source = Join-Path $SharedTree $file.path
    $target = Join-Path $destination $file.path
    Assert-NoLinks $source
    if ((Get-Item -LiteralPath $source).Length -ne $file.bytes) { throw 'Shared tree size mismatch' }
    [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target)) | Out-Null
    $inputFile = [IO.File]::OpenRead($source)
    try {
        $outputFile = [IO.File]::Open($target, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try { $inputFile.CopyTo($outputFile, 65536); $outputFile.Flush($true) }
        finally { $outputFile.Dispose() }
    } finally { $inputFile.Dispose() }
    $bytes += $file.bytes
}
$copyMs = $clock.Elapsed.TotalMilliseconds
$clock.Restart()
foreach ($file in $manifest.files) {
    $target = Join-Path $destination $file.path
    if ((Get-Item -LiteralPath $target).Length -ne $file.bytes -or
        (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant() -ne $file.sha256) { throw 'Copied tree bytes/SHA-256 mismatch' }
}
$verifyMs = $clock.Elapsed.TotalMilliseconds
$cargoVersion = (& cargo.exe --version) -join "`n"
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify Cargo' }
$rustVersion = (& rustc.exe -Vv) -join "`n"
if ($LASTEXITCODE -ne 0) { throw 'Cannot identify Rust compiler' }
[ordered]@{schemaVersion=1;kind='tree_io_microbenchmark';files=$manifest.files.Count;bytes=$bytes;sharedReadCopyMs=$copyMs;localVerifyMs=$verifyMs;destination=$destination;verified=$true;environment=@{os=[Environment]::OSVersion.VersionString;cpuCount=[Environment]::ProcessorCount;powerShell=$PSVersionTable.PSVersion.ToString();cargo=$cargoVersion;rustc=$rustVersion}} | ConvertTo-Json -Depth 4 -Compress
