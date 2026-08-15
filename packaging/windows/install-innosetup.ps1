[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $InstallDir,
    [switch] $ExportGitHubEnvironment
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Pinned to the official immutable GitHub release (files.jrsoftware.org
# redirects there since March 2026). Must stay in sync with the vendored
# packaging/windows/ChineseSimplified.isl, which comes from the issrc
# is-6_7_1 tree.
$innoSetupVersion = "6.7.1"
$installerName = "innosetup-$innoSetupVersion.exe"
$installerUrl = "https://github.com/jrsoftware/issrc/releases/download/is-6_7_1/$installerName"
$expectedSha256 = "4d11e8050b6185e0d49bd9e8cc661a7a59f44959a621d31d11033124c4e8a7b0"
$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$installerPath = Join-Path $temporaryRoot $installerName

if (-not (Test-Path -LiteralPath $installerPath)) {
    Write-Host "Downloading Inno Setup $innoSetupVersion..."
    Invoke-WebRequest -Uri $installerUrl -OutFile $installerPath
}

$actualSha256 = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualSha256 -ne $expectedSha256) {
    throw "Inno Setup installer SHA-256 mismatch: expected $expectedSha256, got $actualSha256"
}

# The GitHub-hosted runner image may already carry a chocolatey Inno Setup
# of an uncontrolled vintage. Installing this exact release into InstallDir
# and prepending it to PATH shadows that copy for all packaging steps.
$resolvedInstallDir = [System.IO.Path]::GetFullPath($InstallDir)
New-Item -ItemType Directory -Force -Path $resolvedInstallDir | Out-Null
$arguments = @(
    "/VERYSILENT"
    "/SUPPRESSMSGBOXES"
    "/NORESTART"
    "/DIR=`"$resolvedInstallDir`""
)
$process = Start-Process -FilePath $installerPath -ArgumentList $arguments -Wait -PassThru
if ($process.ExitCode -ne 0) {
    throw "Inno Setup installer exited with code $($process.ExitCode)"
}

$iscc = Join-Path $resolvedInstallDir "ISCC.exe"
if (-not (Test-Path -LiteralPath $iscc)) {
    throw "Inno Setup installation is incomplete: $iscc was not found"
}

if ($ExportGitHubEnvironment) {
    if (-not $env:GITHUB_PATH) {
        throw "-ExportGitHubEnvironment requires GITHUB_PATH"
    }
    Add-Content -LiteralPath $env:GITHUB_PATH -Value $resolvedInstallDir
}

Write-Host "Inno Setup $innoSetupVersion ready at $resolvedInstallDir"
