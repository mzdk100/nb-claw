# nb-claw Windows Packaging Script
# Requires PowerShell 5.1+ (for Expand-Archive and Compress-Archive)

$ErrorActionPreference = "Stop"

$PYTHON_VERSION = "3.13.12"
$PYTHON_URL = "https://www.python.org/ftp/python/$PYTHON_VERSION/python-$PYTHON_VERSION-embed-amd64.zip"
$PACKAGE_DIR = "nb-claw"
$TARGET_ZIP = "target/nb-claw-windows-x64.zip"
$DIST_EXE = "target/release/nb-claw.exe"

Write-Host "=== nb-claw Packaging Script ===" -ForegroundColor Cyan

# Step 1: Remove existing nb-claw directory if exists
if (Test-Path $PACKAGE_DIR) {
    Write-Host "[1/11] Removing existing $PACKAGE_DIR directory..." -ForegroundColor Yellow
    Remove-Item -Path $PACKAGE_DIR -Recurse -Force
}

# Step 2: Download Python (build starts in background)
Write-Host "[2/11] Downloading Python $PYTHON_VERSION embeddable package..." -ForegroundColor Yellow
$pythonZip = "python-embed.zip"

# Start build job in background
$buildJob = Start-Job -ScriptBlock {
    Set-Location $using:PSScriptRoot
    cargo build --release 2>&1
    $LASTEXITCODE
}

# Download Python (blocking)
try {
    Invoke-WebRequest -Uri $PYTHON_URL -OutFile $pythonZip -UseBasicParsing
} catch {
    Write-Host "Failed to download Python: $_" -ForegroundColor Red
    Remove-Job $buildJob -Force -ErrorAction SilentlyContinue
    exit 1
}

# Step 3: Extract to nb-claw directory
Write-Host "[3/11] Extracting Python to $PACKAGE_DIR..." -ForegroundColor Yellow
Expand-Archive -Path $pythonZip -DestinationPath $PACKAGE_DIR -Force

# Step 4: Remove downloaded zip
Write-Host "[4/11] Removing downloaded Python zip..." -ForegroundColor Yellow
Remove-Item -Path $pythonZip -Force

# Step 5: Remove Python's .exe files
Write-Host "[5/11] Removing Python's .exe files..." -ForegroundColor Yellow
Get-ChildItem -Path $PACKAGE_DIR -Filter "*.exe" | Remove-Item -Force

# Step 6: Wait for build to complete
Write-Host "[6/11] Waiting for build to complete..." -ForegroundColor Yellow
$buildResult = $buildJob | Wait-Job | Receive-Job
Remove-Job $buildJob

if ($buildResult[-1] -ne 0) {
    Write-Host "Build failed!" -ForegroundColor Red
    exit 1
}

# Step 7: Copy executable to nb-claw
Write-Host "[7/11] Copying executable to $PACKAGE_DIR..." -ForegroundColor Yellow
Copy-Item -Path $DIST_EXE -Destination $PACKAGE_DIR

# Step 8: Initialize config
Write-Host "[8/11] Initializing config..." -ForegroundColor Yellow
Push-Location $PACKAGE_DIR
try {
    ./nb-claw.exe --init-config
} catch {
    Write-Host "Config initialization failed: $_" -ForegroundColor Red
    Pop-Location
    exit 1
}
Pop-Location

# Step 9: Copy documentation files
Write-Host "[9/11] Copying documentation files..." -ForegroundColor Yellow
Copy-Item -Path "README.md" -Destination $PACKAGE_DIR -ErrorAction SilentlyContinue
Copy-Item -Path "CHANGELOGS.md" -Destination $PACKAGE_DIR -ErrorAction SilentlyContinue
Copy-Item -Path "CONFIG_GUIDE.md" -Destination $PACKAGE_DIR -ErrorAction SilentlyContinue

# Step 10: Create zip package
Write-Host "[10/11] Creating zip package..." -ForegroundColor Yellow
# Ensure target directory exists
$targetDir = Split-Path $TARGET_ZIP -Parent
if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir -Force | Out-Null
}
# Remove existing zip if exists
if (Test-Path $TARGET_ZIP) {
    Remove-Item -Path $TARGET_ZIP -Force
}
Compress-Archive -Path $PACKAGE_DIR -DestinationPath $TARGET_ZIP -CompressionLevel Optimal

# Step 11: Remove nb-claw directory if zip created successfully
if (Test-Path $TARGET_ZIP) {
    Write-Host "[11/11] Removing $PACKAGE_DIR directory..." -ForegroundColor Yellow
    Remove-Item -Path $PACKAGE_DIR -Recurse -Force
    Write-Host "=== Package created: $TARGET_ZIP ===" -ForegroundColor Green
} else {
    Write-Host "Failed to create zip package!" -ForegroundColor Red
    exit 1
}

Write-Host "Done!" -ForegroundColor Green
