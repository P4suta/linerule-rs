# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

<#
.SYNOPSIS
Fail unless the current machine is a supported Windows 11 release-evidence host.

.DESCRIPTION
Records the OS, interactive desktop, GPU, monitor bounds, and effective DPI.
Publishing requires a workstation build supported by the package manifest,
an interactive runner, a hardware display adapter, and at least two monitors
whose effective DPI values differ.
#>

[CmdletBinding()]
param(
    [string]$OutputDirectory = "artifacts/hardware",

    [ValidateSet("x64", "arm64")]
    [string]$RequiredArchitecture = "x64"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public sealed class LineruleMonitorEvidence
{
    public string DeviceName { get; set; } = "";
    public int Left { get; set; }
    public int Top { get; set; }
    public int Width { get; set; }
    public int Height { get; set; }
    public uint DpiX { get; set; }
    public uint DpiY { get; set; }
    public bool Primary { get; set; }
}

public static class LineruleDisplayEvidence
{
    private const int EffectiveDpi = 0;
    private const uint MonitorInfoPrimary = 1;
    private static readonly IntPtr PerMonitorAwareV2 = new IntPtr(-4);

    [StructLayout(LayoutKind.Sequential)]
    private struct Rect
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct MonitorInfo
    {
        public uint Size;
        public Rect Monitor;
        public Rect Work;
        public uint Flags;

        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)]
        public string DeviceName;
    }

    private delegate bool MonitorCallback(
        IntPtr monitor,
        IntPtr deviceContext,
        ref Rect bounds,
        IntPtr data);

    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool EnumDisplayMonitors(
        IntPtr deviceContext,
        IntPtr clip,
        MonitorCallback callback,
        IntPtr data);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetMonitorInfo(
        IntPtr monitor,
        ref MonitorInfo information);

    [DllImport("shcore.dll")]
    private static extern int GetDpiForMonitor(
        IntPtr monitor,
        int dpiType,
        out uint dpiX,
        out uint dpiY);

    [DllImport("user32.dll")]
    private static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);

    public static LineruleMonitorEvidence[] Capture()
    {
        var monitors = new List<LineruleMonitorEvidence>();
        var previousContext = SetThreadDpiAwarenessContext(PerMonitorAwareV2);
        try
        {
            MonitorCallback callback = delegate(
                IntPtr monitor,
                IntPtr deviceContext,
                ref Rect bounds,
                IntPtr data)
            {
                var information = new MonitorInfo
                {
                    Size = (uint)Marshal.SizeOf<MonitorInfo>()
                };
                if (!GetMonitorInfo(monitor, ref information))
                {
                    throw new InvalidOperationException(
                        "GetMonitorInfo failed for a display monitor.");
                }

                uint dpiX;
                uint dpiY;
                var result = GetDpiForMonitor(
                    monitor,
                    EffectiveDpi,
                    out dpiX,
                    out dpiY);
                if (result < 0)
                {
                    Marshal.ThrowExceptionForHR(result);
                }

                monitors.Add(new LineruleMonitorEvidence
                {
                    DeviceName = information.DeviceName,
                    Left = information.Monitor.Left,
                    Top = information.Monitor.Top,
                    Width = information.Monitor.Right - information.Monitor.Left,
                    Height = information.Monitor.Bottom - information.Monitor.Top,
                    DpiX = dpiX,
                    DpiY = dpiY,
                    Primary = (information.Flags & MonitorInfoPrimary) != 0
                });
                return true;
            };

            if (!EnumDisplayMonitors(
                IntPtr.Zero,
                IntPtr.Zero,
                callback,
                IntPtr.Zero))
            {
                throw new InvalidOperationException(
                    "EnumDisplayMonitors failed.");
            }
        }
        finally
        {
            if (previousContext != IntPtr.Zero)
            {
                SetThreadDpiAwarenessContext(previousContext);
            }
        }
        return monitors.ToArray();
    }
}
"@

$output = [System.IO.Path]::GetFullPath($OutputDirectory)
[System.IO.Directory]::CreateDirectory($output) | Out-Null

$os = Get-CimInstance Win32_OperatingSystem
$build = [int]$os.BuildNumber
if (
    $os.ProductType -ne 1 -or
    $os.Caption -notmatch "Windows 11" -or
    $build -lt 26100
) {
    throw (
        "Release evidence requires Windows 11 workstation build 26100 or " +
        "newer; found '$($os.Caption)' build $build, product type " +
        "$($os.ProductType)"
    )
}

$architecture =
    [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
$expectedArchitecture = if ($RequiredArchitecture -eq "arm64") {
    [System.Runtime.InteropServices.Architecture]::Arm64
} else {
    [System.Runtime.InteropServices.Architecture]::X64
}
if ($architecture -ne $expectedArchitecture) {
    throw (
        "Release evidence requires native $RequiredArchitecture Windows; " +
        "found $architecture"
    )
}

$sessionId = [System.Diagnostics.Process]::GetCurrentProcess().SessionId
$interactiveExplorer = @(
    Get-Process explorer -ErrorAction SilentlyContinue |
        Where-Object SessionId -eq $sessionId
)
if (
    -not [Environment]::UserInteractive -or
    $interactiveExplorer.Count -eq 0
) {
    throw (
        "The self-hosted runner must run interactively in the signed-in " +
        "desktop session, not as a service"
    )
}

$developmentKey =
    "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock"
$developerMode = if (Test-Path -LiteralPath $developmentKey) {
    $development = Get-ItemProperty `
        -LiteralPath $developmentKey `
        -Name AllowDevelopmentWithoutDevLicense `
        -ErrorAction SilentlyContinue
    $null -ne $development -and
        $development.AllowDevelopmentWithoutDevLicense -eq 1
} else {
    $false
}
if (-not $developerMode) {
    throw "Developer Mode must be enabled on the release-evidence host"
}

$monitors = @([LineruleDisplayEvidence]::Capture())
if ($monitors.Count -lt 2) {
    throw "Mixed-DPI evidence requires at least two enabled monitors"
}
$effectiveDpi = @(
    $monitors |
        ForEach-Object { "$($_.DpiX)x$($_.DpiY)" } |
        Sort-Object -Unique
)
if ($effectiveDpi.Count -lt 2) {
    throw (
        "At least two distinct effective DPI values are required; found " +
        ($effectiveDpi -join ", ")
    )
}

$videoControllers = @(
    Get-CimInstance Win32_VideoController |
        Select-Object Name, Status, DriverVersion, VideoProcessor, PNPDeviceID
)
$hardwareControllers = @(
    $videoControllers |
        Where-Object {
            $_.Name -notmatch (
                "Microsoft Basic|Remote Display|Virtual Display|" +
                "Indirect Display"
            )
        }
)
if ($hardwareControllers.Count -eq 0) {
    throw "No hardware display adapter was detected"
}

$evidence = [ordered]@{
    captured_at_utc = [DateTime]::UtcNow.ToString("O")
    os = [ordered]@{
        caption = $os.Caption
        version = $os.Version
        build = $build
        architecture = $architecture.ToString()
        product_type = $os.ProductType
    }
    interactive = [ordered]@{
        user = [Environment]::UserName
        session_id = $sessionId
        user_interactive = [Environment]::UserInteractive
        developer_mode = $developerMode
    }
    monitors = $monitors
    effective_dpi_values = $effectiveDpi
    video_controllers = $videoControllers
}
$evidence |
    ConvertTo-Json -Depth 8 |
    Set-Content `
        -LiteralPath (Join-Path $output "hardware.json") `
        -Encoding utf8

Write-Host (
    "Windows 11 hardware preflight: passed; monitors={0}; dpi={1}; GPUs={2}" -f
    $monitors.Count,
    ($effectiveDpi -join ","),
    $hardwareControllers.Count
)
