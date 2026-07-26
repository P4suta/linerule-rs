# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

<#
.SYNOPSIS
Exercise the complete resident shell through Windows UI Automation.

.DESCRIPTION
Stages the real portable layout, reserves one shortcut from another process,
checks the Fluent conflict presentation and transactional rollback, drives the
notification-area icon and its exact three-item menu, rejects a second
instance, exercises all nine registered shortcuts, verifies show/hide telemetry
and persisted JSON, and measures clean shutdown. Intended for an interactive
Windows 11 CI runner.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$Executable,

    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$SettingsExecutable,

    [ValidateScript({
        [string]::IsNullOrEmpty($_) -or
        (Test-Path -LiteralPath $_ -PathType Leaf)
    })]
    [string]$HardwareEvidence = "",

    [string]$OutputDirectory = "artifacts/shell"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class LineruleShellTestNative
{
    private const uint KeyUp = 0x0002;

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool RegisterHotKey(
        IntPtr window,
        int id,
        uint modifiers,
        uint virtualKey);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool UnregisterHotKey(IntPtr window, int id);

    [DllImport("user32.dll")]
    private static extern void keybd_event(
        byte virtualKey,
        byte scanCode,
        uint flags,
        UIntPtr extraInfo);

    [StructLayout(LayoutKind.Sequential)]
    public struct Point
    {
        public int X;
        public int Y;
    }

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetCursorPos(out Point point);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool SetCursorPos(int x, int y);

    public static void FocusNotificationArea()
    {
        const byte leftWindows = 0x5B;
        const byte keyB = 0x42;
        keybd_event(leftWindows, 0, 0, UIntPtr.Zero);
        keybd_event(keyB, 0, 0, UIntPtr.Zero);
        keybd_event(keyB, 0, KeyUp, UIntPtr.Zero);
        keybd_event(leftWindows, 0, KeyUp, UIntPtr.Zero);
    }
}
"@

$conflictId = 0x4C52
$rollbackProbeId = 0x4C53
$updateConflictId = 0x4C54
$modControlShift = 0x0002 -bor 0x0004
$keyA = 0x41
$keyB = 0x42
$keyJ = 0x4A
$expectedMenu = @("Show/Hide", "Shortcut settings...", "Exit")
$expectedHotkeys = [ordered]@{
    cycle_mode = "Ctrl+Shift+A"
    cycle_effect = "Ctrl+Shift+B"
    toggle_on_off = "Ctrl+Shift+C"
    thicker = "Ctrl+Shift+D"
    thinner = "Ctrl+Shift+E"
    more_opaque = "Ctrl+Shift+F"
    less_opaque = "Ctrl+Shift+G"
    toggle_guide = "Ctrl+Shift+H"
    quit = "Ctrl+Shift+I"
}

function Invoke-WinApp {
    param([Parameter(Mandatory = $true)][string]$Arguments)

    $result = & mise exec "npm:@microsoft/winappcli" --command (
        "winapp $Arguments"
    ) 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "winapp failed: $Arguments`n$($result -join "`n")"
    }
    return $result -join "`n"
}

function Find-NamedElement {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [int]$ProcessId = 0
    )

    $condition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::NameProperty,
        $Name)
    $matches = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        $condition)
    foreach ($match in $matches) {
        try {
            if ($ProcessId -eq 0 -or $match.Current.ProcessId -eq $ProcessId) {
                return $match
            }
        }
        catch [System.Windows.Automation.ElementNotAvailableException] {
            continue
        }
    }
    return $null
}

function Wait-NamedElement {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [int]$ProcessId = 0,
        [int]$TimeoutMilliseconds = 10000
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $element = Find-NamedElement -Name $Name -ProcessId $ProcessId
        if ($null -ne $element) {
            return $element
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "UI Automation element '$Name' did not appear within $TimeoutMilliseconds ms"
}

function Focus-TrayIcon {
    $icon = Find-NamedElement -Name "linerule"
    if ($null -ne $icon) {
        try {
            $icon.SetFocus()
            return $icon
        }
        catch {
            # Notification-area elements can be present in the UIA tree while
            # their overflow flyout is closed. Fall back to the keyboard path.
        }
    }

    [LineruleShellTestNative]::FocusNotificationArea()
    Start-Sleep -Milliseconds 250
    for ($attempt = 0; $attempt -lt 64; $attempt++) {
        $focused = [System.Windows.Automation.AutomationElement]::FocusedElement
        $name = try { $focused.Current.Name } catch { "" }
        if ($name -eq "linerule") {
            return $focused
        }

        if ($name -match "(?i)hidden icons|notification overflow") {
            [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
            Start-Sleep -Milliseconds 350
            $icon = Find-NamedElement -Name "linerule"
            if ($null -ne $icon) {
                try {
                    $icon.SetFocus()
                    return $icon
                }
                catch {
                    # Continue walking the overflow area when Windows exposes
                    # an element before it becomes focusable.
                }
            }
        }
        [System.Windows.Forms.SendKeys]::SendWait("{RIGHT}")
        Start-Sleep -Milliseconds 75
    }
    throw "The linerule notification-area icon was not reachable through Win+B"
}

function Save-Screen {
    param([Parameter(Mandatory = $true)][string]$Path)

    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen(
            $bounds.Left,
            $bounds.Top,
            0,
            0,
            $bounds.Size,
            [System.Drawing.CopyPixelOperation]::SourceCopy)
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Open-TrayMenu {
    param(
        [Parameter(Mandatory = $true)][int]$ResidentProcessId,
        [Parameter(Mandatory = $true)][string]$ScreenshotPath
    )

    $null = Focus-TrayIcon
    [System.Windows.Forms.SendKeys]::SendWait("+{F10}")
    $exitItem = Wait-NamedElement `
        -Name "Exit" `
        -ProcessId $ResidentProcessId `
        -TimeoutMilliseconds 5000

    $walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
    $menu = $exitItem
    while ($null -ne $menu) {
        if ($menu.Current.ControlType -eq [System.Windows.Automation.ControlType]::Menu) {
            break
        }
        $menu = $walker.GetParent($menu)
    }
    if ($null -eq $menu) {
        throw "The tray Exit item had no UI Automation menu ancestor"
    }

    $menuItemCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::MenuItem)
    $items = $menu.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        $menuItemCondition)
    $actual = @(
        foreach ($item in $items) {
            $name = $item.Current.Name
            if (-not [string]::IsNullOrWhiteSpace($name)) {
                $name
            }
        }
    )
    if (($actual -join "|") -ne ($expectedMenu -join "|")) {
        throw "Tray menu was '$($actual -join "', '")'; expected '$($expectedMenu -join "', '")'"
    }
    Save-Screen -Path $ScreenshotPath
    return [pscustomobject]@{
        ExitItem = $exitItem
        SettingsItem = (
            Find-NamedElement `
                -Name "Shortcut settings..." `
                -ProcessId $ResidentProcessId
        )
        Names = $actual
    }
}

function Invoke-MenuItem {
    param(
        [Parameter(Mandatory = $true)]
        [System.Windows.Automation.AutomationElement]$Item
    )

    try {
        $pattern = $Item.GetCurrentPattern(
            [System.Windows.Automation.InvokePattern]::Pattern)
        ([System.Windows.Automation.InvokePattern]$pattern).Invoke()
    }
    catch [System.InvalidOperationException] {
        $Item.SetFocus()
        [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    }
}

function Wait-NewProcess {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [int[]]$ExcludedIds,
        [int]$TimeoutMilliseconds = 15000
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $process = Get-Process -Name $Name -ErrorAction SilentlyContinue |
            Where-Object { $_.Id -notin $ExcludedIds } |
            Select-Object -First 1
        if ($null -ne $process) {
            return $process
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Process '$Name' did not start within $TimeoutMilliseconds ms"
}

function Get-StateModes {
    param([Parameter(Mandatory = $true)][string]$LogDirectory)

    $modes = [System.Collections.Generic.List[string]]::new()
    foreach ($file in Get-ChildItem `
        -LiteralPath $LogDirectory `
        -Filter "events.jsonl*" `
        -File `
        -ErrorAction SilentlyContinue) {
        foreach ($line in Get-Content -LiteralPath $file.FullName) {
            try {
                $event = $line | ConvertFrom-Json
                if ($event.fields.message -eq "state changed") {
                    $modes.Add([string]$event.fields.mode)
                }
            }
            catch {
                # A non-blocking writer may be between bytes; retry on the next poll.
            }
        }
    }
    return $modes
}

function Get-StateActions {
    param([Parameter(Mandatory = $true)][string]$LogDirectory)

    $actions = [System.Collections.Generic.List[string]]::new()
    foreach ($file in Get-ChildItem `
        -LiteralPath $LogDirectory `
        -Filter "events.jsonl*" `
        -File `
        -ErrorAction SilentlyContinue) {
        foreach ($line in Get-Content -LiteralPath $file.FullName) {
            try {
                $event = $line | ConvertFrom-Json
                if ($event.fields.message -eq "state changed") {
                    $actions.Add([string]$event.fields.action)
                }
            }
            catch {
                # A non-blocking writer may be between bytes; retry on the next poll.
            }
        }
    }
    return $actions
}

function Get-RenderTickCount {
    param([Parameter(Mandatory = $true)][string]$LogDirectory)

    $count = 0
    foreach ($file in Get-ChildItem `
        -LiteralPath $LogDirectory `
        -Filter "events.jsonl*" `
        -File `
        -ErrorAction SilentlyContinue) {
        foreach ($line in Get-Content -LiteralPath $file.FullName) {
            try {
                $event = $line | ConvertFrom-Json
                if ($event.fields.message -eq "render tick processed") {
                    $count++
                }
            }
            catch {
                # A non-blocking writer may be between bytes; retry on the next poll.
            }
        }
    }
    return $count
}

function Wait-MonitorFollow {
    param(
        [Parameter(Mandatory = $true)][string]$LogDirectory,
        [Parameter(Mandatory = $true)]$Monitor
    )

    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        foreach ($file in Get-ChildItem `
            -LiteralPath $LogDirectory `
            -Filter "events.jsonl*" `
            -File `
            -ErrorAction SilentlyContinue) {
            foreach ($line in Get-Content -LiteralPath $file.FullName) {
                try {
                    $event = $line | ConvertFrom-Json
                    if (
                        $event.fields.message -eq "active monitor changed" -and
                        $event.fields.new_left -eq $Monitor.Left -and
                        $event.fields.new_top -eq $Monitor.Top -and
                        $event.fields.new_width -eq $Monitor.Width -and
                        $event.fields.new_height -eq $Monitor.Height
                    ) {
                        return
                    }
                }
                catch {
                    # A non-blocking writer may be between bytes; retry.
                }
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw (
        "Resident shell did not follow monitor " +
        "$($Monitor.DeviceName) at $($Monitor.Left),$($Monitor.Top)"
    )
}

function Test-MixedDpiMonitors {
    param(
        [Parameter(Mandatory = $true)][string]$EvidencePath,
        [Parameter(Mandatory = $true)][string]$LogDirectory,
        [Parameter(Mandatory = $true)][string]$ScreenshotDirectory
    )

    $evidence = Get-Content -LiteralPath $EvidencePath -Raw |
        ConvertFrom-Json
    $monitors = @($evidence.monitors)
    if ($monitors.Count -lt 2) {
        throw "Mixed-DPI shell exercise requires at least two monitors"
    }
    $dpiValues = @(
        $monitors |
            ForEach-Object { "$($_.DpiX)x$($_.DpiY)" } |
            Sort-Object -Unique
    )
    if ($dpiValues.Count -lt 2) {
        throw "Mixed-DPI shell exercise requires distinct monitor DPI values"
    }

    $original = [LineruleShellTestNative+Point]::new()
    if (-not [LineruleShellTestNative]::GetCursorPos([ref]$original)) {
        throw [System.ComponentModel.Win32Exception]::new(
            [System.Runtime.InteropServices.Marshal]::GetLastWin32Error(),
            "GetCursorPos failed before mixed-DPI exercise")
    }
    try {
        for ($index = 0; $index -lt $monitors.Count; $index++) {
            $monitor = $monitors[$index]
            $x = [int]($monitor.Left + [Math]::Floor($monitor.Width / 2))
            $y = [int]($monitor.Top + [Math]::Floor($monitor.Height / 2))
            if (-not [LineruleShellTestNative]::SetCursorPos($x, $y)) {
                throw [System.ComponentModel.Win32Exception]::new(
                    [System.Runtime.InteropServices.Marshal]::GetLastWin32Error(),
                    "SetCursorPos failed for $($monitor.DeviceName)")
            }
            Wait-MonitorFollow `
                -LogDirectory $LogDirectory `
                -Monitor $monitor
            Start-Sleep -Milliseconds 250
            Save-Screen -Path (
                Join-Path $ScreenshotDirectory "mixed-dpi-$index.png"
            )
        }
    }
    finally {
        [LineruleShellTestNative]::SetCursorPos(
            $original.X,
            $original.Y) | Out-Null
    }
    return $monitors.Count
}

function Wait-RenderIdle {
    param(
        [Parameter(Mandatory = $true)][string]$LogDirectory,
        [int]$TimeoutMilliseconds = 10000
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    $previous = Get-RenderTickCount -LogDirectory $LogDirectory
    $stableSince = [DateTime]::UtcNow
    do {
        Start-Sleep -Milliseconds 100
        $current = Get-RenderTickCount -LogDirectory $LogDirectory
        if ($current -ne $previous) {
            $previous = $current
            $stableSince = [DateTime]::UtcNow
        } elseif (
            ([DateTime]::UtcNow - $stableSince).TotalMilliseconds -ge 1000
        ) {
            return $current
        }
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Resident renderer did not reach an idle tick count"
}

function Wait-StateMode {
    param(
        [Parameter(Mandatory = $true)][string]$LogDirectory,
        [Parameter(Mandatory = $true)][string]$Expected,
        [Parameter(Mandatory = $true)][int]$PreviousCount
    )

    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $modes = @(Get-StateModes -LogDirectory $LogDirectory)
        if ($modes.Count -gt $PreviousCount -and $modes[-1] -eq $Expected) {
            return $modes.Count
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "State-change telemetry did not advance to $Expected"
}

function Wait-StateAction {
    param(
        [Parameter(Mandatory = $true)][string]$LogDirectory,
        [Parameter(Mandatory = $true)][string]$Expected,
        [Parameter(Mandatory = $true)][int]$PreviousCount
    )

    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $actions = @(Get-StateActions -LogDirectory $LogDirectory)
        if (
            $actions.Count -gt $PreviousCount -and
            $actions[-1].Contains(
                $Expected,
                [System.StringComparison]::Ordinal)
        ) {
            return $actions.Count
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "State-change telemetry did not advance to action $Expected"
}

function Assert-Preferences {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedLastActive
    )

    $preferences = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
    if ($preferences.schema_version -ne 1) {
        throw "Persisted schema_version was not 1"
    }
    if (
        $preferences.ruler.last_active -ne $ExpectedLastActive -or
        $preferences.ruler.effect -ne "dim_black" -or
        $preferences.ruler.thickness -ne 28 -or
        $preferences.ruler.opacity -ne 170 -or
        $preferences.ruler.blur -ne 111
    ) {
        throw "Persisted ruler settings changed unexpectedly"
    }
    $properties = @($preferences.hotkeys.PSObject.Properties)
    if ($properties.Count -ne $expectedHotkeys.Count) {
        throw "Persisted hotkeys had $($properties.Count) entries; expected $($expectedHotkeys.Count)"
    }
    foreach ($command in $expectedHotkeys.Keys) {
        $actual = $preferences.hotkeys.$command
        if ($actual -ne $expectedHotkeys[$command]) {
            throw "Persisted $command was '$actual'; expected '$($expectedHotkeys[$command])'"
        }
    }
}

function Wait-Preferences {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$ExpectedLastActive
    )

    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        try {
            Assert-Preferences `
                -Path $Path `
                -ExpectedLastActive $ExpectedLastActive
            return
        }
        catch {
            if ([DateTime]::UtcNow -ge $deadline) {
                throw
            }
            Start-Sleep -Milliseconds 100
        }
    } while ($true)
}

function Wait-SettingsReady {
    param([Parameter(Mandatory = $true)]$Process)

    Invoke-WinApp (
        "ui wait-for `"SaveSettings`" --timeout 15000 -a $($Process.Id)"
    ) | Out-Null
}

function Set-CycleModeShortcut {
    param(
        [Parameter(Mandatory = $true)]$Process,
        [Parameter(Mandatory = $true)][string]$Keys,
        [Parameter(Mandatory = $true)][string]$ExpectedName
    )

    Invoke-WinApp (
        "ui invoke `"Shortcut_cycle_mode`" -a $($Process.Id)"
    ) | Out-Null
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
    Start-Sleep -Milliseconds 150
    $name = Invoke-WinApp (
        "ui get-property `"Shortcut_cycle_mode`" " +
        "--property Name --json -a $($Process.Id)"
    )
    if ($name -notmatch [regex]::Escape($ExpectedName)) {
        throw "cycle_mode did not record $ExpectedName`:`n$name"
    }
}

function Stop-TestProcess {
    param($Process)

    if ($null -ne $Process) {
        $live = Get-Process -Id $Process.Id -ErrorAction SilentlyContinue
        if ($null -ne $live) {
            Stop-Process -Id $live.Id -Force -ErrorAction SilentlyContinue
        }
    }
}

$sourceExecutable = [System.IO.Path]::GetFullPath($Executable)
$sourceSettingsExecutable = [System.IO.Path]::GetFullPath($SettingsExecutable)
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
[System.IO.Directory]::CreateDirectory($output) | Out-Null
[System.IO.File]::WriteAllText(
    (Join-Path $output "started.txt"),
    "linerule resident shell UIA started`n")

$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$testRoot = Join-Path (
    $temporaryRoot
) "linerule-shell-$([guid]::NewGuid().ToString('N'))"
$portableRoot = Join-Path $testRoot "portable"
$settingsRoot = Join-Path $portableRoot "settings"
$portableData = Join-Path $portableRoot "data"
$logDirectory = Join-Path $portableData "logs"
$preferencesPath = Join-Path $portableData "settings.json"
foreach ($directory in @($portableRoot, $settingsRoot, $portableData)) {
    [System.IO.Directory]::CreateDirectory($directory) | Out-Null
}
Copy-Item -LiteralPath $sourceExecutable `
    -Destination (Join-Path $portableRoot "linerule.exe")
Copy-Item -LiteralPath $sourceSettingsExecutable `
    -Destination (Join-Path $settingsRoot "linerule-settings.exe")
[System.IO.File]::WriteAllText((Join-Path $portableRoot "linerule.portable"), "")
[ordered]@{
    schema_version = 1
    ruler = [ordered]@{
        last_active = "horizontal"
        effect = "dim_black"
        thickness = 28
        opacity = 170
        blur = 111
    }
    hotkeys = $expectedHotkeys
} | ConvertTo-Json -Depth 4 | Set-Content `
    -LiteralPath $preferencesPath `
    -Encoding utf8

$resident = $null
$duplicate = $null
$settings = $null
$conflictRegistered = $false
$rollbackProbeRegistered = $false
$updateConflictRegistered = $false
$beforeSettingsIds = @(
    Get-Process -Name "linerule-settings" -ErrorAction SilentlyContinue |
        ForEach-Object Id
)
$portableExecutable = Join-Path $portableRoot "linerule.exe"
$mixedDpiMonitorCount = 0
$previousLogFilter = [Environment]::GetEnvironmentVariable(
    "LINERULE_LOG",
    [EnvironmentVariableTarget]::Process)
[Environment]::SetEnvironmentVariable(
    "LINERULE_LOG",
    "info,linerule_render_tick=trace",
    [EnvironmentVariableTarget]::Process)

try {
    $conflictRegistered = [LineruleShellTestNative]::RegisterHotKey(
        [IntPtr]::Zero,
        $conflictId,
        $modControlShift,
        $keyB)
    if (-not $conflictRegistered) {
        throw [System.ComponentModel.Win32Exception]::new(
            [System.Runtime.InteropServices.Marshal]::GetLastWin32Error(),
            "Could not reserve Ctrl+Shift+B for the external-conflict scenario")
    }

    $resident = Start-Process -FilePath $portableExecutable -PassThru
    $settings = Wait-NewProcess `
        -Name "linerule-settings" `
        -ExcludedIds $beforeSettingsIds
    Wait-SettingsReady -Process $settings
    $focused = Invoke-WinApp "ui get-focused --json -a $($settings.Id)"
    if ($focused -notmatch "Shortcut_cycle_effect") {
        throw "External conflict did not focus Shortcut_cycle_effect:`n$focused"
    }
    $status = Invoke-WinApp (
        "ui get-property `"SettingsStatus`" --property Name --json -a $($settings.Id)"
    )
    if ($status -notmatch "(?i)registered|conflict") {
        throw "External conflict was not exposed by the Fluent status control:`n$status"
    }
    Invoke-WinApp (
        "ui screenshot --output `"$output\external-conflict.png`" " +
        "--capture-screen -a $($settings.Id)"
    ) | Out-Null

    # Ctrl+Shift+A was registered before B failed. It must be free now, proving
    # the failed transaction rolled back every earlier registration.
    $rollbackProbeRegistered = [LineruleShellTestNative]::RegisterHotKey(
        [IntPtr]::Zero,
        $rollbackProbeId,
        $modControlShift,
        $keyA)
    if (-not $rollbackProbeRegistered) {
        throw "Ctrl+Shift+A remained registered after the shortcut transaction failed"
    }
    [LineruleShellTestNative]::UnregisterHotKey(
        [IntPtr]::Zero,
        $rollbackProbeId) | Out-Null
    $rollbackProbeRegistered = $false

    $conflictMenu = Open-TrayMenu `
        -ResidentProcessId $resident.Id `
        -ScreenshotPath (Join-Path $output "tray-during-conflict.png")
    [System.Windows.Forms.SendKeys]::SendWait("{ESC}")

    Invoke-WinApp "ui invoke `"CancelSettings`" -a $($settings.Id)" | Out-Null
    if (-not $settings.WaitForExit(5000)) {
        throw "Fluent settings did not exit after Cancel"
    }
    $settings = $null

    $exitMenu = Open-TrayMenu `
        -ResidentProcessId $resident.Id `
        -ScreenshotPath (Join-Path $output "tray-conflict-exit.png")
    $exitTimer = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-MenuItem -Item $exitMenu.ExitItem
    $remaining = [Math]::Max(
        0,
        1000 - [int][Math]::Ceiling($exitTimer.Elapsed.TotalMilliseconds))
    if ($remaining -eq 0 -or -not $resident.WaitForExit($remaining)) {
        throw "Conflict-mode resident shell did not exit within one second"
    }
    $exitTimer.Stop()
    if ($resident.ExitCode -ne 0) {
        throw "Conflict-mode resident shell exited with $($resident.ExitCode)"
    }
    $conflictExitMilliseconds = [int][Math]::Ceiling(
        $exitTimer.Elapsed.TotalMilliseconds)
    $resident = $null

    [LineruleShellTestNative]::UnregisterHotKey(
        [IntPtr]::Zero,
        $conflictId) | Out-Null
    $conflictRegistered = $false

    $resident = Start-Process -FilePath $portableExecutable -PassThru
    $null = Focus-TrayIcon

    $duplicate = Start-Process -FilePath $portableExecutable -PassThru
    if (-not $duplicate.WaitForExit(2000)) {
        throw "A second linerule instance did not reject startup promptly"
    }
    if ($duplicate.ExitCode -eq 0) {
        throw "A second linerule instance unexpectedly succeeded"
    }
    $duplicateExitCode = $duplicate.ExitCode
    $duplicate = $null

    $modeCount = @(Get-StateModes -LogDirectory $logDirectory).Count
    $null = Focus-TrayIcon
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    $modeCount = Wait-StateMode `
        -LogDirectory $logDirectory `
        -Expected "Horizontal" `
        -PreviousCount $modeCount
    $null = Focus-TrayIcon
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    $modeCount = Wait-StateMode `
        -LogDirectory $logDirectory `
        -Expected "Off" `
        -PreviousCount $modeCount

    Start-Sleep -Milliseconds 650
    Assert-Preferences `
        -Path $preferencesPath `
        -ExpectedLastActive "horizontal"

    $null = Focus-TrayIcon
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    $modeCount = Wait-StateMode `
        -LogDirectory $logDirectory `
        -Expected "Horizontal" `
        -PreviousCount $modeCount

    # Fail an in-app settings transaction against a real external owner. The
    # resident runtime must restore all nine previous registrations, reopen the
    # editor on the offending row, and keep the tray responsive.
    $updateConflictRegistered = [LineruleShellTestNative]::RegisterHotKey(
        [IntPtr]::Zero,
        $updateConflictId,
        $modControlShift,
        $keyJ)
    if (-not $updateConflictRegistered) {
        throw "Could not reserve Ctrl+Shift+J for the update-conflict scenario"
    }
    $settingsIds = @(
        Get-Process -Name "linerule-settings" -ErrorAction SilentlyContinue |
            ForEach-Object Id
    )
    $settingsMenu = Open-TrayMenu `
        -ResidentProcessId $resident.Id `
        -ScreenshotPath (Join-Path $output "tray-open-settings.png")
    if ($null -eq $settingsMenu.SettingsItem) {
        throw "The tray menu did not expose Shortcut settings..."
    }
    Invoke-MenuItem -Item $settingsMenu.SettingsItem
    $settings = Wait-NewProcess `
        -Name "linerule-settings" `
        -ExcludedIds $settingsIds
    Wait-SettingsReady -Process $settings
    Set-CycleModeShortcut `
        -Process $settings `
        -Keys "^+j" `
        -ExpectedName "Ctrl+Shift+J"
    $failedSettingsId = $settings.Id
    Invoke-WinApp "ui invoke `"SaveSettings`" -a $failedSettingsId" | Out-Null
    if (-not $settings.WaitForExit(5000)) {
        throw "The first settings process did not exit after Save"
    }
    $settings = Wait-NewProcess `
        -Name "linerule-settings" `
        -ExcludedIds ($settingsIds + @($failedSettingsId))
    Wait-SettingsReady -Process $settings
    $focused = Invoke-WinApp "ui get-focused --json -a $($settings.Id)"
    if ($focused -notmatch "Shortcut_cycle_mode") {
        throw "Failed update did not refocus Shortcut_cycle_mode:`n$focused"
    }
    Invoke-WinApp (
        "ui screenshot --output `"$output\update-conflict.png`" " +
        "--capture-screen -a $($settings.Id)"
    ) | Out-Null

    # The old Ctrl+Shift+A assignment must work after the failed transaction.
    [System.Windows.Forms.SendKeys]::SendWait("^+a")
    $modeCount = Wait-StateMode `
        -LogDirectory $logDirectory `
        -Expected "Vertical" `
        -PreviousCount $modeCount
    Invoke-WinApp "ui invoke `"CancelSettings`" -a $($settings.Id)" | Out-Null
    if (-not $settings.WaitForExit(5000)) {
        throw "Conflict settings did not exit after Cancel"
    }
    $settings = $null
    [LineruleShellTestNative]::UnregisterHotKey(
        [IntPtr]::Zero,
        $updateConflictId) | Out-Null
    $updateConflictRegistered = $false

    # Retry the same edit without the external owner and verify registration,
    # debounced persistence, and command delivery from the new chord.
    $settingsIds = @(
        Get-Process -Name "linerule-settings" -ErrorAction SilentlyContinue |
            ForEach-Object Id
    )
    $settingsMenu = Open-TrayMenu `
        -ResidentProcessId $resident.Id `
        -ScreenshotPath (Join-Path $output "tray-retry-settings.png")
    Invoke-MenuItem -Item $settingsMenu.SettingsItem
    $settings = Wait-NewProcess `
        -Name "linerule-settings" `
        -ExcludedIds $settingsIds
    Wait-SettingsReady -Process $settings
    Set-CycleModeShortcut `
        -Process $settings `
        -Keys "^+j" `
        -ExpectedName "Ctrl+Shift+J"
    Invoke-WinApp "ui invoke `"SaveSettings`" -a $($settings.Id)" | Out-Null
    if (-not $settings.WaitForExit(5000)) {
        throw "Retry settings did not exit after Save"
    }
    $settings = $null
    $expectedHotkeys["cycle_mode"] = "Ctrl+Shift+J"
    Wait-Preferences `
        -Path $preferencesPath `
        -ExpectedLastActive "vertical"

    [System.Windows.Forms.SendKeys]::SendWait("^+j")
    $modeCount = Wait-StateMode `
        -LogDirectory $logDirectory `
        -Expected "Horizontal" `
        -PreviousCount $modeCount
    $null = Focus-TrayIcon
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    $modeCount = Wait-StateMode `
        -LogDirectory $logDirectory `
        -Expected "Off" `
        -PreviousCount $modeCount

    # Exercise every remaining registered command through the OS hotkey path.
    # Net-zero pairs restore user preferences after proving both directions.
    [System.Windows.Forms.SendKeys]::SendWait("^+c")
    $modeCount = Wait-StateMode `
        -LogDirectory $logDirectory `
        -Expected "Horizontal" `
        -PreviousCount $modeCount

    if (-not [string]::IsNullOrEmpty($HardwareEvidence)) {
        $mixedDpiMonitorCount = Test-MixedDpiMonitors `
            -EvidencePath ([System.IO.Path]::GetFullPath($HardwareEvidence)) `
            -LogDirectory $logDirectory `
            -ScreenshotDirectory $output
    }

    foreach ($probe in @(
        [pscustomobject]@{ Keys = "^+b"; Action = "CycleEffect" },
        [pscustomobject]@{ Keys = "^+b"; Action = "CycleEffect" },
        [pscustomobject]@{ Keys = "^+b"; Action = "CycleEffect" },
        [pscustomobject]@{ Keys = "^+d"; Action = "BumpThickness(8)" },
        [pscustomobject]@{ Keys = "^+e"; Action = "BumpThickness(-8)" },
        [pscustomobject]@{ Keys = "^+f"; Action = "BumpOpacity(8)" },
        [pscustomobject]@{ Keys = "^+g"; Action = "BumpOpacity(-8)" }
    )) {
        [System.Windows.Forms.SendKeys]::SendWait($probe.Keys)
        $modeCount = Wait-StateAction `
            -LogDirectory $logDirectory `
            -Expected $probe.Action `
            -PreviousCount $modeCount
    }

    $guideBefore = Join-Path $output "guide-hidden.png"
    $guideVisible = Join-Path $output "guide-visible.png"
    Save-Screen -Path $guideBefore
    [System.Windows.Forms.SendKeys]::SendWait("^+h")
    Start-Sleep -Milliseconds 500
    Save-Screen -Path $guideVisible
    if (
        (Get-FileHash -Algorithm SHA256 $guideBefore).Hash -eq
        (Get-FileHash -Algorithm SHA256 $guideVisible).Hash
    ) {
        throw "Toggle-guide hotkey produced no visible screen change"
    }
    [System.Windows.Forms.SendKeys]::SendWait("^+h")
    Start-Sleep -Milliseconds 500

    [System.Windows.Forms.SendKeys]::SendWait("^+c")
    $modeCount = Wait-StateMode `
        -LogDirectory $logDirectory `
        -Expected "Off" `
        -PreviousCount $modeCount

    $idleTicksBefore = Wait-RenderIdle -LogDirectory $logDirectory
    if ($idleTicksBefore -eq 0) {
        throw "Render tick trace did not observe the exercised active runtime"
    }
    Start-Sleep -Seconds 10
    Start-Sleep -Milliseconds 250
    $idleTicksAfter = Get-RenderTickCount -LogDirectory $logDirectory
    if ($idleTicksAfter -ne $idleTicksBefore) {
        throw (
            "Off + hidden resident renderer advanced from " +
            "$idleTicksBefore to $idleTicksAfter ticks during ten-second idle"
        )
    }

    Wait-Preferences `
        -Path $preferencesPath `
        -ExpectedLastActive "horizontal"
    $cleanMenu = Open-TrayMenu `
        -ResidentProcessId $resident.Id `
        -ScreenshotPath (Join-Path $output "tray-clean.png")
    $exitTimer = [System.Diagnostics.Stopwatch]::StartNew()
    Invoke-MenuItem -Item $cleanMenu.ExitItem
    $remaining = [Math]::Max(
        0,
        1000 - [int][Math]::Ceiling($exitTimer.Elapsed.TotalMilliseconds))
    if ($remaining -eq 0 -or -not $resident.WaitForExit($remaining)) {
        throw "Resident shell did not exit within one second"
    }
    $exitTimer.Stop()
    if ($resident.ExitCode -ne 0) {
        throw "Resident shell exited with $($resident.ExitCode)"
    }
    $cleanExitMilliseconds = [int][Math]::Ceiling(
        $exitTimer.Elapsed.TotalMilliseconds)
    $resident = $null

    Assert-Preferences `
        -Path $preferencesPath `
        -ExpectedLastActive "horizontal"
    $logText = (
        Get-ChildItem -LiteralPath $logDirectory -Filter "events.jsonl*" -File |
            Get-Content
    ) -join "`n"
    if (
        $logText -match "COR_E_INVALIDOPERATION" -or
        $logText -match "CompositionDrawingSurface\.BeginDraw failed"
    ) {
        throw "Hidden HUD attempted to draw a zero-sized composition surface"
    }

    # Quit is the ninth command. Test it on a fresh resident so the tray Exit
    # measurement above remains an independent shutdown path.
    $resident = Start-Process -FilePath $portableExecutable -PassThru
    $null = Focus-TrayIcon
    $quitTimer = [System.Diagnostics.Stopwatch]::StartNew()
    [System.Windows.Forms.SendKeys]::SendWait("^+i")
    if (-not $resident.WaitForExit(1000)) {
        throw "Quit hotkey did not stop the resident shell within one second"
    }
    $quitTimer.Stop()
    if ($resident.ExitCode -ne 0) {
        throw "Quit hotkey exited resident shell with $($resident.ExitCode)"
    }
    $hotkeyQuitMilliseconds = [int][Math]::Ceiling(
        $quitTimer.Elapsed.TotalMilliseconds)
    $resident = $null

    [ordered]@{
        external_conflict_highlight = "cycle_effect"
        rollback_probe = "Ctrl+Shift+A"
        menu_items = $expectedMenu
        duplicate_exit_code = $duplicateExitCode
        failed_update_rollback = "Ctrl+Shift+A"
        saved_update = "Ctrl+Shift+J"
        validated_hotkeys = @(
            "cycle_mode",
            "cycle_effect",
            "toggle_on_off",
            "thicker",
            "thinner",
            "more_opaque",
            "less_opaque",
            "toggle_guide",
            "quit"
        )
        state_changes = @(Get-StateModes -LogDirectory $logDirectory)
        state_actions = @(Get-StateActions -LogDirectory $logDirectory)
        conflict_exit_ms = $conflictExitMilliseconds
        clean_exit_ms = $cleanExitMilliseconds
        hotkey_quit_ms = $hotkeyQuitMilliseconds
        mixed_dpi_monitors = $mixedDpiMonitorCount
        off_idle_seconds = 10
        off_idle_tick_delta = $idleTicksAfter - $idleTicksBefore
        persisted_schema_version = 1
    } | ConvertTo-Json -Depth 3 | Set-Content `
        -LiteralPath (Join-Path $output "results.json") `
        -Encoding utf8

    Write-Host (
        (
            "Resident shell UIA: passed; conflict_exit_ms={0}; " +
            "clean_exit_ms={1}; hotkey_quit_ms={2}"
        ) -f
        $conflictExitMilliseconds,
        $cleanExitMilliseconds,
        $hotkeyQuitMilliseconds)
}
finally {
    if ($rollbackProbeRegistered) {
        [LineruleShellTestNative]::UnregisterHotKey(
            [IntPtr]::Zero,
            $rollbackProbeId) | Out-Null
    }
    if ($conflictRegistered) {
        [LineruleShellTestNative]::UnregisterHotKey(
            [IntPtr]::Zero,
            $conflictId) | Out-Null
    }
    if ($updateConflictRegistered) {
        [LineruleShellTestNative]::UnregisterHotKey(
            [IntPtr]::Zero,
            $updateConflictId) | Out-Null
    }
    Stop-TestProcess -Process $settings
    Stop-TestProcess -Process $duplicate
    Stop-TestProcess -Process $resident
    [Environment]::SetEnvironmentVariable(
        "LINERULE_LOG",
        $previousLogFilter,
        [EnvironmentVariableTarget]::Process)

    if (Test-Path -LiteralPath $portableData -PathType Container) {
        $runtimeEvidence = Join-Path $output "runtime"
        [System.IO.Directory]::CreateDirectory($runtimeEvidence) | Out-Null
        foreach ($item in Get-ChildItem -LiteralPath $portableData) {
            Copy-Item -LiteralPath $item.FullName `
                -Destination $runtimeEvidence `
                -Recurse `
                -Force
        }
    }

    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    $temporaryPrefix = $temporaryRoot.TrimEnd("\") + "\"
    if ($resolvedTestRoot.StartsWith(
        $temporaryPrefix,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
