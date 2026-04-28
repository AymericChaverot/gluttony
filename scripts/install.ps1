# Gluttony installer for Windows
# Usage: irm https://raw.githubusercontent.com/AymericChaverot/gluttony/main/scripts/install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "AymericChaverot/gluttony"
$BinaryName = "gluttony.exe"
$InstallDir = "$env:USERPROFILE\.gluttony\bin"
$Target = "x86_64-pc-windows-msvc"

function Write-Info($msg) {
    Write-Host "[*] " -ForegroundColor Green -NoNewline
    Write-Host $msg
}

function Write-Warn($msg) {
    Write-Host "[!] " -ForegroundColor Yellow -NoNewline
    Write-Host $msg
}

function Write-Err($msg) {
    Write-Host "[x] " -ForegroundColor Red -NoNewline
    Write-Host $msg
    exit 1
}

function Get-LatestVersion {
    Write-Info "Fetching latest release..."
    try {
        $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ "User-Agent" = "gluttony-installer" }
        $Version = $Release.tag_name
        Write-Info "Latest version: $Version"
        return $Version
    }
    catch {
        Write-Err "Failed to fetch latest version: $_"
    }
}

function Install-Gluttony {
    param([string]$Version)

    $Asset = "gluttony-$Target.zip"
    $DownloadUrl = "https://github.com/$Repo/releases/download/$Version/$Asset"
    $TempDir = Join-Path $env:TEMP "gluttony-install"
    $ZipPath = Join-Path $TempDir $Asset

    if (Test-Path $TempDir) {
        Remove-Item -Recurse -Force $TempDir
    }
    New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

    Write-Info "Downloading from $DownloadUrl"
    try {
        Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -UseBasicParsing
    }
    catch {
        Write-Err "Download failed. Check if a release exists for your platform: $Target"
    }

    Write-Info "Extracting..."
    Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

    if (-not (Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    $SourceExe = Join-Path $TempDir $BinaryName
    if (-not (Test-Path $SourceExe)) {
        $Found = Get-ChildItem -Path $TempDir -Recurse -Filter $BinaryName | Select-Object -First 1
        if ($Found) {
            $SourceExe = $Found.FullName
        }
        else {
            Write-Err "Could not find $BinaryName in the downloaded archive."
        }
    }

    Write-Info "Installing to $InstallDir\$BinaryName"
    Copy-Item -Path $SourceExe -Destination (Join-Path $InstallDir $BinaryName) -Force

    Remove-Item -Recurse -Force $TempDir

    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($UserPath -notlike "*$InstallDir*") {
        Write-Info "Adding $InstallDir to user PATH"
        [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
        $env:Path = "$env:Path;$InstallDir"
        Write-Warn "PATH updated. Restart your terminal for changes to take effect."
    }

    Write-Info "Installed gluttony $Version to $InstallDir\$BinaryName"
}

function Test-Installation {
    $GluttonyPath = Join-Path $InstallDir $BinaryName
    if (Test-Path $GluttonyPath) {
        $Version = & $GluttonyPath --version 2>&1
        Write-Info "Verification: $Version"
    }
    else {
        Write-Warn "Binary not found at expected location. Add $InstallDir to your PATH manually."
    }
    Write-Host ""
    Write-Host "Installation complete. " -ForegroundColor Green -NoNewline
    Write-Host "Run 'gluttony --help' to get started."
    Write-Host ""
}

# Main
Write-Host ""
Write-Host "  =========================================" -ForegroundColor DarkGray
Write-Host "   G L U T T O N Y  -  Installer (Windows)" -ForegroundColor White
Write-Host "  =========================================" -ForegroundColor DarkGray
Write-Host ""

$LatestVersion = Get-LatestVersion
Install-Gluttony -Version $LatestVersion
Test-Installation
