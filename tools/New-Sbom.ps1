# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

[CmdletBinding()]
param(
    [string]$OutputFile = "dist/linerule-sbom.cdx.json"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$project = Join-Path $repoRoot "ui/linerule-settings/Linerule.Settings.csproj"
$output = if ([System.IO.Path]::IsPathRooted($OutputFile)) {
    [System.IO.Path]::GetFullPath($OutputFile)
} else {
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot $OutputFile))
}
$outputDirectory = Split-Path -Parent $output
[System.IO.Directory]::CreateDirectory($outputDirectory) | Out-Null

$metadata = & cargo metadata --locked --no-deps --format-version 1 |
    ConvertFrom-Json
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed"
}
$versions = @(
    $metadata.packages |
        Where-Object { $_.name -eq "linerule-app" } |
        Select-Object -ExpandProperty version -Unique
)
if ($versions.Count -ne 1) {
    throw "Could not resolve exactly one linerule-app version"
}
$version = [string]$versions[0]

$temporaryRoot = Join-Path (
    (Join-Path $repoRoot "target")
) "sbom-$([guid]::NewGuid().ToString('N'))"
[System.IO.Directory]::CreateDirectory($temporaryRoot) | Out-Null
$rustBom = Join-Path $temporaryRoot "rust.cdx.json"

function New-DotnetBom {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Runtime,

        [Parameter(Mandatory = $true)]
        [string]$Architecture
    )

    & dotnet restore $project `
        --locked-mode `
        "-p:Platform=$Architecture" `
        "-p:RuntimeIdentifier=$Runtime" |
        Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "WinUI locked restore failed for $Runtime"
    }

    $filename = "dotnet-$Runtime.cdx.json"
    & dotnet-CycloneDX $project `
        --framework "net10.0-windows10.0.26100.0" `
        --runtime $Runtime `
        --configuration Release `
        --disable-package-restore `
        --output $temporaryRoot `
        --filename $filename `
        --output-format Json `
        --spec-version 1.6 `
        --set-name "linerule-settings-$Runtime" `
        --set-version $version |
        Out-Host
    if ($LASTEXITCODE -ne 0) {
        throw "dotnet-CycloneDX failed for $Runtime"
    }
    return (Join-Path $temporaryRoot $filename)
}

try {
    $rustJson = & cargo sbom `
        --output-format=cyclone_dx_json_1_6 `
        --cargo-package linerule-app
    if ($LASTEXITCODE -ne 0) {
        throw "cargo sbom failed"
    }
    [System.IO.File]::WriteAllLines(
        $rustBom,
        @($rustJson),
        [System.Text.UTF8Encoding]::new($false)
    )

    $dotnetX64Bom = New-DotnetBom -Runtime "win-x64" -Architecture "x64"
    $dotnetArm64Bom = New-DotnetBom -Runtime "win-arm64" -Architecture "ARM64"

    & cyclonedx merge `
        --input-files $rustBom $dotnetX64Bom $dotnetArm64Bom `
        --output-file $output `
        --input-format json `
        --output-format json `
        --output-version v1_6 `
        --hierarchical `
        --group "P4suta" `
        --name "linerule" `
        --version $version
    if ($LASTEXITCODE -ne 0) {
        throw "CycloneDX SBOM merge failed"
    }

    & cyclonedx validate `
        --input-file $output `
        --input-format json `
        --input-version v1_6 `
        --fail-on-errors
    if ($LASTEXITCODE -ne 0) {
        throw "Merged CycloneDX SBOM validation failed"
    }
}
finally {
    $resolvedTemporaryRoot = [System.IO.Path]::GetFullPath($temporaryRoot)
    $targetRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot "target"))
    $targetPrefix = $targetRoot.TrimEnd("\") + "\"
    if ($resolvedTemporaryRoot.StartsWith(
        $targetPrefix,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        Remove-Item -LiteralPath $resolvedTemporaryRoot -Recurse -Force
    }
}
