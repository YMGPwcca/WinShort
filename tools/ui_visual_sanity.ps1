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
Add-Type -AssemblyName System.Windows.Forms

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
        public static IntPtr[] FindVisibleWindowsForProcess(int pid, string className) {
            var found = new System.Collections.Generic.List<IntPtr>();
            EnumWindows(delegate(IntPtr hwnd, IntPtr lparam) {
                uint owner;
                GetWindowThreadProcessId(hwnd, out owner);
                if (owner == (uint)pid && IsWindowVisible(hwnd) && ClassEquals(hwnd, className)) {
                    found.Add(hwnd);
                }
                return true;
            }, IntPtr.Zero);
            return found.ToArray();
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

function Get-DeterministicOverlayHwnd {
    param([System.Diagnostics.Process]$Process)

    $candidates = [WinShortUiVisual.Native]::FindVisibleWindowsForProcess(
        $Process.Id,
        'WinShort.Overlay'
    )
    if ($candidates.Count -ne 1) {
        return $null
    }
    return $candidates[0]
}

function Write-Utf8NoBom {
    param([string]$Path, [string]$Text)
    $encoding = [System.Text.UTF8Encoding]::new($false)
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
    $bitmap = [System.Drawing.Bitmap]::new(
        [int]$Rectangle.Width,
        [int]$Rectangle.Height,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
    )
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
    $info = [System.Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $ExePath
    $info.WorkingDirectory = $RepoRoot
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.EnvironmentVariables['WINSHORT_DATA_DIR'] = $DataDirectory
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE'] = 'show-deterministic-acceptance-overlay'
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_NO_EXTERNAL'] = '1'
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_THEME'] = $Appearance
    $info.EnvironmentVariables.Remove('WINSHORT_UI_ACCEPTANCE_FORCE_OPAQUE') | Out-Null
    $info.EnvironmentVariables.Remove('WINSHORT_UI_ACCEPTANCE_FORCE_COMPOSITION_FAILURE') | Out-Null
    return $info
}

function New-ColorBackdrop {
    param([pscustomobject]$Background)

    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $form = [System.Windows.Forms.Form]::new()
    $form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
    $form.StartPosition = [System.Windows.Forms.FormStartPosition]::Manual
    $form.Bounds = $bounds
    $form.ShowInTaskbar = $false
    $form.TopMost = $false
    $form.BackColor = [System.Drawing.ColorTranslator]::FromHtml($Background.Hex)

    $label = [System.Windows.Forms.Label]::new()
    $label.Dock = [System.Windows.Forms.DockStyle]::Fill
    $label.TextAlign = [System.Drawing.ContentAlignment]::MiddleCenter
    $label.BackColor = [System.Drawing.Color]::Transparent
    $label.ForeColor = [System.Drawing.ColorTranslator]::FromHtml($Background.ForegroundHex)
    $label.Font = [System.Drawing.Font]::new(
        'Segoe UI',
        [single]34.0,
        [System.Drawing.FontStyle]::Bold,
        [System.Drawing.GraphicsUnit]::Point
    )
    $label.Text = "WINSHORT BLUR TEST - $($Background.Name.ToUpperInvariant())`r`nABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789"
    $form.Controls.Add($label)

    [void]$form.Show()
    [void]$form.Activate()
    [System.Windows.Forms.Application]::DoEvents()
    Start-Sleep -Milliseconds 150
    return $form
}

function Close-ColorBackdrop {
    param([System.Windows.Forms.Form]$Form)
    if ($null -eq $Form) {
        return
    }
    $Form.Close()
    $Form.Dispose()
    [System.Windows.Forms.Application]::DoEvents()
}

function New-VisualSheet {
    param(
        [object[]]$Captures,
        [int]$BackgroundCount,
        [string]$Path
    )
    if ($Captures.Count -eq 0) {
        return
    }

    [int]$gap = 12
    [int]$labelHeight = 34
    $images = @()
    $sheet = $null
    $graphics = $null
    $font = $null
    $brush = $null
    try {
        [int]$width = 0
        [int]$height = 0
        foreach ($capture in $Captures) {
            Assert-Condition (Test-Path $capture.ContextScreenshot) "visual context capture missing: $($capture.ContextScreenshot)"
            $image = [System.Drawing.Bitmap]::FromFile($capture.ContextScreenshot)
            $images += $image
            $width = [Math]::Max($width, [int]$image.Width)
            $height = [Math]::Max($height, [int]$image.Height)
        }

        Assert-Condition ($width -gt 0 -and $height -gt 0) "invalid visual sheet source size: ${width}x${height}"
        Assert-Condition ($BackgroundCount -gt 0) 'background count must be positive'

        [int]$columns = $BackgroundCount
        [int]$rows = [int][Math]::Ceiling($images.Count / [double]$columns)
        [int]$tileHeight = $labelHeight + $height
        [int]$sheetWidth = ($width * $columns) + ($gap * ($columns - 1))
        [int]$sheetHeight = ($tileHeight * $rows) + ($gap * ($rows - 1))
        Assert-Condition ($sheetWidth -gt 0 -and $sheetHeight -gt 0) "invalid visual sheet size: ${sheetWidth}x${sheetHeight}"

        $sheet = [System.Drawing.Bitmap]::new(
            $sheetWidth,
            $sheetHeight,
            [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
        )
        $graphics = [System.Drawing.Graphics]::FromImage($sheet)
        $font = [System.Drawing.Font]::new(
            'Segoe UI',
            [single]10.0,
            [System.Drawing.FontStyle]::Regular,
            [System.Drawing.GraphicsUnit]::Point
        )
        $brush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::White)

        $graphics.Clear([System.Drawing.Color]::FromArgb(24, 24, 24))
        for ($index = 0; $index -lt $images.Count; $index++) {
            [int]$column = $index % $columns
            [int]$row = [Math]::Floor($index / $columns)
            [int]$x = $column * ($width + $gap)
            [int]$y = $row * ($tileHeight + $gap)
            $label = "$($Captures[$index].Appearance.ToUpperInvariant()) / $($Captures[$index].Background.ToUpperInvariant())"
            $graphics.DrawString(
                $label,
                $font,
                $brush,
                [single]($x + 4),
                [single]($y + 7)
            )
            $graphics.DrawImage(
                $images[$index],
                $x,
                $y + $labelHeight,
                [int]$images[$index].Width,
                [int]$images[$index].Height
            )
        }
        $sheet.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        if ($null -ne $brush) { $brush.Dispose() }
        if ($null -ne $font) { $font.Dispose() }
        if ($null -ne $graphics) { $graphics.Dispose() }
        if ($null -ne $sheet) { $sheet.Dispose() }
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

$backgrounds = @(
    [pscustomobject]@{ Name = 'black';   Hex = '#101010'; ForegroundHex = '#FFFFFF' },
    [pscustomobject]@{ Name = 'white';   Hex = '#F2F2F2'; ForegroundHex = '#101010' },
    [pscustomobject]@{ Name = 'gray';    Hex = '#808080'; ForegroundHex = '#FFFFFF' },
    [pscustomobject]@{ Name = 'red';     Hex = '#D13438'; ForegroundHex = '#FFFFFF' },
    [pscustomobject]@{ Name = 'yellow';  Hex = '#FFD335'; ForegroundHex = '#101010' },
    [pscustomobject]@{ Name = 'green';   Hex = '#107C10'; ForegroundHex = '#FFFFFF' },
    [pscustomobject]@{ Name = 'blue';    Hex = '#0067C0'; ForegroundHex = '#FFFFFF' },
    [pscustomobject]@{ Name = 'magenta'; Hex = '#B4009E'; ForegroundHex = '#FFFFFF' }
)

$captures = @()
try {
    foreach ($appearance in @('dark', 'light')) {
        foreach ($background in $backgrounds) {
            $scenarioDirectory = Join-Path (Join-Path $ResultRoot $appearance) $background.Name
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
            $backdrop = $null
            try {
                $backdrop = New-ColorBackdrop $background

                $info = New-ProcessStartInfo $dataDirectory $appearance
                $process = [System.Diagnostics.Process]::Start($info)
                Assert-Condition ($null -ne $process) "could not start WinShort for $appearance/$($background.Name) visual sanity capture"

                $overlayHwnd = Wait-Until {
                    if ($process.HasExited) { return $null }
                    Get-DeterministicOverlayHwnd $process
                } "runtime overlay did not appear for $appearance/$($background.Name)"

                Start-Sleep -Milliseconds 300
                $overlayRectangle = Get-Rect ([WinShortUiVisual.Native]::WindowRect($overlayHwnd))
                $contextMargin = 64
                $contextRectangle = [System.Drawing.Rectangle]::FromLTRB(
                    $overlayRectangle.Left - $contextMargin,
                    $overlayRectangle.Top - $contextMargin,
                    $overlayRectangle.Right + $contextMargin,
                    $overlayRectangle.Bottom + $contextMargin
                )

                $cardPath = Join-Path $scenarioDirectory "runtime-overlay-$appearance-$($background.Name)-visual.png"
                $contextPath = Join-Path $scenarioDirectory "runtime-overlay-$appearance-$($background.Name)-visual-context.png"
                Save-Bitmap (Capture-Bitmap $overlayRectangle) $cardPath
                Save-Bitmap (Capture-Bitmap $contextRectangle) $contextPath
                Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process

                $captures += [pscustomobject]@{
                    Appearance = $appearance
                    Background = $background.Name
                    BackgroundHex = $background.Hex
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
                Close-ColorBackdrop $backdrop
            }
        }
    }

    $sheetPath = Join-Path $ResultRoot 'runtime-overlay-color-matrix.png'
    New-VisualSheet $captures $backgrounds.Count $sheetPath
    Assert-Condition (Test-Path $sheetPath) "visual sheet was not created: $sheetPath"

    $summary = [pscustomobject]@{
        ResultRoot = $ResultRoot
        Binary = $ExePath
        Purpose = 'human visual sanity for Dark and Light Composition overlays across deterministic colored desktop backdrops'
        Backdrops = $backgrounds
        BackdropPattern = 'large high-contrast text is rendered behind the overlay so blur remains visually inspectable on otherwise solid colors'
        MachineAcceptance = 'tools/ui_acceptance.ps1 remains authoritative for blur/outside-card metrics'
        Captures = $captures
        VisualSheet = $sheetPath
    }
    $summaryPath = Join-Path $ResultRoot 'summary.json'
    $summary | ConvertTo-Json -Depth 6 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Output "Visual sanity captures complete: $summaryPath"
    Write-Output "Color matrix: $sheetPath"
    exit 0
}
catch {
    Write-Error $_
    Write-Error "Visual sanity artifacts: $ResultRoot"
    exit 1
}
