# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$BundlePath,

    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$AppInstallerPath,

    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$PortableZip,

    [ValidateScript({
        [string]::IsNullOrEmpty($_) -or
        (Test-Path -LiteralPath $_ -PathType Leaf)
    })]
    [string]$PreviousBundlePath = "",

    [ValidateScript({
        [string]::IsNullOrEmpty($_) -or
        (Test-Path -LiteralPath $_ -PathType Leaf)
    })]
    [string]$PreviousAppInstallerPath = "",

    [string]$IdentityName = "P4suta.linerule",

    [switch]$RequireSignature
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$bundle = [System.IO.Path]::GetFullPath($BundlePath)
$appInstaller = [System.IO.Path]::GetFullPath($AppInstallerPath)
$portable = [System.IO.Path]::GetFullPath($PortableZip)
$previous = if ([string]::IsNullOrEmpty($PreviousBundlePath)) {
    $null
} else {
    [System.IO.Path]::GetFullPath($PreviousBundlePath)
}
$previousAppInstaller = if (
    [string]::IsNullOrEmpty($PreviousAppInstallerPath)
) {
    $null
} else {
    [System.IO.Path]::GetFullPath($PreviousAppInstallerPath)
}
if (($null -eq $previous) -ne ($null -eq $previousAppInstaller)) {
    throw "PreviousBundlePath and PreviousAppInstallerPath must be supplied together"
}

foreach ($artifact in @($bundle, $portable, $previous)) {
    if ($null -eq $artifact) {
        continue
    }
    if ($RequireSignature -and $artifact.EndsWith(
        ".msixbundle",
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        $signature = Get-AuthenticodeSignature -LiteralPath $artifact
        if ($signature.Status -ne "Valid") {
            throw "Invalid package signature on $artifact`: $($signature.Status)"
        }
    }
}

$existing = @(Get-AppxPackage -Name $IdentityName)
if ($existing.Count -ne 0) {
    throw "Refusing to replace an existing $IdentityName installation"
}

$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$testRoot = Join-Path (
    $temporaryRoot
) "linerule-package-test-$([guid]::NewGuid().ToString('N'))"
[System.IO.Directory]::CreateDirectory($testRoot) | Out-Null

$installed = $false
$localState = $null

function Invoke-DiagnosticsDataDir {
    param([Parameter(Mandatory = $true)][string]$Executable)

    $stdout = Join-Path $testRoot "$([guid]::NewGuid().ToString('N')).out"
    $stderr = Join-Path $testRoot "$([guid]::NewGuid().ToString('N')).err"
    $process = Start-Process `
        -FilePath $Executable `
        -ArgumentList @("diagnostics", "--data-dir") `
        -Wait `
        -PassThru `
        -NoNewWindow `
        -RedirectStandardOutput $stdout `
        -RedirectStandardError $stderr
    if ($process.ExitCode -ne 0) {
        $errorText = if (Test-Path -LiteralPath $stderr) {
            Get-Content -LiteralPath $stderr -Raw
        } else {
            ""
        }
        throw "diagnostics --data-dir exited with $($process.ExitCode): $errorText"
    }
    return (Get-Content -LiteralPath $stdout -Raw).Trim()
}

function Write-LocalAppInstaller {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Bundle,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    [xml]$document = Get-Content -LiteralPath $Source -Raw
    $root = $document.DocumentElement
    if (
        $null -eq $root -or
        $root.NamespaceURI -ne
            "http://schemas.microsoft.com/appx/appinstaller/2021"
    ) {
        throw "App Installer fixture must use the 2021 schema: $Source"
    }
    $mainBundle = $root.SelectSingleNode(
        "*[local-name()='MainBundle']"
    )
    if ($null -eq $mainBundle) {
        throw "App Installer fixture has no MainBundle: $Source"
    }

    $destinationUri = [System.Uri]::new(
        [System.IO.Path]::GetFullPath($Destination)
    ).AbsoluteUri
    $bundleUri = [System.Uri]::new(
        [System.IO.Path]::GetFullPath($Bundle)
    ).AbsoluteUri
    $root.SetAttribute("Uri", $destinationUri)
    $mainBundle.SetAttribute("Uri", $bundleUri)

    $settings = [System.Xml.XmlWriterSettings]::new()
    $settings.Encoding = [System.Text.UTF8Encoding]::new($false)
    $settings.Indent = $true
    $writer = [System.Xml.XmlWriter]::Create($Destination, $settings)
    try {
        $document.Save($writer)
    } finally {
        $writer.Dispose()
    }
    return $destinationUri
}

try {
    $initialBundle = if ($null -eq $previous) { $bundle } else { $previous }
    $initialAppInstaller = if ($null -eq $previousAppInstaller) {
        $appInstaller
    } else {
        $previousAppInstaller
    }
    $localAppInstaller = Join-Path $testRoot "linerule.appinstaller"
    $localAppInstallerUri = Write-LocalAppInstaller `
        -Source $initialAppInstaller `
        -Bundle $initialBundle `
        -Destination $localAppInstaller
    Add-AppxPackage -AppInstallerFile $localAppInstaller
    $installed = $true

    $package = Get-AppxPackage -Name $IdentityName
    if ($null -eq $package) {
        throw "$IdentityName was not installed"
    }
    $initialVersion = [version]$package.Version

    if ($null -ne $previous) {
        $localAppInstallerUri = Write-LocalAppInstaller `
            -Source $appInstaller `
            -Bundle $bundle `
            -Destination $localAppInstaller
        Add-AppxPackage -AppInstallerFile $localAppInstaller
        $package = Get-AppxPackage -Name $IdentityName
        if ($null -eq $package) {
            throw "$IdentityName disappeared during update"
        }
        if ([version]$package.Version -le $initialVersion) {
            throw "MSIX update did not increase the package version"
        }
    }

    $family = $package.PackageFamilyName
    $autoUpdate = @(
        Get-AppxPackageAutoUpdateSettings `
            -PackageFullName $package.PackageFullName
    )
    if ($autoUpdate.Count -ne 1) {
        throw "App Installer update settings were not registered"
    }
    $registeredUri = [string]$autoUpdate[0].AppInstallerUri
    if (-not $registeredUri.Equals(
        $localAppInstallerUri,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw (
            "Registered App Installer URI was '$registeredUri'; " +
            "expected '$localAppInstallerUri'"
        )
    }

    $localState = Join-Path $env:LOCALAPPDATA "Packages\$family\LocalState"
    $installedExecutable = Join-Path $package.InstallLocation "linerule.exe"
    $reportedData = Invoke-DiagnosticsDataDir -Executable $installedExecutable
    if (-not [System.IO.Path]::GetFullPath($reportedData).Equals(
        [System.IO.Path]::GetFullPath($localState),
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "Installed data root was '$reportedData'; expected '$localState'"
    }

    [System.IO.Directory]::CreateDirectory($localState) | Out-Null
    $repairSentinel = Join-Path $localState "repair-sentinel"
    [System.IO.File]::WriteAllText($repairSentinel, "preserve")
    Add-AppxPackage `
        -Register (Join-Path $package.InstallLocation "AppxManifest.xml") `
        -DisableDevelopmentMode `
        -ForceApplicationShutdown
    if (-not (Test-Path -LiteralPath $repairSentinel -PathType Leaf)) {
        throw "Package repair unexpectedly removed LocalState"
    }
    $repairedSettings = @(
        Get-AppxPackageAutoUpdateSettings `
            -PackageFullName $package.PackageFullName
    )
    if (
        $repairedSettings.Count -ne 1 -or
        -not ([string]$repairedSettings[0].AppInstallerUri).Equals(
            $localAppInstallerUri,
            [System.StringComparison]::OrdinalIgnoreCase
        )
    ) {
        throw "Package repair lost its App Installer update settings"
    }

    $portableRoot = Join-Path $testRoot "portable"
    Expand-Archive -LiteralPath $portable -DestinationPath $portableRoot
    $portableExecutable = Join-Path $portableRoot "linerule.exe"
    $portableData = Invoke-DiagnosticsDataDir -Executable $portableExecutable
    $expectedPortableData = Join-Path $portableRoot "data"
    if (-not [System.IO.Path]::GetFullPath($portableData).Equals(
        [System.IO.Path]::GetFullPath($expectedPortableData),
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "Portable data root was '$portableData'; expected '$expectedPortableData'"
    }
    if (-not (Test-Path -LiteralPath (
        Join-Path $portableRoot "linerule.portable"
    ) -PathType Leaf)) {
        throw "Portable marker is missing"
    }

    Write-Host (
        "App Installer install/update, MSIX repair, uninstall, " +
        "and Portable isolation: passed"
    )
}
finally {
    $uninstallFailure = $null
    if ($installed) {
        Get-AppxPackage -Name $IdentityName |
            Remove-AppxPackage -Confirm:$false -ErrorAction Continue
    }
    if ($null -ne $localState) {
        $deadline = [DateTime]::UtcNow.AddSeconds(10)
        while (
            (Test-Path -LiteralPath $localState) -and
            [DateTime]::UtcNow -lt $deadline
        ) {
            Start-Sleep -Milliseconds 100
        }
        if (Test-Path -LiteralPath $localState) {
            $uninstallFailure = "Uninstall left package LocalState behind: $localState"
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
    if ($null -ne $uninstallFailure) {
        throw $uninstallFailure
    }
}
