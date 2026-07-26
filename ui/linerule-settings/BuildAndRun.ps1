# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

<#
.SYNOPSIS
Build and optionally run the linerule WinUI settings app.

.DESCRIPTION
This is the only supported local WinUI entry point. It verifies Developer
Mode, uses the versions pinned by the repository's mise.toml, and launches
through winapp so XAML failures include debug output.
#>

[CmdletBinding()]
param(
    [ValidateSet("x64", "ARM64")]
    [string]$Platform = $(
        if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "ARM64" } else { "x64" }
    ),

    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",

    [switch]$SkipRun,
    [switch]$Detach
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$developmentKey =
    "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock"
$development = if (Test-Path -LiteralPath $developmentKey) {
    Get-ItemProperty `
        -LiteralPath $developmentKey `
        -Name AllowDevelopmentWithoutDevLicense `
        -ErrorAction SilentlyContinue
} else {
    $null
}
$developerMode = $null -ne $development -and
    $development.AllowDevelopmentWithoutDevLicense -eq 1
if (-not $developerMode) {
    throw @"
Developer Mode is not enabled.
Enable Settings > System > For developers > Developer Mode, then rerun this script.
"@
}

$projectDirectory = $PSScriptRoot
$project = Join-Path $projectDirectory "Linerule.Settings.csproj"
$runtime = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }

& mise exec dotnet --command (
    "dotnet restore `"$project`" --locked-mode " +
    "-p:Platform=$Platform -p:RuntimeIdentifier=$runtime"
)
if ($LASTEXITCODE -ne 0) {
    throw "WinUI restore failed with exit code $LASTEXITCODE"
}

& mise exec dotnet --command (
    "dotnet build `"$project`" --no-restore " +
    "-p:Platform=$Platform -p:Configuration=$Configuration " +
    "-p:RuntimeIdentifier=$runtime -warnaserror"
)
if ($LASTEXITCODE -ne 0) {
    throw "WinUI build failed with exit code $LASTEXITCODE"
}

if ($SkipRun) {
    return
}

$configurationRoot = Join-Path $projectDirectory "bin\$Platform\$Configuration"
$framework = Get-ChildItem -LiteralPath $configurationRoot -Directory |
    Where-Object Name -Like "net*-windows*" |
    Sort-Object Name -Descending |
    Select-Object -First 1
if (-not $framework) {
    throw "WinUI build output was not found below $configurationRoot"
}
$output = Join-Path $framework.FullName $runtime
if (-not (Test-Path -LiteralPath $output -PathType Container)) {
    $output = $framework.FullName
}

if ($Detach) {
    & mise exec "npm:@microsoft/winappcli" --command (
        "winapp run `"$output`" --detach --json"
    )
} else {
    & mise exec "npm:@microsoft/winappcli" --command (
        "winapp run `"$output`" --debug-output"
    )
}
if ($LASTEXITCODE -ne 0) {
    throw "winapp run failed with exit code $LASTEXITCODE"
}
