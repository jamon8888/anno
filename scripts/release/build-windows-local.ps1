param(
    [string]$TargetDir = "D:\cargo-windows-md-target",
    [ValidateSet("release", "dist", "dev", "dev-fast")]
    [string]$Profile = "release",
    [switch]$CleanNative,
    [switch]$SkipGateway,
    [switch]$SkipRag,
    [switch]$SeparatePackages,
    [switch]$LowMemory,
    [switch]$NoSccache
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

if ($LowMemory) {
    $env:CARGO_BUILD_JOBS = "1"
    Remove-Item Env:\CARGO_INCREMENTAL -ErrorAction SilentlyContinue
    if ($NoSccache) {
        Remove-Item Env:\RUSTC_WRAPPER -ErrorAction SilentlyContinue
    } else {
        $sccache = Get-Command sccache -ErrorAction SilentlyContinue
        if ($sccache) {
            $env:RUSTC_WRAPPER = $sccache.Source
        }
    }
}

if ($CleanNative) {
    cargo clean -p esaxx-rs --target x86_64-pc-windows-msvc
    cargo clean -p ort-sys --target x86_64-pc-windows-msvc
}

function New-BaseCargoArgs {
    $base = @("build")
    if ($Profile -eq "release") {
        $base += "--release"
    } else {
        $base += @("--profile", $Profile)
    }
    return $base
}

$packageArgs = @()

if (-not $SkipRag) {
    $packageArgs += ,@("-p", "anno-rag-bin", "--bin", "anno-rag")
}

if (-not $SkipGateway) {
    $packageArgs += ,@("-p", "anno-privacy-gateway")
}

if ($packageArgs.Count -eq 0) {
    Write-Error "Nothing to build: both -SkipRag and -SkipGateway were supplied."
    exit 1
}

Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "RUSTFLAGS=$env:RUSTFLAGS"
Write-Host "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS=$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"
Write-Host "CFLAGS_x86_64_pc_windows_msvc=$env:CFLAGS_x86_64_pc_windows_msvc"
Write-Host "CXXFLAGS_x86_64_pc_windows_msvc=$env:CXXFLAGS_x86_64_pc_windows_msvc"
Write-Host "PROFILE=$Profile"
if ($env:CARGO_BUILD_JOBS) {
    Write-Host "CARGO_BUILD_JOBS=$env:CARGO_BUILD_JOBS"
}
if ($env:RUSTC_WRAPPER) {
    Write-Host "RUSTC_WRAPPER=$env:RUSTC_WRAPPER"
}

if ($SeparatePackages) {
    foreach ($pkg in $packageArgs) {
        $args = (New-BaseCargoArgs) + $pkg + @("--target", "x86_64-pc-windows-msvc")
        Write-Host ("cargo " + ($args -join " "))
        & cargo @args
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }
    exit 0
}

$args = (New-BaseCargoArgs)
foreach ($pkg in $packageArgs) {
    $args += $pkg
}
$args += @("--target", "x86_64-pc-windows-msvc")

Write-Host ("cargo " + ($args -join " "))
& cargo @args
exit $LASTEXITCODE
