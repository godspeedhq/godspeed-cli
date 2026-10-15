# Install the Godspeed developer CLI (`godspeed`) on Windows.
#
#   irm https://github.com/godspeedhq/godspeed-cli/releases/latest/download/install.ps1 | iex
#
# What it does, in order, and nothing else:
#   1. works out which build fits this machine (x64 or Arm64);
#   2. downloads that build and the release's SHA256SUMS from GitHub Releases;
#   3. checks the build's SHA-256 against SHA256SUMS, and stops if it does not match;
#   4. installs it as %USERPROFILE%\.godspeed\bin\godspeed.exe and runs `godspeed --version` to confirm it;
#   5. adds that folder to your user PATH (not the system's), and to this session's.
#
# Settings, all optional (set them first, e.g. $env:GODSPEED_VERSION = "0.1.0"):
#   GODSPEED_VERSION          install that release instead of the latest
#   GODSPEED_INSTALL_DIR      install there instead of %USERPROFILE%\.godspeed\bin
#   GODSPEED_NO_MODIFY_PATH   set to 1 to leave PATH alone
#   GODSPEED_DOWNLOAD_URL     fetch from a mirror (or a local test server) instead of GitHub
#
# A checksum proves the download is the file the release lists. It does not prove who made the release:
# releases are not signed yet.
#
# Run through `iex`, this executes inside your own PowerShell session. So it never calls `exit` (that would
# close your window) and keeps every setting it changes inside its own scope.

& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue' # Invoke-WebRequest is many times slower with the progress bar

    $repo = 'godspeedhq/godspeed-cli'
    function Say([string] $m) { Write-Host "godspeed-install: $m" }
    function Fail([string] $m, [string] $next) {
        Write-Host "godspeed-install: error: $m" -ForegroundColor Red
        if ($next) { Write-Host "godspeed-install: next: $next" -ForegroundColor Red }
    }

    # Windows PowerShell 5.1 does not offer TLS 1.2 by default.
    if ($PSVersionTable.PSVersion.Major -lt 6) {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    }

    # --- 1. Which build ------------------------------------------------------------------------------
    # PROCESSOR_ARCHITEW6432 is set when a 32-bit PowerShell runs on 64-bit Windows; it names the real CPU.
    $cpu = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    switch ($cpu) {
        'AMD64' { $target = 'x86_64-pc-windows-msvc' }
        'ARM64' { $target = 'aarch64-pc-windows-msvc' }
        default {
            Fail "no build for CPU '$cpu'." "Builds exist for x64 and Arm64; or build it: cargo install --git https://github.com/$repo --locked godspeed-cli"
            return
        }
    }
    $asset = "godspeed-$target.exe"

    if ($env:GODSPEED_DOWNLOAD_URL) { $base = $env:GODSPEED_DOWNLOAD_URL.TrimEnd('/') }
    elseif ($env:GODSPEED_VERSION) { $base = "https://github.com/$repo/releases/download/cli-v$($env:GODSPEED_VERSION.TrimStart('v'))" }
    else { $base = "https://github.com/$repo/releases/latest/download" }
    if (-not $env:GODSPEED_DOWNLOAD_URL -and -not $base.StartsWith('https://')) { Fail "refusing a non-HTTPS download URL: $base"; return }

    $dir = if ($env:GODSPEED_INSTALL_DIR) { $env:GODSPEED_INSTALL_DIR } else { Join-Path $env:USERPROFILE '.godspeed\bin' }

    # --- 2. Download ---------------------------------------------------------------------------------
    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("godspeed-install-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Say "downloading $asset from $base"
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile (Join-Path $tmp $asset)
            Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS')
        } catch {
            Fail "could not download from ${base}: $($_.Exception.Message)" 'Check the network, and that the release exists. Nothing was installed.'
            return
        }

        # --- 3. Verify -------------------------------------------------------------------------------
        $expected = $null
        foreach ($line in Get-Content (Join-Path $tmp 'SHA256SUMS')) {
            $parts = $line -split '\s+', 2
            if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq $asset) { $expected = $parts[0].ToLowerInvariant() }
        }
        if (-not $expected) { Fail "SHA256SUMS does not list $asset." 'Nothing was installed.'; return }
        # .NET directly, not Get-FileHash: Windows PowerShell 5.1 started from PowerShell 7 inherits a module
        # path where Get-FileHash cannot be found.
        $sha = [Security.Cryptography.SHA256]::Create()
        $stream = [IO.File]::OpenRead((Join-Path $tmp $asset))
        try { $actual = -join ($sha.ComputeHash($stream) | ForEach-Object { $_.ToString('x2') }) }
        finally { $stream.Dispose(); $sha.Dispose() }
        if ($actual -ne $expected) {
            Fail "checksum mismatch for ${asset}: expected $expected, got $actual." 'Nothing was installed. Try again; if it persists, report it.'
            return
        }
        Say "verified sha256 $actual"

        # --- 4. Install ------------------------------------------------------------------------------
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        $exe = Join-Path $dir 'godspeed.exe'
        try {
            Move-Item -Force -Path (Join-Path $tmp $asset) -Destination $exe
        } catch {
            Fail "could not write ${exe}: $($_.Exception.Message)" 'If godspeed is running (or open in another terminal), close it and run this again.'
            return
        }
        $installed = & $exe --version
        if ($LASTEXITCODE -ne 0) { Fail 'the installed godspeed does not run.' 'Report it, with the output of: [Environment]::OSVersion'; return }
        Say "installed $installed at $exe"
    } finally {
        Remove-Item -Recurse -Force -Path $tmp -ErrorAction SilentlyContinue
    }

    # --- 5. PATH -------------------------------------------------------------------------------------
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @(if ($userPath) { $userPath -split ';' | Where-Object { $_ } })
    if ($entries -notcontains $dir) {
        if ($env:GODSPEED_NO_MODIFY_PATH -eq '1') {
            Say "$dir is not on PATH (GODSPEED_NO_MODIFY_PATH=1 left it alone)."
        } else {
            [Environment]::SetEnvironmentVariable('Path', (@($entries) + $dir) -join ';', 'User')
            Say "added $dir to your user PATH; new terminals will find godspeed."
        }
    }
    if (($env:Path -split ';') -notcontains $dir) { $env:Path = "$dir;$env:Path" }

    if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
        Say 'godspeed builds with Rust, and rustup is not installed: https://rustup.rs'
    }
    Say 'next: godspeed doctor'
}
