# Match the exact schema compiled from this verified source, never a rolling
# minimum or a hardcoded version that silently expires at the next migration.
function Assert-WinBridgeUpdaterSchema {
    param([Parameter(Mandatory)][string]$SourceDirectory, [Parameter(Mandatory)]$ReportedSchema)
    $content = Get-Content -LiteralPath (Join-Path $SourceDirectory 'apps/update/src/journal.rs') -Raw
    $definitions = [regex]::Matches($content, '(?m)^pub const UPDATE_SCHEMA_VERSION: u32 = ([1-9][0-9]*);\r?$')
    if ($definitions.Count -ne 1) { throw 'Expected exactly one updater schema constant in the verified source' }
    $expected = [uint32]$definitions[0].Groups[1].Value
    if (($ReportedSchema -isnot [int] -and $ReportedSchema -isnot [long]) -or $ReportedSchema -le 0 -or $ReportedSchema -ne $expected) {
        throw "Updater proof schema does not match verified source: expected $expected; received $ReportedSchema"
    }
}
