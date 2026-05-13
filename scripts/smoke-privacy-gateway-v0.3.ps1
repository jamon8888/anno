param(
    [int]$GatewayPort = 3300,
    [int]$UpstreamPort = 3310
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if (-not $cargo) {
    $cargoPath = Join-Path $HOME ".cargo\bin\cargo.exe"
    if (-not (Test-Path -LiteralPath $cargoPath)) {
        throw "cargo not found in PATH or $cargoPath"
    }
    $cargo = [pscustomobject]@{ Source = $cargoPath }
}

$node = Get-Command node -ErrorAction SilentlyContinue
if (-not $node) {
    throw "node is required for the local mock upstream"
}

$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("anno-privacy-gateway-v03-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tempDir | Out-Null
$capturePath = Join-Path $tempDir "upstream-request.json"
$mockPath = Join-Path $tempDir "mock-upstream.js"
$gatewayOut = Join-Path $tempDir "gateway.out.log"
$gatewayErr = Join-Path $tempDir "gateway.err.log"
$mockOut = Join-Path $tempDir "mock.out.log"
$mockErr = Join-Path $tempDir "mock.err.log"

$mockSource = @'
const http = require("http");
const fs = require("fs");

const port = Number(process.argv[2]);
const capturePath = process.argv[3];

const server = http.createServer((req, res) => {
  let body = "";
  req.on("data", chunk => { body += chunk; });
  req.on("end", () => {
    if (req.url === "/v1/messages" && req.method === "POST") {
      fs.writeFileSync(capturePath, body, "utf8");
      const token = (body.match(/PERSON_[A-Za-z0-9_-]+/) || ["PERSON_1"])[0];
      const response = {
        content: [{
          type: "text",
          text: `Bonjour ${token}. Fuite test: Jean Martin jean.martin@example.com`
        }]
      };
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(response));
      return;
    }

    if (req.url === "/v1/models" && req.method === "GET") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ data: [{ id: "mock-model" }] }));
      return;
    }

    res.writeHead(404, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "not found" }));
  });
});

server.listen(port, "127.0.0.1", () => {
  console.log(`mock upstream listening on ${port}`);
});
'@
Set-Content -LiteralPath $mockPath -Value $mockSource -Encoding UTF8

$mockProcess = $null
$gatewayProcess = $null

function Wait-HttpOk {
    param([string]$Url)
    for ($i = 0; $i -lt 60; $i++) {
        try {
            Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 2 | Out-Null
            return
        } catch {
            Start-Sleep -Milliseconds 500
        }
    }
    throw "Timed out waiting for $Url"
}

try {
    Push-Location $repoRoot

    & $cargo.Source build -p anno-privacy-gateway
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed"
    }

    $targetDir = $env:CARGO_TARGET_DIR
    if ([string]::IsNullOrWhiteSpace($targetDir)) {
        $targetDir = Join-Path $repoRoot "target"
    }
    $gatewayBin = Join-Path $targetDir "debug\anno-privacy-gateway.exe"
    if (-not (Test-Path -LiteralPath $gatewayBin)) {
        throw "gateway binary not found at $gatewayBin"
    }

    $mockProcess = Start-Process `
        -FilePath $node.Source `
        -ArgumentList @($mockPath, $UpstreamPort, $capturePath) `
        -RedirectStandardOutput $mockOut `
        -RedirectStandardError $mockErr `
        -PassThru `
        -WindowStyle Hidden

    Start-Sleep -Milliseconds 500
    if ($mockProcess.HasExited) {
        throw "mock upstream exited early: $(Get-Content -LiteralPath $mockErr -Raw)"
    }

    $env:ANNO_GATEWAY_LISTEN = "127.0.0.1:$GatewayPort"
    $env:ANNO_GATEWAY_UPSTREAM_ANTHROPIC_BASE = "http://127.0.0.1:$UpstreamPort"
    $env:ANNO_GATEWAY_PROVIDER_PROFILE = "global_anonymized"

    $gatewayProcess = Start-Process `
        -FilePath $gatewayBin `
        -RedirectStandardOutput $gatewayOut `
        -RedirectStandardError $gatewayErr `
        -PassThru `
        -WindowStyle Hidden

    Wait-HttpOk "http://127.0.0.1:$GatewayPort/health"

    $request = @{
        model = "claude-smoke"
        messages = @(@{
            role = "user"
            content = "Bonjour Marie Dupont, contactez marie.dupont@example.com"
        })
    } | ConvertTo-Json -Depth 8 -Compress

    $response = Invoke-WebRequest `
        -Uri "http://127.0.0.1:$GatewayPort/v1/messages" `
        -Method POST `
        -ContentType "application/json" `
        -Body $request `
        -UseBasicParsing

    $captured = Get-Content -LiteralPath $capturePath -Raw
    if ($captured -match "Marie Dupont" -or $captured -match "marie\.dupont@example\.com") {
        throw "cleartext PII reached upstream: $captured"
    }
    if ($captured -notmatch "PERSON_" -or $captured -notmatch "EMAIL_") {
        throw "upstream request did not contain expected pseudonyms: $captured"
    }

    $body = $response.Content | ConvertFrom-Json
    $text = [string]$body.content[0].text
    if ($text -notmatch "Marie Dupont") {
        throw "known pseudonym was not rehydrated in response: $text"
    }
    if ($text -match "Jean Martin" -or $text -match "jean\.martin@example\.com") {
        throw "fresh model PII leak was not redacted: $text"
    }

    $redactedHeader = [string]$response.Headers["X-Anno-PII-Leak-Redacted"]
    if ($redactedHeader -ne "2") {
        throw "expected X-Anno-PII-Leak-Redacted=2, got '$redactedHeader'"
    }

    $filesStatus = $null
    try {
        Invoke-WebRequest `
            -Uri "http://127.0.0.1:$GatewayPort/v1/files" `
            -Method POST `
            -UseBasicParsing | Out-Null
    } catch {
        $filesStatus = $_.Exception.Response.StatusCode.value__
    }
    if ($filesStatus -ne 400) {
        throw "expected POST /v1/files to fail closed with 400, got $filesStatus"
    }

    Write-Host "[privacy-gateway-v0.3] PASS"
    Write-Host "  gateway:  http://127.0.0.1:$GatewayPort"
    Write-Host "  upstream captured only pseudonyms"
    Write-Host "  response rehydrated known tokens and redacted fresh PII"
} finally {
    Pop-Location
    if ($gatewayProcess -and -not $gatewayProcess.HasExited) {
        Stop-Process -Id $gatewayProcess.Id -Force
    }
    if ($mockProcess -and -not $mockProcess.HasExited) {
        Stop-Process -Id $mockProcess.Id -Force
    }
    Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}
