# Test install.ps1, run the documented way (irm | iex), against a fake release served from <dist> on
# localhost: a real install that edits the USER PATH (run this only somewhere disposable, like a CI runner),
# then a tampered SHA256SUMS that must install nothing.
#
#   pwsh install/test/installers.ps1 -Dist <dist>
param([Parameter(Mandatory)] [string] $Dist)
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $PSScriptRoot
$port = 8791
$url = "http://127.0.0.1:$port"

$work = Join-Path ([IO.Path]::GetTempPath()) ("installers-" + [Guid]::NewGuid())
$srv = Join-Path $work 'srv'
New-Item -ItemType Directory -Force -Path (Join-Path $srv 'bad') | Out-Null
Copy-Item (Join-Path $Dist '*') $srv
Copy-Item (Join-Path $here 'install.ps1') $srv
$bad = foreach ($f in Get-ChildItem (Join-Path $Dist 'godspeed-*')) {
    Copy-Item $f.FullName (Join-Path $srv 'bad')
    ('0' * 64) + '  ' + $f.Name
}
Set-Content -Path (Join-Path $srv 'bad\SHA256SUMS') -Value $bad
Copy-Item (Join-Path $here 'install.ps1') (Join-Path $srv 'bad')

$server = Start-Process -PassThru -NoNewWindow python -ArgumentList '-m', 'http.server', $port, '--bind', '127.0.0.1', '--directory', $srv
try {
    for ($i = 0; $i -lt 60; $i++) {
        try { Invoke-WebRequest -UseBasicParsing "$url/SHA256SUMS" | Out-Null; break } catch { Start-Sleep -Milliseconds 500 }
    }

    Write-Host '--- install'
    $env:GODSPEED_DOWNLOAD_URL = $url
    irm "$url/install.ps1" | iex
    $dir = Join-Path $env:USERPROFILE '.godspeed\bin'
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($userPath -split ';') -notcontains $dir) { throw "the user PATH does not contain $dir" }
    $version = & godspeed --version # found through this session's PATH, which the installer also updated
    Write-Host "--- found on the PATH the installer set: $version"

    Write-Host '--- reinstall'
    irm "$url/install.ps1" | iex
    $count = @(([Environment]::GetEnvironmentVariable('Path', 'User')) -split ';' | Where-Object { $_ -eq $dir }).Count
    if ($count -ne 1) { throw "the user PATH lists $dir $count times" }
    Write-Host '--- the user PATH was edited once'

    Write-Host '--- tampered SHA256SUMS'
    $env:GODSPEED_DOWNLOAD_URL = "$url/bad"
    $env:GODSPEED_INSTALL_DIR = Join-Path $work 'tampered'
    irm "$url/bad/install.ps1" | iex
    if (Test-Path (Join-Path $env:GODSPEED_INSTALL_DIR 'godspeed.exe')) { throw 'a binary was installed despite the mismatch' }
    Write-Host '--- refused, and nothing was installed'
    Write-Host 'install.ps1: all checks passed'
} finally {
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
    Remove-Item Env:GODSPEED_DOWNLOAD_URL, Env:GODSPEED_INSTALL_DIR -ErrorAction SilentlyContinue
}
