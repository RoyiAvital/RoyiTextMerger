[CmdletBinding()]
param(
    [switch] $SearchPanel,
    [switch] $SyntaxLanguages,
    [switch] $Spelling,
    [switch] $ReadOnlySave
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$Root = Split-Path -Parent $PSScriptRoot
$AppTarget = Join-Path $Root 'target\docs-app'
$AppExe = Join-Path $AppTarget 'release\RoyiTextMerger.exe'
$OutputDir = Join-Path $Root 'target\docs-assets'
New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
$Modes = @()
if ($SearchPanel) { $Modes += 'search' }
if ($SyntaxLanguages) { $Modes += 'syntax-languages' }
if ($Spelling) { $Modes += 'spelling' }
if ($ReadOnlySave) { $Modes += 'read-only-save' }
if ($Modes.Count -gt 1) { throw 'Choose only one screenshot mode.' }

function Write-Fixture([string] $Name, [string[]] $Lines) {
    $Path = Join-Path $OutputDir $Name
    if (Test-Path $Path) {
        (Get-Item $Path).IsReadOnly = $false
    }
    [IO.File]::WriteAllText($Path, ($Lines -join "`n") + "`n", (New-Object Text.UTF8Encoding $false))
    $Path
}

# Each screenshot compares a pair of generated files: the left one, then the right one.
$Fixtures = if ($Spelling) {
    @(
        (Write-Fixture 'Notes-v1.txt' @(
            'Recieve the package before the meeting starts.',
            'Spelling marks show misspelled words with a blue wave, like teh one here.',
            'Press Ctrl+. on a marked word to see suggestions first, then the commands.'
        )),
        (Write-Fixture 'Notes-v2.txt' @(
            'Receive the package before the meeting starts.',
            'Spelling marks show misspelled words with a blue wave, like the one here.',
            'Press Ctrl+. on a marked word to see suggestions first, then the commands.',
            'F8 moves to the next misspelling and Shift+F8 to the previous one.'
        ))
    )
} elseif ($ReadOnlySave) {
    # The right file gets the Read-only attribute; the merger refuses to save it.
    $Pair = @(
        (Write-Fixture 'Notes.txt' @(
            'This file can be saved.',
            'The right file has the Windows Read-only attribute.'
        )),
        (Write-Fixture 'ReadOnly.txt' @(
            'This file has the Windows Read-only attribute.',
            'An edit is made and Ctrl+S is pressed: nothing is written.'
        ))
    )
    (Get-Item $Pair[1]).IsReadOnly = $true
    $Pair
} else {
    @(
        (Write-Fixture 'shapes-v1.rs' @(
            'use std::f64::consts::PI;',
            '',
            '/// A shape on the drawing board.',
            '#[derive(Debug, Clone)]',
            'pub enum Shape {',
            '    Circle { radius: f64 },',
            '    Square { side: f64 },',
            '}',
            '',
            '// Only circles and squares for now.',
            'impl Shape {',
            '    pub fn area(&self) -> f64 {',
            '        match self {',
            '            Shape::Circle { radius } => PI * radius * radius,',
            '            Shape::Square { side } => side * side,',
            '        }',
            '    }',
            '',
            '    pub fn name(&self) -> &''static str {',
            '        match self {',
            '            Shape::Circle { .. } => "circle",',
            '            Shape::Square { .. } => "square",',
            '        }',
            '    }',
            '}',
            '',
            'fn main() {',
            '    let shapes = vec![Shape::Circle { radius: 1.0 }, Shape::Square { side: 2.0 }];',
            '    for shape in &shapes {',
            '        println!("{} has area {:.2}", shape.name(), shape.area());',
            '    }',
            '}'
        )),
        (Write-Fixture 'shapes-v2.rs' @(
            'use std::f64::consts::PI;',
            '',
            '/// A shape on the drawing board.',
            '#[derive(Debug, Clone, PartialEq)]',
            'pub enum Shape {',
            '    Circle { radius: f64 },',
            '    Square { side: f64 },',
            '    Rectangle { width: f64, height: f64 },',
            '}',
            '',
            'impl Shape {',
            '    pub fn area(&self) -> f64 {',
            '        match self {',
            '            Shape::Circle { radius } => PI * radius * radius,',
            '            Shape::Square { side } => side.powi(2),',
            '            Shape::Rectangle { width, height } => width * height,',
            '        }',
            '    }',
            '',
            '    pub fn name(&self) -> &''static str {',
            '        match self {',
            '            Shape::Circle { .. } => "circle",',
            '            Shape::Square { .. } => "square",',
            '            Shape::Rectangle { .. } => "rectangle",',
            '        }',
            '    }',
            '}',
            '',
            'fn main() {',
            '    let shapes = vec![',
            '        Shape::Circle { radius: 1.0 },',
            '        Shape::Square { side: 2.0 },',
            '        Shape::Rectangle { width: 2.0, height: 3.0 },',
            '    ];',
            '    for shape in &shapes {',
            '        println!("{} has area {:.2}", shape.name(), shape.area());',
            '    }',
            '}'
        ))
    )
}
$OutputName = if ($SearchPanel) {
    'search-panel.png'
} elseif ($SyntaxLanguages) {
    'syntax-language-picker.png'
} elseif ($Spelling) {
    'spelling.png'
} elseif ($ReadOnlySave) {
    'read-only-save.png'
} else {
    'merger-screenshot.png'
}
$Output = Join-Path $OutputDir $OutputName

& cargo build --release --locked -p rtm_app --target-dir $AppTarget
if ($LASTEXITCODE -ne 0) {
    throw "Application build failed with exit code $LASTEXITCODE."
}

if (-not ('DocsBuildCaptureV2' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DocsBuildCaptureV2 {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X; public int Y; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hwnd, IntPtr insertAfter, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint attach, uint attachTo, bool attachState);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("dwmapi.dll")] public static extern int DwmFlush();
}
'@
}

[void][DocsBuildCaptureV2]::SetProcessDPIAware()
if (Test-Path $Output) {
    Remove-Item $Output -Force
}

$Arguments = @($Fixtures | ForEach-Object { '"{0}"' -f $_ })
if ($Spelling) {
    $Arguments = @('--spelling', 'builtin') + $Arguments
}
$AppProcess = Start-Process -FilePath $AppExe -ArgumentList $Arguments -PassThru
try {
    if (-not $AppProcess.WaitForInputIdle(10000)) {
        throw "The application did not become idle while opening $($Fixtures -join ' and ')."
    }
    $AppProcess.Refresh()
    $Window = $AppProcess.MainWindowHandle
    if ($Window -eq [IntPtr]::Zero) {
        throw 'The application window handle was not available for the documentation screenshot.'
    }
    [void][DocsBuildCaptureV2]::ShowWindow($Window, 9)
    [void][DocsBuildCaptureV2]::SetWindowPos($Window, [IntPtr](-1), 0, 0, 0, 0, 0x0043)
    $ForegroundWindow = [DocsBuildCaptureV2]::GetForegroundWindow()
    [uint32]$IgnoredProcessId = 0
    $ForegroundThread = [DocsBuildCaptureV2]::GetWindowThreadProcessId($ForegroundWindow, [ref]$IgnoredProcessId)
    $TargetThread = [DocsBuildCaptureV2]::GetWindowThreadProcessId($Window, [ref]$IgnoredProcessId)
    $CurrentThread = [DocsBuildCaptureV2]::GetCurrentThreadId()
    $AttachedForeground = $false
    $AttachedTarget = $false
    try {
        if ($ForegroundThread -ne 0 -and $ForegroundThread -ne $CurrentThread) {
            $AttachedForeground = [DocsBuildCaptureV2]::AttachThreadInput($CurrentThread, $ForegroundThread, $true)
        }
        if ($TargetThread -ne 0 -and $TargetThread -ne $CurrentThread) {
            $AttachedTarget = [DocsBuildCaptureV2]::AttachThreadInput($CurrentThread, $TargetThread, $true)
        }
        [void][DocsBuildCaptureV2]::BringWindowToTop($Window)
        [void][DocsBuildCaptureV2]::SetForegroundWindow($Window)
    }
    finally {
        if ($AttachedTarget) {
            [void][DocsBuildCaptureV2]::AttachThreadInput($CurrentThread, $TargetThread, $false)
        }
        if ($AttachedForeground) {
            [void][DocsBuildCaptureV2]::AttachThreadInput($CurrentThread, $ForegroundThread, $false)
        }
    }
    if ([DocsBuildCaptureV2]::GetForegroundWindow() -ne $Window) {
        throw 'Windows did not bring the application window to the foreground; refusing an unrelated screenshot.'
    }
    if ($SearchPanel) {
        [System.Windows.Forms.SendKeys]::SendWait('^f')
        [System.Windows.Forms.SendKeys]::SendWait('Shape')
    }
    if ($SyntaxLanguages) {
        [System.Windows.Forms.SendKeys]::SendWait('^k')
        [System.Windows.Forms.SendKeys]::SendWait('m')
        [System.Windows.Forms.SendKeys]::SendWait('^a')
        [System.Windows.Forms.SendKeys]::SendWait('{BACKSPACE}')
    }
    if ($Spelling) {
        # The marks come from the spelling thread after the first frame; the caret stays
        # on the first word (left of the palette), so its wave and the palette show together.
        Start-Sleep -Milliseconds 800
        [System.Windows.Forms.SendKeys]::SendWait('^{HOME}')
        Start-Sleep -Milliseconds 300
        [System.Windows.Forms.SendKeys]::SendWait('^.')
        Start-Sleep -Milliseconds 800
    }
    if ($ReadOnlySave) {
        [System.Windows.Forms.SendKeys]::SendWait('^2')
        [System.Windows.Forms.SendKeys]::SendWait('^{END}')
        [System.Windows.Forms.SendKeys]::SendWait('{ENTER}A new line, not saved.')
        [System.Windows.Forms.SendKeys]::SendWait('^s')
        Start-Sleep -Milliseconds 500
    }
    # The inner changes and the highlights follow the first frame.
    Start-Sleep -Milliseconds 500
    [void][DocsBuildCaptureV2]::DwmFlush()

    $Client = New-Object 'DocsBuildCaptureV2+Rect'
    $Origin = New-Object 'DocsBuildCaptureV2+Point'
    if (-not [DocsBuildCaptureV2]::GetClientRect($Window, [ref]$Client) -or
        -not [DocsBuildCaptureV2]::ClientToScreen($Window, [ref]$Origin)) {
        throw 'Could not determine the application client area for the documentation screenshot.'
    }
    $Width = $Client.Right - $Client.Left
    $Height = $Client.Bottom - $Client.Top
    if ($Width -lt 800 -or $Height -lt 500) {
        throw "Application client area is unexpectedly small: ${Width}x${Height}."
    }

    $Bitmap = New-Object System.Drawing.Bitmap -ArgumentList $Width, $Height
    try {
        $Graphics = [System.Drawing.Graphics]::FromImage($Bitmap)
        try {
            $Graphics.CopyFromScreen($Origin.X, $Origin.Y, 0, 0, $Bitmap.Size)
            $Bitmap.Save($Output, [System.Drawing.Imaging.ImageFormat]::Png)
        }
        finally {
            $Graphics.Dispose()
        }
    }
    finally {
        $Bitmap.Dispose()
    }
}
finally {
    if (-not $AppProcess.HasExited) {
        # The refused save leaves unsaved changes: closing would only ask.
        if (-not $ReadOnlySave) {
            [void]$AppProcess.CloseMainWindow()
        }
        if ($ReadOnlySave -or -not $AppProcess.WaitForExit(5000)) {
            $AppProcess.Kill()
            $AppProcess.WaitForExit()
        }
    }
    $AppProcess.Dispose()
    if ($ReadOnlySave) {
        (Get-Item $Fixtures[1]).IsReadOnly = $false
    }
}

Write-Output $Output
