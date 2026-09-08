[CmdletBinding()]
param(
    [string]$RepoRoot = '',
    [int]$TimeoutSeconds = 30,
    [switch]$SkipUiReview
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ([string]::IsNullOrWhiteSpace($RepoRoot)) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
}
$RepoRoot = (Resolve-Path $RepoRoot).Path
Set-Location $RepoRoot

function Require-Command {
    param([Parameter(Mandatory)][string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command is not available on PATH: $Name"
    }
}

function Invoke-NativeGate {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Command,
        [Parameter()][string[]]$Arguments = @()
    )

    Write-Output ''
    Write-Output "=== $Name ==="
    & $Command @Arguments
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "$Name failed with exit code $exitCode"
    }
    Write-Output "PASS: $Name"
}

if ($env:OS -ne 'Windows_NT') {
    throw 'WinShort final validation must run on Windows.'
}

foreach ($command in @('git', 'cargo', 'rustup', 'python', 'cargo-deny', 'powershell.exe')) {
    Require-Command $command
}

$status = (& git status --porcelain=v1 2>&1) -join "`n"
if ($LASTEXITCODE -ne 0) {
    throw "git status failed: $status"
}
if (-not [string]::IsNullOrWhiteSpace($status)) {
    throw "Final validation requires a clean worktree. Current status:`n$status"
}

$head = (& git rev-parse HEAD 2>&1) -join ''
if ($LASTEXITCODE -ne 0) {
    throw "git rev-parse HEAD failed: $head"
}
Write-Output "WinShort final validation"
Write-Output "Repository: $RepoRoot"
Write-Output "Commit:     $head"

$installedTargets = @(& rustup target list --installed)
if ($LASTEXITCODE -ne 0) {
    throw 'rustup target list --installed failed.'
}
foreach ($target in @('i686-pc-windows-msvc', 'aarch64-pc-windows-msvc')) {
    if ($installedTargets -notcontains $target) {
        throw "Required Rust target is not installed: $target. Install it with: rustup target add $target"
    }
}

Invoke-NativeGate -Name 'Rust 1.85 MSRV toolchain preflight' -Command 'rustup' -Arguments @(
    'run', '1.85.0', 'rustc', '--version'
)

Invoke-NativeGate -Name 'cargo fmt --check' -Command 'cargo' -Arguments @(
    'fmt', '--all', '--', '--check'
)
Invoke-NativeGate -Name 'cargo check --all-targets --all-features' -Command 'cargo' -Arguments @(
    'check', '--all-targets', '--all-features'
)
Invoke-NativeGate -Name 'cargo clippy -D warnings' -Command 'cargo' -Arguments @(
    'clippy', '--all-targets', '--all-features', '--', '-D', 'warnings'
)
Invoke-NativeGate -Name 'cargo test --all-features' -Command 'cargo' -Arguments @(
    'test', '--all-features'
)
Invoke-NativeGate -Name 'cargo build --release --all-features' -Command 'cargo' -Arguments @(
    'build', '--release', '--all-features'
)

$releaseExe = Join-Path $RepoRoot 'target\release\winshort.exe'
if (-not (Test-Path -LiteralPath $releaseExe)) {
    throw "Release executable missing after build: $releaseExe"
}
Invoke-NativeGate -Name 'embedded manifest resources' -Command 'python' -Arguments @(
    '-c',
    "from pathlib import Path; import sys; data=Path(sys.argv[1]).read_bytes(); assert b'PerMonitorV2' in data, 'PerMonitorV2 missing from manifest'; assert b'asInvoker' in data, 'asInvoker missing from manifest'; print('manifest OK')",
    $releaseExe
)
Invoke-NativeGate -Name 'embedded application icon' -Command 'python' -Arguments @(
    (Join-Path $RepoRoot 'tools\verify_pe_icon.py'),
    $releaseExe
)
Invoke-NativeGate -Name 'release nullable UIA provider ABI regression' -Command 'cargo' -Arguments @(
    'test', '--release', '--all-features', 'nullable_provider_abi_regression', '--', '--nocapture'
)
Invoke-NativeGate -Name 'i686 all-features check' -Command 'cargo' -Arguments @(
    'check', '--target', 'i686-pc-windows-msvc', '--all-features'
)
Invoke-NativeGate -Name 'aarch64 all-features check' -Command 'cargo' -Arguments @(
    'check', '--target', 'aarch64-pc-windows-msvc', '--all-features'
)
Invoke-NativeGate -Name 'Rust 1.85 MSRV all-features check' -Command 'rustup' -Arguments @(
    'run', '1.85.0', 'cargo', 'check', '--all-features'
)
Invoke-NativeGate -Name 'cargo deny check' -Command 'cargo' -Arguments @(
    'deny', 'check'
)

if (-not $SkipUiReview) {
    $reviewScript = Join-Path $PSScriptRoot 'ui_acceptance_review.ps1'
    if (-not (Test-Path -LiteralPath $reviewScript)) {
        throw "UI acceptance review script missing: $reviewScript"
    }
    Invoke-NativeGate -Name 'Windows UI acceptance + visual sanity capture' -Command 'powershell.exe' -Arguments @(
        '-NoProfile',
        '-ExecutionPolicy', 'Bypass',
        '-File', $reviewScript,
        '-RepoRoot', $RepoRoot,
        '-TimeoutSeconds', [string]$TimeoutSeconds
    )
}

Write-Output ''
Write-Output '=== FINAL VALIDATION SUMMARY ==='
Write-Output 'PASS: formatting, compile checks, Clippy, tests, release build, resources, UIA ABI, cross-target checks, MSRV, dependency policy.'
if ($SkipUiReview) {
    Write-Output 'SKIPPED: Windows UI acceptance and visual sanity capture.'
} else {
    Write-Output 'PASS: machine UI acceptance and visual-sanity capture completed.'
    Write-Output 'MANUAL REVIEW STILL REQUIRED: inspect the generated runtime-overlay-visual-sheet.png before declaring final acceptance.'
}
