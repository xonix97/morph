param(
    [switch]$Install,
    [switch]$Direct
)

$ErrorActionPreference = "Stop"
$DemoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent (Split-Path -Parent $DemoRoot)
$VenvRoot = Join-Path $DemoRoot ".venv"
$VenvPython = Join-Path $VenvRoot "Scripts\python.exe"
$VenvScripts = Join-Path $VenvRoot "Scripts"

function Invoke-BasePython {
    param([string[]]$Arguments)

    if ($script:BasePythonLauncher -eq "py") {
        $PythonArguments = @($script:BasePythonArguments) + @($Arguments)
        & py @PythonArguments
    } else {
        & python @Arguments
    }
    if ($LASTEXITCODE -ne 0) {
        throw "Python command failed with exit code $LASTEXITCODE"
    }
}

function Test-VenvImports {
    $PreviousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $VenvPython -c "import pygame; import OpenGL.GL" 1>$null 2>$null
        return ($LASTEXITCODE -eq 0)
    } finally {
        $ErrorActionPreference = $PreviousErrorActionPreference
    }
}

if (Get-Command py -ErrorAction SilentlyContinue) {
    $script:BasePythonLauncher = "py"
    $script:BasePythonArguments = @("-3.11")
    $VersionArguments = @($script:BasePythonArguments) + @("--version")
    & py @VersionArguments 2>$null
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "Python 3.11 was not found; trying the default Python 3 launcher."
        $script:BasePythonArguments = @("-3")
    }
} elseif (Get-Command python -ErrorAction SilentlyContinue) {
    $script:BasePythonLauncher = "python"
    $script:BasePythonArguments = @()
} else {
    throw "Python was not found. Install Python 3.11+ and reopen PowerShell."
}

if (-not (Test-Path $VenvPython)) {
    Write-Host "Creating project-local Python environment: $VenvRoot"
    Invoke-BasePython @("-m", "venv", $VenvRoot)
}

$env:PATH = "$VenvScripts;$env:PATH"
$env:PYTHONPATH = "$RepoRoot\sdk\python;$DemoRoot;$env:PYTHONPATH"

$importsWork = Test-VenvImports
if ($Install -or -not $importsWork) {
    Write-Host "Installing pinned particle-demo dependencies..."
    $PreviousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        & $VenvPython -m pip install --disable-pip-version-check -r (Join-Path $DemoRoot "requirements.lock")
        $PipExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $PreviousErrorActionPreference
    }
    if ($PipExitCode -ne 0) {
        throw "Could not install the particle-demo dependencies. Check network access and the Python version."
    }
}

if (-not (Test-VenvImports)) {
    throw "pygame/PyOpenGL are unavailable in $VenvRoot"
}

if ($Direct) {
    Push-Location $DemoRoot
    try {
        & $VenvPython "app.py"
        $ExitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }
} else {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "Cargo was not found. Install Rust and reopen PowerShell, or use -Direct."
    }

    $IsWslUncPath = $RepoRoot -match '^(\\\\wsl\$|\\\\wsl\.localhost)\\'
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

    $CargoArguments = @(
        "run",
        "--manifest-path",
        (Join-Path $RepoRoot "Cargo.toml"),
        "--locked",
        "--",
        "dev",
        $DemoRoot
    )
    Push-Location $RepoRoot
    try {
        & cargo @CargoArguments
        $ExitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }
}

if ($ExitCode -ne 0) {
    exit $ExitCode
}
