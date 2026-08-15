[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $InstallDir,
    [ValidateSet("runtime", "devel")]
    [string] $InstallType = "devel",
    [switch] $ExportGitHubEnvironment
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$gstreamerVersion = "1.28.6"
$installerName = "gstreamer-1.0-msvc-x86_64-$gstreamerVersion.exe"
$installerUrl = "https://gstreamer.freedesktop.org/data/pkg/windows/$gstreamerVersion/msvc/$installerName"
$expectedSha256 = "059251444d1267b486eba390b18d25fed87e10315e72f757ec6c7e912fa746b5"
$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$installerPath = Join-Path $temporaryRoot $installerName

if (-not (Test-Path -LiteralPath $installerPath)) {
    Write-Host "Downloading GStreamer $gstreamerVersion MSVC x86_64..."
    Invoke-WebRequest -Uri $installerUrl -OutFile $installerPath
}

$actualSha256 = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualSha256 -ne $expectedSha256) {
    throw "GStreamer installer SHA-256 mismatch: expected $expectedSha256, got $actualSha256"
}

$resolvedInstallDir = [System.IO.Path]::GetFullPath($InstallDir)
New-Item -ItemType Directory -Force -Path $resolvedInstallDir | Out-Null
$arguments = @(
    "/TYPE=$InstallType"
    "/CURRENTUSER"
    "/VERYSILENT"
    "/SUPPRESSMSGBOXES"
    "/NORESTART"
    "/DIR=`"$resolvedInstallDir`""
)
$process = Start-Process -FilePath $installerPath -ArgumentList $arguments -Wait -PassThru
if ($process.ExitCode -ne 0) {
    throw "GStreamer installer exited with code $($process.ExitCode)"
}

$binDir = Join-Path $resolvedInstallDir "bin"
$inspect = Join-Path $binDir "gst-inspect-1.0.exe"
if (-not (Test-Path -LiteralPath $inspect)) {
    throw "GStreamer installation is incomplete: $inspect was not found"
}

if ($InstallType -eq "devel") {
    $pkgConfig = Join-Path $binDir "pkg-config.exe"
    if (-not (Test-Path -LiteralPath $pkgConfig)) {
        throw "GStreamer development installation is incomplete: $pkgConfig was not found"
    }
}

if ($ExportGitHubEnvironment) {
    if (-not $env:GITHUB_PATH -or -not $env:GITHUB_ENV) {
        throw "-ExportGitHubEnvironment requires GITHUB_PATH and GITHUB_ENV"
    }
    Add-Content -LiteralPath $env:GITHUB_PATH -Value $binDir
    Add-Content -LiteralPath $env:GITHUB_ENV -Value "GSTREAMER_1_0_ROOT_MSVC_X86_64=$resolvedInstallDir"
    Add-Content -LiteralPath $env:GITHUB_ENV -Value "PKG_CONFIG=$(Join-Path $binDir 'pkg-config.exe')"
    Add-Content -LiteralPath $env:GITHUB_ENV -Value "PKG_CONFIG_PATH=$(Join-Path $resolvedInstallDir 'lib\pkgconfig')"
}

Write-Host "GStreamer $InstallType installation ready at $resolvedInstallDir"
