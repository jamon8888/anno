param(
    [string]$TargetDir = "D:\cargo-windows-md-target",
    [switch]$CleanNative,
    [switch]$SkipGateway
)

$ErrorActionPreference = "Stop"

$repoRoot = (& git rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $repoRoot

$env:CARGO_TARGET_DIR = $TargetDir
$env:RUSTFLAGS = "-C target-feature=-crt-static"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = "-C target-feature=-crt-static"
$env:CFLAGS_x86_64_pc_windows_msvc = "/MD"
$env:CXXFLAGS_x86_64_pc_windows_msvc = "/MD"
Set-Item -Path "Env:CFLAGS_x86_64-pc-windows-msvc" -Value "/MD"
Set-Item -Path "Env:CXXFLAGS_x86_64-pc-windows-msvc" -Value "/MD"

if ($CleanNative) {
    cargo clean -p esaxx-rs --target x86_64-pc-windows-msvc
    cargo clean -p ort-sys --target x86_64-pc-windows-msvc
}

$args = @(
    "build",
    "--release",
    "-p", "anno-rag-bin",
    "--bin", "anno-rag"
)

if (-not $SkipGateway) {
    $args += @("-p", "anno-privacy-gateway")
}

$args += @("--target", "x86_64-pc-windows-msvc")

Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "RUSTFLAGS=$env:RUSTFLAGS"
Write-Host "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS=$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"
Write-Host "CFLAGS_x86_64_pc_windows_msvc=$env:CFLAGS_x86_64_pc_windows_msvc"
Write-Host "CXXFLAGS_x86_64_pc_windows_msvc=$env:CXXFLAGS_x86_64_pc_windows_msvc"
Write-Host ("cargo " + ($args -join " "))

& cargo @args
exit $LASTEXITCODE
