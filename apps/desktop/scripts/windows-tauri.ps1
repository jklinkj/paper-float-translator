param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("dev", "build")]
    [string]$Action
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$desktopRoot = Join-Path $projectRoot "apps\desktop"
$tauriCommand = Join-Path $projectRoot "node_modules\.bin\tauri.cmd"

. (Join-Path $PSScriptRoot "windows-dev-env.ps1")

if (-not (Test-Path -LiteralPath $tauriCommand)) {
    throw "Tauri CLI is missing. Run npm ci from the repository root first."
}

Push-Location $desktopRoot
try {
    if ($Action -eq "dev") {
        & $tauriCommand dev
    }
    else {
        & $tauriCommand build --bundles nsis
    }
    $exitCode = $LASTEXITCODE
}
finally {
    Pop-Location
}

exit $exitCode
