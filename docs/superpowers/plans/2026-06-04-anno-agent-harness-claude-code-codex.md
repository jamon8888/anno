# Anno Agent Harness Claude Code Codex Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a repo-local developer harness that gives Claude Code and Codex deterministic safety gates, focused agents, workflow skills, changelog and PR automation, docs generation, crate dependency insight, and CLI feature parity checks for this Rust workspace.

**Architecture:** Use PowerShell-first scripts under `scripts/agent-harness/` with Bash shims where useful, then wire them into `.claude/`, `.codex/`, and `.agents/skills/`. Keep shared logic in small PowerShell modules, test scripts with JSON fixtures, and make all automation dry-run first so the harness can be audited before it edits committed docs or config.

**Tech Stack:** PowerShell 5.1 compatible scripts, Bash shims, JSON/TOML/Markdown files, Cargo metadata, GitNexus CLI, existing `scripts/dev-fast.ps1`, Claude Code `.claude` project config, Codex `.codex` project config, repo-local skills under `.agents/skills`.

---

## Scope Check

The approved design covers multiple subsystems: hooks, setup, Claude Code agents, Codex agents, skills, changelog generation, PR review, docs generation, crate maps, CLI parity, and agent context generation. This plan implements them as phased, independently testable slices. Each phase can be committed and verified before the next phase begins.

Do not implement all scripts in one large edit. Keep each task small, run fixture tests after each script, and commit frequently.

## File Map

Create these directories:

- `scripts/agent-harness/` - all executable harness scripts.
- `scripts/agent-harness/lib/` - shared PowerShell functions.
- `scripts/agent-harness/tests/` - fixture-driven local tests.
- `scripts/agent-harness/tests/fixtures/` - JSON and text fixtures for hook and automation tests.
- `.claude/agents/` - Claude Code repo-local agents.
- `.claude/rules/` - scoped Claude Code rules.
- `.codex/` - Codex repo config and hooks.
- `.codex/agents/` - Codex custom agent configs.
- `.agents/skills/anno-fast-debug-loop/` and related directories - Codex-readable skills.

Modify these existing files only when the task says to:

- `.claude/settings.json` - merge hook entries after scripts are tested.
- `AGENTS.md` - add short pointers only after skills exist.
- `CLAUDE.md` - add short pointers only after rules and skills exist.
- `docs/README.md` or `docs/developers/configuration.md` - document setup after scripts are tested.
- `.gitignore` - add local harness state and logs if not already ignored.

Do not touch unrelated dirty files already present in the worktree.

## Verification Commands

Use these commands throughout:

```powershell
git status --short
git diff --cached --name-status
npx gitnexus status
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\setup-agent-harness.ps1 -DryRun
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\dev-fast.ps1 -PrintOnly
```

If a task edits Rust, run the targeted command printed by `scripts/dev-fast.ps1`. Most tasks in this plan edit scripts and Markdown only.

---

### Task 1: Add Shared Harness Test Runner and Fixtures

**Files:**
- Create: `scripts/agent-harness/tests/test-agent-harness.ps1`
- Create: `scripts/agent-harness/tests/fixtures/pretool-dangerous.json`
- Create: `scripts/agent-harness/tests/fixtures/pretool-safe.json`
- Create: `scripts/agent-harness/tests/fixtures/prompt-secret.json`
- Create: `scripts/agent-harness/tests/fixtures/prompt-safe-legal.json`
- Create: `scripts/agent-harness/tests/fixtures/post-edit-rust.json`
- Create: `scripts/agent-harness/tests/fixtures/cargo-metadata.fixture.json`
- Create: `scripts/agent-harness/tests/fixtures/commits.fixture.txt`
- Create: `scripts/agent-harness/tests/fixtures/diff-name-status.fixture.txt`

- [ ] **Step 1: Create the test runner**

Create `scripts/agent-harness/tests/test-agent-harness.ps1`:

```powershell
param(
    [string]$Filter = ""
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$HarnessRoot = Split-Path -Parent $ScriptDir
$RepoRoot = Split-Path -Parent (Split-Path -Parent $HarnessRoot)

function Assert-Equal {
    param(
        [object]$Actual,
        [object]$Expected,
        [string]$Name
    )
    if ($Actual -ne $Expected) {
        throw "ASSERT FAIL: $Name expected '$Expected' but got '$Actual'"
    }
}

function Assert-Contains {
    param(
        [string]$Actual,
        [string]$Needle,
        [string]$Name
    )
    if (-not $Actual.Contains($Needle)) {
        throw "ASSERT FAIL: $Name expected output to contain '$Needle'. Output: $Actual"
    }
}

function Invoke-HarnessScript {
    param(
        [string]$ScriptName,
        [string]$InputPath,
        [string[]]$Args = @()
    )
    $scriptPath = Join-Path $HarnessRoot $ScriptName
    if (-not (Test-Path -LiteralPath $scriptPath)) {
        throw "Missing script under test: $scriptPath"
    }
    $inputText = Get-Content -LiteralPath $InputPath -Raw
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "powershell"
    $psi.ArgumentList.Add("-NoProfile")
    $psi.ArgumentList.Add("-ExecutionPolicy")
    $psi.ArgumentList.Add("Bypass")
    $psi.ArgumentList.Add("-File")
    $psi.ArgumentList.Add($scriptPath)
    foreach ($arg in $Args) {
        $psi.ArgumentList.Add($arg)
    }
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = $RepoRoot
    $p = [System.Diagnostics.Process]::Start($psi)
    $p.StandardInput.Write($inputText)
    $p.StandardInput.Close()
    $stdout = $p.StandardOutput.ReadToEnd()
    $stderr = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    [pscustomobject]@{
        ExitCode = $p.ExitCode
        Stdout = $stdout
        Stderr = $stderr
    }
}

$fixtures = Join-Path $ScriptDir "fixtures"
$tests = New-Object System.Collections.Generic.List[scriptblock]

$tests.Add({
    $r = Invoke-HarnessScript "block-dangerous-tool.ps1" (Join-Path $fixtures "pretool-dangerous.json")
    Assert-Equal $r.ExitCode 2 "dangerous command is blocked"
    Assert-Contains ($r.Stdout + $r.Stderr) "destructive command" "dangerous command reason"
})

$tests.Add({
    $r = Invoke-HarnessScript "block-dangerous-tool.ps1" (Join-Path $fixtures "pretool-safe.json")
    Assert-Equal $r.ExitCode 0 "safe command is allowed"
})

$tests.Add({
    $r = Invoke-HarnessScript "prompt-secret-scan.ps1" (Join-Path $fixtures "prompt-secret.json")
    Assert-Equal $r.ExitCode 2 "secret prompt is blocked"
    Assert-Contains ($r.Stdout + $r.Stderr) "secret-like" "secret prompt reason"
})

$tests.Add({
    $r = Invoke-HarnessScript "prompt-secret-scan.ps1" (Join-Path $fixtures "prompt-safe-legal.json")
    Assert-Equal $r.ExitCode 0 "ordinary legal prompt is allowed"
})

$tests.Add({
    $r = Invoke-HarnessScript "post-edit-rust-check.ps1" (Join-Path $fixtures "post-edit-rust.json") @("-NoRun")
    Assert-Equal $r.ExitCode 0 "post-edit dry mapping passes"
    Assert-Contains $r.Stdout "anno-rag" "crate detection"
})

$ran = 0
foreach ($test in $tests) {
    if ($Filter -and ($test.ToString() -notlike "*$Filter*")) {
        continue
    }
    & $test
    $ran += 1
}

Write-Host "agent-harness tests passed: $ran"
```

- [ ] **Step 2: Add initial fixtures**

Create `scripts/agent-harness/tests/fixtures/pretool-dangerous.json`:

```json
{
  "tool_name": "Bash",
  "tool_input": {
    "command": "rm -rf .git"
  }
}
```

Create `scripts/agent-harness/tests/fixtures/pretool-safe.json`:

```json
{
  "tool_name": "Bash",
  "tool_input": {
    "command": "git status --short"
  }
}
```

Create `scripts/agent-harness/tests/fixtures/prompt-secret.json`:

```json
{
  "prompt": "Please use this token: sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789"
}
```

Create `scripts/agent-harness/tests/fixtures/prompt-safe-legal.json`:

```json
{
  "prompt": "Analyse ce contrat entre Dupont SAS et Martin SARL. Le dossier mentionne un email professionnel et une clause de resiliation."
}
```

Create `scripts/agent-harness/tests/fixtures/post-edit-rust.json`:

```json
{
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "crates/anno-rag/src/pipeline.rs"
  },
  "tool_response": {
    "filePath": "crates/anno-rag/src/pipeline.rs"
  }
}
```

Create `scripts/agent-harness/tests/fixtures/cargo-metadata.fixture.json`:

```json
{
  "packages": [
    {
      "name": "anno-rag",
      "id": "path+file:///repo/crates/anno-rag#0.10.0",
      "manifest_path": "C:/repo/crates/anno-rag/Cargo.toml",
      "dependencies": [
        { "name": "anno-corpus-core", "path": "C:/repo/crates/anno-corpus-core" }
      ],
      "features": { "default": [], "rerank": [] }
    },
    {
      "name": "anno-rag-bin",
      "id": "path+file:///repo/crates/anno-rag-bin#0.10.0",
      "manifest_path": "C:/repo/crates/anno-rag-bin/Cargo.toml",
      "dependencies": [
        { "name": "anno-rag", "path": "C:/repo/crates/anno-rag" }
      ],
      "features": { "default": [] }
    },
    {
      "name": "anno-corpus-core",
      "id": "path+file:///repo/crates/anno-corpus-core#0.10.0",
      "manifest_path": "C:/repo/crates/anno-corpus-core/Cargo.toml",
      "dependencies": [],
      "features": { "default": [] }
    }
  ],
  "workspace_members": [
    "path+file:///repo/crates/anno-rag#0.10.0",
    "path+file:///repo/crates/anno-rag-bin#0.10.0",
    "path+file:///repo/crates/anno-corpus-core#0.10.0"
  ]
}
```

Create `scripts/agent-harness/tests/fixtures/commits.fixture.txt`:

```text
feat: add corpus setup command
fix: create corpus store parent directory
docs: update MCP setup guide
test: cover setup merge behavior
```

Create `scripts/agent-harness/tests/fixtures/diff-name-status.fixture.txt`:

```text
M	crates/anno-rag/src/pipeline.rs
M	crates/anno-rag-bin/src/main.rs
M	docs/developers/cli.md
```

- [ ] **Step 3: Run the test runner and verify it fails because scripts do not exist**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: FAIL with `Missing script under test: ...block-dangerous-tool.ps1`.

- [ ] **Step 4: Record the red state and continue without committing**

Run:

```powershell
git status --short -- scripts/agent-harness/tests
```

Expected: test runner and fixtures are uncommitted. Continue to Task 2 and commit after the shared library exists.

---

### Task 2: Add Shared PowerShell Harness Library

**Files:**
- Create: `scripts/agent-harness/lib/AgentHarness.psm1`
- Create: `scripts/agent-harness/lib/AgentHarness.psd1`
- Modify: `scripts/agent-harness/tests/test-agent-harness.ps1`

- [ ] **Step 1: Add focused tests for command extraction and crate detection**

Append these tests before the final loop in `scripts/agent-harness/tests/test-agent-harness.ps1`:

```powershell
Import-Module (Join-Path $HarnessRoot "lib/AgentHarness.psm1") -Force

$tests.Add({
    $json = Get-Content -LiteralPath (Join-Path $fixtures "pretool-safe.json") -Raw | ConvertFrom-Json
    $command = Get-AgentHarnessCommandText -InputObject $json
    Assert-Equal $command "git status --short" "command extraction"
})

$tests.Add({
    $crate = Get-AgentHarnessCrateFromPath -PathText "crates/anno-rag/src/pipeline.rs"
    Assert-Equal $crate "anno-rag" "crate extraction"
})
```

- [ ] **Step 2: Run tests to verify shared library is missing**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: FAIL with `The specified module ... AgentHarness.psm1 was not loaded`.

- [ ] **Step 3: Create `AgentHarness.psm1`**

Create `scripts/agent-harness/lib/AgentHarness.psm1`:

```powershell
Set-StrictMode -Version 2.0

function Get-AgentHarnessRepoRoot {
    param([string]$StartPath = (Get-Location).Path)
    $dir = Resolve-Path -LiteralPath $StartPath
    while ($dir) {
        if (Test-Path -LiteralPath (Join-Path $dir ".git")) {
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

function Test-AgentHarnessDangerousCommand {
    param([string]$Command)
    if ([string]::IsNullOrWhiteSpace($Command)) {
        return [pscustomobject]@{ Block = $false; Reason = "" }
    }

    $patterns = @(
        @{ Regex = "(?i)\brm\s+-rf\s+(/|\*|\.|~|\.git|\.claude|\.codex)\b"; Reason = "destructive command targets root, wildcard, repo metadata, or home" },
        @{ Regex = "(?i)\bgit\s+reset\s+--hard\b"; Reason = "destructive git reset" },
        @{ Regex = "(?i)\bgit\s+clean\s+-f(dx|xd|d)?\b"; Reason = "destructive git clean" },
        @{ Regex = "(?i)\bgit\s+checkout\s+--\b"; Reason = "destructive checkout of working tree files" },
        @{ Regex = "(?i)\bRemove-Item\b.*\s-Recurse\b.*(\.git|\.claude|\.codex|\*)"; Reason = "recursive removal of protected path" },
        @{ Regex = "(?i)\bcargo\s+build\s+--workspace\b"; Reason = "broad workspace build is not the default debug loop" },
        @{ Regex = "(?i)\bcargo\s+build\s+--release\b"; Reason = "release build is too broad for normal agent iteration" },
        @{ Regex = "(?i)>\s*\.env(\.|$)"; Reason = "write to env file may expose secrets" }
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
    Get-AgentHarnessCommandText, `
    Get-AgentHarnessPromptText, `
    Get-AgentHarnessFilePath, `
    Get-AgentHarnessCrateFromPath, `
    Test-AgentHarnessDangerousCommand, `
    Test-AgentHarnessSecretText, `
    Write-AgentHarnessBlockJson
```

- [ ] **Step 4: Create module manifest**

Create `scripts/agent-harness/lib/AgentHarness.psd1`:

```powershell
@{
    RootModule = 'AgentHarness.psm1'
    ModuleVersion = '0.1.0'
    GUID = 'cceaf91f-14e2-44f3-a431-c6d37fbf8071'
    Author = 'Hacienda'
    Description = 'Shared helpers for the Hacienda agent harness.'
    PowerShellVersion = '5.1'
    FunctionsToExport = @(
        'Get-AgentHarnessRepoRoot',
        'Read-AgentHarnessJsonFromStdin',
        'Get-AgentHarnessCommandText',
        'Get-AgentHarnessPromptText',
        'Get-AgentHarnessFilePath',
        'Get-AgentHarnessCrateFromPath',
        'Test-AgentHarnessDangerousCommand',
        'Test-AgentHarnessSecretText',
        'Write-AgentHarnessBlockJson'
    )
    CmdletsToExport = @()
    VariablesToExport = '*'
    AliasesToExport = @()
}
```

- [ ] **Step 5: Run tests and verify missing script failures remain**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: shared library tests pass, then FAIL on missing `block-dangerous-tool.ps1`.

- [ ] **Step 6: Record the shared-library red state and continue without committing**

Run:

```powershell
git status --short -- scripts/agent-harness
```

Expected: shared library, fixture files, and test runner are uncommitted. Continue to Task 3 and commit after the first hooks make the suite green for the tests that exist.

---

### Task 3: Implement Dangerous Command and Secret Prompt Hooks

**Files:**
- Create: `scripts/agent-harness/block-dangerous-tool.ps1`
- Create: `scripts/agent-harness/block-dangerous-tool.sh`
- Create: `scripts/agent-harness/prompt-secret-scan.ps1`
- Create: `scripts/agent-harness/prompt-secret-scan.sh`

- [ ] **Step 1: Create the dangerous command PowerShell hook**

Create `scripts/agent-harness/block-dangerous-tool.ps1`:

```powershell
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $ScriptDir "lib/AgentHarness.psm1") -Force

try {
    $inputObject = Read-AgentHarnessJsonFromStdin
    $command = Get-AgentHarnessCommandText -InputObject $inputObject
    $result = Test-AgentHarnessDangerousCommand -Command $command
    if ($result.Block) {
        $reason = "destructive command blocked by Hacienda agent harness: $($result.Reason)"
        Write-Error $reason
        Write-AgentHarnessBlockJson -Reason $reason
        exit 2
    }
    exit 0
} catch {
    Write-Error "agent harness dangerous-command hook error: $($_.Exception.Message)"
    exit 1
}
```

- [ ] **Step 2: Create the dangerous command Bash shim**

Create `scripts/agent-harness/block-dangerous-tool.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
powershell -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_DIR/block-dangerous-tool.ps1"
```

- [ ] **Step 3: Create the prompt secret scanner**

Create `scripts/agent-harness/prompt-secret-scan.ps1`:

```powershell
$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $ScriptDir "lib/AgentHarness.psm1") -Force

try {
    $inputObject = Read-AgentHarnessJsonFromStdin
    $prompt = Get-AgentHarnessPromptText -InputObject $inputObject
    $result = Test-AgentHarnessSecretText -Text $prompt
    if ($result.Block) {
        $reason = "secret-like prompt content blocked by Hacienda agent harness: $($result.Reason)"
        Write-Error $reason
        Write-AgentHarnessBlockJson -Reason $reason
        exit 2
    }
    exit 0
} catch {
    Write-Error "agent harness prompt-secret hook error: $($_.Exception.Message)"
    exit 1
}
```

- [ ] **Step 4: Create the prompt secret Bash shim**

Create `scripts/agent-harness/prompt-secret-scan.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
powershell -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_DIR/prompt-secret-scan.ps1"
```

- [ ] **Step 5: Run tests**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: dangerous command and prompt tests pass, then FAIL on missing `post-edit-rust-check.ps1`.

- [ ] **Step 6: Record the safety-hook red state and continue without committing**

Run:

```powershell
git status --short -- scripts/agent-harness
```

Expected: safety hooks are uncommitted. Continue to Task 4 and commit after the post-edit hook makes the current suite pass.

---

### Task 4: Implement Rust Post-Edit Check and Stop Verification

**Files:**
- Create: `scripts/agent-harness/post-edit-rust-check.ps1`
- Create: `scripts/agent-harness/post-edit-rust-check.sh`
- Create: `scripts/agent-harness/stop-verify.ps1`
- Create: `scripts/agent-harness/stop-verify.sh`
- Modify: `.gitignore`

- [ ] **Step 1: Add ignored local harness state**

Modify `.gitignore` to include:

```gitignore
# Agent harness local state
.agent-harness/
```

- [ ] **Step 2: Create the Rust post-edit check script**

Create `scripts/agent-harness/post-edit-rust-check.ps1`:

```powershell
param(
    [switch]$NoRun
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $ScriptDir "lib/AgentHarness.psm1") -Force

try {
    $repo = Get-AgentHarnessRepoRoot
    $inputObject = Read-AgentHarnessJsonFromStdin
    $pathText = Get-AgentHarnessFilePath -InputObject $inputObject
    $crate = Get-AgentHarnessCrateFromPath -PathText $pathText

    if (-not $pathText.EndsWith(".rs")) {
        Write-Output "agent harness: no Rust file detected"
        exit 0
    }
    if (-not $crate) {
        Write-Output "agent harness: Rust path is outside crates/: $pathText"
        exit 0
    }

    Write-Output "agent harness: detected crate $crate from $pathText"
    if ($NoRun) {
        exit 0
    }

    $fullPath = Join-Path $repo $pathText
    if (Test-Path -LiteralPath $fullPath) {
        rustfmt --edition 2021 $fullPath
    }

    $devFast = Join-Path $repo "scripts/dev-fast.ps1"
    powershell -NoProfile -ExecutionPolicy Bypass -File $devFast -Package $crate -Mode check

    $stateDir = Join-Path $repo ".agent-harness/state"
    New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
    $stamp = [ordered]@{
        crate = $crate
        file = $pathText
        command = "scripts/dev-fast.ps1 -Package $crate -Mode check"
        checked_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    }
    $stamp | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $stateDir "last-check.json") -Encoding UTF8
    exit 0
} catch {
    Write-Error "agent harness post-edit hook error: $($_.Exception.Message)"
    exit 1
}
```

- [ ] **Step 3: Create the Bash shim**

Create `scripts/agent-harness/post-edit-rust-check.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
powershell -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_DIR/post-edit-rust-check.ps1" "$@"
```

- [ ] **Step 4: Create the stop verification script**

Create `scripts/agent-harness/stop-verify.ps1`:

```powershell
param(
    [switch]$NoBlock
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Import-Module (Join-Path $ScriptDir "lib/AgentHarness.psm1") -Force

try {
    $repo = Get-AgentHarnessRepoRoot
    $changed = git -C $repo diff --name-only HEAD 2>$null
    $rustChanged = @($changed | Where-Object { $_ -match "\.rs$" })
    if ($rustChanged.Count -eq 0) {
        Write-Output "agent harness stop gate: no Rust changes detected"
        exit 0
    }

    $stampPath = Join-Path $repo ".agent-harness/state/last-check.json"
    if (Test-Path -LiteralPath $stampPath) {
        $stamp = Get-Content -LiteralPath $stampPath -Raw | ConvertFrom-Json
        Write-Output "agent harness stop gate: last targeted check found for $($stamp.crate)"
        exit 0
    }

    $reason = "Rust changes detected without a recent agent harness targeted check. Run scripts/dev-fast.ps1 for the changed crate or explain why verification is impossible."
    if ($NoBlock) {
        Write-Warning $reason
        exit 0
    }
    Write-Error $reason
    Write-AgentHarnessBlockJson -Reason $reason
    exit 2
} catch {
    Write-Error "agent harness stop gate error: $($_.Exception.Message)"
    exit 1
}
```

- [ ] **Step 5: Create stop verification Bash shim**

Create `scripts/agent-harness/stop-verify.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
powershell -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_DIR/stop-verify.ps1" "$@"
```

- [ ] **Step 6: Run tests**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: all existing tests pass.

- [ ] **Step 7: Commit tested hook foundation**

Run:

```powershell
git add .gitignore scripts/agent-harness/lib scripts/agent-harness/tests scripts/agent-harness/block-dangerous-tool.ps1 scripts/agent-harness/block-dangerous-tool.sh scripts/agent-harness/prompt-secret-scan.ps1 scripts/agent-harness/prompt-secret-scan.sh scripts/agent-harness/post-edit-rust-check.ps1 scripts/agent-harness/post-edit-rust-check.sh scripts/agent-harness/stop-verify.ps1 scripts/agent-harness/stop-verify.sh
git commit -m "feat: add agent harness hook foundation"
```

Expected: commit includes `.gitignore`, fixtures, shared library, safety hooks, and verification hooks.

---

### Task 5: Add Claude Code Project Layer

**Files:**
- Modify: `.claude/settings.json`
- Create: `.claude/rules/rust.md`
- Create: `.claude/rules/privacy.md`
- Create: `.claude/rules/gitnexus.md`
- Create: `.claude/agents/anno-rust-reviewer.md`
- Create: `.claude/agents/anno-security-reviewer.md`
- Create: `.claude/agents/anno-build-resolver.md`
- Create: `.claude/agents/anno-doc-writer.md`
- Create: `.claude/agents/anno-gitnexus-explorer.md`
- Create: `.claude/agents/anno-changelog-writer.md`
- Create: `.claude/agents/anno-pr-reviewer.md`
- Create: `.claude/agents/anno-crate-graph-auditor.md`
- Create: `.claude/agents/anno-cli-parity-auditor.md`
- Create: `.claude/agents/anno-release-gate.md`

- [ ] **Step 1: Merge script-backed Claude Code hooks**

Modify `.claude/settings.json` to preserve existing permissions and use script-backed hooks:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "shell": "powershell",
            "command": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/prompt-secret-scan.ps1",
            "timeout": 20,
            "statusMessage": "secret scan ..."
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "shell": "powershell",
            "command": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/block-dangerous-tool.ps1",
            "timeout": 20,
            "statusMessage": "safety gate ..."
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "shell": "powershell",
            "command": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/post-edit-rust-check.ps1",
            "timeout": 90,
            "statusMessage": "targeted Rust check ...",
            "async": true
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "shell": "powershell",
            "command": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/stop-verify.ps1",
            "timeout": 30,
            "statusMessage": "completion gate ..."
          }
        ]
      }
    ]
  }
}
```

If `.claude/settings.json` already contains other keys, merge this `hooks` object without deleting those keys.

- [ ] **Step 2: Validate settings JSON**

Run:

```powershell
Get-Content .claude\settings.json -Raw | ConvertFrom-Json | Out-Null
```

Expected: no output and exit code 0.

- [ ] **Step 3: Add rule files**

Create `.claude/rules/rust.md`:

```markdown
# Rust Rules

- Prefer `scripts/dev-fast.ps1` over broad workspace builds.
- Use targeted crate checks after Rust edits.
- Avoid `unwrap()` and `expect()` in production paths.
- Use `thiserror` for public error types and `anyhow` for application-level CLI errors.
- Use `tracing` for structured runtime logging.
- Add `// SAFETY:` comments for every unsafe block.
```

Create `.claude/rules/privacy.md`:

```markdown
# Privacy Rules

- Do not log secrets, vault passphrases, full prompts, transcripts, or legal matter text.
- Treat local legal text and PII as sensitive even when it stays on disk.
- Do not write `.env` or credential files unless the user explicitly asks.
- Keep generated harness state under `.agent-harness/`.
- Full transcript backups require `ANNO_AGENT_HARNESS_BACKUP_TRANSCRIPTS=1`.
```

Create `.claude/rules/gitnexus.md`:

```markdown
# GitNexus Rules

- Run GitNexus impact analysis before editing functions, classes, methods, or public symbols.
- If the index is stale, run `npx gitnexus analyze`.
- Use `npx gitnexus query --repo anno "<concept>"` before grepping unfamiliar flows.
- Use `npx gitnexus context --repo anno <symbol>` for callers and callees.
- Before commits, use GitNexus detect-change tooling when available; otherwise use `npx gitnexus status` and `git diff --name-status`.
```

- [ ] **Step 4: Add Claude Code agents**

Create each `.claude/agents/*.md` with this pattern. Example for `.claude/agents/anno-pr-reviewer.md`:

```markdown
---
name: anno-pr-reviewer
description: Read-only PR reviewer for Hacienda. Use for pull request review, local diff review, PR summaries, test-plan validation, docs impact, crate impact, and CLI/MCP parity checks.
tools: Read, Grep, Glob, Bash(git diff*), Bash(git log*), Bash(git status*), Bash(npx gitnexus*), Bash(cargo metadata*)
disallowedTools: Edit, Write, MultiEdit
model: sonnet
permissionMode: default
---

Review the requested diff or PR. Start with findings ordered by severity. Include file references, affected crates, missing tests, docs impact, changelog impact, security notes, and CLI/MCP parity notes. Do not modify files.
```

For the other agents, use the same frontmatter pattern and these descriptions:

```text
anno-rust-reviewer: Read-only Rust correctness and maintainability reviewer.
anno-security-reviewer: Read-only security reviewer for secrets, auth, paths, crypto, vault, network IO, and unsafe Rust.
anno-build-resolver: Build/test failure resolver that may edit only when delegated.
anno-doc-writer: Documentation writer for docs and public API comments.
anno-gitnexus-explorer: Read-only code explorer that uses GitNexus before source reads.
anno-changelog-writer: Changelog and release-note writer that edits only changelog or release-note docs.
anno-crate-graph-auditor: Read-only crate dependency and feature propagation auditor.
anno-cli-parity-auditor: Read-only auditor for anno-rag to anno-rag-bin/docs/tests parity.
anno-release-gate: Read-only release readiness checker unless explicitly delegated.
```

- [ ] **Step 5: Commit Claude Code layer**

Run:

```powershell
git add .claude/settings.json .claude/rules .claude/agents
git commit -m "feat: add Claude Code agent harness layer"
```

Expected: commit includes Claude Code config, rules, and agents only.

---

### Task 6: Add Codex Project Layer and Shared Skills

**Files:**
- Create: `.codex/config.toml`
- Create: `.codex/hooks.json`
- Create: `.codex/agents/explorer.toml`
- Create: `.codex/agents/reviewer.toml`
- Create: `.codex/agents/security.toml`
- Create: `.codex/agents/build-fixer.toml`
- Create: `.codex/agents/docs.toml`
- Create: `.codex/agents/release-notes.toml`
- Create: `.codex/agents/crate-auditor.toml`
- Create: `.agents/skills/anno-fast-debug-loop/SKILL.md`
- Create: `.agents/skills/anno-gitnexus-impact/SKILL.md`
- Create: `.agents/skills/anno-security-review/SKILL.md`
- Create: `.agents/skills/anno-mcp-smoke/SKILL.md`
- Create: `.agents/skills/anno-changelog/SKILL.md`
- Create: `.agents/skills/anno-pr-review/SKILL.md`
- Create: `.agents/skills/anno-doc-generation/SKILL.md`
- Create: `.agents/skills/anno-crate-dependency-map/SKILL.md`
- Create: `.agents/skills/anno-cli-feature-parity/SKILL.md`
- Create: `.agents/skills/anno-agent-context-generation/SKILL.md`
- Create: `.agents/skills/anno-release-check/SKILL.md`

- [ ] **Step 1: Add Codex config**

Create `.codex/config.toml`:

```toml
[features]
multi_agent = true

[project]
name = "anno"

[harness]
preferred_check = "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev-fast.ps1"
agent_state_dir = ".agent-harness"
```

- [ ] **Step 2: Add Codex hooks JSON**

Create `.codex/hooks.json`:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "command": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/prompt-secret-scan.ps1",
        "timeout_ms": 20000
      }
    ],
    "PreToolUse": [
      {
        "command": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/block-dangerous-tool.ps1",
        "timeout_ms": 20000
      }
    ],
    "PostToolUse": [
      {
        "command": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/post-edit-rust-check.ps1",
        "timeout_ms": 90000
      }
    ],
    "Stop": [
      {
        "command": "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/stop-verify.ps1",
        "timeout_ms": 30000
      }
    ]
  }
}
```

- [ ] **Step 3: Validate Codex JSON**

Run:

```powershell
Get-Content .codex\hooks.json -Raw | ConvertFrom-Json | Out-Null
```

Expected: no output and exit code 0.

- [ ] **Step 4: Add Codex agents**

Create `.codex/agents/reviewer.toml`:

```toml
name = "reviewer"
description = "Read-only correctness and maintainability reviewer for Hacienda diffs."
instructions = """
Review local diffs with findings first. Include file references, test gaps, docs impact, crate impact, and CLI/MCP parity impact. Do not edit files.
"""
```

Create the other `.codex/agents/*.toml` files with the same shape and these descriptions:

```text
explorer: Read-only GitNexus-first codebase explorer.
security: Read-only security reviewer for secrets, auth, paths, crypto, vault, network IO, and unsafe Rust.
build-fixer: Build and test failure investigator; edits only when explicitly delegated.
docs: Documentation generator and stale-doc detector.
release-notes: Changelog and PR summary generator.
crate-auditor: Crate dependency and CLI feature parity auditor.
```

- [ ] **Step 5: Add shared skill template**

For each `.agents/skills/<name>/SKILL.md`, use this structure. Example `.agents/skills/anno-cli-feature-parity/SKILL.md`:

```markdown
---
name: anno-cli-feature-parity
description: Use when anno-rag, anno-rag-mcp, or anno-rag-tabular changes may require matching anno-rag-bin CLI, docs, examples, or smoke-test updates.
---

# Anno CLI Feature Parity

Run this workflow when user-facing `anno-rag`, `anno-rag-mcp`, or `anno-rag-tabular` behavior changes.

1. Inspect changed files with `git diff --name-status`.
2. Use GitNexus context or impact for changed public symbols when available.
3. Check `crates/anno-rag-bin/src/main.rs` and related command modules for CLI coverage.
4. Check `docs/developers/cli.md`, `docs/developers/mcp-tools.md`, `README.md`, and release docs for stale references.
5. Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/agent-harness/cli-feature-parity.ps1 -DryRun`.
6. Report whether drift is internal-only, warning-level, or high-confidence user-facing drift.
```

Write the remaining skill files with the same level of concrete commands:

```text
anno-fast-debug-loop: use dev-fast, test-local, nextest profiles, and package detection.
anno-gitnexus-impact: use status, query, context, impact, and fallback diff checks.
anno-security-review: review secrets, auth, path traversal, crypto, vault, MCP, gateway, network IO, unsafe Rust.
anno-mcp-smoke: run MCP initialize/tools/list/anno_health smoke or existing mcp smoke script.
anno-changelog: generate dry-run changelog from commits and diffs.
anno-pr-review: generate findings and PR summary from base diff.
anno-doc-generation: refresh command, MCP, crate, and agent context docs from captured evidence.
anno-crate-dependency-map: generate and read local crate graph from cargo metadata.
anno-agent-context-generation: update concise Claude/Codex context from GitNexus, docs, and Cargo metadata.
anno-release-check: run release validation, docs audit, packaging checks, and changelog review.
```

- [ ] **Step 6: Commit Codex layer and skills**

Run:

```powershell
git add .codex .agents/skills
git commit -m "feat: add Codex agent harness layer"
```

Expected: commit includes Codex config, Codex agents, and shared skills only.

---

### Task 7: Add Crate Map and CLI Parity Automation

**Files:**
- Create: `scripts/agent-harness/crate-map-generate.ps1`
- Create: `scripts/agent-harness/cli-feature-parity.ps1`
- Modify: `scripts/agent-harness/tests/test-agent-harness.ps1`

- [ ] **Step 1: Add tests for crate map and parity scripts**

Append to `scripts/agent-harness/tests/test-agent-harness.ps1`:

```powershell
$tests.Add({
    $metadata = Join-Path $fixtures "cargo-metadata.fixture.json"
    $script = Join-Path $HarnessRoot "crate-map-generate.ps1"
    $out = powershell -NoProfile -ExecutionPolicy Bypass -File $script -MetadataPath $metadata -DryRun
    Assert-Contains ($out -join "`n") "anno-rag-bin depends on anno-rag" "crate dependency output"
    Assert-Contains ($out -join "`n") "anno-rag is depended on by anno-rag-bin" "reverse dependency output"
})

$tests.Add({
    $diff = Join-Path $fixtures "diff-name-status.fixture.txt"
    $script = Join-Path $HarnessRoot "cli-feature-parity.ps1"
    $out = powershell -NoProfile -ExecutionPolicy Bypass -File $script -DiffNameStatusPath $diff -DryRun
    Assert-Contains ($out -join "`n") "anno-rag change detected" "parity detects anno-rag"
    Assert-Contains ($out -join "`n") "anno-rag-bin touched" "parity detects CLI coverage"
})
```

- [ ] **Step 2: Run tests and verify missing script failure**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: FAIL with missing `crate-map-generate.ps1`.

- [ ] **Step 3: Create crate map generator**

Create `scripts/agent-harness/crate-map-generate.ps1`:

```powershell
param(
    [string]$MetadataPath = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
if ($MetadataPath) {
    $metadata = Get-Content -LiteralPath $MetadataPath -Raw | ConvertFrom-Json
} else {
    $metadata = cargo metadata --format-version 1 | ConvertFrom-Json
}

$workspaceIds = @{}
foreach ($id in $metadata.workspace_members) {
    $workspaceIds[[string]$id] = $true
}

$packages = @{}
foreach ($pkg in $metadata.packages) {
    if ($workspaceIds.ContainsKey([string]$pkg.id)) {
        $packages[$pkg.name] = $pkg
    }
}

$reverse = @{}
foreach ($name in $packages.Keys) {
    $reverse[$name] = New-Object System.Collections.Generic.List[string]
}

foreach ($name in $packages.Keys) {
    $pkg = $packages[$name]
    foreach ($dep in $pkg.dependencies) {
        if ($packages.ContainsKey($dep.name)) {
            Write-Output "$name depends on $($dep.name)"
            $reverse[$dep.name].Add($name)
        }
    }
}

foreach ($name in ($reverse.Keys | Sort-Object)) {
    foreach ($dependent in ($reverse[$name] | Sort-Object)) {
        Write-Output "$name is depended on by $dependent"
    }
}

if ($DryRun) {
    Write-Output "dry-run: no files written"
}
```

- [ ] **Step 4: Create CLI parity checker**

Create `scripts/agent-harness/cli-feature-parity.ps1`:

```powershell
param(
    [string]$DiffNameStatusPath = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
if ($DiffNameStatusPath) {
    $lines = Get-Content -LiteralPath $DiffNameStatusPath
} else {
    $lines = git diff --name-status HEAD
}

$paths = @($lines | ForEach-Object {
    $parts = $_ -split "`t"
    if ($parts.Count -ge 2) { $parts[1] } else { $_ }
})

$ragChanged = @($paths | Where-Object { $_ -match "^crates/anno-rag(/|-mcp|-tabular)" })
$cliTouched = @($paths | Where-Object { $_ -match "^crates/anno-rag-bin/" })
$docsTouched = @($paths | Where-Object { $_ -match "^(README.md|docs/)" })

if ($ragChanged.Count -gt 0) {
    Write-Output "anno-rag change detected: $($ragChanged -join ', ')"
    if ($cliTouched.Count -gt 0) {
        Write-Output "anno-rag-bin touched: $($cliTouched -join ', ')"
    } else {
        Write-Output "warning: anno-rag-bin not touched; verify this is internal-only"
    }
    if ($docsTouched.Count -gt 0) {
        Write-Output "docs touched: $($docsTouched -join ', ')"
    } else {
        Write-Output "warning: docs not touched; verify user-facing docs are still current"
    }
} else {
    Write-Output "no anno-rag user-facing surface change detected"
}

if ($DryRun) {
    Write-Output "dry-run: no files written"
}
```

- [ ] **Step 5: Run tests**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: crate map and parity tests pass.

- [ ] **Step 6: Commit automation scripts**

Run:

```powershell
git add scripts/agent-harness/crate-map-generate.ps1 scripts/agent-harness/cli-feature-parity.ps1 scripts/agent-harness/tests/test-agent-harness.ps1
git commit -m "feat: add crate map and CLI parity automation"
```

Expected: commit includes two scripts and test runner update.

---

### Task 8: Add Changelog, PR Review, Docs, and Context Automation

**Files:**
- Create: `scripts/agent-harness/changelog-generate.ps1`
- Create: `scripts/agent-harness/pr-review-generate.ps1`
- Create: `scripts/agent-harness/docs-generate.ps1`
- Create: `scripts/agent-harness/agent-context-generate.ps1`
- Modify: `scripts/agent-harness/tests/test-agent-harness.ps1`

- [ ] **Step 1: Add tests for changelog and PR scripts**

Append to `scripts/agent-harness/tests/test-agent-harness.ps1`:

```powershell
$tests.Add({
    $commits = Join-Path $fixtures "commits.fixture.txt"
    $script = Join-Path $HarnessRoot "changelog-generate.ps1"
    $out = powershell -NoProfile -ExecutionPolicy Bypass -File $script -CommitsPath $commits -DryRun
    Assert-Contains ($out -join "`n") "Features" "changelog feature section"
    Assert-Contains ($out -join "`n") "Bug Fixes" "changelog fix section"
})

$tests.Add({
    $diff = Join-Path $fixtures "diff-name-status.fixture.txt"
    $script = Join-Path $HarnessRoot "pr-review-generate.ps1"
    $out = powershell -NoProfile -ExecutionPolicy Bypass -File $script -DiffNameStatusPath $diff
    Assert-Contains ($out -join "`n") "Findings" "PR review findings section"
    Assert-Contains ($out -join "`n") "CLI and MCP Parity" "PR review parity section"
})
```

- [ ] **Step 2: Create changelog generator**

Create `scripts/agent-harness/changelog-generate.ps1`:

```powershell
param(
    [string]$Since = "main",
    [string]$CommitsPath = "",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
if ($CommitsPath) {
    $commits = Get-Content -LiteralPath $CommitsPath
} else {
    $commits = git log --format="%s" "$Since..HEAD"
}

$groups = [ordered]@{
    "Features" = New-Object System.Collections.Generic.List[string]
    "Bug Fixes" = New-Object System.Collections.Generic.List[string]
    "Performance" = New-Object System.Collections.Generic.List[string]
    "Refactors" = New-Object System.Collections.Generic.List[string]
    "Documentation" = New-Object System.Collections.Generic.List[string]
    "Tests" = New-Object System.Collections.Generic.List[string]
    "CI and Chores" = New-Object System.Collections.Generic.List[string]
}

foreach ($c in $commits) {
    if ($c -match "^feat:\s*(.+)") { $groups["Features"].Add($Matches[1]); continue }
    if ($c -match "^fix:\s*(.+)") { $groups["Bug Fixes"].Add($Matches[1]); continue }
    if ($c -match "^perf:\s*(.+)") { $groups["Performance"].Add($Matches[1]); continue }
    if ($c -match "^refactor:\s*(.+)") { $groups["Refactors"].Add($Matches[1]); continue }
    if ($c -match "^docs:\s*(.+)") { $groups["Documentation"].Add($Matches[1]); continue }
    if ($c -match "^test:\s*(.+)") { $groups["Tests"].Add($Matches[1]); continue }
    if ($c -match "^(ci|chore):\s*(.+)") { $groups["CI and Chores"].Add($Matches[2]); continue }
}

Write-Output "## Unreleased"
foreach ($name in $groups.Keys) {
    if ($groups[$name].Count -eq 0) { continue }
    Write-Output ""
    Write-Output "### $name"
    foreach ($item in $groups[$name]) {
        Write-Output "- $item"
    }
}

if ($DryRun) {
    Write-Output ""
    Write-Output "dry-run: no files written"
}
```

- [ ] **Step 3: Create PR review generator**

Create `scripts/agent-harness/pr-review-generate.ps1`:

```powershell
param(
    [string]$Base = "main",
    [string]$DiffNameStatusPath = ""
)

$ErrorActionPreference = "Stop"
if ($DiffNameStatusPath) {
    $lines = Get-Content -LiteralPath $DiffNameStatusPath
} else {
    $lines = git diff --name-status "$Base...HEAD"
}

$paths = @($lines | ForEach-Object {
    $parts = $_ -split "`t"
    if ($parts.Count -ge 2) { $parts[1] } else { $_ }
})

Write-Output "## Findings"
Write-Output ""
Write-Output "No automatic critical findings from path-level review. Manual code review still required."
Write-Output ""
Write-Output "## Changed Areas"
foreach ($path in $paths) {
    Write-Output "- $path"
}
Write-Output ""
Write-Output "## CLI and MCP Parity"
if (@($paths | Where-Object { $_ -match "^crates/anno-rag" }).Count -gt 0) {
    Write-Output "- anno-rag surface changed. Verify anno-rag-bin CLI, MCP docs, examples, and smoke tests."
} else {
    Write-Output "- No anno-rag surface path detected."
}
Write-Output ""
Write-Output "## Test Plan"
Write-Output "- Run targeted dev-fast checks for changed crates."
Write-Output "- Run docs audit when docs changed."
```

- [ ] **Step 4: Create docs generator dry-run**

Create `scripts/agent-harness/docs-generate.ps1`:

```powershell
param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$commands = @(
    "anno-rag --help",
    "anno-rag review --help"
)

Write-Output "docs generation evidence plan"
foreach ($command in $commands) {
    Write-Output "- capture: $command"
}
Write-Output "- run: cargo metadata --format-version 1"
Write-Output "- run: just docs-audit"
if ($DryRun) {
    Write-Output "dry-run: no files written"
}
```

- [ ] **Step 5: Create agent context generator dry-run**

Create `scripts/agent-harness/agent-context-generate.ps1`:

```powershell
param(
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Write-Output "agent context generation plan"
Write-Output "- AGENTS.md: keep concise cross-agent rules"
Write-Output "- CLAUDE.md: keep concise Claude Code memory"
Write-Output "- .claude/rules: scoped rules"
Write-Output "- .agents/skills: detailed workflows"
Write-Output "- docs/developers/agent-context.md: optional longer generated context"
if ($DryRun) {
    Write-Output "dry-run: no files written"
}
```

- [ ] **Step 6: Run tests**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: all tests pass.

- [ ] **Step 7: Commit automation scripts**

Run:

```powershell
git add scripts/agent-harness/changelog-generate.ps1 scripts/agent-harness/pr-review-generate.ps1 scripts/agent-harness/docs-generate.ps1 scripts/agent-harness/agent-context-generate.ps1 scripts/agent-harness/tests/test-agent-harness.ps1
git commit -m "feat: add agent harness maintenance automation"
```

Expected: commit includes four scripts and test runner update.

---

### Task 9: Add Setup and Status Commands

**Files:**
- Create: `scripts/agent-harness/setup-agent-harness.ps1`
- Create: `scripts/agent-harness/setup-agent-harness.sh`
- Create: `scripts/agent-harness/harness-status.ps1`
- Modify: `scripts/agent-harness/tests/test-agent-harness.ps1`

- [ ] **Step 1: Create status script**

Create `scripts/agent-harness/harness-status.ps1`:

```powershell
$ErrorActionPreference = "Stop"
$repo = git rev-parse --show-toplevel
$checks = [ordered]@{
    claude_settings = Test-Path -LiteralPath (Join-Path $repo ".claude/settings.json")
    codex_config = Test-Path -LiteralPath (Join-Path $repo ".codex/config.toml")
    codex_hooks = Test-Path -LiteralPath (Join-Path $repo ".codex/hooks.json")
    shared_skills = Test-Path -LiteralPath (Join-Path $repo ".agents/skills/anno-fast-debug-loop/SKILL.md")
    harness_scripts = Test-Path -LiteralPath (Join-Path $repo "scripts/agent-harness/block-dangerous-tool.ps1")
}
$checks.GetEnumerator() | ForEach-Object {
    Write-Output "$($_.Key): $($_.Value)"
}
```

- [ ] **Step 2: Create setup script**

Create `scripts/agent-harness/setup-agent-harness.ps1`:

```powershell
param(
    [ValidateSet("all", "claude-code", "codex", "git-hooks", "mcp", "automation")]
    [string]$Target = "all",
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$repo = git rev-parse --show-toplevel
Write-Output "Hacienda agent harness setup"
Write-Output "target: $Target"
Write-Output "repo: $repo"

$paths = @(
    ".claude/settings.json",
    ".codex/config.toml",
    ".codex/hooks.json",
    ".agents/skills/anno-fast-debug-loop/SKILL.md",
    "scripts/agent-harness/block-dangerous-tool.ps1",
    "scripts/agent-harness/changelog-generate.ps1"
)

foreach ($path in $paths) {
    $full = Join-Path $repo $path
    Write-Output "$(if (Test-Path -LiteralPath $full) { 'ok' } else { 'missing' }): $path"
}

if ($DryRun) {
    Write-Output "dry-run: no files written"
    exit 0
}

Write-Output "setup verified existing repo-local harness files"
```

- [ ] **Step 3: Create Bash setup shim**

Create `scripts/agent-harness/setup-agent-harness.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
powershell -NoProfile -ExecutionPolicy Bypass -File "$SCRIPT_DIR/setup-agent-harness.ps1" "$@"
```

- [ ] **Step 4: Run setup dry-run**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\setup-agent-harness.ps1 -DryRun
```

Expected: prints existing/missing status and `dry-run: no files written`.

- [ ] **Step 5: Run status**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\harness-status.ps1
```

Expected: prints boolean status lines for config, skills, and scripts.

- [ ] **Step 6: Commit setup scripts**

Run:

```powershell
git add scripts/agent-harness/setup-agent-harness.ps1 scripts/agent-harness/setup-agent-harness.sh scripts/agent-harness/harness-status.ps1
git commit -m "feat: add agent harness setup command"
```

Expected: commit includes three setup/status scripts.

---

### Task 10: Document Harness Usage

**Files:**
- Modify: `docs/developers/configuration.md`
- Modify: `docs/README.md`
- Modify: `AGENTS.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add a developer docs section**

In `docs/developers/configuration.md`, add:

````markdown
## Agent Harness

The repo-local agent harness configures Claude Code and Codex for Hacienda development. It provides safety hooks, targeted Rust checks, GitNexus-first exploration, changelog generation, PR review, docs generation, crate dependency mapping, CLI feature parity checks, and compact agent context generation.

Dry-run setup:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\setup-agent-harness.ps1 -DryRun
```

Status:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\harness-status.ps1
```

Run fixture tests:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```
````

- [ ] **Step 2: Add `docs/README.md` pointer**

Add this line under the Developer section in `docs/README.md`:

```markdown
- [Agent harness configuration](developers/configuration.md#agent-harness) - Claude Code and Codex setup for repo-local agent workflows.
```

- [ ] **Step 3: Keep `AGENTS.md` pointer short**

Add this short section to `AGENTS.md`:

```markdown
## Agent Harness

Repo-local Claude Code and Codex harness files live under `.claude/`, `.codex/`, `.agents/skills/`, and `scripts/agent-harness/`. Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\harness-status.ps1` to inspect setup state.
```

- [ ] **Step 4: Keep `CLAUDE.md` pointer short**

Add the same short pointer to `CLAUDE.md` near the local development guidance:

```markdown
## Agent Harness

Repo-local Claude Code and Codex harness files live under `.claude/`, `.codex/`, `.agents/skills/`, and `scripts/agent-harness/`. Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\harness-status.ps1` to inspect setup state.
```

- [ ] **Step 5: Run docs audit**

Run:

```powershell
just docs-audit
```

Expected: PASS. If `just` is unavailable, run:

```powershell
python scripts\docs_audit.py
```

Expected: PASS.

- [ ] **Step 6: Commit docs**

Run:

```powershell
git add docs/developers/configuration.md docs/README.md AGENTS.md CLAUDE.md
git commit -m "docs: document agent harness setup"
```

Expected: commit includes only docs and instruction pointers.

---

### Task 11: Final Verification and GitNexus Refresh

**Files:**
- No new files expected unless GitNexus updates generated instruction stats.

- [ ] **Step 1: Run harness tests**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\tests\test-agent-harness.ps1
```

Expected: `agent-harness tests passed: <count>`.

- [ ] **Step 2: Run setup dry-run**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\setup-agent-harness.ps1 -DryRun
```

Expected: prints setup status and does not write files.

- [ ] **Step 3: Run status**

Run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\agent-harness\harness-status.ps1
```

Expected: required harness paths report `True`.

- [ ] **Step 4: Run docs audit**

Run:

```powershell
just docs-audit
```

Expected: PASS.

- [ ] **Step 5: Run GitNexus status**

Run:

```powershell
npx gitnexus status
```

Expected: up-to-date or stale because HEAD changed.

- [ ] **Step 6: Refresh GitNexus if stale**

Run only if status is stale:

```powershell
npx gitnexus analyze
```

Expected: repository indexed successfully.

- [ ] **Step 7: Inspect final staged state**

Run:

```powershell
git status --short
git diff --cached --name-status
```

Expected: no staged files. Unrelated pre-existing dirty files may remain.

- [ ] **Step 8: Create final summary**

Report:

```text
Implemented:
- Safety hooks
- Rust verification hooks
- Claude Code agents/rules
- Codex agents/config
- Shared skills
- Maintenance automation scripts
- Setup/status scripts
- Documentation

Verified:
- test-agent-harness.ps1
- setup-agent-harness.ps1 -DryRun
- harness-status.ps1
- docs audit
- GitNexus status/analyze

Known residuals:
- Existing unrelated dirty worktree entries, if any.
```

## Self-Review

Spec coverage:

- Safety hooks: Tasks 2, 3, 4, 5, 6.
- Claude Code setup: Task 5.
- Codex compatibility: Task 6.
- Skills: Task 6.
- Changelog automation: Task 8.
- PR review automation: Task 8.
- Docs generation: Task 8 and Task 10.
- Crate dependency graph: Task 7.
- CLI feature parity: Task 7.
- Agent context generation: Task 8.
- Setup/status: Task 9.
- Verification and GitNexus: Task 11.

Red-flag scan:

- The plan uses concrete paths, commands, and snippets.
- The plan avoids broad release builds as default checks.
- The plan stages only task-owned files at each commit.
- The plan keeps generated transcript behavior out of scope for committed files.

Type and naming consistency:

- Scripts use `scripts/agent-harness/<name>.ps1`.
- Shared module is `scripts/agent-harness/lib/AgentHarness.psm1`.
- State directory is `.agent-harness/`.
- Skills use `anno-*` directory names under `.agents/skills/`.
