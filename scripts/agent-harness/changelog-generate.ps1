param(
    [string]$Since = "main",
    [string]$CommitsPath = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$typeToSection = @{
    "feat" = "Features"
    "fix" = "Bug Fixes"
    "perf" = "Performance"
    "refactor" = "Refactors"
    "docs" = "Documentation"
    "test" = "Tests"
    "tests" = "Tests"
    "ci" = "CI and Chores"
    "chore" = "CI and Chores"
    "build" = "CI and Chores"
}

$sectionOrder = @(
    "Features",
    "Bug Fixes",
    "Performance",
    "Refactors",
    "Documentation",
    "Tests",
    "CI and Chores"
)

$sectionItems = [ordered]@{}
foreach ($section in $sectionOrder) {
    $sectionItems[$section] = New-Object System.Collections.Generic.List[string]
}

if (-not [string]::IsNullOrWhiteSpace($CommitsPath)) {
    $subjects = @(Get-Content -LiteralPath $CommitsPath)
} else {
    $subjects = @(git log "--format=%s" "$Since..HEAD")
}

foreach ($subject in $subjects) {
    if ([string]::IsNullOrWhiteSpace($subject)) {
        continue
    }

    $trimmedSubject = ([string]$subject).Trim()
    if ($trimmedSubject -match "^\s*([A-Za-z]+)(\([^)]+\))?(!)?:\s*(.+)$") {
        $commitType = $matches[1].ToLowerInvariant()
        if ($typeToSection.ContainsKey($commitType)) {
            $sectionName = $typeToSection[$commitType]
            $sectionItems[$sectionName].Add($matches[4].Trim())
        }
    }
}

Write-Output "## Unreleased"

foreach ($section in $sectionOrder) {
    if ($sectionItems[$section].Count -eq 0) {
        continue
    }

    Write-Output ""
    Write-Output ("### {0}" -f $section)
    foreach ($item in $sectionItems[$section]) {
        Write-Output ("- {0}" -f $item)
    }
}

if ($DryRun) {
    Write-Output ""
    Write-Output "dry-run: no files written"
}
