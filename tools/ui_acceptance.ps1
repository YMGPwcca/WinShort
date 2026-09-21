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
$AcceptanceShowDeterministicOverlayMessage = [uint32]0x8003
$AcceptanceHideAllOverlaysMessage = [uint32]0x8004

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
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
if (-not ('WinShortUiAcceptance.PatternBackdrop' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Drawing;
using System.Threading;
using System.Drawing.Drawing2D;
using System.Windows.Forms;

namespace WinShortUiAcceptance {
    public static class PatternBackdrop {
        private static readonly object Gate = new object();
        private static Thread thread;
        private static PatternForm underForm;
        private static PatternForm topForm;

        [System.Runtime.InteropServices.DllImport("user32.dll")]
        private static extern bool SetWindowPos(
            IntPtr hwnd,
            IntPtr hwndInsertAfter,
            int x,
            int y,
            int width,
            int height,
            uint flags
        );

        public static bool Start(
            int left,
            int top,
            int width,
            int height,
            IntPtr overlay,
            int cardLeft,
            int cardTop,
            int cardWidth,
            int cardHeight,
            int cardRadius,
            bool underOutsideOnly
        ) {
            Stop();
            var ready = new ManualResetEvent(false);
            var worker = new Thread(() => Run(
                left,
                top,
                width,
                height,
                overlay,
                cardLeft,
                cardTop,
                cardWidth,
                cardHeight,
                cardRadius,
                underOutsideOnly,
                ready
            ));
            worker.IsBackground = true;
            worker.SetApartmentState(ApartmentState.STA);
            lock (Gate) {
                thread = worker;
            }
            worker.Start();
            if (!ready.WaitOne(5000)) {
                Stop();
                return false;
            }
            return true;
        }

        public static void Stop() {
            PatternForm under;
            PatternForm top;
            Thread worker;
            lock (Gate) {
                under = underForm;
                top = topForm;
                worker = thread;
            }
            if (under != null) {
                try {
                    if (!under.IsDisposed && under.IsHandleCreated) {
                        under.BeginInvoke(new Action(() => {
                            if (top != null && !top.IsDisposed) {
                                top.Close();
                            }
                            under.Close();
                        }));
                    }
                } catch (InvalidOperationException) {
                }
            }
            if (worker != null && worker != Thread.CurrentThread) {
                worker.Join(3000);
            }
            lock (Gate) {
                if (thread == worker) {
                    thread = null;
                    underForm = null;
                    topForm = null;
                }
            }
        }

        public static IntPtr Handle() {
            lock (Gate) {
                return underForm == null ? IntPtr.Zero : underForm.Handle;
            }
        }

        public static void LowerBelow(IntPtr overlay) {
            PatternForm under;
            PatternForm top;
            lock (Gate) {
                under = underForm;
                top = topForm;
            }
            if (under == null || top == null || under.IsDisposed || top.IsDisposed) {
                return;
            }
            var done = new ManualResetEvent(false);
            try {
                under.BeginInvoke(new Action(() => {
                    under.TopMost = true;
                    top.TopMost = true;
                    SetWindowPos(under.Handle, overlay, 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010);
                    SetWindowPos(overlay, new IntPtr(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010);
                    SetWindowPos(top.Handle, new IntPtr(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010);
                    done.Set();
                }));
                done.WaitOne(1000);
            } finally {
                done.Dispose();
            }
        }

        private static void Run(
            int left,
            int top,
            int width,
            int height,
            IntPtr overlay,
            int cardLeft,
            int cardTop,
            int cardWidth,
            int cardHeight,
            int cardRadius,
            bool underOutsideOnly,
            ManualResetEvent ready
        ) {
            using (var under = new PatternForm(
                left,
                top,
                width,
                height,
                underOutsideOnly,
                cardLeft,
                cardTop,
                cardWidth,
                cardHeight,
                cardRadius
            ))
            using (var outside = new PatternForm(
                left,
                top,
                width,
                height,
                true,
                cardLeft,
                cardTop,
                cardWidth,
                cardHeight,
                cardRadius
            )) {
                lock (Gate) {
                    underForm = under;
                    topForm = outside;
                }
                under.Show();
                outside.Show();
                SetWindowPos(under.Handle, overlay, left, top, width, height, 0x0010 | 0x0040);
                SetWindowPos(overlay, new IntPtr(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010);
                SetWindowPos(outside.Handle, new IntPtr(-1), left, top, width, height, 0x0010 | 0x0040);
                ready.Set();
                Application.Run(under);
                lock (Gate) {
                    if (underForm == under) {
                        underForm = null;
                    }
                    if (topForm == outside) {
                        topForm = null;
                    }
                }
            }
        }

        private sealed class PatternForm : Form {
            public PatternForm(
                int left,
                int top,
                int width,
                int height,
                bool outsideOnly,
                int cardLeft,
                int cardTop,
                int cardWidth,
                int cardHeight,
                int cardRadius
            ) {
                SetStyle(
                    ControlStyles.AllPaintingInWmPaint
                        | ControlStyles.UserPaint
                        | ControlStyles.OptimizedDoubleBuffer,
                    true
                );
                FormBorderStyle = FormBorderStyle.None;
                ShowInTaskbar = false;
                StartPosition = FormStartPosition.Manual;
                Bounds = new Rectangle(left, top, width, height);
                TopMost = true;
                BackColor = Color.FromArgb(8, 28, 100);
                if (outsideOnly) {
                    var visibleRegion = new Region(new Rectangle(0, 0, width, height));
                    using (var cardPath = new GraphicsPath()) {
                        var diameter = cardRadius * 2;
                        cardPath.AddArc(cardLeft, cardTop, diameter, diameter, 180, 90);
                        cardPath.AddArc(cardLeft + cardWidth - diameter, cardTop, diameter, diameter, 270, 90);
                        cardPath.AddArc(cardLeft + cardWidth - diameter, cardTop + cardHeight - diameter, diameter, diameter, 0, 90);
                        cardPath.AddArc(cardLeft, cardTop + cardHeight - diameter, diameter, diameter, 90, 90);
                        cardPath.CloseFigure();
                        visibleRegion.Exclude(cardPath);
                    }
                    Region = visibleRegion;
                }
            }

            protected override bool ShowWithoutActivation {
                get { return true; }
            }

            protected override CreateParams CreateParams {
                get {
                    var value = base.CreateParams;
                    value.ExStyle |= 0x08000000 | 0x00000080;
                    return value;
                }
            }

            protected override void OnPaint(PaintEventArgs e) {
                e.Graphics.Clear(Color.FromArgb(8, 28, 100));
                e.Graphics.SmoothingMode = SmoothingMode.None;
                for (var x = 0; x < ClientSize.Width; x += 8) {
                    using (var brush = new SolidBrush(
                        (x / 8) % 2 == 0
                            ? Color.FromArgb(235, 245, 255)
                            : Color.FromArgb(8, 28, 100)
                    )) {
                        e.Graphics.FillRectangle(brush, x, 0, 8, ClientSize.Height);
                    }
                }
                using (var pen = new Pen(Color.FromArgb(230, 80, 40), 2.0f)) {
                    for (var y = 8; y < ClientSize.Height; y += 32) {
                        e.Graphics.DrawLine(pen, 0, y, ClientSize.Width, y);
                    }
                }
            }
        }
    }
}
'@ -ReferencedAssemblies @('System.Windows.Forms.dll', 'System.Drawing.dll')
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

    $candidates = [WinShortUiAcceptance.Native]::FindVisibleWindowsForProcess(
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
    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function New-ProcessStartInfo {
    param(
        [string]$Executable,
        [string]$DataDirectory,
        [string]$PickerTheme,
        [bool]$AcceptanceTrigger,
        [bool]$ForceOpaque = $false,
        [bool]$ForceCompositionFailure = $false,
        [bool]$ForceRenderFailure = $false
    )
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $Executable
    $info.WorkingDirectory = $RepoRoot
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.EnvironmentVariables['WINSHORT_DATA_DIR'] = $DataDirectory
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_THEME'] = $PickerTheme
    $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_NO_EXTERNAL'] = '1'
    $info.EnvironmentVariables.Remove('WINSHORT_UI_ACCEPTANCE_FORCE_OPAQUE') | Out-Null
    $info.EnvironmentVariables.Remove('WINSHORT_UI_ACCEPTANCE_FORCE_COMPOSITION_FAILURE') | Out-Null
    $info.EnvironmentVariables.Remove('WINSHORT_UI_ACCEPTANCE_FORCE_RENDER_FAILURE') | Out-Null
    if ($AcceptanceTrigger) {
        $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE'] = 'show-deterministic-acceptance-overlay'
    } else {
        $info.EnvironmentVariables.Remove('WINSHORT_UI_ACCEPTANCE') | Out-Null
    }
    if ($ForceOpaque) {
        $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_FORCE_OPAQUE'] = '1'
    }
    if ($ForceCompositionFailure) {
        $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_FORCE_COMPOSITION_FAILURE'] = '1'
    }
    if ($ForceRenderFailure) {
        $info.EnvironmentVariables['WINSHORT_UI_ACCEPTANCE_FORCE_RENDER_FAILURE'] = '1'
    }
    return $info
}

function Start-WinShort {
    param(
        [string]$DataDirectory,
        [string]$PickerTheme,
        [bool]$AcceptanceTrigger,
        [bool]$ForceOpaque = $false,
        [bool]$ForceCompositionFailure = $false,
        [bool]$ForceRenderFailure = $false
    )
    $info = New-ProcessStartInfo `
        $ExePath `
        $DataDirectory `
        $PickerTheme `
        $AcceptanceTrigger `
        $ForceOpaque `
        $ForceCompositionFailure `
        $ForceRenderFailure
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
        [double]$Radius,
        [ValidateRange(0, 2)]
        [int]$FringePx = 2,
        [ValidateRange(0, 32)]
        [int]$ColorTolerance = 12,
        [string]$HeatmapPath = ''
    )
    Assert-Condition ($Baseline.Width -eq $Shown.Width -and $Baseline.Height -eq $Shown.Height) 'overlay capture dimensions changed'
    $outsideChanged = 0
    $outsidePixels = 0
    $insideChanged = 0
    $insidePixels = 0
    $fringeChanged = 0
    $fringePixels = 0
    $fringeInset = $Inset - $FringePx
    $fringeRadius = $Radius + $FringePx
    $heatmap = $null
    $heatmapWritten = $false
    if (-not [string]::IsNullOrWhiteSpace($HeatmapPath)) {
        $heatmap = New-Object System.Drawing.Bitmap($Baseline.Width, $Baseline.Height)
    }
    try {
        for ($y = 0; $y -lt $Baseline.Height; $y++) {
            for ($x = 0; $x -lt $Baseline.Width; $x++) {
                $before = $Baseline.GetPixel($x, $y)
                $after = $Shown.GetPixel($x, $y)
                $difference = [Math]::Max(
                    [Math]::Abs($before.R - $after.R),
                    [Math]::Max([Math]::Abs($before.G - $after.G), [Math]::Abs($before.B - $after.B))
                )
                $body = Test-RoundedRectPoint $x $y $Inset $Inset ($Baseline.Width - $Inset - 1) ($Baseline.Height - $Inset - 1) $Radius
                $withinFringe = $body -or (
                    Test-RoundedRectPoint `
                        $x `
                        $y `
                        $fringeInset `
                        $fringeInset `
                        ($Baseline.Width - $fringeInset - 1) `
                        ($Baseline.Height - $fringeInset - 1) `
                        $fringeRadius
                )
                if ($body) {
                    $insidePixels++
                    if ($difference -gt $ColorTolerance) { $insideChanged++ }
                } elseif ($withinFringe) {
                    $fringePixels++
                    if ($difference -gt $ColorTolerance) { $fringeChanged++ }
                } else {
                    $outsidePixels++
                    if ($difference -gt $ColorTolerance) { $outsideChanged++ }
                }
                if ($null -ne $heatmap) {
                    $heatColor = [System.Drawing.Color]::FromArgb(255, 0, 0, 0)
                    if ($difference -gt $ColorTolerance) {
                        if ($body) {
                            $heatColor = [System.Drawing.Color]::FromArgb(255, 40, 100, 255)
                        } elseif ($withinFringe) {
                            $heatColor = [System.Drawing.Color]::FromArgb(255, 255, 210, 0)
                        } else {
                            $heatColor = [System.Drawing.Color]::FromArgb(255, 255, 30, 30)
                        }
                    }
                    $heatmap.SetPixel($x, $y, $heatColor)
                }
            }
        }
        if ($outsideChanged -gt 0 -and $null -ne $heatmap) {
            $heatmap.Save($HeatmapPath, [System.Drawing.Imaging.ImageFormat]::Png)
            $heatmapWritten = $true
            $heatmap.Dispose()
            $heatmap = $null
        }
    }
    finally {
        if ($null -ne $heatmap) {
            $heatmap.Dispose()
        }
    }
    return [pscustomobject]@{
        OutsideChanged = $outsideChanged
        OutsidePixels = $outsidePixels
        OutsideRatio = if ($outsidePixels -eq 0) { 0.0 } else { $outsideChanged / $outsidePixels }
        InsideChanged = $insideChanged
        InsidePixels = $insidePixels
        FringeChanged = $fringeChanged
        FringePixels = $fringePixels
        ColorTolerance = $ColorTolerance
        AntialiasFringePx = $FringePx
        HeatmapPath = if ($heatmapWritten) { $HeatmapPath } else { $null }
    }
}

function New-ComparatorTestBitmap {
    param(
        [int]$Width,
        [int]$Height,
        [System.Drawing.Color]$Color
    )
    $bitmap = New-Object System.Drawing.Bitmap($Width, $Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.Clear($Color)
    }
    finally {
        $graphics.Dispose()
    }
    return $bitmap
}

function Invoke-ComparatorRegressionTests {
    $baseline = New-ComparatorTestBitmap 64 64 ([System.Drawing.Color]::FromArgb(255, 12, 24, 48))
    $clean = $null
    $fringe = $null
    $backing = $null
    $broad = $null
    $heatmapPath = Join-Path $ResultRoot 'comparator-rectangular-backing-heatmap.png'
    try {
        $clean = $baseline.Clone()
        $clean.SetPixel(32, 32, [System.Drawing.Color]::White)
        $cleanResult = Compare-OverlayOutsideBody `
            -Baseline $baseline `
            -Shown $clean `
            -Inset 0 `
            -Radius 14
        Assert-Condition ($cleanResult.OutsideChanged -eq 0) 'clean rounded comparator regression failed'

        $fringe = $baseline.Clone()
        $fringe.SetPixel(3, 3, [System.Drawing.Color]::White)
        $fringeResult = Compare-OverlayOutsideBody `
            -Baseline $baseline `
            -Shown $fringe `
            -Inset 0 `
            -Radius 14
        Assert-Condition ($fringeResult.OutsideChanged -eq 0 -and $fringeResult.FringeChanged -eq 1) 'one-pixel antialias fringe comparator regression failed'

        $backing = $baseline.Clone()
        for ($y = 0; $y -lt 8; $y++) {
            for ($x = 0; $x -lt 8; $x++) {
                $backing.SetPixel($x, $y, [System.Drawing.Color]::White)
            }
        }
        if (Test-Path $heatmapPath) {
            Remove-Item $heatmapPath -Force
        }
        $backingResult = Compare-OverlayOutsideBody `
            -Baseline $baseline `
            -Shown $backing `
            -Inset 0 `
            -Radius 14 `
            -HeatmapPath $heatmapPath
        Assert-Condition ($backingResult.OutsideChanged -gt 0) 'rectangular backing comparator regression passed unexpectedly'
        Assert-Condition (Test-Path $heatmapPath) 'contamination heatmap was not produced'
        Remove-Item $heatmapPath -Force

        $broad = $baseline.Clone()
        for ($y = 0; $y -lt 3; $y++) {
            for ($x = 0; $x -lt 3; $x++) {
                $broad.SetPixel($x, $y, [System.Drawing.Color]::White)
            }
        }
        $broadResult = Compare-OverlayOutsideBody `
            -Baseline $baseline `
            -Shown $broad `
            -Inset 0 `
            -Radius 14
        Assert-Condition ($broadResult.OutsideChanged -gt 0) 'broad three-pixel contamination comparator regression passed unexpectedly'
    }
    finally {
        foreach ($bitmap in @($baseline, $clean, $fringe, $backing, $broad)) {
            if ($null -ne $bitmap) {
                $bitmap.Dispose()
            }
        }
        if (Test-Path $heatmapPath) {
            Remove-Item $heatmapPath -Force
        }
    }
    Write-Output "Comparator regression tests passed (fringe=$($fringeResult.AntialiasFringePx)px, tolerance=$($fringeResult.ColorTolerance))"
}

function Get-PixelLuma {
    param([System.Drawing.Color]$Color)
    return (0.2126 * $Color.R) + (0.7152 * $Color.G) + (0.0722 * $Color.B)
}

function Measure-EdgeEnergy {
    param(
        [System.Drawing.Bitmap]$Bitmap,
        [System.Drawing.Rectangle]$Rectangle
    )
    $left = [Math]::Max(0, $Rectangle.Left)
    $top = [Math]::Max(0, $Rectangle.Top)
    $right = [Math]::Min($Bitmap.Width, $Rectangle.Right)
    $bottom = [Math]::Min($Bitmap.Height, $Rectangle.Bottom)
    $energy = 0.0
    $samples = 0
    for ($y = $top; $y -lt $bottom; $y++) {
        for ($x = $left; $x -lt $right; $x++) {
            $current = Get-PixelLuma $Bitmap.GetPixel($x, $y)
            if ($x + 1 -lt $right) {
                $energy += [Math]::Abs($current - (Get-PixelLuma $Bitmap.GetPixel($x + 1, $y)))
                $samples++
            }
            if ($y + 1 -lt $bottom) {
                $energy += [Math]::Abs($current - (Get-PixelLuma $Bitmap.GetPixel($x, $y + 1)))
                $samples++
            }
        }
    }
    if ($samples -eq 0) {
        return 0.0
    }
    return $energy / $samples
}

function Measure-BackdropBlur {
    param(
        [System.Drawing.Bitmap]$Baseline,
        [System.Drawing.Bitmap]$Shown,
        [int]$CardWidth,
        [int]$CardHeight,
        [int]$ContextMargin
    )
    $inside = New-Object System.Drawing.Rectangle(
        ($ContextMargin + 2),
        ($ContextMargin + 20),
        12,
        ([Math]::Max(1, $CardHeight - 40))
    )
    $outside = New-Object System.Drawing.Rectangle(
        4,
        ($ContextMargin + 20),
        40,
        ([Math]::Max(1, $CardHeight - 40))
    )
    $insideBaseline = Measure-EdgeEnergy $Baseline $inside
    $insideShown = Measure-EdgeEnergy $Shown $inside
    $outsideBaseline = Measure-EdgeEnergy $Baseline $outside
    $outsideShown = Measure-EdgeEnergy $Shown $outside
    return [pscustomobject]@{
        InsideBaseline = $insideBaseline
        InsideShown = $insideShown
        InsideSofteningRatio = $insideShown / [Math]::Max($insideBaseline, 1.0)
        OutsideBaseline = $outsideBaseline
        OutsideShown = $outsideShown
        OutsidePreservationRatio = $outsideShown / [Math]::Max($outsideBaseline, 1.0)
        InsideRegion = $inside
        OutsideRegion = $outside
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

function Get-OverlayContextRectangle {
    param(
        [System.Drawing.Rectangle]$Rectangle,
        [int]$Margin
    )
    return [System.Drawing.Rectangle]::FromLTRB(
        ($Rectangle.Left - $Margin),
        ($Rectangle.Top - $Margin),
        ($Rectangle.Right + $Margin),
        ($Rectangle.Bottom + $Margin)
    )
}

function Invoke-HistoricalDwmReplay {
    param([string]$Root)
    if (-not (Test-Path $Root)) {
        return [pscustomobject]@{
            Available = $false
            Status = 'unavailable'
            Root = $Root
            Results = @()
        }
    }
    $replayRoot = Join-Path $ResultRoot 'historical-dwm-replay'
    New-Item -ItemType Directory -Path $replayRoot -Force | Out-Null
    $results = @()
    foreach ($appearance in @('system', 'dark', 'light')) {
        $scenarioRoot = Join-Path $Root $appearance
        $shownPath = Join-Path $scenarioRoot "runtime-overlay-$appearance.png"
        $baselinePath = Join-Path $scenarioRoot "runtime-overlay-$appearance-baseline.png"
        if (-not (Test-Path $shownPath) -or -not (Test-Path $baselinePath)) {
            return [pscustomobject]@{
                Available = $false
                Status = "unavailable: missing $appearance capture pair"
                Root = $Root
                Results = @()
            }
        }
        $baseline = [System.Drawing.Bitmap]::FromFile($baselinePath)
        $shown = [System.Drawing.Bitmap]::FromFile($shownPath)
        try {
            $result = Compare-OverlayOutsideBody `
                -Baseline $baseline `
                -Shown $shown `
                -Inset 0 `
                -Radius 14 `
                -HeatmapPath (Join-Path $replayRoot "runtime-overlay-$appearance-heatmap.png")
            Assert-Condition ($result.OutsideChanged -gt 0) "historical DWM $appearance capture passed the strict outside-card oracle"
            $results += [pscustomobject]@{
                Appearance = $appearance
                StrictFailureObserved = $true
                OutsideChanged = $result.OutsideChanged
                OutsidePixels = $result.OutsidePixels
                OutsideRatio = $result.OutsideRatio
                AntialiasFringePx = $result.AntialiasFringePx
                ColorTolerance = $result.ColorTolerance
                Heatmap = $result.HeatmapPath
            }
        }
        finally {
            $baseline.Dispose()
            $shown.Dispose()
        }
    }
    return [pscustomobject]@{
        Available = $true
        Status = 'historical captures failed strict oracle as expected'
        Root = $Root
        Results = $results
    }
}

function Invoke-ForcedOverlayScenario {
    param(
        [string]$Name,
        [string]$Appearance,
        [bool]$ForceOpaque,
        [bool]$ForceCompositionFailure
    )
    $scenarioDirectory = Join-Path $ResultRoot $Name
    $dataDirectory = Join-Path $scenarioDirectory 'data'
    New-Item -ItemType Directory -Path $dataDirectory -Force | Out-Null
    $config = @"
schema_version = 10

[overlay]
enabled = true
duration_ms = 10000
position = "bottom-right"
monitor = "primary"
scale = 1.0
opacity = 1.0
appearance = "$Appearance"
show_external_audio_changes = false
"@
    Write-Utf8NoBom (Join-Path $dataDirectory 'config.toml') $config

    $process = $null
    $baseline = $null
    $shown = $null
    try {
        $pickerTheme = if ($Appearance -eq 'light') { 'light' } else { 'dark' }
        $process = Start-WinShort `
            $dataDirectory `
            $pickerTheme `
            $true `
            $ForceOpaque `
            $ForceCompositionFailure
        Assert-Condition ($null -ne $process) "could not start $Name release binary"
        Start-Sleep -Milliseconds 1800
        $overlayHwnd = Wait-Until {
            if ($process.HasExited) { return $null }
            Get-DeterministicOverlayHwnd $process
        } "$Name overlay did not appear"
        $mainHwnd = Wait-Until {
            if ($process.HasExited) { return $null }
            $candidate = [WinShortUiAcceptance.Native]::FindMessageWindowForProcess($process.Id, 'WinShort.Main')
            if ($candidate -eq [IntPtr]::Zero) { return $null }
            $candidate
        } "$Name main message window did not appear"
        $overlayRectangle = Get-Rect ([WinShortUiAcceptance.Native]::WindowRect($overlayHwnd))
        $dpi = [WinShortUiAcceptance.Native]::Dpi($overlayHwnd)
        $radius = [Math]::Max(1, [int][Math]::Round(14 * $dpi / 96.0))
        $contextMargin = 48
        $contextRectangle = Get-OverlayContextRectangle $overlayRectangle $contextMargin
        Assert-Condition ([WinShortUiAcceptance.PatternBackdrop]::Start(
            $contextRectangle.Left,
            $contextRectangle.Top,
            $contextRectangle.Width,
            $contextRectangle.Height,
            $overlayHwnd,
            $contextMargin,
            $contextMargin,
            $overlayRectangle.Width,
            $overlayRectangle.Height,
            $radius,
            $true
        )) "$Name deterministic backdrop did not start"

        [WinShortUiAcceptance.Native]::PostMessageTo($mainHwnd, $AcceptanceHideAllOverlaysMessage) | Out-Null
        Wait-Until {
            ([WinShortUiAcceptance.Native]::FindVisibleWindowsForProcess(
                $process.Id,
                'WinShort.Overlay'
            ).Count -eq 0)
        } "$Name overlay did not hide for baseline capture" | Out-Null
        Start-Sleep -Milliseconds 100
        $baseline = Capture-Bitmap $overlayRectangle

        [WinShortUiAcceptance.Native]::PostMessageTo($mainHwnd, $AcceptanceShowDeterministicOverlayMessage) | Out-Null
        $overlayHwnd = Wait-Until {
            if ($process.HasExited) { return $null }
            Get-DeterministicOverlayHwnd $process
        } "$Name overlay did not reappear"
        Start-Sleep -Milliseconds 180
        [WinShortUiAcceptance.PatternBackdrop]::LowerBelow($overlayHwnd)
        $shown = Capture-Bitmap $overlayRectangle
        $result = Compare-OverlayOutsideBody `
            -Baseline $baseline `
            -Shown $shown `
            -Inset 0 `
            -Radius $radius `
            -HeatmapPath (Join-Path $scenarioDirectory "runtime-overlay-$Name-heatmap.png")
        $opaqueRegion = New-Object System.Drawing.Rectangle(
            2,
            20,
            12,
            ([Math]::Max(1, $overlayRectangle.Height - 40))
        )
        $opaqueEdgeEnergy = Measure-EdgeEnergy $shown $opaqueRegion
        Assert-Condition ($opaqueEdgeEnergy -lt 2.0) "$Name fallback exposed high-frequency backdrop detail"
        $screenshotPath = Join-Path $scenarioDirectory "runtime-overlay-$Name.png"
        $baselinePath = Join-Path $scenarioDirectory "runtime-overlay-$Name-baseline.png"
        Save-Bitmap $baseline $baselinePath
        $baseline = $null
        Save-Bitmap $shown $screenshotPath
        $shown = $null
        Assert-Condition ($result.InsideChanged -gt 20) "$Name overlay did not repaint (inside changed=$($result.InsideChanged))"
        Assert-Condition ($result.OutsideChanged -eq 0) "$Name fallback contaminated pixels outside the strict rounded card"
        Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process
        $fallbackLogObserved = $false
        if ($ForceCompositionFailure) {
            $logs = Get-Content (Join-Path $dataDirectory 'logs\*.log') -Raw -ErrorAction SilentlyContinue
            $fallbackLogObserved = $logs -match 'Composition unavailable; using opaque D2D fallback'
            Assert-Condition $fallbackLogObserved "$Name did not log the opaque Composition-failure fallback"
        }
        return [pscustomobject]@{
            Name = $Name
            Appearance = $Appearance
            Mode = if ($ForceCompositionFailure) { 'composition-failure-fallback' } else { 'forced-opaque-composition' }
            BlurEnabled = $false
            Width = $overlayRectangle.Width
            Height = $overlayRectangle.Height
            OutsideChanged = $result.OutsideChanged
            OutsidePixels = $result.OutsidePixels
            OutsideRatio = $result.OutsideRatio
            AntialiasFringePx = $result.AntialiasFringePx
            ColorTolerance = $result.ColorTolerance
            InsideEdgeEnergy = $opaqueEdgeEnergy
            BaselineInsideEdgeEnergy = $null
            Screenshot = $screenshotPath
            Heatmap = $result.HeatmapPath
            FallbackLogObserved = $fallbackLogObserved
        }
    }
    finally {
        [WinShortUiAcceptance.PatternBackdrop]::Stop()
        if ($null -ne $baseline) { $baseline.Dispose() }
        if ($null -ne $shown) { $shown.Dispose() }
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

function Invoke-RenderFailureVisibilityScenario {
    param([string]$Name)

    $scenarioDirectory = Join-Path $ResultRoot $Name
    $dataDirectory = Join-Path $scenarioDirectory 'data'
    New-Item -ItemType Directory -Path $dataDirectory -Force | Out-Null
    $config = @"
schema_version = 10

[overlay]
enabled = true
duration_ms = 10000
position = "bottom-right"
monitor = "primary"
scale = 1.0
opacity = 1.0
appearance = "dark"
show_external_audio_changes = false
"@
    Write-Utf8NoBom (Join-Path $dataDirectory 'config.toml') $config

    $process = $null
    try {
        $process = Start-WinShort $dataDirectory 'dark' $true $false $false $true
        Assert-Condition ($null -ne $process) "could not start $Name release binary"
        $mainHwnd = Wait-Until {
            if ($process.HasExited) { return $null }
            $candidate = [WinShortUiAcceptance.Native]::FindMessageWindowForProcess($process.Id, 'WinShort.Main')
            if ($candidate -eq [IntPtr]::Zero) { return $null }
            $candidate
        } "$Name main message window did not appear"

        [WinShortUiAcceptance.Native]::PostMessageTo($mainHwnd, $AcceptanceShowDeterministicOverlayMessage) | Out-Null
        Start-Sleep -Milliseconds 350
        $visible = @([WinShortUiAcceptance.Native]::FindVisibleWindowsForProcess(
            $process.Id,
            'WinShort.Overlay'
        ))
        Assert-Condition ($visible.Count -eq 0) "$Name exposed an overlay HWND after forced render failure"

        Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process
        $logs = Get-Content (Join-Path $dataDirectory 'logs\*.log') -Raw -ErrorAction SilentlyContinue
        $failureLogged = $logs -match 'overlay show failed: forced acceptance overlay render failure'
        Assert-Condition $failureLogged "$Name did not exercise the forced render-failure path"

        return [pscustomobject]@{
            Name = $Name
            VisibleOverlayCount = $visible.Count
            FailureLogged = $failureLogged
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

$existing = Get-Process -Name 'winshort' -ErrorAction SilentlyContinue
Assert-Condition ($null -eq $existing) 'an existing winshort.exe is running; refusing to touch the user process'

Invoke-ComparatorRegressionTests
$historicalRoot = Join-Path $RepoRoot 'target\ui-acceptance-results\20260903-053756'
$historicalReplay = Invoke-HistoricalDwmReplay $historicalRoot

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
        $baseline = $null
        $shown = $null
        $baselineContext = $null
        $shownContext = $null
        try {
            $process = Start-WinShort $dataDirectory $pickerTheme $true
            Assert-Condition ($null -ne $process) 'could not start release binary'
            Start-Sleep -Milliseconds 1800

            $overlayHwnd = Wait-Until {
                if ($process.HasExited) { return $null }
                Get-DeterministicOverlayHwnd $process
            } "runtime overlay did not appear for $appearance"
            $mainHwnd = Wait-Until {
                if ($process.HasExited) { return $null }
                $candidate = [WinShortUiAcceptance.Native]::FindMessageWindowForProcess($process.Id, 'WinShort.Main')
                if ($candidate -eq [IntPtr]::Zero) { return $null }
                $candidate
            } "main message window did not appear for $appearance"
            $overlayRectangle = Get-Rect ([WinShortUiAcceptance.Native]::WindowRect($overlayHwnd))
            $dpi = [WinShortUiAcceptance.Native]::Dpi($overlayHwnd)
            $inset = 0
            $radius = [Math]::Max(1, [int][Math]::Round(14 * $dpi / 96.0))
            $contextMargin = 48
            $contextRectangle = Get-OverlayContextRectangle $overlayRectangle $contextMargin
            Assert-Condition ([WinShortUiAcceptance.PatternBackdrop]::Start(
                $contextRectangle.Left,
                $contextRectangle.Top,
                $contextRectangle.Width,
                $contextRectangle.Height,
                $overlayHwnd,
                $contextMargin,
                $contextMargin,
                $overlayRectangle.Width,
                $overlayRectangle.Height,
                $radius,
                $false
            )) "deterministic backdrop did not start for $appearance"

            [WinShortUiAcceptance.Native]::PostMessageTo($mainHwnd, $AcceptanceHideAllOverlaysMessage) | Out-Null
            Wait-Until {
                (Get-DeterministicOverlayHwnd $process) -eq $null
            } 'overlay did not hide for baseline capture' | Out-Null
            Start-Sleep -Milliseconds 100
            $baseline = Capture-Bitmap $overlayRectangle
            $baselineContext = Capture-Bitmap $contextRectangle

            [WinShortUiAcceptance.Native]::PostMessageTo($mainHwnd, $AcceptanceShowDeterministicOverlayMessage) | Out-Null
            $overlayHwnd = Wait-Until {
                if ($process.HasExited) { return $null }
                Get-DeterministicOverlayHwnd $process
            } "runtime overlay did not reappear for $appearance"
            Start-Sleep -Milliseconds 180
            [WinShortUiAcceptance.PatternBackdrop]::LowerBelow($overlayHwnd)
            $shown = Capture-Bitmap $overlayRectangle
            $shownContext = Capture-Bitmap $contextRectangle
            $outside = Compare-OverlayOutsideBody `
                -Baseline $baseline `
                -Shown $shown `
                -Inset $inset `
                -Radius $radius `
                -HeatmapPath (Join-Path $scenarioDirectory "runtime-overlay-$appearance-heatmap.png")
            $blurMetric = Measure-BackdropBlur `
                -Baseline $baselineContext `
                -Shown $shownContext `
                -CardWidth $overlayRectangle.Width `
                -CardHeight $overlayRectangle.Height `
                -ContextMargin $contextMargin
            $baselinePath = Join-Path $scenarioDirectory "runtime-overlay-$appearance-baseline.png"
            $shownPath = Join-Path $scenarioDirectory "runtime-overlay-$appearance.png"
            Save-Bitmap $baseline $baselinePath
            $baseline = $null
            Save-Bitmap $shown $shownPath
            $shown = $null
            Save-Bitmap $baselineContext (Join-Path $scenarioDirectory "runtime-overlay-$appearance-context-baseline.png")
            $baselineContext = $null
            Save-Bitmap $shownContext (Join-Path $scenarioDirectory "runtime-overlay-$appearance-context.png")
            $shownContext = $null
            Assert-Condition ($outside.InsideChanged -gt 20) "runtime overlay did not repaint for $appearance"
            Assert-Condition ($outside.OutsideChanged -eq 0) "runtime overlay contaminated pixels outside its strict rounded card for $appearance"
            Assert-Condition ($blurMetric.OutsideBaseline -gt 10 -and $blurMetric.InsideBaseline -gt 10) "deterministic backdrop lacked usable contrast for $appearance (outside=$($blurMetric.OutsideBaseline), inside=$($blurMetric.InsideBaseline))"
            Assert-Condition ($blurMetric.OutsidePreservationRatio -gt 0.70) "outside deterministic backdrop feature was not preserved for $appearance"
            Assert-Condition ($blurMetric.InsideSofteningRatio -lt 0.90) "inside deterministic backdrop feature was not measurably softened for $appearance"
            $overlayResults += [pscustomobject]@{
                Appearance = $appearance
                Width = $overlayRectangle.Width
                Height = $overlayRectangle.Height
                Dpi = $dpi
                OutsideChanged = $outside.OutsideChanged
                OutsidePixels = $outside.OutsidePixels
                OutsideChangedRatio = $outside.OutsideRatio
                FringeChanged = $outside.FringeChanged
                FringePixels = $outside.FringePixels
                AntialiasFringePx = $outside.AntialiasFringePx
                ColorTolerance = $outside.ColorTolerance
                InsideChanged = $outside.InsideChanged
                BlurMetric = $blurMetric
                Screenshot = $shownPath
                ContextScreenshot = Join-Path $scenarioDirectory "runtime-overlay-$appearance-context.png"
                Heatmap = $outside.HeatmapPath
            }
            Copy-ScenarioLogs $dataDirectory $scenarioDirectory $process

            [WinShortUiAcceptance.PatternBackdrop]::Stop()
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
            [WinShortUiAcceptance.PatternBackdrop]::Stop()
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

    $fallbackResults = @()
    $fallbackResults += Invoke-ForcedOverlayScenario `
        -Name 'fallback-forced-opaque' `
        -Appearance 'dark' `
        -ForceOpaque $true `
        -ForceCompositionFailure $false
    $fallbackResults += Invoke-ForcedOverlayScenario `
        -Name 'fallback-composition-failure' `
        -Appearance 'dark' `
        -ForceOpaque $false `
        -ForceCompositionFailure $true
    foreach ($fallback in $fallbackResults) {
        Assert-Condition ([Math]::Abs($fallback.Width - $overlayResults[0].Width) -le 2) "$($fallback.Name) width differs from Composition card"
        Assert-Condition ([Math]::Abs($fallback.Height - $overlayResults[0].Height) -le 2) "$($fallback.Name) height differs from Composition card"
    }

    $renderFailureResult = Invoke-RenderFailureVisibilityScenario -Name 'render-failure-remains-hidden'

    for ($index = 1; $index -lt $overlayResults.Count; $index++) {
        Assert-Condition ([Math]::Abs($overlayResults[$index].Width - $overlayResults[0].Width) -le 2) 'runtime overlay widths differ between appearance styles'
        Assert-Condition ([Math]::Abs($overlayResults[$index].Height - $overlayResults[0].Height) -le 2) 'runtime overlay heights differ between appearance styles'
    }

    $summary = [pscustomobject]@{
        ResultRoot = $ResultRoot
        Binary = $ExePath
        Comparator = [pscustomobject]@{
            AntialiasFringePx = 2
            ColorTolerance = 12
            RegressionTests = 'passed: clean, one-pixel fringe, rectangular backing, three-pixel contamination'
        }
        HistoricalDwmReplay = $historicalReplay
        Fallback = $fallbackResults
        RenderFailureVisibility = $renderFailureResult
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
