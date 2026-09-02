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
$ResultRoot = Join-Path $RepoRoot "target\ui-acceptance-results\$Stamp"
New-Item -ItemType Directory -Path $ResultRoot -Force | Out-Null
$AcceptanceShowMessage = [uint32]0x8003
$AcceptanceHideOverlayMessage = [uint32]0x8004

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

if (-not ('WinShortUiAcceptance.Native' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

namespace WinShortUiAcceptance {
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

        [DllImport("user32.dll")]
        private static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr lparam);
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern IntPtr FindWindowEx(
            IntPtr parent,
            IntPtr childAfter,
            string className,
            string windowName
        );

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        private static extern int GetClassName(IntPtr hwnd, StringBuilder name, int capacity);

        [DllImport("user32.dll")]
        private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

        [DllImport("user32.dll")]
        private static extern bool IsWindowVisible(IntPtr hwnd);


        [DllImport("user32.dll")]
        private static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

        [DllImport("user32.dll")]
        private static extern uint GetDpiForWindow(IntPtr hwnd);

        [DllImport("user32.dll")]
        private static extern bool SetForegroundWindow(IntPtr hwnd);
        [DllImport("user32.dll")]
        private static extern bool AttachThreadInput(uint sourceThreadId, uint targetThreadId, bool attach);

        [DllImport("user32.dll")]
        private static extern IntPtr GetFocus();

        [DllImport("kernel32.dll")]
        private static extern uint GetCurrentThreadId();

        [DllImport("user32.dll")]
        private static extern bool PostMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
        [DllImport("user32.dll")]
        private static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
        [DllImport("user32.dll", EntryPoint = "SendMessageW")]
        private static extern IntPtr SendMessageRaw(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);

        [DllImport("user32.dll", EntryPoint = "SendMessageW", CharSet = CharSet.Unicode)]
        private static extern IntPtr SendMessageText(
            IntPtr hwnd,
            uint message,
            IntPtr wparam,
            StringBuilder lparam
        );

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
        public static IntPtr FindMessageWindowForProcess(int pid, string className) {
            var hwnd = FindWindowEx(new IntPtr(-3), IntPtr.Zero, className, null);
            if (hwnd == IntPtr.Zero) return IntPtr.Zero;
            uint owner;
            GetWindowThreadProcessId(hwnd, out owner);
            return owner == (uint)pid ? hwnd : IntPtr.Zero;
        }

        public static IntPtr FindChildWindow(IntPtr parent, string className) {
            IntPtr found = IntPtr.Zero;
            if (parent == IntPtr.Zero) return found;
            EnumChildWindows(parent, delegate(IntPtr hwnd, IntPtr lparam) {
                if (ClassEquals(hwnd, className)) {
                    found = hwnd;
                    return false;
                }
                return true;
            }, IntPtr.Zero);
            return found;
        }

        private static bool ClassEquals(IntPtr hwnd, string expected) {
            var name = new StringBuilder(256);
            GetClassName(hwnd, name, name.Capacity);
            return string.Equals(name.ToString(), expected, StringComparison.Ordinal);
        }

        public static int[] WindowRect(IntPtr hwnd) {
            RECT rect;
            if (hwnd == IntPtr.Zero || !GetWindowRect(hwnd, out rect)) return new int[0];
            return new[] { rect.Left, rect.Top, rect.Right, rect.Bottom };
        }

        public static int Dpi(IntPtr hwnd) {
            var dpi = GetDpiForWindow(hwnd);
            return (int)(dpi == 0 ? 96 : dpi);
        }

        public static bool ActivateWindow(IntPtr hwnd) {
            return SetForegroundWindow(hwnd);
        }
        public static IntPtr FocusedWindowForThread(IntPtr targetWindow) {
            uint processId;
            var targetThread = GetWindowThreadProcessId(targetWindow, out processId);
            var currentThread = GetCurrentThreadId();
            var attached = targetThread != 0 && targetThread != currentThread
                && AttachThreadInput(currentThread, targetThread, true);
            var focus = GetFocus();
            if (attached) AttachThreadInput(currentThread, targetThread, false);
            return focus;
        }
        public static bool Visible(IntPtr hwnd) {
            return IsWindowVisible(hwnd);
        }


        public static bool PostEscape(IntPtr hwnd) {
            return PostMessage(hwnd, 0x0100, (IntPtr)0x1B, IntPtr.Zero)
                && PostMessage(hwnd, 0x0101, (IntPtr)0x1B, IntPtr.Zero);
        }
        public static bool PostMessageTo(IntPtr hwnd, uint message) {
            return PostMessage(hwnd, message, IntPtr.Zero, IntPtr.Zero);
        }
        public static bool PrintWindowToHdc(IntPtr hwnd, IntPtr hdc) {
            return PrintWindow(hwnd, hdc, 2);
        }
        public static string[] ListLabels(IntPtr list) {
            var count = SendMessageRaw(list, 0x018B, IntPtr.Zero, IntPtr.Zero).ToInt32();
            if (count <= 0) return new string[0];
            var labels = new string[count];
            for (var index = 0; index < count; index++) {
                var length = SendMessageRaw(list, 0x018A, (IntPtr)index, IntPtr.Zero).ToInt32();
                if (length < 0) return new string[0];
                var text = new StringBuilder(length + 1);
                SendMessageText(list, 0x0189, (IntPtr)index, text);
                labels[index] = text.ToString();
            }
            return labels;
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

function New-ProcessStartInfo {
    param(
        [string]$Executable,
        [string]$DataDirectory,
        [string]$PickerTheme,
        [bool]$AcceptanceTrigger
    )
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $Executable
    $info.WorkingDirectory = $RepoRoot
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.EnvironmentVariables['WINSHORT_DATA_DIR'] = $DataDirectory
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_THEME'] = $PickerTheme
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_NO_EXTERNAL'] = '1'
    if ($AcceptanceTrigger) {
        $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE'] = 'show-status-overlay'
    }
    return $info
}

function Start-WinShort {
    param(
        [string]$DataDirectory,
        [string]$PickerTheme,
        [bool]$AcceptanceTrigger
    )
    $info = New-ProcessStartInfo $ExePath $DataDirectory $PickerTheme $AcceptanceTrigger
    return [System.Diagnostics.Process]::Start($info)
}

function Activate-ControlCenter {
    param(
        [string]$DataDirectory,
        [string]$PickerTheme
    )
    $secondary = Start-WinShort $DataDirectory $PickerTheme $false
    try {
        Assert-Condition $secondary.WaitForExit(10000) 'secondary activation did not exit'
        Assert-Condition ($secondary.ExitCode -eq 0) "secondary activation failed with exit code $($secondary.ExitCode)"
    }
    finally {
        $secondary.Dispose()
    }
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


function Capture-WindowBitmap {
    param(
        [IntPtr]$Hwnd,
        [System.Drawing.Rectangle]$Rectangle
    )
    Assert-Condition ($Rectangle.Width -gt 0 -and $Rectangle.Height -gt 0) 'cannot capture an empty window rectangle'
    $bitmap = New-Object System.Drawing.Bitmap($Rectangle.Width, $Rectangle.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $hdc = $graphics.GetHdc()
    try {
        Assert-Condition ([WinShortUiAcceptance.Native]::PrintWindowToHdc($Hwnd, $hdc)) 'PrintWindow failed'
    }
    finally {
        $graphics.ReleaseHdc($hdc)
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

function Test-RoundedRectPoint {
    param(
        [int]$X,
        [int]$Y,
        [int]$Left,
        [int]$Top,
        [int]$Right,
        [int]$Bottom,
        [double]$Radius
    )
    if ($X -lt $Left -or $X -gt $Right -or $Y -lt $Top -or $Y -gt $Bottom) {
        return $false
    }
    $cornerX = if ($X -lt ($Left + $Radius)) {
        $Left + $Radius
    } elseif ($X -gt ($Right - $Radius)) {
        $Right - $Radius
    } else {
        $X
    }
    $cornerY = if ($Y -lt ($Top + $Radius)) {
        $Top + $Radius
    } elseif ($Y -gt ($Bottom - $Radius)) {
        $Bottom - $Radius
    } else {
        $Y
    }
    $dx = $X - $cornerX
    $dy = $Y - $cornerY
    return (($dx * $dx) + ($dy * $dy)) -le ($Radius * $Radius)
}

function Compare-OverlayOutsideBody {
    param(
        [System.Drawing.Bitmap]$Baseline,
        [System.Drawing.Bitmap]$Shown,
        [int]$Inset,
        [double]$Radius
    )
    Assert-Condition ($Baseline.Width -eq $Shown.Width -and $Baseline.Height -eq $Shown.Height) 'overlay capture dimensions changed'
    $outsideChanged = 0
    $outsidePixels = 0
    $insideChanged = 0
    $insidePixels = 0
    $edgeAllowance = 8
    $expandedInset = $Inset - $edgeAllowance
    $expandedRadius = $Radius + $edgeAllowance
    for ($y = 0; $y -lt $Baseline.Height; $y++) {
        for ($x = 0; $x -lt $Baseline.Width; $x++) {
            $before = $Baseline.GetPixel($x, $y)
            $after = $Shown.GetPixel($x, $y)
            $difference = [Math]::Max(
                [Math]::Abs($before.R - $after.R),
                [Math]::Max([Math]::Abs($before.G - $after.G), [Math]::Abs($before.B - $after.B))
            )
            $body = Test-RoundedRectPoint $x $y $Inset $Inset ($Baseline.Width - $Inset - 1) ($Baseline.Height - $Inset - 1) $Radius
            $allowedEdge = Test-RoundedRectPoint $x $y $expandedInset $expandedInset ($Baseline.Width - $expandedInset - 1) ($Baseline.Height - $expandedInset - 1) $expandedRadius
            if ($body) {
                $insidePixels++
                if ($difference -gt 14) { $insideChanged++ }
            } elseif (-not $allowedEdge) {
                $outsidePixels++
                if ($difference -gt 14) { $outsideChanged++ }
            }
        }
    }
    return [pscustomobject]@{
        OutsideChanged = $outsideChanged
        OutsidePixels = $outsidePixels
        OutsideRatio = if ($outsidePixels -eq 0) { 0.0 } else { $outsideChanged / $outsidePixels }
        InsideChanged = $insideChanged
        InsidePixels = $insidePixels
    }
}

function New-NameCondition {
    param([string]$Name)
    return [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $Name
    )
}

function Get-UiaByName {
    param(
        [System.Windows.Automation.AutomationElement]$Root,
        [string]$Name
    )
    if ($null -eq $Root) { return $null }
    return $Root.FindFirst(
        [System.Windows.Automation.TreeScope]::Descendants,
        (New-NameCondition $Name)
    )
}

function Get-UiaByNamePrefix {
    param(
        [System.Windows.Automation.AutomationElement]$Root,
        [string]$Prefix
    )
    if ($null -eq $Root) { return $null }
    $nodes = $Root.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition
    )
    foreach ($node in $nodes) {
        if ($node.Current.Name.StartsWith($Prefix, [StringComparison]::Ordinal)) {
            return $node
        }
    }
    return $null
}

function Invoke-UiaElement {
    param([System.Windows.Automation.AutomationElement]$Element)
    Assert-Condition ($null -ne $Element) 'UI Automation element is missing'
    $pattern = $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
}

function Get-UiaRectangle {
    param([System.Windows.Automation.AutomationElement]$Element)
    $bounds = $Element.Current.BoundingRectangle
    return [pscustomobject]@{
        Left = [int][Math]::Round($bounds.Left)
        Top = [int][Math]::Round($bounds.Top)
        Right = [int][Math]::Round($bounds.Right)
        Bottom = [int][Math]::Round($bounds.Bottom)
        Width = [int][Math]::Round($bounds.Width)
        Height = [int][Math]::Round($bounds.Height)
    }
}

function Assert-OverlayPageGeometry {
    param([System.Windows.Automation.AutomationElement]$Window)
    $enabled = Get-UiaByName $Window 'Show status overlay'
    $monitor = Get-UiaByName $Window 'Monitor'
    Assert-Condition ($null -ne $enabled -and $null -ne $monitor) 'Overlay status controls are missing from UIA'
    $enabledRect = Get-UiaRectangle $enabled
    $monitorRect = Get-UiaRectangle $monitor
    $tolerance = 2
    Assert-Condition ([Math]::Abs($enabledRect.Top - $monitorRect.Top) -le $tolerance) 'status controls are not top-aligned'
    Assert-Condition ([Math]::Abs($enabledRect.Width - $monitorRect.Width) -le $tolerance) 'status controls are not equal width'
    Assert-Condition ([Math]::Abs($enabledRect.Height - $monitorRect.Height) -le $tolerance) 'status controls are not equal height'

    $names = @('Top left', 'Top center', 'Top right', 'Center left', 'Center', 'Center right', 'Bottom left', 'Bottom center', 'Bottom right')
    $cells = @($names | ForEach-Object {
        $cell = Get-UiaByName $Window $_
        Assert-Condition ($null -ne $cell) "missing Position cell $_"
        [pscustomobject]@{ Name = $_; Rect = Get-UiaRectangle $cell }
    })
    $cellWidth = $cells[0].Rect.Width
    $cellHeight = $cells[0].Rect.Height
    foreach ($cell in $cells) {
        Assert-Condition ([Math]::Abs($cell.Rect.Width - $cellWidth) -le $tolerance) 'Position grid cell widths differ'
        Assert-Condition ([Math]::Abs($cell.Rect.Height - $cellHeight) -le $tolerance) 'Position grid cell heights differ'
    }
    $position = @($cells | Where-Object { $_.Name -eq 'Top left' })[0].Rect
    $right = @($cells | Where-Object { $_.Name -eq 'Top right' })[0].Rect
    Assert-Condition ($right.Right -gt $position.Right) 'Position grid does not span its column'
    return [pscustomobject]@{
        Enabled = $enabledRect
        Monitor = $monitorRect
        PositionCell = $position
        PositionGridRight = $right.Right
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
        $logs = Get-ChildItem $logDirectory -Filter '*.log' -ErrorAction SilentlyContinue
        foreach ($log in $logs) {
            $text = Get-Content $log.FullName -Raw -ErrorAction SilentlyContinue
            if ($text -match '\[PANIC\]|RefCell already borrowed') {
                throw "panic evidence found in $($log.Name)"
            }
        }
    }
    if ($null -ne $Process -and $Process.HasExited) {
        throw "WinShort exited during acceptance scenario with code $($Process.ExitCode)"
    }
}

$existing = Get-Process -Name 'winshort' -ErrorAction SilentlyContinue
Assert-Condition ($null -eq $existing) 'an existing winshort.exe is running; refusing to touch the user process'

try {
    & cargo build --release --all-features
    Assert-Condition ($LASTEXITCODE -eq 0) 'release build failed'
    Assert-Condition (Test-Path $ExePath) "release binary missing: $ExePath"

    $scenarioResults = @()
    $overlayResults = @()
    foreach ($appearance in @('system', 'dark', 'light')) {
        $scenarioDirectory = Join-Path $ResultRoot $appearance
        $dataDirectory = Join-Path $scenarioDirectory 'data'
        New-Item -ItemType Directory -Path $dataDirectory -Force | Out-Null
        $pickerTheme = if ($appearance -eq 'light') { 'light' } else { 'dark' }
        $config = @"
schema_version = 10

[overlay]
enabled = true
duration_ms = 10000
position = "bottom-right"
monitor = "primary"
scale = 1.0
opacity = 1.0
appearance = "$appearance"
show_external_audio_changes = false
"@
        Write-Utf8NoBom (Join-Path $dataDirectory 'config.toml') $config

        $process = $null
        try {
            $process = Start-WinShort $dataDirectory $pickerTheme $true
            Assert-Condition ($null -ne $process) 'could not start release binary'
            Start-Sleep -Milliseconds 1800

            $overlayHwnd = Wait-Until {
                if ($process.HasExited) { return $null }
                $candidate = [WinShortUiAcceptance.Native]::FindVisibleWindowForProcess($process.Id, 'WinShort.Overlay')
                if ($candidate -eq [IntPtr]::Zero) { return $null }
                $candidate
            } "runtime overlay did not appear for $appearance"
            $mainHwnd = Wait-Until {
                if ($process.HasExited) { return $null }
                $candidate = [WinShortUiAcceptance.Native]::FindMessageWindowForProcess($process.Id, 'WinShort.Main')
                if ($candidate -eq [IntPtr]::Zero) { return $null }
                $candidate
            } "main message window did not appear for $appearance"
            $overlayRectangle = Get-Rect ([WinShortUiAcceptance.Native]::WindowRect($overlayHwnd))
            $dpi = [WinShortUiAcceptance.Native]::Dpi($overlayHwnd)
            # The DWM frame contributes a few antialiased edge pixels; the
            # comparator allows a small expansion around the intended card.
            $inset = [Math]::Max(1, [int][Math]::Round($dpi / 96.0))
            $radius = [Math]::Max(1, [int][Math]::Round(14 * $dpi / 96.0))
            [WinShortUiAcceptance.Native]::PostMessageTo($overlayHwnd, $AcceptanceHideOverlayMessage) | Out-Null
            Wait-Until {
                -not [WinShortUiAcceptance.Native]::Visible($overlayHwnd)
            } 'overlay did not hide for baseline capture' | Out-Null
            Start-Sleep -Milliseconds 100
            $baseline = Capture-Bitmap $overlayRectangle
            [WinShortUiAcceptance.Native]::PostMessageTo($mainHwnd, $AcceptanceShowMessage) | Out-Null
            $overlayHwnd = Wait-Until {
                if ($process.HasExited) { return $null }
                $candidate = [WinShortUiAcceptance.Native]::FindVisibleWindowForProcess($process.Id, 'WinShort.Overlay')
                if ($candidate -eq [IntPtr]::Zero) { return $null }
                $candidate
            } "runtime overlay did not reappear for $appearance"
            Start-Sleep -Milliseconds 120
            $shown = Capture-Bitmap $overlayRectangle
            $outside = Compare-OverlayOutsideBody $baseline $shown $inset $radius
            $baselinePath = Join-Path $scenarioDirectory "runtime-overlay-$appearance-baseline.png"
            $shownPath = Join-Path $scenarioDirectory "runtime-overlay-$appearance.png"
            Save-Bitmap $baseline $baselinePath
            Save-Bitmap $shown $shownPath
            Assert-Condition ($outside.InsideChanged -gt 20) "runtime overlay did not repaint for $appearance"
            Assert-Condition ($outside.OutsideRatio -le 0.02) "runtime overlay painted a backing surface outside its card for $appearance (outside ratio $($outside.OutsideRatio))"
            $overlayResults += [pscustomobject]@{
                Appearance = $appearance
                Width = $overlayRectangle.Width
                Height = $overlayRectangle.Height
                OutsideChangedRatio = $outside.OutsideRatio
                Screenshot = $shownPath
            }
            Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process

            Activate-ControlCenter $dataDirectory $pickerTheme
            $controlCenterHwnd = Wait-Until {
                if ($process.HasExited) { return $null }
                $candidate = [WinShortUiAcceptance.Native]::FindVisibleWindowForProcess($process.Id, 'WinShort.ControlCenter')
                if ($candidate -eq [IntPtr]::Zero) { return $null }
                $candidate
            } 'Control Center did not open through second-instance activation'
            [WinShortUiAcceptance.Native]::ActivateWindow($controlCenterHwnd) | Out-Null
            Start-Sleep -Milliseconds 100
            $window = [System.Windows.Automation.AutomationElement]::FromHandle($controlCenterHwnd)
            Assert-Condition ($null -ne $window) 'Control Center UIA root is missing'
            $overlayNavigation = Wait-Until {
                $current = [System.Windows.Automation.AutomationElement]::FromHandle($controlCenterHwnd)
                Get-UiaByName $current 'Overlay'
            } 'Overlay navigation node did not appear'
            Invoke-UiaElement $overlayNavigation
            $window = Wait-Until {
                $current = [System.Windows.Automation.AutomationElement]::FromHandle($controlCenterHwnd)
                if ($null -eq (Get-UiaByName $current 'Overlay style')) { return $null }
                $current
            } 'Overlay page did not render'
            $geometry = Assert-OverlayPageGeometry $window
            $controlCenterPath = Join-Path $scenarioDirectory "control-center-overlay-$appearance.png"
            Save-Bitmap (Capture-WindowBitmap $controlCenterHwnd (Get-Rect ([WinShortUiAcceptance.Native]::WindowRect($controlCenterHwnd)))) $controlCenterPath

            $monitorNode = Get-UiaByName $window 'Monitor'
            Invoke-UiaElement $monitorNode
            $pickerHostHwnd = Wait-Until {
                $candidate = [WinShortUiAcceptance.Native]::FindChildWindow($controlCenterHwnd, 'WinShort.ControlCenterPicker')
                if ($candidate -eq [IntPtr]::Zero) { return $null }
                $candidate
            } 'Monitor picker host did not open'
            $listHwnd = Wait-Until {
                $candidate = [WinShortUiAcceptance.Native]::FindChildWindow($pickerHostHwnd, 'ListBox')
                if ($candidate -eq [IntPtr]::Zero) { return $null }
                $candidate
            } 'Monitor picker LISTBOX did not open'
            $listElement = [System.Windows.Automation.AutomationElement]::FromHandle($listHwnd)
            Assert-Condition ($null -ne $listElement) 'Monitor picker LISTBOX UIA root is missing'
            Assert-Condition ($listElement.Current.NativeWindowHandle -eq $listHwnd.ToInt32()) 'Monitor picker UIA handle does not match native LISTBOX'
            $focusedWindow = [WinShortUiAcceptance.Native]::FocusedWindowForThread($controlCenterHwnd)
            Assert-Condition ($focusedWindow -eq $listHwnd) 'Monitor picker LISTBOX does not own native keyboard focus'
            $labels = @([WinShortUiAcceptance.Native]::ListLabels($listHwnd))
            Assert-Condition (($labels.Count -eq 2) -and ($labels -contains 'Primary') -and ($labels -contains 'Cursor position')) "Monitor picker choices are not exactly Primary and Cursor position: $($labels -join ', ')"
            $pickerRectangle = Get-Rect ([WinShortUiAcceptance.Native]::WindowRect($pickerHostHwnd))
            $controlRectangle = Get-Rect ([WinShortUiAcceptance.Native]::WindowRect($controlCenterHwnd))
            Assert-Condition ($pickerRectangle.Width -le [Math]::Ceiling($controlRectangle.Width * 0.60)) 'picker is wider than the bounded design surface'
            Assert-Condition ($pickerRectangle.Width -le [Math]::Ceiling(400 * $dpi / 96.0) + 4) 'picker exceeds the 400 DIP maximum'
            $pickerPath = Join-Path $scenarioDirectory "picker-$pickerTheme.png"
            Save-Bitmap (Capture-WindowBitmap $pickerHostHwnd $pickerRectangle) $pickerPath
            [WinShortUiAcceptance.Native]::PostEscape($listHwnd) | Out-Null
            Wait-Until {
                [WinShortUiAcceptance.Native]::FindChildWindow($controlCenterHwnd, 'WinShort.ControlCenterPicker') -eq [IntPtr]::Zero
            } 'Monitor picker did not close after Escape' | Out-Null
            Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process

            $window = [System.Windows.Automation.AutomationElement]::FromHandle($controlCenterHwnd)
            $audioNavigation = Get-UiaByName $window 'Audio'
            Invoke-UiaElement $audioNavigation
            $audioWindow = Wait-Until {
                $current = [System.Windows.Automation.AutomationElement]::FromHandle($controlCenterHwnd)
                if ($null -eq (Get-UiaByNamePrefix $current 'Windows playback device')) { return $null }
                $current
            } 'Audio page did not render'
            $audioNodes = @($audioWindow.FindAll(
                [System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.Condition]::TrueCondition
            ))
            $currentAppShortcuts = @($audioNodes | Where-Object {
                $_.Current.Name -like 'Mute current app*' -or
                $_.Current.Name -like 'Current app volume*'
            })
            Assert-Condition ($currentAppShortcuts.Count -ge 3) 'current-app shortcut controls are missing from Audio page'
            $emptyStatus = @($audioNodes | Where-Object {
                $_.Current.Name -eq 'No controllable app' -or
                $_.Current.HelpText -eq 'Switch to another app to control its audio'
            })
            Assert-Condition ($emptyStatus.Count -eq 0) 'empty current-app status card leaked into Audio page'
            $audioPath = Join-Path $scenarioDirectory "control-center-audio-$appearance.png"
            Save-Bitmap (Capture-WindowBitmap $controlCenterHwnd (Get-Rect ([WinShortUiAcceptance.Native]::WindowRect($controlCenterHwnd)))) $audioPath

            $workspacesNavigation = Get-UiaByName $audioWindow 'Workspaces'
            Invoke-UiaElement $workspacesNavigation
            $workspacesWindow = Wait-Until {
                $current = [System.Windows.Automation.AutomationElement]::FromHandle($controlCenterHwnd)
                if ($null -eq (Get-UiaByNamePrefix $current 'Move window to Special Desktop')) { return $null }
                $current
            } 'Workspaces page did not render'
            $workspaceNodes = @($workspacesWindow.FindAll(
                [System.Windows.Automation.TreeScope]::Descendants,
                [System.Windows.Automation.Condition]::TrueCondition
            ))
            $forbiddenName = 'Special Workspace'
            $leakedTerminology = @($workspaceNodes | Where-Object {
                $_.Current.Name -like "*$forbiddenName*" -or
                $_.Current.HelpText -like "*$forbiddenName*"
            })
            Assert-Condition ($leakedTerminology.Count -eq 0) 'old Special Workspace terminology leaked into Workspaces UIA copy'
            $workspacesPath = Join-Path $scenarioDirectory "control-center-workspaces-$appearance.png"
            Save-Bitmap (Capture-WindowBitmap $controlCenterHwnd (Get-Rect ([WinShortUiAcceptance.Native]::WindowRect($controlCenterHwnd)))) $workspacesPath
            $scenarioResults += [pscustomobject]@{
                Appearance = $appearance
                PickerTheme = $pickerTheme
                ControlCenterOverlayScreenshot = $controlCenterPath
                PickerScreenshot = $pickerPath
                AudioScreenshot = $audioPath
                WorkspacesScreenshot = $workspacesPath
                StatusRow = $geometry
                MonitorChoices = $labels
            }
        }
        finally {
            if ($null -ne $process) {
                if (-not $process.HasExited) {
                    $process.Kill()
                    $process.WaitForExit(5000)
                }
                try { Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process } catch { }
                $process.Dispose()
            }
        }
    }

    for ($index = 1; $index -lt $overlayResults.Count; $index++) {
        Assert-Condition ([Math]::Abs($overlayResults[$index].Width - $overlayResults[0].Width) -le 2) 'runtime overlay widths differ between appearance styles'
        Assert-Condition ([Math]::Abs($overlayResults[$index].Height - $overlayResults[0].Height) -le 2) 'runtime overlay heights differ between appearance styles'
    }

    $summary = [pscustomobject]@{
        ResultRoot = $ResultRoot
        Binary = $ExePath
        Acceptance = [pscustomobject]@{
            RuntimeOverlay = $overlayResults
            ControlCenterAndPickers = $scenarioResults
        }
    }
    $summaryPath = Join-Path $ResultRoot 'summary.json'
    $summary | ConvertTo-Json -Depth 8 | Set-Content -Path $summaryPath -Encoding UTF8
    Write-Output "UI acceptance passed: $summaryPath"
    exit 0
}
catch {
    Write-Error $_
    Write-Error "UI acceptance artifacts: $ResultRoot"
    exit 1
}
