[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)] [string] $ProjectDir,
    [Parameter(Mandatory = $true)] [string] $RuntimeDir,
    [Parameter(Mandatory = $true)] [string] $MusicPlayerExe,
    [Parameter(Mandatory = $true)] [string] $Version,
    [Parameter(Mandatory = $true)] [string] $OutputDir
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$project = [System.IO.Path]::GetFullPath($ProjectDir)
$runtime = [System.IO.Path]::GetFullPath($RuntimeDir)
$executable = [System.IO.Path]::GetFullPath($MusicPlayerExe)
$output = [System.IO.Path]::GetFullPath($OutputDir)
$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$packageName = "music-player-$Version-windows-x86_64"
$stageRoot = Join-Path $temporaryRoot $packageName

foreach ($required in @(
    $runtime,
    $executable,
    (Join-Path $runtime "bin\gst-inspect-1.0.exe"),
    (Join-Path $runtime "lib\gstreamer-1.0"),
    (Join-Path $project "assets\icons\music-player.ico")
)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "Required packaging input was not found: $required"
    }
}

$runtimeLicense = Get-ChildItem -LiteralPath $runtime -Recurse -File |
    Where-Object { $_.Name -match '^(?i:copying|license|notice)' } |
    Select-Object -First 1
if (-not $runtimeLicense) {
    throw "The GStreamer runtime does not contain discoverable license or notice files"
}

if (Test-Path -LiteralPath $stageRoot) {
    Remove-Item -LiteralPath $stageRoot -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $stageRoot | Out-Null
Copy-Item -Path (Join-Path $runtime "*") -Destination $stageRoot -Recurse -Force

$stageBin = Join-Path $stageRoot "bin"
Copy-Item -LiteralPath $executable -Destination (Join-Path $stageBin "music-player.exe") -Force
Copy-Item -LiteralPath (Join-Path $project "packaging\windows\music-player.cmd") -Destination $stageRoot
Copy-Item -LiteralPath (Join-Path $project "packaging\windows\README-Windows.txt") -Destination $stageRoot
Copy-Item -LiteralPath (Join-Path $project "packaging\windows\THIRD-PARTY-NOTICES.txt") -Destination $stageRoot
Copy-Item -LiteralPath (Join-Path $project "assets\icons\music-player.ico") -Destination $stageRoot
Copy-Item -LiteralPath (Join-Path $project "LICENSE") -Destination $stageRoot

$oldPath = $env:PATH
$oldPluginSystemPath = $env:GST_PLUGIN_SYSTEM_PATH_1_0
$oldPluginPath = $env:GST_PLUGIN_PATH_1_0
try {
    $env:PATH = "$stageBin;$oldPath"
    $env:GST_PLUGIN_SYSTEM_PATH_1_0 = Join-Path $stageRoot "lib\gstreamer-1.0"
    $env:GST_PLUGIN_PATH_1_0 = ""
    $inspect = Join-Path $stageBin "gst-inspect-1.0.exe"
    foreach ($feature in @("playbin3", "decodebin3", "spectrum", "fakesink", "autoaudiosink")) {
        & $inspect $feature *> $null
        if ($LASTEXITCODE -ne 0) {
            throw "Packaged GStreamer runtime is missing feature: $feature"
        }
    }
    & (Join-Path $stageRoot "music-player.cmd") --version
    if ($LASTEXITCODE -ne 0) {
        throw "Packaged music-player launcher failed with exit code $LASTEXITCODE"
    }
}
finally {
    $env:PATH = $oldPath
    $env:GST_PLUGIN_SYSTEM_PATH_1_0 = $oldPluginSystemPath
    $env:GST_PLUGIN_PATH_1_0 = $oldPluginPath
}

New-Item -ItemType Directory -Force -Path $output | Out-Null
$portableZip = Join-Path $output "$packageName-portable.zip"
if (Test-Path -LiteralPath $portableZip) {
    Remove-Item -LiteralPath $portableZip -Force
}
Compress-Archive -LiteralPath $stageRoot -DestinationPath $portableZip -CompressionLevel Optimal

$isccCommand = Get-Command "ISCC.exe" -ErrorAction SilentlyContinue
$iscc = if ($isccCommand) { $isccCommand.Source } else { $null }
if (-not $iscc) {
    $fallback = "C:\Program Files (x86)\Inno Setup 6\ISCC.exe"
    if (Test-Path -LiteralPath $fallback) {
        $iscc = $fallback
    } else {
        throw "ISCC.exe was not found"
    }
}

$installerScript = Join-Path $project "packaging\windows\music-player.iss"
& $iscc "/DAppVersion=$Version" "/DStageDir=$stageRoot" "/DOutputDir=$output" $installerScript
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup compiler exited with code $LASTEXITCODE"
}

$setup = Join-Path $output "$packageName-setup.exe"
if (-not (Test-Path -LiteralPath $setup)) {
    throw "Expected installer was not created: $setup"
}

Write-Host "Windows artifacts:"
Write-Host $portableZip
Write-Host $setup
