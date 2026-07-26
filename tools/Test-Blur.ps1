# SPDX-FileCopyrightText: linerule-rs contributors <https://github.com/P4suta/linerule-rs>
# SPDX-License-Identifier: MIT OR Apache-2.0

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateScript({ Test-Path -LiteralPath $_ -PathType Leaf })]
    [string]$Executable,

    [string]$OutputDirectory = "artifacts/blur"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

$sourceExecutable = [System.IO.Path]::GetFullPath($Executable)
$output = [System.IO.Path]::GetFullPath($OutputDirectory)
[System.IO.Directory]::CreateDirectory($output) | Out-Null

$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$testRoot = Join-Path (
    $temporaryRoot
) "linerule-blur-$([guid]::NewGuid().ToString('N'))"
$portableRoot = Join-Path $testRoot "portable"
[System.IO.Directory]::CreateDirectory($portableRoot) | Out-Null
Copy-Item -LiteralPath $sourceExecutable `
    -Destination (Join-Path $portableRoot "linerule.exe")
[System.IO.File]::WriteAllText((Join-Path $portableRoot "linerule.portable"), "")
$portableData = Join-Path $portableRoot "data"
[System.IO.Directory]::CreateDirectory($portableData) | Out-Null
@{
    schema_version = 1
    ruler = @{
        last_active = "horizontal"
        effect = "dim_black"
        thickness = 28
        opacity = 170
        blur = 111
    }
    hotkeys = [ordered]@{
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
} | ConvertTo-Json -Depth 4 | Set-Content `
    -LiteralPath (Join-Path $portableData "settings.json") -Encoding utf8

$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$ready = Join-Path $testRoot "background-ready"
$background = Start-Job `
    -ArgumentList $screen.Left, $screen.Top, $screen.Width, $screen.Height, $ready `
    -ScriptBlock {
        param($Left, $Top, $Width, $Height, $Ready)
        Add-Type -AssemblyName System.Drawing
        Add-Type -AssemblyName System.Windows.Forms

        $bitmap = [System.Drawing.Bitmap]::new($Width, $Height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $dark = [System.Drawing.SolidBrush]::new(
            [System.Drawing.Color]::FromArgb(16, 16, 16))
        $light = [System.Drawing.SolidBrush]::new(
            [System.Drawing.Color]::FromArgb(240, 240, 240))
        try {
            $cell = 12
            for ($y = 0; $y -lt $Height; $y += $cell) {
                for ($x = 0; $x -lt $Width; $x += $cell) {
                    $brush = if ((($x / $cell) + ($y / $cell)) % 2 -eq 0) {
                        $dark
                    } else {
                        $light
                    }
                    $graphics.FillRectangle($brush, $x, $y, $cell, $cell)
                }
            }
        }
        finally {
            $dark.Dispose()
            $light.Dispose()
            $graphics.Dispose()
        }

        $form = [System.Windows.Forms.Form]::new()
        $form.FormBorderStyle =
            [System.Windows.Forms.FormBorderStyle]::None
        $form.StartPosition =
            [System.Windows.Forms.FormStartPosition]::Manual
        $form.Bounds =
            [System.Drawing.Rectangle]::new($Left, $Top, $Width, $Height)
        $form.BackgroundImage = $bitmap
        $form.BackgroundImageLayout =
            [System.Windows.Forms.ImageLayout]::None
        # Keep the synthetic backdrop above the agent terminal. The overlay is
        # created later as a topmost window and therefore remains above it.
        $form.TopMost = $true
        $form.ShowInTaskbar = $false
        $form.Add_Shown({
            [System.IO.File]::WriteAllText($Ready, "")
            $form.Activate()
        })
        try {
            [System.Windows.Forms.Application]::Run($form)
        }
        finally {
            $form.Dispose()
            $bitmap.Dispose()
        }
    }

function Save-Screen {
    param([Parameter(Mandatory = $true)][string]$Path)

    $bitmap = [System.Drawing.Bitmap]::new($screen.Width, $screen.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.CopyFromScreen(
            $screen.Left,
            $screen.Top,
            0,
            0,
            $screen.Size,
            [System.Drawing.CopyPixelOperation]::SourceCopy)
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally {
        $graphics.Dispose()
        $bitmap.Dispose()
    }
}

function Get-ImageMetrics {
    param([Parameter(Mandatory = $true)][string]$Path)

    $bitmap = [System.Drawing.Bitmap]::FromFile($Path)
    try {
        $left = 0
        $top = [int]($bitmap.Height * 2 / 3)
        $right = [int]($bitmap.Width / 3)
        $bottom = $bitmap.Height
        $step = 2
        [double]$sum = 0
        [double]$sumSquares = 0
        [double]$edge = 0
        [int]$samples = 0
        [int]$edges = 0
        $buckets = [bool[]]::new(256)

        for ($y = $top; $y -lt $bottom; $y += $step) {
            for ($x = $left; $x -lt $right; $x += $step) {
                $color = $bitmap.GetPixel($x, $y)
                $luma = [int](
                    (54 * $color.R + 183 * $color.G + 19 * $color.B) / 256)
                $sum += $luma
                $sumSquares += $luma * $luma
                $samples++
                $buckets[$luma] = $true

                if ($x + $step -lt $right) {
                    $neighbor = $bitmap.GetPixel($x + $step, $y)
                    $neighborLuma = [int](
                        (54 * $neighbor.R + 183 * $neighbor.G +
                            19 * $neighbor.B) / 256)
                    $edge += [Math]::Abs($neighborLuma - $luma)
                    $edges++
                }
            }
        }

        $mean = $sum / $samples
        $variance = [Math]::Max(0, $sumSquares / $samples - $mean * $mean)
        $edgeMean = $edge / $edges
        $normalizedEdge = $edgeMean / [Math]::Max(1, [Math]::Sqrt($variance))
        $distinct = @($buckets | Where-Object { $_ }).Count
        return [pscustomobject]@{
            Mean = $mean
            Variance = $variance
            Edge = $edgeMean
            NormalizedEdge = $normalizedEdge
            Distinct = $distinct
        }
    }
    finally {
        $bitmap.Dispose()
    }
}

function Wait-PerformanceSummary {
    param(
        [Parameter(Mandatory = $true)][string]$LogDirectory,
        [int]$TimeoutMilliseconds = 5000
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        $events = @(
            Get-ChildItem -LiteralPath $LogDirectory `
                -Filter "events.jsonl*" -File -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTimeUtc
        )
        foreach ($eventFile in $events) {
            foreach ($line in Get-Content -LiteralPath $eventFile.FullName) {
                try {
                    $event = $line | ConvertFrom-Json
                }
                catch {
                    continue
                }
                if (
                    $event.target -eq "performance" -and
                    $event.fields.message -eq "runtime performance summary"
                ) {
                    return $event.fields
                }
            }
        }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)

    throw "The runtime performance summary was not flushed to JSON logs"
}

$process = $null
$duplicate = $null
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    while (-not (Test-Path -LiteralPath $ready -PathType Leaf)) {
        if ([DateTime]::UtcNow -ge $deadline) {
            throw "Checkerboard background did not become ready"
        }
        Start-Sleep -Milliseconds 100
    }

    [System.Windows.Forms.Cursor]::Position =
        [System.Drawing.Point]::new(
            $screen.Left + [int]($screen.Width / 2),
            $screen.Top + [int]($screen.Height / 2))
    Start-Sleep -Milliseconds 300

    $beforePath = Join-Path $output "checkerboard.png"
    Save-Screen -Path $beforePath

    $process = Start-Process `
        -FilePath (Join-Path $portableRoot "linerule.exe") `
        -PassThru
    Start-Sleep -Seconds 2

    $duplicate = Start-Process `
        -FilePath (Join-Path $portableRoot "linerule.exe") `
        -PassThru
    if (-not $duplicate.WaitForExit(2000)) {
        throw "A second linerule instance did not reject startup promptly"
    }
    if ($duplicate.ExitCode -eq 0) {
        throw "A second linerule instance unexpectedly succeeded"
    }

    # Show first, then cycle Dim -> White -> Blur while the ruler is active.
    [System.Windows.Forms.SendKeys]::SendWait("^+c")
    Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait("^+b")
    Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait("^+b")
    Start-Sleep -Seconds 2

    $blurPath = Join-Path $output "backdrop-blur.png"
    Save-Screen -Path $blurPath

    $before = Get-ImageMetrics -Path $beforePath
    $after = Get-ImageMetrics -Path $blurPath
    $metrics = [ordered]@{
        before = $before
        after = $after
    }
    $metrics | ConvertTo-Json -Depth 4 |
        Set-Content -LiteralPath (Join-Path $output "metrics.json") -Encoding utf8

    if ($before.Variance -lt 1000) {
        throw "The checkerboard baseline is not sufficiently high contrast"
    }
    if ($after.Distinct -lt 4 -or $after.Variance -le 1) {
        throw "Blur output became a flat color"
    }
    if ($after.Variance -ge $before.Variance * 0.9) {
        throw "Blur did not reduce pixel variance"
    }
    if ($after.NormalizedEdge -ge $before.NormalizedEdge * 0.9) {
        throw "Blur did not reduce normalized high-frequency edges"
    }

    $exitTimer = [System.Diagnostics.Stopwatch]::StartNew()
    [System.Windows.Forms.SendKeys]::SendWait("^+i")
    if (-not $process.WaitForExit(1000)) {
        throw "linerule did not exit within one second"
    }
    $exitTimer.Stop()
    $performance = Wait-PerformanceSummary `
        -LogDirectory (Join-Path $portableData "logs")
    $performance |
        ConvertTo-Json -Depth 4 |
        Set-Content `
            -LiteralPath (Join-Path $output "performance.json") `
            -Encoding utf8
    if ([int]$performance.tick_samples -lt 30) {
        throw "Too few active render ticks were sampled for the p99 gate"
    }
    if (
        -not [bool]$performance.within_frame_budget -or
        [double]$performance.tick_p99_ms -gt
            [double]$performance.frame_budget_ms
    ) {
        throw (
            "Active tick p99 {0:N3} ms exceeded one {1} Hz frame ({2:N3} ms)" -f
            [double]$performance.tick_p99_ms,
            [int]$performance.refresh_hz,
            [double]$performance.frame_budget_ms
        )
    }
    Write-Host (
        "Backdrop Blur pixels and p99: passed; p99_ms={0:N3}; exit_ms={1:N0}" -f
        [double]$performance.tick_p99_ms,
        $exitTimer.Elapsed.TotalMilliseconds)
}
finally {
    if ($null -ne $duplicate -and -not $duplicate.HasExited) {
        Stop-Process -Id $duplicate.Id -Force -ErrorAction SilentlyContinue
    }
    if ($null -ne $process -and -not $process.HasExited) {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $portableData -PathType Container) {
        $runtimeEvidence = Join-Path (
            $output
        ) "runtime-$([System.IO.Path]::GetFileName($testRoot))"
        Copy-Item -LiteralPath $portableData `
            -Destination $runtimeEvidence -Recurse -Force
    }
    Stop-Job -Job $background -ErrorAction SilentlyContinue
    Remove-Job -Job $background -Force -ErrorAction SilentlyContinue

    $resolvedTestRoot = [System.IO.Path]::GetFullPath($testRoot)
    $temporaryPrefix = $temporaryRoot.TrimEnd("\") + "\"
    if ($resolvedTestRoot.StartsWith(
        $temporaryPrefix,
        [System.StringComparison]::OrdinalIgnoreCase
    )) {
        Remove-Item -LiteralPath $resolvedTestRoot -Recurse -Force
    }
}
