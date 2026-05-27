param(
    [ValidateSet("check", "build", "test")]
    [string]$Mode = "check",

    [string]$Since = "HEAD",

    [string[]]$Package = @(),

    [string]$Profile = "dev",

    [string[]]$Features = @(),

    [switch]$AllAffected,

    [switch]$NoSccache,

    [switch]$PrintOnly
)

$ErrorActionPreference = "Stop"

function Get-PackageName {
    param([string]$CrateDir)

    $manifest = Join-Path -Path $CrateDir -ChildPath "Cargo.toml"
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
        return $null
    }

    $inPackage = $false
    foreach ($line in Get-Content -LiteralPath $manifest) {
        if ($line -match '^\s*\[package\]\s*$') {
            $inPackage = $true
            continue
        }
        if ($line -match '^\s*\[') {
            $inPackage = $false
        }
        if ($inPackage -and $line -match '^\s*name\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    }

    return $null
}

function Add-Unique {
    param(
        [System.Collections.Generic.List[string]]$Items,
        [string]$Value
    )

    if ($Value -and -not $Items.Contains($Value)) {
        $Items.Add($Value)
    }
}

$repoRoot = (& git rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $repoRoot

$selected = [System.Collections.Generic.List[string]]::new()

if ($Package.Count -gt 0) {
    foreach ($name in $Package) {
        Add-Unique -Items $selected -Value $name
    }
} else {
    $changedFiles = @(& git diff --name-only $Since --)
    $changedFiles += @(& git diff --name-only --cached --)
    $changedFiles = $changedFiles | Where-Object { $_ } | Sort-Object -Unique

    foreach ($file in $changedFiles) {
        $normalized = $file -replace '\\', '/'

        if ($normalized -match '^crates/([^/]+)/') {
            $crateDir = Join-Path -Path $repoRoot -ChildPath ("crates/" + $Matches[1])
            Add-Unique -Items $selected -Value (Get-PackageName -CrateDir $crateDir)
            continue
        }

        if ($normalized -match '^vendor/cloakpipe/') {
            Add-Unique -Items $selected -Value "anno-rag"
            Add-Unique -Items $selected -Value "anno-privacy-gateway"
            continue
        }

        if ($normalized -in @("Cargo.toml", "Cargo.lock", "rust-toolchain.toml") -or $normalized.StartsWith(".cargo/")) {
            Add-Unique -Items $selected -Value "anno-rag-bin"
            Add-Unique -Items $selected -Value "anno-privacy-gateway"
            continue
        }
    }
}

if ($AllAffected) {
    $expanded = [System.Collections.Generic.List[string]]::new()
    foreach ($name in $selected) {
        Add-Unique -Items $expanded -Value $name

        switch ($name) {
            "anno" {
                foreach ($dependent in @("anno-cli", "anno-eval", "anno-rag", "anno-rag-bin", "anno-rag-mcp", "anno-rag-tabular")) {
                    Add-Unique -Items $expanded -Value $dependent
                }
            }
            "anno-rag" {
                foreach ($dependent in @("anno-rag-mcp", "anno-rag-bin")) {
                    Add-Unique -Items $expanded -Value $dependent
                }
            }
            "anno-rag-mcp" {
                Add-Unique -Items $expanded -Value "anno-rag-bin"
            }
        }
    }
    $selected = $expanded
}

if ($selected.Count -eq 0) {
    Write-Host "No changed Rust crates detected since '$Since'. Pass -Package <name> to run a targeted command."
    exit 0
}

if (-not $NoSccache) {
    $sccache = Get-Command sccache -ErrorAction SilentlyContinue
    if ($sccache) {
        $env:RUSTC_WRAPPER = $sccache.Source
    }
}

$cargoArgs = @($Mode)
if ($Profile) {
    $cargoArgs += @("--profile", $Profile)
}
foreach ($name in $selected) {
    $cargoArgs += @("-p", $name)
}
if ($Features.Count -gt 0) {
    $cargoArgs += @("--features", ($Features -join ","))
}

Write-Host ("Packages: " + ($selected -join ", "))
if ($env:RUSTC_WRAPPER) {
    Write-Host "RUSTC_WRAPPER=$env:RUSTC_WRAPPER"
}
Write-Host ("cargo " + ($cargoArgs -join " "))

if ($PrintOnly) {
    exit 0
}

& cargo @cargoArgs
exit $LASTEXITCODE
