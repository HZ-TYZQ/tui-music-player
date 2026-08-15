[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $SetupPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-UserPath {
    $value = Get-ItemPropertyValue -Path "HKCU:\Environment" -Name "Path" -ErrorAction SilentlyContinue
    if ($null -eq $value) { return "" }
    return [string] $value
}

function Set-UserPath([string] $Value) {
    New-Item -Path "HKCU:\Environment" -Force | Out-Null
    Set-ItemProperty -Path "HKCU:\Environment" -Name "Path" -Value $Value
}

function Normalize-PathEntry([string] $Value) {
    return $Value.Trim().Trim('"').TrimEnd('\', '/').ToLowerInvariant()
}

function Test-PathEntry([string] $PathValue, [string] $Expected) {
    $normalized = Normalize-PathEntry $Expected
    return [bool]($PathValue.Split(';') | Where-Object { (Normalize-PathEntry $_) -eq $normalized })
}

function Test-LicenseMaterial([string] $Directory) {
    $required = @(
        (Join-Path $Directory "LICENSE"),
        (Join-Path $Directory "THIRD-PARTY-NOTICES.txt"),
        (Join-Path $Directory "SOURCE-CODE-OFFER.txt"),
        (Join-Path $Directory "third-party-licenses\gstreamer-1.28.6-license.txt"),
        (Join-Path $Directory "third-party-licenses\LGPL-2.1.txt")
    )
    foreach ($file in $required) {
        if (-not (Test-Path -LiteralPath $file)) {
            throw "Installed package is missing required license material: $file"
        }
    }
}

function Invoke-Setup([string] $Installer, [string] $Directory, [switch] $AddToPath) {
    $arguments = @(
        "/CURRENTUSER",
        "/VERYSILENT",
        "/SUPPRESSMSGBOXES",
        "/NORESTART",
        "/DIR=`"$Directory`""
    )
    if ($AddToPath) {
        $arguments += "/TASKS=addtopath"
    }
    $process = Start-Process -FilePath $Installer -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Installer exited with code $($process.ExitCode)"
    }
}

function Invoke-Uninstall([string] $Directory) {
    $uninstaller = Get-ChildItem -LiteralPath $Directory -Filter "unins*.exe" -File |
        Select-Object -First 1
    if (-not $uninstaller) {
        throw "Uninstaller was not found in $Directory"
    }
    $process = Start-Process -FilePath $uninstaller.FullName -ArgumentList @(
        "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"
    ) -Wait -PassThru
    if ($process.ExitCode -ne 0) {
        throw "Uninstaller exited with code $($process.ExitCode)"
    }
}

$setup = [System.IO.Path]::GetFullPath($SetupPath)
$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$installDir = Join-Path $temporaryRoot "music-player-installer-test"
$pathBefore = Get-UserPath

Invoke-Setup -Installer $setup -Directory $installDir
if ((Get-UserPath) -ne $pathBefore) {
    throw "The default installation unexpectedly changed the user PATH"
}
Test-LicenseMaterial -Directory $installDir
& (Join-Path $installDir "music-player.cmd") --version
if ($LASTEXITCODE -ne 0) {
    throw "Default installation launcher failed"
}
Invoke-Uninstall -Directory $installDir
if ((Get-UserPath) -ne $pathBefore) {
    throw "Default uninstall unexpectedly changed the user PATH"
}

$pathWithExistingEntry = if ($pathBefore.TrimEnd(';')) {
    "$($pathBefore.TrimEnd(';'));$installDir"
} else {
    $installDir
}
Set-UserPath $pathWithExistingEntry
Invoke-Setup -Installer $setup -Directory $installDir -AddToPath
Invoke-Uninstall -Directory $installDir
if (-not (Test-PathEntry (Get-UserPath) $installDir)) {
    throw "Uninstall removed a PATH entry that was not added by the installer"
}
Set-UserPath $pathBefore

Invoke-Setup -Installer $setup -Directory $installDir -AddToPath
if (-not (Test-PathEntry (Get-UserPath) $installDir)) {
    throw "The add-to-PATH task did not add the installation directory"
}
Test-LicenseMaterial -Directory $installDir
& (Join-Path $installDir "music-player.cmd") --version
if ($LASTEXITCODE -ne 0) {
    throw "PATH-enabled installation launcher failed"
}
Invoke-Uninstall -Directory $installDir
if (Test-PathEntry (Get-UserPath) $installDir) {
    throw "Uninstall did not remove the application PATH entry"
}

Write-Host "Installer default and optional PATH behavior passed"
