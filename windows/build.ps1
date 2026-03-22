#Requires -Version 5.1
<#
.SYNOPSIS
    Build script for Yap (Windows) — equivalent to macOS build.sh
.DESCRIPTION
    Cleans previous builds, runs dotnet publish for win-x64 self-contained,
    optionally signs the binary, and creates a portable zip archive.
.EXAMPLE
    .\build.ps1
    .\build.ps1 -Configuration Debug
    .\build.ps1 -SkipZip
#>

param(
    [ValidateSet("Release", "Debug")]
    [string]$Configuration = "Release",

    [string]$Runtime = "win-x64",

    [switch]$SkipZip,

    [string]$CertificateThumbprint = ""
)

$ErrorActionPreference = "Stop"

$AppName       = "Yap"
$ScriptDir     = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectDir    = Join-Path $ScriptDir "Yap"
$CsprojPath    = Join-Path $ProjectDir "Yap.csproj"
$BuildDir      = Join-Path $ScriptDir "build"
$RepoRoot      = Split-Path -Parent $ScriptDir
$SoundsSource  = Join-Path $ProjectDir "Resources" "Sounds"

# ── Extract version from .csproj ─────────────────────────────────────────────
Write-Host ""
Write-Host "=== Building $AppName for Windows ===" -ForegroundColor Cyan

if (-not (Test-Path $CsprojPath)) {
    Write-Host "ERROR: $CsprojPath not found." -ForegroundColor Red
    exit 1
}

[xml]$csproj = Get-Content $CsprojPath
$Version = $csproj.Project.PropertyGroup.Version
if (-not $Version) {
    $Version = "0.0.0"
    Write-Host "WARNING: No <Version> found in .csproj, defaulting to $Version" -ForegroundColor DarkYellow
}

Write-Host "  Version:       $Version"
Write-Host "  Configuration: $Configuration"
Write-Host "  Runtime:       $Runtime"
Write-Host ""

# ── Step 1: Clean previous build ─────────────────────────────────────────────
Write-Host "[1/6] Cleaning previous build..." -ForegroundColor Yellow

if (Test-Path $BuildDir) {
    Remove-Item -Recurse -Force $BuildDir
    Write-Host "       Removed $BuildDir"
}

# Clean dotnet build artifacts
$binDir = Join-Path $ProjectDir "bin"
$objDir = Join-Path $ProjectDir "obj"
if (Test-Path $binDir) { Remove-Item -Recurse -Force $binDir }
if (Test-Path $objDir) { Remove-Item -Recurse -Force $objDir }
Write-Host "       Cleaned bin/ and obj/"

# ── Step 2: Restore ──────────────────────────────────────────────────────────
Write-Host "[2/6] Restoring NuGet packages..." -ForegroundColor Yellow
Push-Location $ProjectDir
try {
    dotnet restore --verbosity quiet
    if ($LASTEXITCODE -ne 0) { throw "dotnet restore failed with exit code $LASTEXITCODE" }
    Write-Host "       Restore complete"
}
catch {
    Pop-Location
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}

# ── Step 3: Publish ──────────────────────────────────────────────────────────
Write-Host "[3/6] Publishing self-contained single-file app..." -ForegroundColor Yellow
try {
    dotnet publish `
        --configuration $Configuration `
        --runtime $Runtime `
        --self-contained true `
        --output $BuildDir `
        -p:PublishSingleFile=true `
        -p:PublishTrimmed=true `
        -p:IncludeNativeLibrariesForSelfExtract=true `
        -p:TrimMode=partial `
        --verbosity minimal

    if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed with exit code $LASTEXITCODE" }
    Write-Host "       Publish complete"
}
catch {
    Pop-Location
    Write-Host "ERROR: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
finally {
    Pop-Location
}

# ── Step 4: Copy sound assets ────────────────────────────────────────────────
Write-Host "[4/6] Copying sound assets..." -ForegroundColor Yellow

if (Test-Path $SoundsSource) {
    $SoundsDest = Join-Path $BuildDir "Resources" "Sounds"
    New-Item -ItemType Directory -Path $SoundsDest -Force | Out-Null

    $wavFiles = Get-ChildItem -Path $SoundsSource -Filter "*.wav" -ErrorAction SilentlyContinue
    if ($wavFiles) {
        $wavFiles | Copy-Item -Destination $SoundsDest
        Write-Host "       Copied $($wavFiles.Count) .wav sound file(s)"
    } else {
        Write-Host "       WARNING: No .wav files found in $SoundsSource" -ForegroundColor DarkYellow
        Write-Host "       Run convert-sounds.sh to convert .aiff -> .wav" -ForegroundColor DarkYellow
    }
} else {
    Write-Host "       Sounds source directory not found, skipping" -ForegroundColor DarkYellow
}

# ── Step 5: Code signing (optional) ─────────────────────────────────────────
Write-Host "[5/6] Code signing..." -ForegroundColor Yellow

$exePath = Join-Path $BuildDir "$AppName.exe"
if (-not (Test-Path $exePath)) {
    Write-Host "ERROR: $AppName.exe not found in build output." -ForegroundColor Red
    exit 1
}

$signed = $false
if ($CertificateThumbprint) {
    $signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($signtool) {
        Write-Host "       Signing with certificate $CertificateThumbprint..."
        & signtool.exe sign `
            /sha1 $CertificateThumbprint `
            /tr http://timestamp.digicert.com `
            /td sha256 `
            /fd sha256 `
            /v `
            $exePath

        if ($LASTEXITCODE -ne 0) {
            Write-Host "       WARNING: Signing failed (exit code $LASTEXITCODE)" -ForegroundColor DarkYellow
        } else {
            $signed = $true
            Write-Host "       Signed successfully"
        }
    } else {
        Write-Host "       WARNING: signtool.exe not found in PATH, skipping signing" -ForegroundColor DarkYellow
    }
} else {
    Write-Host "       No certificate thumbprint provided, skipping signing" -ForegroundColor DarkYellow
}

# ── Step 6: Create portable zip ──────────────────────────────────────────────
Write-Host "[6/6] Creating portable zip..." -ForegroundColor Yellow

if (-not $SkipZip) {
    $ZipName = "$AppName-v$Version-win64.zip"
    $ZipPath = Join-Path $ScriptDir $ZipName

    if (Test-Path $ZipPath) {
        Remove-Item -Force $ZipPath
    }

    Compress-Archive -Path "$BuildDir\*" -DestinationPath $ZipPath -CompressionLevel Optimal

    if (Test-Path $ZipPath) {
        $zipSize = [math]::Round((Get-Item $ZipPath).Length / 1MB, 1)
        Write-Host "       Created $ZipName ($zipSize MB)"
    } else {
        Write-Host "       WARNING: Failed to create zip archive" -ForegroundColor DarkYellow
    }
} else {
    Write-Host "       Skipped (use without -SkipZip to create)"
}

# ── Summary ──────────────────────────────────────────────────────────────────
Write-Host ""
Write-Host "=== Build complete ===" -ForegroundColor Green
Write-Host ""

$exeSize = [math]::Round((Get-Item $exePath).Length / 1MB, 1)
Write-Host "  Output:  $BuildDir" -ForegroundColor White
Write-Host "  Binary:  $AppName.exe ($exeSize MB)" -ForegroundColor White
Write-Host "  Version: $Version" -ForegroundColor White
Write-Host "  Signed:  $signed" -ForegroundColor White

if (-not $SkipZip) {
    $ZipName = "$AppName-v$Version-win64.zip"
    $ZipPath = Join-Path $ScriptDir $ZipName
    if (Test-Path $ZipPath) {
        Write-Host "  Zip:     $ZipName" -ForegroundColor White
    }
}

Write-Host ""
Write-Host "To run:"
Write-Host "  & `"$exePath`""
Write-Host ""
