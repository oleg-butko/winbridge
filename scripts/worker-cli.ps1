# Rust CLI emits UTF-8 JSON. PowerShell native capture otherwise uses the console
# code page (often OEM 866 on Russian Windows), corrupting journal text in reports.
function Invoke-WorkerJson {
    param([Parameter(Mandatory)][string]$Executable, [Parameter(Mandatory)][string]$ConfigPath,
          [Parameter(Mandatory)][string[]]$Arguments)
    $start = [Diagnostics.ProcessStartInfo]::new($Executable)
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
    $start.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
    foreach ($argument in @('--config', $ConfigPath) + $Arguments) { $start.ArgumentList.Add($argument) }
    $client = [Diagnostics.Process]::new()
    $client.StartInfo = $start
    try {
        $null = $client.Start()
        # Drain both streams concurrently; no pipe can block the other.
        $stdout = $client.StandardOutput.ReadToEndAsync()
        $stderr = $client.StandardError.ReadToEndAsync()
        if (-not $client.WaitForExit(15000)) {
            $client.Kill(); $client.WaitForExit()
            throw 'Worker CLI timeout; client stopped, accepted commands must be reconciled by their original key'
        }
        $text = $stdout.GetAwaiter().GetResult()
        $diagnostic = $stderr.GetAwaiter().GetResult()
        if ($client.ExitCode -ne 0) { throw "Worker CLI failed: $diagnostic $text" }
        return ($text | ConvertFrom-Json)
    } finally { $client.Dispose() }
}
