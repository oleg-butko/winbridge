<# Fixed24 ->25 copy-only migration; no installed runtime change. #>
param(
  [Parameter(Mandatory)][string]$SourceDirectory,
  [Parameter(Mandatory)][string]$SourceArchiveSha256,
  [Parameter(Mandatory)][string]$ReferenceFile,
  [Parameter(Mandatory)][string]$ReferenceSha256,
  [Parameter(Mandatory)][string]$ValidationCacheDirectory,
  [Parameter(Mandatory)][string]$ReportDirectory
)
$ErrorActionPreference='Stop'
& (Join-Path $PSScriptRoot 'test-update-journal-migration.ps1') -Profile 'Bootstrap24To25' @PSBoundParameters
