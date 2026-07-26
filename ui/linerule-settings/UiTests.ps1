# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

[CmdletBinding()]
param(
    [ValidateSet("x64", "ARM64")]
    [string]$Platform = $(
        if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "ARM64" } else { "x64" }
    ),

    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",

    [string]$OutputDirectory = (
        Join-Path $PSScriptRoot "TestResults\UI"
    ),

    [switch]$SkipBuild,
    [switch]$EnableHighContrast
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$commands = @(
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
$defaults = [ordered]@{
    cycle_mode = "Ctrl+Alt+R"
    cycle_effect = "Ctrl+Alt+E"
    toggle_on_off = "Ctrl+Alt+H"
    thicker = "Ctrl+Alt+Up"
    thinner = "Ctrl+Alt+Down"
    more_opaque = "Ctrl+Alt+Right"
    less_opaque = "Ctrl+Alt+Left"
    toggle_guide = "Ctrl+Alt+K"
    quit = "Ctrl+Alt+Q"
}
$recorded = [ordered]@{
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

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class LineruleHighContrastApi
{
    public const uint GetHighContrast = 0x0042;
    public const uint SetHighContrast = 0x0043;
    public const uint HighContrastOn = 0x00000001;
    public const uint SendChange = 0x00000002;

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct HighContrast
    {
        public uint Size;
        public uint Flags;
        public IntPtr DefaultScheme;
    }

    [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool SystemParametersInfo(
        uint action,
        uint parameter,
        ref HighContrast value,
        uint flags);

    [DllImport("kernel32.dll")]
    public static extern IntPtr LocalFree(IntPtr memory);
}
"@

function Get-HighContrastSnapshot {
    $value = [LineruleHighContrastApi+HighContrast]::new()
    $value.Size = [uint32][System.Runtime.InteropServices.Marshal]::SizeOf($value)
    if (-not [LineruleHighContrastApi]::SystemParametersInfo(
        [LineruleHighContrastApi]::GetHighContrast,
        $value.Size,
        [ref]$value,
        0
    )) {
        throw [System.ComponentModel.Win32Exception]::new(
            [System.Runtime.InteropServices.Marshal]::GetLastWin32Error())
    }

    try {
        $scheme = if ($value.DefaultScheme -eq [IntPtr]::Zero) {
            ""
        } else {
            [System.Runtime.InteropServices.Marshal]::PtrToStringUni(
                $value.DefaultScheme)
        }
        return [pscustomobject]@{
            Flags = $value.Flags
            Scheme = $scheme
        }
    }
    finally {
        if ($value.DefaultScheme -ne [IntPtr]::Zero) {
            [LineruleHighContrastApi]::LocalFree($value.DefaultScheme) |
                Out-Null
        }
    }
}

function Set-HighContrastSnapshot {
    param([Parameter(Mandatory)]$Snapshot)

    $value = [LineruleHighContrastApi+HighContrast]::new()
    $value.Size = [uint32][System.Runtime.InteropServices.Marshal]::SizeOf($value)
    $value.Flags = [uint32]$Snapshot.Flags
    $schemePointer = [IntPtr]::Zero
    try {
        if (-not [string]::IsNullOrEmpty($Snapshot.Scheme)) {
            $schemePointer =
                [System.Runtime.InteropServices.Marshal]::StringToHGlobalUni(
                    $Snapshot.Scheme)
            $value.DefaultScheme = $schemePointer
        }
        if (-not [LineruleHighContrastApi]::SystemParametersInfo(
            [LineruleHighContrastApi]::SetHighContrast,
            $value.Size,
            [ref]$value,
            [LineruleHighContrastApi]::SendChange
        )) {
            throw [System.ComponentModel.Win32Exception]::new(
                [System.Runtime.InteropServices.Marshal]::GetLastWin32Error())
        }
    }
    finally {
        if ($schemePointer -ne [IntPtr]::Zero) {
            [System.Runtime.InteropServices.Marshal]::FreeHGlobal($schemePointer)
        }
    }
}

function Invoke-MiseCommand {
    param([Parameter(Mandatory)][string]$Command)

    $output = & mise exec "npm:@microsoft/winappcli" --command $Command 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed: $Command`n$($output -join "`n")"
    }
    return $output
}

function Invoke-Ui {
    param(
        [Parameter(Mandatory)][string]$Arguments,
        [switch]$ReturnOutput
    )

    $output = Invoke-MiseCommand (
        "winapp ui $Arguments -a $script:TargetProcessId"
    )
    if ($ReturnOutput) {
        return $output -join "`n"
    }
}

function Assert-Focused {
    param([Parameter(Mandatory)][string]$AutomationId)

    $focused = Invoke-Ui "get-focused --json" -ReturnOutput
    if ($focused -notmatch [regex]::Escape($AutomationId)) {
        throw "Expected focus on $AutomationId, received:`n$focused"
    }
}

function Assert-NameContains {
    param(
        [Parameter(Mandatory)][string]$AutomationId,
        [Parameter(Mandatory)][string]$Value
    )

    $property = Invoke-Ui (
        "get-property `"$AutomationId`" --property Name --json"
    ) -ReturnOutput
    if ($property -notmatch [regex]::Escape($Value)) {
        throw "$AutomationId did not contain '$Value' in its accessible name:`n$property"
    }
}

function Send-Keys {
    param([Parameter(Mandatory)][string]$Keys)

    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
    Start-Sleep -Milliseconds 150
}

function Get-BuildOutput {
    $configurationRoot = Join-Path $PSScriptRoot "bin\$Platform\$Configuration"
    $framework = Get-ChildItem -LiteralPath $configurationRoot -Directory |
        Where-Object Name -Like "net*-windows*" |
        Sort-Object Name -Descending |
        Select-Object -First 1
    if (-not $framework) {
        throw "WinUI build output was not found below $configurationRoot"
    }

    $runtime = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }
    $output = Join-Path $framework.FullName $runtime
    if (Test-Path -LiteralPath $output -PathType Container) {
        return $output
    }
    return $framework.FullName
}

function Start-Settings {
    param(
        [Parameter(Mandatory)][string]$RequestPath,
        [Parameter(Mandatory)][string]$ResponsePath
    )

    $arguments = "--request `"$RequestPath`" --response `"$ResponsePath`""
    $launch = Invoke-MiseCommand (
        "winapp run `"$script:BuildOutput`" " +
        "--manifest `"$script:PackageManifest`" " +
        "--exe linerule-settings.exe --detach --json --args `"$arguments`""
    )
    $launchText = $launch -join "`n"
    $pidMatch = [regex]::Match(
        $launchText,
        '"(?:pid|processId|process_id)"\s*:\s*(\d+)',
        [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    if (-not $pidMatch.Success) {
        throw "winapp did not return the launched process id:`n$launchText"
    }
    $script:TargetProcessId = [int]$pidMatch.Groups[1].Value
    Invoke-Ui "wait-for `"SaveSettings`" --timeout 15000"
}

function Stop-Settings {
    if ($script:TargetProcessId -le 0) {
        return
    }
    $process = Get-Process -Id $script:TargetProcessId -ErrorAction SilentlyContinue
    if ($null -ne $process) {
        $process.CloseMainWindow() | Out-Null
        if (-not $process.WaitForExit(2000)) {
            Stop-Process -Id $script:TargetProcessId -Force
        }
    }
    $script:TargetProcessId = 0
}

function Wait-SettingsExit {
    $process = Get-Process `
        -Id $script:TargetProcessId `
        -ErrorAction SilentlyContinue
    if ($null -ne $process -and -not $process.WaitForExit(5000)) {
        throw "Settings process did not exit within five seconds."
    }
    $script:TargetProcessId = 0
}

if (-not $SkipBuild) {
    & (Join-Path $PSScriptRoot "BuildAndRun.ps1") `
        -Platform $Platform `
        -Configuration $Configuration `
        -SkipRun
    if ($LASTEXITCODE -ne 0) {
        throw "BuildAndRun.ps1 failed with exit code $LASTEXITCODE"
    }
}

$script:PackageManifest = Join-Path $PSScriptRoot "Package.appxmanifest"

$highContrastRestore = $null
if ($EnableHighContrast) {
    $highContrastRestore = Get-HighContrastSnapshot
    $enabled = [pscustomobject]@{
        Flags = [uint32](
            $highContrastRestore.Flags -bor
            [LineruleHighContrastApi]::HighContrastOn)
        Scheme = $highContrastRestore.Scheme
    }
    Set-HighContrastSnapshot -Snapshot $enabled
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    while (
        ((Get-HighContrastSnapshot).Flags -band
            [LineruleHighContrastApi]::HighContrastOn) -eq 0
    ) {
        if ([DateTime]::UtcNow -ge $deadline) {
            throw "High Contrast did not become active within five seconds."
        }
        Start-Sleep -Milliseconds 100
    }
}

$script:BuildOutput = Get-BuildOutput
$script:TargetProcessId = 0
$resolvedOutput = [System.IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $resolvedOutput -Force | Out-Null

$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$protocolDirectory = Join-Path (
    $temporaryRoot
) "linerule-uia-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $protocolDirectory | Out-Null

try {
    $requestPath = Join-Path $protocolDirectory "request.json"
    $responsePath = Join-Path $protocolDirectory "response.json"
    [ordered]@{
        hotkeys = $defaults
        error = "The shortcut is already registered by another application."
        highlight = "cycle_effect"
    } | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath $requestPath -Encoding utf8

    Start-Settings -RequestPath $requestPath -ResponsePath $responsePath

    foreach ($command in $commands) {
        Invoke-Ui "wait-for `"Shortcut_$command`" --timeout 5000"
        Invoke-Ui "wait-for `"Card_$command`" --timeout 5000"
    }
    foreach ($id in @(
        "SettingsStatus",
        "ResetShortcuts",
        "CancelSettings",
        "SaveSettings"
    )) {
        Invoke-Ui "wait-for `"$id`" --timeout 5000"
    }

    Assert-Focused "Shortcut_cycle_effect"
    Invoke-Ui (
        "screenshot --output `"$resolvedOutput\initial.png`" --capture-screen"
    )
    if ($EnableHighContrast) {
        Invoke-Ui (
            "screenshot --output `"$resolvedOutput\high-contrast.png`" --capture-screen"
        )
    }

    Invoke-Ui "focus `"Shortcut_cycle_mode`""
    foreach ($command in $commands | Select-Object -Skip 1) {
        Send-Keys "{TAB}"
        Assert-Focused "Shortcut_$command"
    }
    foreach ($automationId in @(
        "ResetShortcuts",
        "CancelSettings",
        "SaveSettings"
    )) {
        Send-Keys "{TAB}"
        Assert-Focused $automationId
    }

    Invoke-Ui "invoke `"Shortcut_cycle_mode`""
    Assert-NameContains "Shortcut_cycle_mode" "Press shortcut"
    Send-Keys "{ESC}"
    Assert-NameContains "Shortcut_cycle_mode" "Ctrl+Alt+R"

    Invoke-Ui "invoke `"Shortcut_cycle_mode`""
    Send-Keys "z"
    Assert-NameContains "SettingsStatus" "Shortcut not accepted"
    Assert-NameContains "Shortcut_cycle_mode" "Ctrl+Alt+R"

    foreach ($command in @("cycle_mode", "cycle_effect")) {
        Invoke-Ui "invoke `"Shortcut_$command`""
        Send-Keys "^+z"
        Assert-NameContains "Shortcut_$command" "Ctrl+Shift+Z"
    }
    Invoke-Ui "invoke `"SaveSettings`""
    Invoke-Ui "wait-for `"SettingsStatus`" --timeout 5000"
    if (Test-Path -LiteralPath $responsePath) {
        throw "Invalid duplicate shortcuts unexpectedly produced a response."
    }
    Invoke-Ui (
        "screenshot --output `"$resolvedOutput\conflict.png`" --capture-screen"
    )

    Invoke-Ui "invoke `"ResetShortcuts`""
    Invoke-Ui "wait-for `"ResetConfirmation`" --timeout 5000"
    Send-Keys "{ENTER}"
    Invoke-Ui "wait-for `"ResetConfirmation`" --gone --timeout 5000"
    Assert-NameContains "Shortcut_cycle_mode" "Ctrl+Alt+R"

    Invoke-Ui "invoke `"CancelSettings`""
    Wait-SettingsExit
    if (Test-Path -LiteralPath $responsePath) {
        throw "Cancel unexpectedly produced a response."
    }

    $invalid = [ordered]@{}
    foreach ($command in $commands) {
        $invalid[$command] = $defaults[$command]
    }
    $invalid["cycle_mode"] = "not-a-chord"
    [ordered]@{ hotkeys = $invalid } |
        ConvertTo-Json -Depth 3 |
        Set-Content -LiteralPath $requestPath -Encoding utf8
    Start-Settings -RequestPath $requestPath -ResponsePath $responsePath
    Invoke-Ui "invoke `"SaveSettings`""
    Start-Sleep -Milliseconds 150
    Assert-Focused "Shortcut_cycle_mode"
    Assert-NameContains "SettingsStatus" "Resolve shortcut conflicts"
    if (Test-Path -LiteralPath $responsePath) {
        throw "An unparsable shortcut unexpectedly produced a response."
    }
    Invoke-Ui "invoke `"CancelSettings`""
    Wait-SettingsExit

    [ordered]@{ hotkeys = $defaults } |
        ConvertTo-Json -Depth 3 |
        Set-Content -LiteralPath $requestPath -Encoding utf8
    Start-Settings -RequestPath $requestPath -ResponsePath $responsePath
    $letter = [int][char]"a"
    foreach ($command in $commands) {
        Invoke-Ui "invoke `"Shortcut_$command`""
        Send-Keys ("^+" + [char]$letter)
        Assert-NameContains "Shortcut_$command" $recorded[$command]
        $letter++
    }
    Invoke-Ui "invoke `"SaveSettings`""
    Wait-SettingsExit

    if (-not (Test-Path -LiteralPath $responsePath -PathType Leaf)) {
        throw "Save did not produce a settings response."
    }
    $response = Get-Content -Raw -LiteralPath $responsePath | ConvertFrom-Json
    foreach ($command in $commands) {
        $actual = $response.hotkeys.$command
        if ($actual -ne $recorded[$command]) {
            throw "Saved shortcut $command was '$actual'; expected '$($recorded[$command])'."
        }
    }

    Write-Host "WinUI settings UIA: passed"
    Write-Host "Evidence: $resolvedOutput"
}
finally {
    Stop-Settings
    $resolvedProtocol = [System.IO.Path]::GetFullPath($protocolDirectory)
    if ($resolvedProtocol.StartsWith(
        $temporaryRoot,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        Remove-Item -LiteralPath $resolvedProtocol -Recurse -Force -ErrorAction SilentlyContinue
    }
    if ($null -ne $highContrastRestore) {
        Set-HighContrastSnapshot -Snapshot $highContrastRestore
    }
}
