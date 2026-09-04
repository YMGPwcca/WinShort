[CmdletBinding()]
param(
    [string]$RepoRoot = '',
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
}
$RepoRoot = (Resolve-Path $RepoRoot).Path

$machineScript = Join-Path $PSScriptRoot 'ui_acceptance.ps1'
$visualScript = Join-Path $PSScriptRoot 'ui_visual_sanity.ps1'

if (-not (Test-Path $machineScript)) {
    throw "Machine acceptance script missing: $machineScript"
}
if (-not (Test-Path $visualScript)) {
    throw "Visual sanity script missing: $visualScript"
}

Write-Output '=== WinShort machine UI acceptance ==='
& powershell.exe `
    -NoProfile `
    -ExecutionPolicy Bypass `
    -File $machineScript `
    -RepoRoot $RepoRoot `
    -TimeoutSeconds $TimeoutSeconds
if ($LASTEXITCODE -ne 0) {
    throw "Machine UI acceptance failed with exit code $LASTEXITCODE"
}

Write-Output ''
Write-Output '=== WinShort clean visual sanity capture ==='
& powershell.exe `
    -NoProfile `
    -ExecutionPolicy Bypass `
    -File $visualScript `
    -RepoRoot $RepoRoot `
    -TimeoutSeconds $TimeoutSeconds
if ($LASTEXITCODE -ne 0) {
    throw "Visual sanity capture failed with exit code $LASTEXITCODE"
}

Write-Output ''
Write-Output 'Machine artifacts: target\ui-acceptance-results\<timestamp>'
Write-Output 'Clean visual artifacts: target\ui-visual-sanity\<timestamp>'
Write-Output 'Review runtime-overlay-visual-sheet.png for System / Dark / Light without the synthetic blur-test pattern.'
