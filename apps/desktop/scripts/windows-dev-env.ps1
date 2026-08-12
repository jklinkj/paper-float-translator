$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path

$env:RUSTUP_HOME = Join-Path $projectRoot ".tooling\rustup"
$env:CARGO_HOME = Join-Path $projectRoot ".tooling\cargo"
$env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-msvc"
$env:CARGO_TARGET_DIR = Join-Path $projectRoot "target\cargo-windows"
$env:NPM_CONFIG_CACHE = Join-Path $projectRoot ".cache\npm"
$env:PNPM_HOME = Join-Path $projectRoot ".tooling\pnpm"
$env:PNPM_STORE_DIR = Join-Path $projectRoot ".cache\pnpm-store"
$env:TEMP = Join-Path $projectRoot ".cache\tmp"
$env:TMP = $env:TEMP

New-Item -ItemType Directory -Force -Path $env:TEMP | Out-Null

$vcvarsPath = Join-Path $projectRoot ".tooling\vs-buildtools\VC\Auxiliary\Build\vcvars64.bat"
if (Test-Path -LiteralPath $vcvarsPath) {
    $vcEnvironment = & $env:ComSpec /d /c "call `"$vcvarsPath`" >nul && set"
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to initialize the Visual Studio x64 build environment."
    }

    foreach ($entry in $vcEnvironment) {
        $separator = $entry.IndexOf("=")
        if ($separator -le 0) {
            continue
        }

        $name = $entry.Substring(0, $separator)
        if ($name -ieq "PATH" -and $name -cne "PATH") {
            continue
        }

        $value = $entry.Substring($separator + 1)
        [Environment]::SetEnvironmentVariable($name, $value, "Process")
    }
}

$toolPaths = @(
    (Join-Path $env:CARGO_HOME "bin"),
    (Join-Path $projectRoot ".tooling\node"),
    (Join-Path $env:ProgramFiles "Git\bin")
)

foreach ($toolPath in $toolPaths) {
    if (Test-Path -LiteralPath $toolPath) {
        $env:PATH = "$toolPath;$env:PATH"
    }
}

Write-Host "Paper Float Windows development environment enabled."
Write-Host "RUSTUP_HOME=$env:RUSTUP_HOME"
Write-Host "CARGO_HOME=$env:CARGO_HOME"
Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "NPM_CONFIG_CACHE=$env:NPM_CONFIG_CACHE"
Write-Host "TEMP=$env:TEMP"
if ($env:VSCMD_VER) {
    Write-Host "MSVC environment=$env:VSCMD_VER"
}
