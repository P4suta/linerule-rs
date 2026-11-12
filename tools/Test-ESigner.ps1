# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

BeforeAll {
    $helper = Join-Path $PSScriptRoot "Invoke-ESigner.ps1"
    $tokens = $null
    $parseErrors = $null
    $syntax = [System.Management.Automation.Language.Parser]::ParseFile(
        $helper, [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count -ne 0) { throw "Signing helper failed to parse" }
    $definition = $syntax.Find({
        param([System.Management.Automation.Language.Ast]$Node)
        $Node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
            $Node.Name -eq "ConvertFrom-Base32"
    }, $true)
    $script:base32Decoder = $definition.Body.GetScriptBlock()
}

Describe "Certificate seed decoding" {
    It "decodes <Encoded>" -ForEach @(
        @{ Encoded = "MY======"; Expected = "f" },
        @{ Encoded = "MZXQ===="; Expected = ("f" + "o") },
        @{ Encoded = "MZXW6==="; Expected = "foo" },
        @{ Encoded = "MZXW6YQ="; Expected = "foob" },
        @{ Encoded = "MZXW6YTB"; Expected = "fooba" },
        @{ Encoded = "MZXW6YTBOI======"; Expected = "foobar" },
        @{ Encoded = " mzxw6 "; Expected = "foo" }
    ) {
        param([string]$Encoded, [string]$Expected)
        [byte[]]$bytes = & $script:base32Decoder -Value $Encoded
        [System.Text.Encoding]::ASCII.GetString($bytes) | Should-Be $Expected
    }

    It "rejects malformed seed <Encoded>" -ForEach @(
        @{ Encoded = "A" },
        @{ Encoded = "AAA" },
        @{ Encoded = "AAAAAA" },
        @{ Encoded = "MZXW6====" },
        @{ Encoded = "MZXW6=======" },
        @{ Encoded = "MZ======" },
        @{ Encoded = "MZX!" }
    ) {
        param([string]$Encoded)
        [string]$seed = $Encoded
        { & $script:base32Decoder -Value $seed } | Should-Throw -ExceptionMessage "*ES_TOTP_SECRET*"
    }
}

Describe "Signing credential lifetime" {
    BeforeAll {
        $repoRoot = Split-Path $PSScriptRoot -Parent
        $fixtureRoot = Join-Path $repoRoot ("target/esigner-fixture-" + [guid]::NewGuid())
        $cache = Join-Path $fixtureRoot "linerule-tools"
        [System.IO.Directory]::CreateDirectory($cache) | Out-Null
        [System.IO.File]::WriteAllBytes((Join-Path $cache "jsign-7.5.jar"), [byte[]]@(42))
        $bundle = Join-Path $fixtureRoot "probe.msixbundle"
        [System.IO.File]::WriteAllBytes($bundle, [byte[]]@(42))
        $bundle = [System.IO.Path]::GetRelativePath($repoRoot, $bundle)
        $saved = @{}
        foreach ($name in @(
            "RUNNER_TEMP", "ES_USERNAME", "ES_PASSWORD", "CREDENTIAL_ID",
            "ES_TOTP_SECRET", "JSIGN_STOREPASS", "JSIGN_KEYPASS"
        )) {
            $saved[$name] = [System.Environment]::GetEnvironmentVariable($name)
        }
    }

    BeforeEach {
        $env:RUNNER_TEMP = $fixtureRoot
        $env:ES_USERNAME = "fixture"
        $env:ES_PASSWORD = "fixture"
        $env:CREDENTIAL_ID = "fixture"
        $env:ES_TOTP_SECRET = "bad!"
        [System.Environment]::SetEnvironmentVariable("JSIGN_STOREPASS", [NullString]::Value)
        [System.Environment]::SetEnvironmentVariable("JSIGN_KEYPASS", [NullString]::Value)
        Mock Get-FileHash {
            [pscustomobject]@{
                Hash = "602a51c3545a6dc4fb99bd2ea7152b26d1345916d0c93ddfbd5936cb735af91c"
            }
        }
        Mock Invoke-WebRequest { throw "Fixture forbids network access" }
        Mock mise { throw "Fixture forbids signing execution" }
    }

    AfterEach {
        foreach ($name in $saved.Keys) {
            [System.Environment]::SetEnvironmentVariable($name, ($saved[$name] ?? [NullString]::Value))
        }
    }

    AfterAll {
        [System.IO.Directory]::Delete($fixtureRoot, $true)
    }

    It "clears temporary credentials when seed decoding fails" {
        { & $helper -Path $bundle } | Should-Throw -ExceptionMessage "*not valid base32*"
        [System.Environment]::GetEnvironmentVariable("JSIGN_STOREPASS") | Should-BeNull
        [System.Environment]::GetEnvironmentVariable("JSIGN_KEYPASS") | Should-BeNull
        Should-Invoke -CommandName mise -Times 0 -Exactly
    }

    It "restores prior credentials when seed decoding fails" {
        $env:JSIGN_STOREPASS = "prior-store"
        $env:JSIGN_KEYPASS = "prior-key"
        { & $helper -Path $bundle } | Should-Throw -ExceptionMessage "*not valid base32*"
        $env:JSIGN_STOREPASS | Should-Be "prior-store"
        $env:JSIGN_KEYPASS | Should-Be "prior-key"
    }

    It "restores prior credentials when the signing process fails" {
        $env:JSIGN_STOREPASS = "prior-store"
        $env:JSIGN_KEYPASS = "prior-key"
        $env:ES_TOTP_SECRET = "MZXW6==="
        { & $helper -Path $bundle } | Should-Throw -ExceptionMessage "*Fixture forbids signing execution*"
        $env:JSIGN_STOREPASS | Should-Be "prior-store"
        $env:JSIGN_KEYPASS | Should-Be "prior-key"
        Should-Invoke -CommandName mise -Times 1 -Exactly
    }

    It "rejects a path outside the project before invoking the signer" {
        $outside = "../outside.msixbundle"
        { & $helper -Path $outside } | Should-Throw -ExceptionMessage "*escapes the repository*"
        Should-Invoke -CommandName mise -Times 0 -Exactly
    }
}
