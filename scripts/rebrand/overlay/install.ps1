# Kete installer for Windows: installs or updates kete.exe, and bro.exe
# (Kete's old name, the same program) beside it.
#
#   irm https://raw.githubusercontent.com/JayRichh/kete/main/install.ps1 | iex
#
# Downloads kete-<target>.zip from the latest GitHub release into
# %LOCALAPPDATA%\kete\bin and puts that folder on your user PATH. When the
# release has no asset for this machine, it builds from source with
# `cargo install` instead and says so. Running it again updates in place.
#
# Coming from Bro: an install in %LOCALAPPDATA%\bro\bin is taken off your
# PATH (kete's folder has a bro.exe of its own), and its bro.exe is updated
# too, so a shortcut or Windows Terminal profile that points at it still
# starts the new build. Your data moves to %LOCALAPPDATA%\kete the first
# time kete runs.
#
# On x64 it also installs the desktop window: when the release has
# kete-desktop-x86_64-setup.exe, it runs that installer silently, for your
# user only (no admin prompt), into %LOCALAPPDATA%\kete\desktop, and copies
# kete-desktop.exe beside kete.exe so `kete desktop` finds it. A release
# without it (older ones) just skips this step. The installer isn't code
# signed yet, so Windows SmartScreen may warn the first time it runs.
#
# Skip the desktop window with -NoDesktop, or KETE_NO_DESKTOP=1:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/JayRichh/kete/main/install.ps1))) -NoDesktop
#
# Environment overrides:
#   KETE_INSTALL_DIR   install destination (default: %LOCALAPPDATA%\kete\bin)
#   KETE_BRANCH        branch the source fallback builds (default: windows)
#   KETE_NO_DESKTOP    set to 1 to skip the desktop window
#   (BRO_INSTALL_DIR and BRO_BRANCH still work.)
#
# Written for Windows PowerShell 5.1 and PowerShell 7. It never calls `exit`,
# so piping it into `iex` can't close your shell.

param([switch]$NoDesktop)

& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'

    $Repo = 'JayRichh/kete'
    $Branch = if ($env:KETE_BRANCH) { $env:KETE_BRANCH } elseif ($env:BRO_BRANCH) { $env:BRO_BRANCH } else { 'windows' }
    $InstallDir = if ($env:KETE_INSTALL_DIR) { $env:KETE_INSTALL_DIR } elseif ($env:BRO_INSTALL_DIR) { $env:BRO_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'kete\bin' }
    $LegacyDir = Join-Path $env:LOCALAPPDATA 'bro\bin'

    function Say([string]$Message) { Write-Host $Message }

    # GitHub needs TLS 1.2; Windows PowerShell 5.1 may default to older.
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    } catch { }

    function Get-KeteTarget {
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

    # Put $Source at $Dest. A running daemon keeps its exe locked, but a
    # locked file can still be renamed out of the way. Leftovers from earlier
    # updates go when nothing holds them any more; one still held gets a
    # fresh name instead.
    function Replace-Exe([string]$Source, [string]$Dest) {
        $dir = Split-Path $Dest -Parent
        $name = Split-Path $Dest -Leaf
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        Get-ChildItem -Path $dir -Filter "$name*.old" -ErrorAction SilentlyContinue |
            ForEach-Object { Remove-Item $_.FullName -Force -ErrorAction SilentlyContinue }
        $old = "$Dest.old"
        if (Test-Path $old) { $old = Join-Path $dir ("$name." + [Guid]::NewGuid().ToString('N') + ".old") }
        if (Test-Path $Dest) { Move-Item $Dest $old -Force }
        Copy-Item $Source $Dest -Force
        if (Test-Path $old) { Remove-Item $old -Force -ErrorAction SilentlyContinue }
    }

    function Install-FromRelease([string]$Target) {
        $url = "https://github.com/$Repo/releases/latest/download/kete-$Target.zip"
        $tmp = Join-Path ([IO.Path]::GetTempPath()) ("kete-install-" + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tmp | Out-Null
        try {
            $zip = Join-Path $tmp 'kete.zip'
            Say "downloading $url"
            try {
                Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
            } catch {
                return $null
            }
            $unpacked = Join-Path $tmp 'unpacked'
            Expand-Archive -Path $zip -DestinationPath $unpacked -Force
            $exe = Get-ChildItem -Path $unpacked -Filter 'kete.exe' -Recurse | Select-Object -First 1
            if (-not $exe) { throw "the release archive has no kete.exe in it" }

            $dest = Join-Path $InstallDir 'kete.exe'
            Replace-Exe $exe.FullName $dest
            Replace-Exe $exe.FullName (Join-Path $InstallDir 'bro.exe')
            return $dest
        } finally {
            Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    # The desktop window: run the release's NSIS installer silently into
    # %LOCALAPPDATA%\kete\desktop (kept off PATH, since NSIS puts an
    # uninstall.exe there), then copy kete-desktop.exe beside kete.exe.
    # Returns $false, after saying why, when there's nothing to install;
    # never throws, so a missing desktop never fails the CLI install.
    function Install-Desktop([string]$BinDir) {
        $url = "https://github.com/$Repo/releases/latest/download/kete-desktop-x86_64-setup.exe"
        $desktopDir = Join-Path $env:LOCALAPPDATA 'kete\desktop'
        $tmp = Join-Path ([IO.Path]::GetTempPath()) ("kete-desktop-" + [Guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tmp | Out-Null
        try {
            $setup = Join-Path $tmp 'kete-desktop-setup.exe'
            Say "downloading $url"
            try {
                Invoke-WebRequest -Uri $url -OutFile $setup -UseBasicParsing
            } catch {
                Say "no desktop installer in the latest release; skipping the desktop window."
                return $false
            }
            # NSIS wants /D= last and unquoted, even with spaces in the path.
            $proc = Start-Process -FilePath $setup -ArgumentList "/S /D=$desktopDir" -Wait -PassThru
            if ($proc.ExitCode -ne 0) {
                Say "the desktop installer failed (exit $($proc.ExitCode)); skipping the desktop window."
                return $false
            }
            $exe = Join-Path $desktopDir 'kete-desktop.exe'
            if (-not (Test-Path $exe)) {
                Say "the desktop installer left no kete-desktop.exe in $desktopDir; skipping the desktop window."
                return $false
            }
            Replace-Exe $exe (Join-Path $BinDir 'kete-desktop.exe')
            Say "installed the desktop window at $desktopDir; run 'kete desktop' to open it"
            return $true
        } catch {
            Say "couldn't install the desktop window: $($_.Exception.Message)"
            return $false
        } finally {
            Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    function Install-FromSource {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            throw "there's no prebuilt kete for this machine and cargo isn't installed. Get Rust from https://rustup.rs and run this again."
        }
        Say "building kete from source with cargo (this takes a few minutes)..."
        & cargo install --git "https://github.com/$Repo" --branch $Branch nebula --locked --force | Out-Host
        if ($LASTEXITCODE -ne 0) { throw "cargo install failed (exit $LASTEXITCODE)" }
        return (Join-Path $env:USERPROFILE '.cargo\bin\kete.exe')
    }

    function Same-Dir([string]$A, [string]$B) { $A.TrimEnd('\') -ieq $B.TrimEnd('\') }

    function Add-ToUserPath([string]$Dir) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        $parts = @()
        if ($userPath) { $parts = $userPath -split ';' | Where-Object { $_ } }
        if (-not ($parts | Where-Object { Same-Dir $_ $Dir })) {
            [Environment]::SetEnvironmentVariable('Path', (($parts + $Dir) -join ';'), 'User')
            Say "added $Dir to your user PATH (new terminals pick it up)"
        }
        if (-not (($env:Path -split ';') | Where-Object { Same-Dir $_ $Dir })) {
            $env:Path = "$env:Path;$Dir"
        }
    }

    function Remove-FromUserPath([string]$Dir) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if ($userPath) {
            $parts = $userPath -split ';' | Where-Object { $_ }
            $kept = $parts | Where-Object { -not (Same-Dir $_ $Dir) }
            if (@($kept).Count -ne @($parts).Count) {
                [Environment]::SetEnvironmentVariable('Path', (@($kept) -join ';'), 'User')
                Say "took $Dir off your user PATH; kete's folder has bro.exe now"
            }
        }
        $env:Path = (($env:Path -split ';') | Where-Object { $_ -and -not (Same-Dir $_ $Dir) }) -join ';'
    }

    $installed = $null
    $target = Get-KeteTarget
    if ($target) {
        $installed = Install-FromRelease $target
        if ($installed) {
            Add-ToUserPath $InstallDir
        } else {
            Say "no kete-$target.zip in the latest release of $Repo."
        }
    } else {
        Say "no prebuilt kete for this CPU architecture."
    }

    if (-not $installed) {
        Say "falling back to: cargo install --git https://github.com/$Repo --branch $Branch nebula --locked"
        $installed = Install-FromSource
        Add-ToUserPath (Split-Path $installed -Parent)
    }

    # A Bro install from before the rename: refresh its bro.exe so anything
    # pointing at it runs this build, and take its folder off PATH so it
    # can't shadow the new one.
    $legacyExe = Join-Path $LegacyDir 'bro.exe'
    if ((Test-Path $legacyExe) -and -not (Same-Dir $LegacyDir (Split-Path $installed -Parent))) {
        Replace-Exe $installed $legacyExe
        Remove-FromUserPath $LegacyDir
        Say "updated the old Bro install at $legacyExe to this build"
        Say "run 'kete setup terminal' to point your Windows Terminal profile at kete"
    }

    $version = & $installed --version
    Say "installed $version at $installed (and bro.exe beside it)"

    # The desktop installer is x64 only.
    if ($NoDesktop -or $env:KETE_NO_DESKTOP -eq '1') {
        Say "skipping the desktop window (-NoDesktop)."
    } elseif ($target -eq 'x86_64-pc-windows-msvc') {
        Install-Desktop (Split-Path $installed -Parent) | Out-Null
    } else {
        Say "no desktop window for this CPU architecture yet; 'kete' runs in the terminal."
    }
    Say "run 'kete' to start."
}
