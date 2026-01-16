$ErrorActionPreference = "Stop"

$Repo = "zjy-dev/morrow"
$InstallDir = if ($env:MORROW_INSTALL_DIR) { $env:MORROW_INSTALL_DIR } else { "$env:LOCALAPPDATA\morrow\bin" }
$ConfigDir = "$env:APPDATA\morrow"

function Get-Platform {
    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
    switch ($arch) {
        "X64" { return "windows-x86_64" }
        "Arm64" { return "windows-aarch64" }
        default { throw "Unsupported architecture: $arch" }
    }
}

function Main {
    Write-Host "Installing morrow..."

    $platform = Get-Platform
    Write-Host "Detected platform: $platform"

    # Create install directory
    if (!(Test-Path $InstallDir)) {
        New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    }

    # Get latest release
    $releases = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
    $latest = $releases.tag_name

    if (!$latest) {
        throw "Could not determine latest version"
    }

    Write-Host "Latest version: $latest"

    # Download binary
    $url = "https://github.com/$Repo/releases/download/$latest/morrow-$platform.exe"
    $dest = "$InstallDir\morrow.exe"
    
    Write-Host "Downloading from: $url"
    Invoke-WebRequest -Uri $url -OutFile $dest

    # Download default config if not exists
    if (!(Test-Path $ConfigDir)) {
        New-Item -ItemType Directory -Path $ConfigDir -Force | Out-Null
    }
    
    $configFile = "$ConfigDir\config.yaml"
    if (!(Test-Path $configFile)) {
        Write-Host "Downloading default configuration..."
        $configUrl = "https://raw.githubusercontent.com/$Repo/main/config.example.yaml"
        Invoke-WebRequest -Uri $configUrl -OutFile $configFile
        Write-Host "Default config created at: $configFile"
    } else {
        Write-Host "Config file already exists at: $configFile"
    }

    Write-Host ""
    Write-Host "morrow installed to: $dest"
    Write-Host ""

    # Check if in PATH
    $currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($currentPath -notlike "*$InstallDir*") {
        $addToPath = Read-Host "Add morrow to PATH? (y/N)"
        if ($addToPath -eq "y" -or $addToPath -eq "Y") {
            [Environment]::SetEnvironmentVariable("Path", "$currentPath;$InstallDir", "User")
            Write-Host "Added to PATH. Restart your terminal to use 'morrow' command."
        } else {
            Write-Host "To add manually, run:"
            Write-Host ""
            Write-Host "  `$env:Path += `";$InstallDir`""
            Write-Host ""
        }
    }

    Write-Host "Run 'morrow --help' to get started."
    Write-Host "Run 'morrow config init' to customize your configuration."
}

Main
