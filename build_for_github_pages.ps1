# Warning AI generated
# Exit on error
$ErrorActionPreference = "Stop"

$ScriptDir = $PSScriptRoot

Write-Host "[1/4] Building dictionary data..." -ForegroundColor Yellow
Set-Location "$ScriptDir\console"
cargo run -- build no_query
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Dictionary build failed" -ForegroundColor Red
    exit 1
}
Write-Host "Dictionary data built successfully!" -ForegroundColor Green
Write-Host ""

Write-Host "[2/4] Building web client..." -ForegroundColor Yellow
Set-Location "$ScriptDir\web_client"
npm run build
if ($LASTEXITCODE -ne 0) {
    Write-Host "Error: Web client build failed" -ForegroundColor Red
    exit 1
}
Write-Host "Web client built successfully!" -ForegroundColor Green
Write-Host ""

Write-Host "[3/4] Preparing docs directory..." -ForegroundColor Yellow
Set-Location $ScriptDir

if (Test-Path "docs") {
    Remove-Item -Path "docs" -Recurse -Force
    Write-Host "Removed old docs directory" -ForegroundColor Gray
}

New-Item -ItemType Directory -Path "docs" | Out-Null
Write-Host "Created docs directory" -ForegroundColor Green
Write-Host ""

Write-Host "[4/4] Copying build output to docs..." -ForegroundColor Yellow
Copy-Item -Path "web_client\dist\*" -Destination "docs\" -Recurse -Force
Write-Host "Build output copied to docs directory!" -ForegroundColor Green
Write-Host ""

Write-Host "Removing duplicate files..."
Remove-Item -Path "docs\*.txt"
Remove-Item -Path "docs\*.u8"
Write-Host "Done"

Write-Host "Docs directory contents:" -ForegroundColor Cyan
Get-ChildItem -Path "docs" | ForEach-Object { Write-Host "  - $($_.Name)" -ForegroundColor Gray }
Write-Host ""

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Build complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan