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
$ExePath = Join-Path $RepoRoot 'target\release\winshort.exe'
$Stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$ResultRoot = Join-Path $RepoRoot "target\ui-visual-sanity\$Stamp"
New-Item -ItemType Directory -Path $ResultRoot -Force | Out-Null

Add-Type -AssemblyName System.Drawing

if (-not ('WinShortUiVisual.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

namespace WinShortUiVisual {
    public static class Native {
        public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr lparam);

        [StructLayout(LayoutKind.Sequential)]
        private struct RECT {
            public int Left;
            public int Top;
            public int Right;
            public int Bottom;
        }

        [DllImport("user32.dll")]
        private static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lparam);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern int GetClassName(IntPtr hwnd, StringBuilder name, int capacity);

        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

        [DllImport("user32.dll")]
        private static extern bool IsWindowVisible(IntPtr hwnd);

        [DllImport("user32.dll")]
        private static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

        public static IntPtr FindVisibleWindowForProcess(int pid, string className) {
            IntPtr found = IntPtr.Zero;
            EnumWindows(delegate(IntPtr hwnd, IntPtr lparam) {
                uint owner;
                GetWindowThreadProcessId(hwnd, out owner);
                if (owner == (uint)pid && IsWindowVisible(hwnd) && ClassEquals(hwnd, className)) {
                    found = hwnd;
                    return false;
                }
                return true;
            }, IntPtr.Zero);
            return found;
        }

        public static int[] WindowRect(IntPtr hwnd) {
            RECT rect;
            if (hwnd == IntPtr.Zero || !GetWindowRect(hwnd, out rect)) return new int[0];
            return new[] { rect.Left, rect.Top, rect.Right, rect.Bottom };
        }

        private static bool ClassEquals(IntPtr hwnd, string expected) {
            var name = new StringBuilder(256);
            GetClassName(hwnd, name, name.Capacity);
            return string.Equals(name.ToString(), expected, StringComparison.Ordinal);
        }
    }
}
'@
}

function Assert-Condition {
    param(
        [bool]$Condition,
        [string]$Message
    )
    if (-not $Condition) {
        throw $Message
    }
}

function Wait-Until {
    param(
        [scriptblock]$Condition,
        [string]$Failure
    )
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $value = & $Condition
        if ($null -ne $value -and $value -ne $false) {
            return $value
        }
        Start-Sleep -Milliseconds 200
    }
    throw $Failure
}

function Write-Utf8NoBom {
    param([string]$Path, [string]$Text)
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Get-Rect {
    param([int[]]$Values)
    Assert-Condition ($Values.Count -eq 4) 'window rectangle unavailable'
    return [System.Drawing.Rectangle]::FromLTRB($Values[0], $Values[1], $Values[2], $Values[3])
}

function Capture-Bitmap {
    param([System.Drawing.Rectangle]$Rectangle)
    Assert-Condition ($Rectangle.Width -gt 0 -and $Rectangle.Height -gt 0) 'cannot capture an empty rectangle'
    $bitmap = New-Object System.Drawing.Bitmap($Rectangle.Width, $Rectangle.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen($Rectangle.Left, $Rectangle.Top, 0, 0, $Rectangle.Size)
    }
    finally {
        $graphics.Dispose()
    }
    return $bitmap
}

function Save-Bitmap {
    param(
        [System.Drawing.Bitmap]$Bitmap,
        [string]$Path
    )
    try {
        $Bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $Bitmap.Dispose()
    }
}

function Copy-ScenarioLogs {
    param(
        [string]$DataDirectory,
        [string]$ScenarioDirectory,
        [System.Diagnostics.Process]$Process
    )
    $logDirectory = Join-Path $DataDirectory 'logs'
    if (Test-Path $logDirectory) {
        $destination = Join-Path $ScenarioDirectory 'logs'
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        Copy-Item (Join-Path $logDirectory '*.log') $destination -Force -ErrorAction SilentlyContinue
        foreach ($log in Get-ChildItem $logDirectory -Filter '*.log' -ErrorAction SilentlyContinue) {
            $text = Get-Content $log.FullName -Raw -ErrorAction SilentlyContinue
            if ($text -match '\[PANIC\]|RefCell already borrowed') {
                throw "panic evidence found in $($log.Name)"
            }
        }
    }
    if ($null -ne $Process -and $Process.HasExited) {
        throw "WinShort exited during visual sanity capture with code $($Process.ExitCode)"
    }
}

function New-ProcessStartInfo {
    param(
        [string]$DataDirectory,
        [string]$Appearance
    )
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $ExePath
    $info.WorkingDirectory = $RepoRoot
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.EnvironmentVariables['WINSHORT_DATA_DIR'] = $DataDirectory
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE'] = 'show-status-overlay'
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_NO_EXTERNAL'] = '1'
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_THEME'] = if ($Appearance -eq 'light') { 'light' } else { 'dark' }
    $info.EnvironmentVariables.Remove('WINSHORT_UI_ACCEPTANCE_FORCE_OPAQUE') | Out-Null
    $info.EnvironmentVariables.Remove('WINSHORT_UI_ACCEPTANCE_FORCE_COMPOSITION_FAILURE') | Out-Null
    return $info
}

function New-VisualSheet {
    param(
        [object[]]$Captures,
        [string]$Path
    )
    if ($Captures.Count -eq 0) {
        return
    }

    $gap = 12
    $labelHeight = 30
    $images = @()
    try {
        foreach ($capture in $Captures) {
            $images += [System.Drawing.Bitmap]::FromFile($capture.ContextScreenshot)
        }
        $width = ($images | Measure-Object -Property Width -Maximum).Maximum
        $height = ($images | Measure-Object -Property Height -Maximum).Maximum
        $sheetWidth = ($width * $images.Count) + ($gap * ($images.Count - 1))
        $sheetHeight = $labelHeight + $height
        $sheet = New-Object System.Drawing.Bitmap($sheetWidth, $sheetHeight)
        $graphics = [System.Drawing.Graphics]::FromImage($sheet)
        $font = New-Object System.Drawing.Font('Segoe UI', 10.0, [System.Drawing.FontStyle]::Regular)
        $brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
        try {
            $graphics.Clear([System.Drawing.Color]::FromArgb(24, 24, 24))
            for ($index = 0; $index -lt $images.Count; $index++) {
                $x = $index * ($width + $gap)
                $graphics.DrawString($Captures[$index].Appearance.ToUpperInvariant(), $font, $brush, $x + 4, 6)
                $graphics.DrawImage($images[$index], $x, $labelHeight, $images[$index].Width, $images[$index].Height)
            }
            $sheet.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
        }
        finally {
            $brush.Dispose()
            $font.Dispose()
            $graphics.Dispose()
            $sheet.Dispose()
        }
    }
    finally {
        foreach ($image in $images) {
            if ($null -ne $image) {
                $image.Dispose()
            }
        }
    }
}

Assert-Condition (Test-Path $ExePath) "release binary missing: $ExePath"
$existing = Get-Process -Name 'winshort' -ErrorAction SilentlyContinue
Assert-Condition ($null -eq $existing) 'an existing winshort.exe is running; refusing to touch the user process'

$captures = @()
try {
    foreach ($appearance in @('system', 'dark', 'light')) {
        $scenarioDirectory = Join-Path $ResultRoot $appearance
        $dataDirectory = Join-Path $scenarioDirectory 'data'
        New-Item -ItemType Directory -Path $dataDirectory -Force | Out-Null

        $config = @"
schema_version = 10

[overlay]
enabled = true
duration_ms = 10000
position = "center"
monitor = "primary"
scale = 1.0
opacity = 1.0
appearance = "$appearance"
show_external_audio_changes = false
"@
        Write-Utf8NoBom (Join-Path $dataDirectory 'config.toml') $config

        $process = $null
        try {
            $info = New-ProcessStartInfo $dataDirectory $appearance
            $process = [System.Diagnostics.Process]::Start($info)
            Assert-Condition ($null -ne $process) "could not start WinShort for $appearance visual sanity capture"

            $overlayHwnd = Wait-Until {
                if ($process.HasExited) { return $null }
                $candidate = [WinShortUiVisual.Native]::FindVisibleWindowForProcess($process.Id, 'WinShort.Overlay')
                if ($candidate -eq [IntPtr]::Zero) { return $null }
                $candidate
            } "runtime overlay did not appear for $appearance"

            Start-Sleep -Milliseconds 300
            $overlayRectangle = Get-Rect ([WinShortUiVisual.Native]::WindowRect($overlayHwnd))
            $contextMargin = 48
            $contextRectangle = [System.Drawing.Rectangle]::FromLTRB(
                $overlayRectangle.Left - $contextMargin,
                $overlayRectangle.Top - $contextMargin,
                $overlayRectangle.Right + $contextMargin,
                $overlayRectangle.Bottom + $contextMargin
            )

            $cardPath = Join-Path $scenarioDirectory "runtime-overlay-$appearance-visual.png"
            $contextPath = Join-Path $scenarioDirectory "runtime-overlay-$appearance-visual-context.png"
            Save-Bitmap (Capture-Bitmap $overlayRectangle) $cardPath
            Save-Bitmap (Capture-Bitmap $contextRectangle) $contextPath
            Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process

            $captures += [pscustomobject]@{
                Appearance = $appearance
                Screenshot = $cardPath
                ContextScreenshot = $contextPath
                Width = $overlayRectangle.Width
                Height = $overlayRectangle.Height
            }
        }
        finally {
            if ($null -ne $process) {
                if (-not $process.HasExited) {
                    $process.Kill() | Out-Null
                    $process.WaitForExit(5000) | Out-Null
                }
                try { Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process } catch { }
                $process.Dispose()
            }
        }
    }

    $sheetPath = Join-Path $ResultRoot 'runtime-overlay-visual-sheet.png'
    New-VisualSheet $captures $sheetPath
    $summary = [pscustomobject]@{
        ResultRoot = $ResultRoot
        Binary = $ExePath
        Purpose = 'human visual sanity only; no synthetic PatternBackdrop is used'
        MachineAcceptance = 'tools/ui_acceptance.ps1 remains authoritative for blur/outside-card metrics'
        Captures = $captures
        VisualSheet = $sheetPath
    }
    $summaryPath = Join-Path $ResultRoot 'summary.json'
    $summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Output "Visual sanity captures complete: $summaryPath"
    exit 0
}
catch {
    Write-Error $_
    Write-Error "Visual sanity artifacts: $ResultRoot"
    exit 1
}
