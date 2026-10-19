param(
    [switch]$Dev,
    [switch]$Particles,
    [switch]$Install,
    [switch]$Template
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path

if ($Particles) {
    $ParticleLauncher = Join-Path $Root "demos\particles\run.ps1"
    if ($Install) {
        & $ParticleLauncher -Install
    } else {
        & $ParticleLauncher
    }
    exit $LASTEXITCODE
}

# Windows exposes a WSL checkout through a UNC/9P path (`\\wsl$` or
# `\\wsl.localhost`). Cargo's incremental lock files do not work reliably on
# that filesystem, so keep build artifacts on a normal local Windows volume.
$IsWslUncPath = $Root -match '^(\\\\wsl\$|\\\\wsl\.localhost)\\'
if ($IsWslUncPath) {
    $LocalTargetBase = if ($env:LOCALAPPDATA) {
        Join-Path $env:LOCALAPPDATA "Morph"
    } else {
        Join-Path ([System.IO.Path]::GetTempPath()) "Morph"
    }
    $LocalTargetDir = Join-Path $LocalTargetBase "target"
    New-Item -ItemType Directory -Force -Path $LocalTargetDir | Out-Null
    $env:CARGO_TARGET_DIR = $LocalTargetDir
    $env:CARGO_INCREMENTAL = "0"
    Write-Host "WSL checkout detected; using local Cargo target directory: $LocalTargetDir"
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Cargo was not found on PATH. Install Rust from https://rustup.rs/ and reopen PowerShell."
}

if ($Template) {
    $CargoArguments = @(
        "run",
        "--manifest-path",
        (Join-Path $Root "Cargo.toml"),
        "--locked",
        "--offline",
        "--",
        "dev",
        (Join-Path $Root "templates/python-controlled")
    )
} elseif (-not (Get-Command g++ -ErrorAction SilentlyContinue)) {
    throw "g++ was not found on PATH. Add your MinGW/MSYS2 bin directory to PATH, then retry."
} else {
    $CargoArguments = @(
        "run",
        "--manifest-path",
        (Join-Path $Root "Cargo.toml"),
        "--locked",
        "--offline",
        "--"
    )

    if ($Dev) {
        $CargoArguments += @("dev", (Join-Path $Root "demos/c"))
    } else {
        $CargoArguments += @("demo", "c")
    }
}

Push-Location $Root
try {
    & cargo @CargoArguments
    $ExitCode = $LASTEXITCODE
} finally {
    Pop-Location
}

if ($ExitCode -ne 0) {
    exit $ExitCode
}
