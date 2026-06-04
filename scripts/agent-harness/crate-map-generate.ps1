param(
    [string]$MetadataPath = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Get-CargoMetadata {
    param(
        [string]$Path
    )

    if (-not [string]::IsNullOrWhiteSpace($Path)) {
        return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
    }

    return cargo metadata --format-version 1 | ConvertFrom-Json
}

$metadata = Get-CargoMetadata -Path $MetadataPath
$workspaceIds = @{}
foreach ($member in @($metadata.workspace_members)) {
    $workspaceIds[[string]$member] = $true
}

$workspacePackages = @()
$workspaceNames = @{}
foreach ($package in @($metadata.packages)) {
    if ($workspaceIds.ContainsKey([string]$package.id)) {
        $workspacePackages += $package
        $workspaceNames[[string]$package.name] = $true
    }
}

$directLines = New-Object System.Collections.Generic.List[string]
$reversePairs = @{}

foreach ($package in $workspacePackages) {
    $name = [string]$package.name
    foreach ($dependency in @($package.dependencies)) {
        $depName = [string]$dependency.name
        if ($workspaceNames.ContainsKey($depName)) {
            $directLines.Add(("{0} depends on {1}" -f $name, $depName))
            if (-not $reversePairs.ContainsKey($depName)) {
                $reversePairs[$depName] = New-Object System.Collections.Generic.List[string]
            }
            $reversePairs[$depName].Add($name)
        }
    }
}

foreach ($line in @($directLines | Sort-Object)) {
    Write-Output $line
}

$reverseLines = New-Object System.Collections.Generic.List[string]
foreach ($depName in @($reversePairs.Keys | Sort-Object)) {
    foreach ($dependent in @($reversePairs[$depName] | Sort-Object)) {
        $reverseLines.Add(("{0} is depended on by {1}" -f $depName, $dependent))
    }
}

foreach ($line in @($reverseLines | Sort-Object)) {
    Write-Output $line
}

if ($DryRun) {
    Write-Output "dry-run: no files written"
}
