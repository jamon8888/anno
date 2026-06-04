param(
    [string]$Exe = "$env:LOCALAPPDATA\anno-rag\anno-rag.exe",
    [string]$ModelsDir = "$env:USERPROFILE\.anno-rag\models"
)

$ErrorActionPreference = "Stop"

$repoRoot = (& git rev-parse --show-toplevel).Trim()
Set-Location -LiteralPath $repoRoot

if (-not (Test-Path -LiteralPath $Exe)) {
    throw "anno-rag exe not found: $Exe"
}
if (-not (Test-Path -LiteralPath $ModelsDir)) {
    throw "models dir not found: $ModelsDir"
}

$env:ANNO_RAG_EXE = (Resolve-Path -LiteralPath $Exe).Path
$env:ANNO_MODELS_DIR = (Resolve-Path -LiteralPath $ModelsDir).Path

python scripts\mcp_full_smoke.py
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
