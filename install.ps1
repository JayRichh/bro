# bro installer for Windows: installs or updates bro.exe.
#
#   irm https://raw.githubusercontent.com/JayRichh/bro/main/install.ps1 | iex
#
# Downloads bro-<target>.zip from the latest GitHub release into
# %LOCALAPPDATA%\bro\bin and puts that folder on your user PATH. When the
# release has no asset for this machine, it builds from source with
# `cargo install` instead and says so. Running it again updates in place.
#
# Environment overrides:
#   BRO_INSTALL_DIR   install destination (default: %LOCALAPPDATA%\bro\bin)
#   BRO_BRANCH        branch the source fallback builds (default: windows)
#
# Written for Windows PowerShell 5.1 and PowerShell 7. It never calls `exit`,
# so piping it into `iex` can't close your shell.

& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'

    $Repo = 'JayRichh/bro'
    $Branch = if ($env:BRO_BRANCH) { $env:BRO_BRANCH } else { 'windows' }
    $InstallDir = if ($env:BRO_INSTALL_DIR) { $env:BRO_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'bro\bin' }

    function Say([string]$Message) { Write-Host $Message }

    # GitHub needs TLS 1.2; Windows PowerShell 5.1 may default to older.
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    } catch { }

    function Get-BroTarget {
        $arch = $null
        try {
            $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
        } catch { }
        if (-not $arch) {
            $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
        }
        switch -Regex ($arch) {
            '^(X64|AMD64)$' { return 'x86_64-pc-windows-msvc' }
            '^(Arm64|ARM64)$' { return 'aarch64-pc-windows-msvc' }
            default { return $null }
        }
    }

    function Install-FromRelease([string]$Target) {
        $url = "https://github.com/$Repo/releases/latest/download/bro-$Target.zip"
        $tmp = Join-Path ([IO.Path]::GetTempPath()) ("bro-install-" + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tmp | Out-Null
        try {
            $zip = Join-Path $tmp 'bro.zip'
            Say "downloading $url"
            try {
                Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
            } catch {
                return $false
            }
            $unpacked = Join-Path $tmp 'unpacked'
            Expand-Archive -Path $zip -DestinationPath $unpacked -Force
            $exe = Get-ChildItem -Path $unpacked -Filter 'bro.exe' -Recurse | Select-Object -First 1
            if (-not $exe) { throw "the release archive has no bro.exe in it" }

            New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
            $dest = Join-Path $InstallDir 'bro.exe'
            $old = "$dest.old"
            # A running daemon keeps bro.exe locked, but a locked file can
            # still be renamed out of the way. Leftovers from earlier updates
            # go when nothing holds them any more; one still held gets a
            # fresh name instead.
            Get-ChildItem -Path $InstallDir -Filter 'bro.exe*.old' -ErrorAction SilentlyContinue |
                ForEach-Object { Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue }
            if (Test-Path $old) { $old = Join-Path $InstallDir ("bro.exe." + [Guid]::NewGuid().ToString('N') + ".old") }
            if (Test-Path $dest) { Move-Item $dest $old -Force }
            Copy-Item $exe.FullName $dest -Force
            if (Test-Path $old) { Remove-Item $old -Force -ErrorAction SilentlyContinue }
            return $true
        } finally {
            Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    function Install-FromSource {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            throw "no prebuilt bro for this machine and cargo is not installed. Get Rust from https://rustup.rs and run this again."
        }
        Say "building bro from source with cargo (this takes a few minutes)..."
        & cargo install --git "https://github.com/$Repo" --branch $Branch nebula --locked --force | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "cargo install failed (exit $LASTEXITCODE)" }
        return (Join-Path $env:USERPROFILE '.cargo\bin\bro.exe')
    }

    function Add-ToUserPath([string]$Dir) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $parts = @()
        if ($userPath) { $parts = $userPath -split ';' | Where-Object { $_ } }
        $already = $parts | Where-Object { $_.TrimEnd('\') -ieq $Dir.TrimEnd('\') }
        if (-not $already) {
            [Environment]::SetEnvironmentVariable('Path', (($parts + $Dir) -join ';'), 'User')
            Say "added $Dir to your user PATH (new terminals pick it up)"
        }
        if (-not (($env:Path -split ';') | Where-Object { $_.TrimEnd('\') -ieq $Dir.TrimEnd('\') })) {
            $env:Path = "$env:Path;$Dir"
        }
    }

    $installed = $null
    $target = Get-BroTarget
    if ($target) {
        if (Install-FromRelease $target) {
            $installed = Join-Path $InstallDir 'bro.exe'
            Add-ToUserPath $InstallDir
        } else {
            Say "no bro-$target.zip in the latest release of $Repo."
        }
    } else {
        Say "no prebuilt bro for this CPU architecture."
    }

    if (-not $installed) {
        Say "falling back to: cargo install --git https://github.com/$Repo --branch $Branch nebula --locked"
        $installed = Install-FromSource
        Add-ToUserPath (Split-Path $installed -Parent)
    }

    $version = & $installed --version
    Say "installed $version -> $installed"
    Say "run 'bro' to start."
}
