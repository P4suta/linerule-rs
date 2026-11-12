# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

[CmdletBinding()]
param(
    [ValidateNotNullOrEmpty()]
    [string[]]$Path = @(),

    [switch]$VerifyToolOnly
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$jsignVersion = "7.5"
$jsignSha256 = "602a51c3545a6dc4fb99bd2ea7152b26d1345916d0c93ddfbd5936cb735af91c"
$jsignUri = "https://github.com/ebourg/jsign/releases/download/$jsignVersion/jsign-$jsignVersion.jar"

function ConvertFrom-Base32 {
    [OutputType([byte[]])]
    param(
        [Parameter(Mandatory)]
        [string]$Value
    )

    $encoded = ($Value -replace "\s", "").ToUpperInvariant()
    $normalized = $encoded.TrimEnd("=")
    if ($normalized.Length -eq 0) {
        throw "ES_TOTP_SECRET is empty"
    }
    $remainder = $normalized.Length % 8
    $padding = $encoded.Length - $normalized.Length
    if ($remainder -notin @(0, 2, 4, 5, 7) -or
        ($padding -gt 0 -and $padding -ne (8 - $remainder) % 8)) {
        throw "ES_TOTP_SECRET is not valid base32"
    }

    $alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
    $buffer = 0
    $bitCount = 0
    $bytes = [System.Collections.Generic.List[byte]]::new()
    foreach ($character in $normalized.ToCharArray()) {
        $value = $alphabet.IndexOf($character)
        if ($value -lt 0) {
            throw "ES_TOTP_SECRET is not valid base32"
        }

        $buffer = ($buffer -shl 5) -bor $value
        $bitCount += 5
        if ($bitCount -ge 8) {
            $bitCount -= 8
            $bytes.Add([byte](($buffer -shr $bitCount) -band 0xff))
            if ($bitCount -eq 0) {
                $buffer = 0
            } else {
                $buffer = $buffer -band ((1 -shl $bitCount) - 1)
            }
        }
    }

    if ($bitCount -gt 0 -and $buffer -ne 0) {
        throw "ES_TOTP_SECRET has non-zero base32 padding bits"
    }

    return $bytes.ToArray()
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$toolRoot = if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
    Join-Path $repoRoot "target/tools"
} else {
    Join-Path $env:RUNNER_TEMP "linerule-tools"
}
[System.IO.Directory]::CreateDirectory($toolRoot) | Out-Null
$jsignJar = Join-Path $toolRoot "jsign-$jsignVersion.jar"

if (Test-Path -LiteralPath $jsignJar -PathType Leaf) {
    $actualHash = (Get-FileHash -LiteralPath $jsignJar -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $jsignSha256) {
        Remove-Item -LiteralPath $jsignJar -Force
    }
}
if (-not (Test-Path -LiteralPath $jsignJar -PathType Leaf)) {
    Invoke-WebRequest -Uri $jsignUri -OutFile $jsignJar
}
$actualHash = (Get-FileHash -LiteralPath $jsignJar -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualHash -ne $jsignSha256) {
    Remove-Item -LiteralPath $jsignJar -Force
    throw "Jsign $jsignVersion checksum mismatch"
}

if ($VerifyToolOnly) {
    & mise exec java -- java -jar $jsignJar --version
    if ($LASTEXITCODE -ne 0) {
        throw "Jsign verification failed with exit code $LASTEXITCODE"
    }
    return
}
if ($Path.Count -eq 0) {
    throw "At least one bundle path is required"
}

$requiredVariables = @(
    "ES_USERNAME",
    "ES_PASSWORD",
    "CREDENTIAL_ID",
    "ES_TOTP_SECRET"
)
foreach ($name in $requiredVariables) {
    if ([string]::IsNullOrWhiteSpace(
        [System.Environment]::GetEnvironmentVariable($name)
    )) {
        throw "Required signing variable is missing: $name"
    }
}

$resolvedPaths = foreach ($item in $Path) {
    $resolved = [System.IO.Path]::GetFullPath((Join-Path $repoRoot $item))
    if (-not $resolved.StartsWith(
        $repoRoot + [System.IO.Path]::DirectorySeparatorChar,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        throw "Signing path escapes the repository: $item"
    }
    if (-not (Test-Path -LiteralPath $resolved -PathType Leaf)) {
        throw "Signing input does not exist: $item"
    }
    $extension = [System.IO.Path]::GetExtension($resolved)
    if ($extension -notin @(".msixbundle", ".appxbundle")) {
        throw "Unsupported bundle type: $extension"
    }
    $resolved
}

$previousStorePass = [System.Environment]::GetEnvironmentVariable("JSIGN_STOREPASS")
$previousKeyPass = [System.Environment]::GetEnvironmentVariable("JSIGN_KEYPASS")
[byte[]]$totpSecret = @()

try {
    $env:JSIGN_STOREPASS = "$env:ES_USERNAME|$env:ES_PASSWORD"
    $totpSecret = ConvertFrom-Base32 -Value $env:ES_TOTP_SECRET
    $env:JSIGN_KEYPASS = [System.Convert]::ToBase64String($totpSecret)
    if ($env:GITHUB_ACTIONS -eq "true") {
        Write-Output "::add-mask::$env:JSIGN_STOREPASS"
        Write-Output "::add-mask::$env:JSIGN_KEYPASS"
    }

    $arguments = @(
    "-jar",
    $jsignJar,
    "sign",
    "--storetype",
    "ESIGNER",
    "--storepass",
    "env:JSIGN_STOREPASS",
    "--alias",
    $env:CREDENTIAL_ID,
    "--keypass",
    "env:JSIGN_KEYPASS",
    "--alg",
    "SHA-256",
    "--tsaurl",
    "http://ts.ssl.com",
    "--tsmode",
    "RFC3161"
    ) + $resolvedPaths

    & mise exec java -- java @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Jsign failed with exit code $LASTEXITCODE"
    }
} finally {
    [System.Environment]::SetEnvironmentVariable("JSIGN_STOREPASS", ($previousStorePass ?? [NullString]::Value))
    [System.Environment]::SetEnvironmentVariable("JSIGN_KEYPASS", ($previousKeyPass ?? [NullString]::Value))
    [System.Array]::Clear($totpSecret, 0, $totpSecret.Length)
}
