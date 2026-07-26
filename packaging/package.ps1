# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [string]$Publisher,

    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9.-]{1,48}[A-Za-z0-9]$')]
    [string]$Identity = "P4suta.linerule",

    [ValidatePattern('^[A-Za-z0-9][A-Za-z0-9.-]*$')]
    [string]$ArtifactName = "linerule",

    [AllowEmptyString()]
    [string]$PackageVersion = "",

    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$X64Exe,

    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$Arm64Exe,

    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$X64SettingsExe,

    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$Arm64SettingsExe,

    [string]$OutputDirectory = "dist",

    [string]$BaseUri = "https://github.com/P4suta/linerule-rs/releases/latest/download",

    [switch]$SkipAppInstaller
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$sourceRoot = Split-Path -Parent $PSScriptRoot
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $output) {
    throw "Output directory already exists: $output"
}
[System.IO.Directory]::CreateDirectory($output) | Out-Null

$identity = $Identity
$packageVersion = if ($PackageVersion.Length -eq 0) {
    "$Version.0"
} else {
    $PackageVersion
}
$versionParts = @($packageVersion -split "\.")
if (
    $versionParts.Count -ne 4 -or
    @($versionParts | Where-Object { $_ -notmatch '^\d+$' }).Count -ne 0
) {
    throw "PackageVersion must contain four numeric components: $packageVersion"
}
foreach ($part in $versionParts) {
    if ([uint64]$part -gt 65535) {
        throw "PackageVersion component exceeds 65535: $packageVersion"
    }
}

$assetsFile = Join-Path `
    $sourceRoot "ui\linerule-settings\obj\project.assets.json"
if (-not (Test-Path -LiteralPath $assetsFile -PathType Leaf)) {
    throw "Restore the locked WinUI project before packaging: $assetsFile"
}
$assetsGraph = Get-Content -LiteralPath $assetsFile -Raw | ConvertFrom-Json
$packageFolder = $assetsGraph.packageFolders.PSObject.Properties.Name |
    Select-Object -First 1
$sdkBuildTools = $assetsGraph.libraries.PSObject.Properties.Name |
    Where-Object { $_ -like "Microsoft.Windows.SDK.BuildTools/*" }
if (-not $packageFolder -or @($sdkBuildTools).Count -ne 1) {
    throw "The locked Windows SDK BuildTools package could not be resolved"
}
$sdkPackage = Join-Path $packageFolder ($sdkBuildTools.Replace("/", "\"))
$makeAppx = Get-ChildItem -LiteralPath (Join-Path $sdkPackage "bin") `
    -Recurse -Filter makeappx.exe |
    Where-Object { $_.Directory.Name -eq "x64" } |
    Select-Object -ExpandProperty FullName -First 1
if (-not $makeAppx) {
    throw "makeappx.exe was not found in the locked Windows SDK BuildTools package"
}

Add-Type -AssemblyName System.Drawing

function New-Logo {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination,
        [Parameter(Mandatory = $true)][int]$Width,
        [Parameter(Mandatory = $true)][int]$Height
    )
    $inputImage = [System.Drawing.Image]::FromFile($Source)
    try {
        $bitmap = New-Object System.Drawing.Bitmap $Width, $Height
        try {
            $bitmap.SetResolution(96, 96)
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.Clear([System.Drawing.Color]::Transparent)
                $graphics.InterpolationMode =
                    [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $side = [Math]::Min($Width, $Height)
                $left = [int](($Width - $side) / 2)
                $top = [int](($Height - $side) / 2)
                $graphics.DrawImage($inputImage, $left, $top, $side, $side)
            } finally {
                $graphics.Dispose()
            }
            $bitmap.Save($Destination, [System.Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $bitmap.Dispose()
        }
    } finally {
        $inputImage.Dispose()
    }
}

function New-Package {
    param(
        [Parameter(Mandatory = $true)][string]$Architecture,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$SettingsExecutable
    )
    $layout = Join-Path $output "layout-$Architecture"
    $assets = Join-Path $layout "Assets"
    $settings = Join-Path $layout "settings"
    [System.IO.Directory]::CreateDirectory($assets) | Out-Null
    [System.IO.Directory]::CreateDirectory($settings) | Out-Null
    Copy-Item -LiteralPath $Executable -Destination (Join-Path $layout "linerule.exe")
    Copy-Item -LiteralPath $SettingsExecutable `
        -Destination (Join-Path $settings "linerule-settings.exe")

    $manifestTemplate = Get-Content `
        -LiteralPath (Join-Path $PSScriptRoot "AppxManifest.xml.in") -Raw
    $manifest = $manifestTemplate.
        Replace("@IDENTITY@", $identity).
        Replace("@PUBLISHER@", $Publisher).
        Replace("@VERSION@", $packageVersion).
        Replace("@ARCH@", $Architecture)
    [System.IO.File]::WriteAllText(
        (Join-Path $layout "AppxManifest.xml"),
        $manifest,
        [System.Text.UTF8Encoding]::new($false)
    )

    $logo = Join-Path $sourceRoot "crates\linerule-app\assets\linerule.png"
    New-Logo $logo (Join-Path $assets "Square44x44Logo.png") 44 44
    New-Logo $logo (Join-Path $assets "Square150x150Logo.png") 150 150
    New-Logo $logo (Join-Path $assets "Wide310x150Logo.png") 310 150
    New-Logo $logo (Join-Path $assets "StoreLogo.png") 50 50

    $package = Join-Path $output "$ArtifactName-$Architecture.msix"
    & $makeAppx pack /d $layout /p $package /o | Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "MakeAppx pack failed for $Architecture"
    }
    return $package
}

function New-PortableZip {
    param(
        [Parameter(Mandatory = $true)][string]$Architecture,
        [Parameter(Mandatory = $true)][string]$Executable,
        [Parameter(Mandatory = $true)][string]$SettingsExecutable
    )
    $layout = Join-Path $output "portable-$Architecture"
    $settings = Join-Path $layout "settings"
    [System.IO.Directory]::CreateDirectory($layout) | Out-Null
    [System.IO.Directory]::CreateDirectory($settings) | Out-Null
    Copy-Item -LiteralPath $Executable -Destination (Join-Path $layout "linerule.exe")
    Copy-Item -LiteralPath $SettingsExecutable `
        -Destination (Join-Path $settings "linerule-settings.exe")
    [System.IO.File]::WriteAllText((Join-Path $layout "linerule.portable"), "")
    Copy-Item -LiteralPath (Join-Path $sourceRoot "LICENSES") `
        -Destination (Join-Path $layout "LICENSES") -Recurse
    Compress-Archive -Path (Join-Path $layout "*") `
        -DestinationPath (
            Join-Path $output "$ArtifactName-portable-$Architecture.zip"
        )
}

$x64Package = New-Package "x64" $X64Exe $X64SettingsExe
$arm64Package = New-Package "arm64" $Arm64Exe $Arm64SettingsExe
New-PortableZip "x64" $X64Exe $X64SettingsExe
New-PortableZip "arm64" $Arm64Exe $Arm64SettingsExe

$bundleInput = Join-Path $output "bundle-input"
[System.IO.Directory]::CreateDirectory($bundleInput) | Out-Null
Copy-Item -LiteralPath $x64Package -Destination $bundleInput
Copy-Item -LiteralPath $arm64Package -Destination $bundleInput
& $makeAppx bundle /d $bundleInput `
    /p (Join-Path $output "$ArtifactName.msixbundle") /o | Out-Host
if ($LASTEXITCODE -ne 0) {
    throw "MakeAppx bundle failed"
}

if (-not $SkipAppInstaller) {
    $appInstallerTemplate = Get-Content `
        -LiteralPath (Join-Path $PSScriptRoot "linerule.appinstaller.in") -Raw
    $appInstaller = $appInstallerTemplate.
        Replace("@BASE_URI@", $BaseUri.TrimEnd("/")).
        Replace("@ARTIFACT_NAME@", $ArtifactName).
        Replace("@IDENTITY@", $identity).
        Replace("@PUBLISHER@", $Publisher).
        Replace("@VERSION@", $packageVersion)
    [System.IO.File]::WriteAllText(
        (Join-Path $output "$ArtifactName.appinstaller"),
        $appInstaller,
        [System.Text.UTF8Encoding]::new($false)
    )
}

$cleanup = @(
    (Join-Path $output "layout-x64"),
    (Join-Path $output "layout-arm64"),
    (Join-Path $output "portable-x64"),
    (Join-Path $output "portable-arm64"),
    (Join-Path $output "bundle-input"),
    $x64Package,
    $arm64Package
)
$outputPrefix = $output.TrimEnd("\") + "\"
foreach ($path in $cleanup) {
    $resolved = [System.IO.Path]::GetFullPath($path)
    if (-not $resolved.StartsWith(
        $outputPrefix,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "Refusing to clean a path outside the output directory: $resolved"
    }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
