Set-StrictMode -Version 2.0

function Get-AgentHarnessRepoRoot {
    param([string]$StartPath = (Get-Location).Path)

    $dir = Resolve-Path -LiteralPath $StartPath
    while ($dir) {
        if (Test-Path -LiteralPath (Join-Path $dir.Path ".git")) {
            return $dir.Path
        }
        $parent = Split-Path -Parent $dir.Path
        if (-not $parent -or $parent -eq $dir.Path) {
            break
        }
        $dir = Resolve-Path -LiteralPath $parent
    }
    throw "Could not find repository root from $StartPath"
}

function Read-AgentHarnessJsonFromStdin {
    $inputText = [Console]::In.ReadToEnd()
    if ([string]::IsNullOrWhiteSpace($inputText)) {
        return [pscustomobject]@{}
    }
    try {
        return $inputText | ConvertFrom-Json
    } catch {
        throw "Hook input was not valid JSON: $($_.Exception.Message)"
    }
}

function Get-AgentHarnessProperty {
    param(
        [object]$Object,
        [string[]]$Path
    )

    $current = $Object
    foreach ($part in $Path) {
        if ($null -eq $current) {
            return $null
        }
        $prop = $current.PSObject.Properties[$part]
        if ($null -eq $prop) {
            return $null
        }
        $current = $prop.Value
    }
    return $current
}

function Get-AgentHarnessCommandText {
    param([object]$InputObject)

    $candidates = @(
        @("tool_input", "command"),
        @("tool_input", "cmd"),
        @("command")
    )
    foreach ($path in $candidates) {
        $value = Get-AgentHarnessProperty -Object $InputObject -Path $path
        if ($value) {
            return [string]$value
        }
    }
    return ""
}

function Get-AgentHarnessPromptText {
    param([object]$InputObject)

    $candidates = @(
        @("prompt"),
        @("tool_input", "prompt"),
        @("message", "content")
    )
    foreach ($path in $candidates) {
        $value = Get-AgentHarnessProperty -Object $InputObject -Path $path
        if ($value) {
            return [string]$value
        }
    }
    return ""
}

function Get-AgentHarnessFilePath {
    param([object]$InputObject)

    $candidates = @(
        @("tool_input", "file_path"),
        @("tool_input", "path"),
        @("tool_response", "filePath"),
        @("file_path"),
        @("path")
    )
    foreach ($path in $candidates) {
        $value = Get-AgentHarnessProperty -Object $InputObject -Path $path
        if ($value) {
            return [string]$value
        }
    }
    return ""
}

function Get-AgentHarnessCrateFromPath {
    param([string]$PathText)

    if ([string]::IsNullOrWhiteSpace($PathText)) {
        return ""
    }
    $normalized = $PathText -replace "\\", "/"
    if ($normalized -match "(^|/)crates/([^/]+)/") {
        return $Matches[2]
    }
    return ""
}

function Get-AgentHarnessChangedRustFiles {
    param([string]$Repo)

    if ([string]::IsNullOrWhiteSpace($Repo)) {
        throw "Repository path is required"
    }

    $changed = git -C $Repo diff --name-only HEAD 2>$null
    if ($LASTEXITCODE -ne 0) {
        throw "git diff --name-only HEAD failed with exit code $LASTEXITCODE"
    }

    $files = @(
        $changed |
            Where-Object { $_ -match "\.rs$" } |
            ForEach-Object { ($_ -replace "\\", "/").Trim() } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            Sort-Object -Unique
    )
    return $files
}

function Get-AgentHarnessRustDiffFingerprint {
    param(
        [string]$Repo,
        [string[]]$Files
    )

    if ([string]::IsNullOrWhiteSpace($Repo)) {
        throw "Repository path is required"
    }

    $normalizedFiles = @(
        $Files |
            ForEach-Object { if ($null -ne $_) { ($_ -replace "\\", "/").Trim() } } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            Sort-Object -Unique
    )

    $parts = New-Object System.Collections.Generic.List[string]
    foreach ($file in $normalizedFiles) {
        $parts.Add("FILE:$file")
        $diff = git -C $Repo diff -- $file
        if ($LASTEXITCODE -ne 0) {
            throw "git diff for $file failed with exit code $LASTEXITCODE"
        }
        $parts.Add(($diff -join "`n"))
    }

    $fingerprintText = $parts -join "`n---AGENT-HARNESS-DIFF---`n"
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($fingerprintText)
    $sha = New-Object System.Security.Cryptography.SHA256Managed
    try {
        $hash = $sha.ComputeHash($bytes)
    } finally {
        $sha.Dispose()
    }
    return (($hash | ForEach-Object { $_.ToString("x2") }) -join "")
}

function Test-AgentHarnessDangerousCommand {
    param([string]$Command)

    if ([string]::IsNullOrWhiteSpace($Command)) {
        return [pscustomobject]@{ Block = $false; Reason = "" }
    }

    $patterns = @(
        @{ Regex = "(?i)\brm\s+-(?:[a-z]*r[a-z]*f|[a-z]*f[a-z]*r)[a-z]*\s+(?:/|~(?:/|$)|\.($|\s)|\*(?:\s|$)|\.git(?:\s|$)|\.claude(?:\s|$)|\.codex(?:\s|$))"; Reason = "destructive command targets root, wildcard, repo metadata, or home" },
        @{ Regex = '(?i)\brm\s+-(?:[a-z]*r[a-z]*f|[a-z]*f[a-z]*r)[a-z]*\s+\$HOME(?:/|\s|$)'; Reason = "destructive command targets home" },
        @{ Regex = "(?i)\bgit\s+reset\s+--hard\b"; Reason = "destructive git reset" },
        @{ Regex = "(?i)\bgit\s+clean\s+-[a-z]*f[a-z]*d[a-z]*x[a-z]*\b"; Reason = "destructive git clean" },
        @{ Regex = "(?i)\bgit\s+checkout\s+--\b"; Reason = "destructive checkout of working tree files" },
        @{ Regex = "(?i)\bRemove-Item\b(?=.*\s-Recurse\b).*(?:\.git|\.claude|\.codex|\*|~|`"\s*/|'\s*/)"; Reason = "recursive removal of protected path" },
        @{ Regex = "(?i)\bcargo\s+build\b(?=.*\s--workspace\b)"; Reason = "broad workspace build is not the default debug loop" },
        @{ Regex = "(?i)\bcargo\s+build\b(?=.*\s--release\b)"; Reason = "release build is too broad for normal agent iteration" },
        @{ Regex = "(?i)(?:^|[;&|]\s*)(?:Set-Content|Add-Content|Out-File)\b.*(?:^|\s)\.env(?:\s|$)"; Reason = "write to env file may expose secrets" },
        @{ Regex = "(?i)(?:>|>>)\s*\.env(?:\s|$|[.;])"; Reason = "write to env file may expose secrets" }
    )

    foreach ($p in $patterns) {
        if ($Command -match $p.Regex) {
            return [pscustomobject]@{ Block = $true; Reason = $p.Reason }
        }
    }
    return [pscustomobject]@{ Block = $false; Reason = "" }
}

function Test-AgentHarnessSecretText {
    param([string]$Text)

    if ([string]::IsNullOrWhiteSpace($Text)) {
        return [pscustomobject]@{ Block = $false; Reason = "" }
    }

    $patterns = @(
        @{ Regex = "-----BEGIN (RSA |EC |OPENSSH |)PRIVATE KEY-----"; Reason = "private key block" },
        @{ Regex = "(?i)\bBearer\s+[A-Za-z0-9._~+/=-]{30,}\b"; Reason = "bearer token" },
        @{ Regex = "(?i)\b(api[_-]?key|password|secret|token)\s*=\s*['""]?[A-Za-z0-9._~+/=-]{20,}"; Reason = "secret-like assignment" },
        @{ Regex = "\bsk-[A-Za-z0-9_-]{40,}\b"; Reason = "secret-like API key" },
        @{ Regex = "\bsk-ant-[A-Za-z0-9_-]{40,}\b"; Reason = "secret-like API key" }
    )

    foreach ($p in $patterns) {
        if ($Text -match $p.Regex) {
            return [pscustomobject]@{ Block = $true; Reason = $p.Reason }
        }
    }
    return [pscustomobject]@{ Block = $false; Reason = "" }
}

function Write-AgentHarnessBlockJson {
    param([string]$Reason)

    $payload = [ordered]@{
        hookSpecificOutput = [ordered]@{
            permissionDecision = "deny"
            permissionDecisionReason = $Reason
        }
    }
    $payload | ConvertTo-Json -Depth 6
}

Export-ModuleMember -Function `
    Get-AgentHarnessRepoRoot, `
    Read-AgentHarnessJsonFromStdin, `
    Get-AgentHarnessProperty, `
    Get-AgentHarnessCommandText, `
    Get-AgentHarnessPromptText, `
    Get-AgentHarnessFilePath, `
    Get-AgentHarnessCrateFromPath, `
    Get-AgentHarnessChangedRustFiles, `
    Get-AgentHarnessRustDiffFingerprint, `
    Test-AgentHarnessDangerousCommand, `
    Test-AgentHarnessSecretText, `
    Write-AgentHarnessBlockJson
