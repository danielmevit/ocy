<#
    Builds the Windows MSIX package for Ocy into builds\store\.

    KEEP THIS FILE PURE ASCII. Windows PowerShell 5.1 reads a BOM-less .ps1 as ANSI
    (cp1252), so non-ASCII punctuation in strings can fail to parse.

    Usage:
        powershell -ExecutionPolicy Bypass -File packaging\windows\build_msix.ps1 -Store
        powershell -ExecutionPolicy Bypass -File packaging\windows\build_msix.ps1 -Store -PdbPath C:\path\to\Ocy.pdb
        powershell -ExecutionPolicy Bypass -File packaging\windows\build_msix.ps1 -SignForSideload C:\path\to\test.pfx
#>
param(
    [string]$ExePath,
    [string]$NoticesPath,
    [string]$SignForSideload,
    [string]$CertPassword,
    [string]$PdbPath,
    [switch]$Store,
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
Push-Location $root

try {
    if ($Store -and $SignForSideload) {
        Write-Error "Cannot combine -Store and -SignForSideload: Microsoft Store packages must be uploaded unsigned."
    }
    if ($PdbPath -and -not $Store) {
        Write-Error "-PdbPath is only supported with -Store."
    }

    # -- 1. Version from rust/Cargo.toml --
    $cargoTomlPath = Join-Path $root "rust\Cargo.toml"
    if (-not (Test-Path $cargoTomlPath)) {
        Write-Error "Missing $cargoTomlPath"
    }
    $cargoToml = Get-Content $cargoTomlPath -Raw
    $wsMatch = [regex]::Match($cargoToml, '(?ms)^\[workspace\.package\](.*?)(?=^\[|\z)')
    if (-not $wsMatch.Success) {
        Write-Error "Could not find [workspace.package] in rust\Cargo.toml"
    }
    $verMatch = [regex]::Match($wsMatch.Groups[1].Value, '(?m)^\s*version\s*=\s*"([^"]+)"')
    if (-not $verMatch.Success) {
        Write-Error "Could not find workspace.package.version in rust\Cargo.toml"
    }
    $rawVersion = $verMatch.Groups[1].Value.Trim()
    $baseVersion = ($rawVersion -split '[-+]', 2)[0]
    if ($baseVersion -notmatch '^\d+\.\d+\.\d+$') {
        Write-Error "Expected three-part SemVer base in rust\Cargo.toml (got '$rawVersion')"
    }
    $msixVersion = "$baseVersion.0"
    Write-Host "=== Ocy $rawVersion (MSIX $msixVersion) ===" -ForegroundColor Cyan

    # -- 2. Locate or build Ocy.exe --
    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "rust\target" }
    if (-not $ExePath) {
        $candidates = @(
            (Join-Path $targetDir "release\Ocy.exe"),
            (Join-Path $targetDir "x86_64-pc-windows-msvc\release\Ocy.exe")
        )
        $ExePath = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
        if (-not $ExePath) {
            $ExePath = $candidates[0]
        }
    }

    if (-not (Test-Path $ExePath)) {
        if ($SkipBuild) {
            Write-Error "Missing Ocy.exe at $ExePath (and -SkipBuild was passed)"
        }
        Write-Host "`n--- Building ocy-app (release) ---" -ForegroundColor Cyan
        Push-Location (Join-Path $root "rust")
        try {
            & cargo build -p ocy-app --release --locked
            if ($LASTEXITCODE -ne 0) {
                Write-Error "cargo build -p ocy-app --release --locked failed"
            }
        } finally {
            Pop-Location
        }
        if (-not (Test-Path $ExePath)) {
            $altExe = Join-Path $targetDir "x86_64-pc-windows-msvc\release\Ocy.exe"
            if (Test-Path $altExe) {
                $ExePath = $altExe
            } else {
                Write-Error "Build finished but Ocy.exe was not found at $ExePath"
            }
        }
    }
    Write-Host "[ok] Executable: $ExePath" -ForegroundColor Green

    # -- 3. Find Windows 10 SDK (makeappx / signtool / makepri) --
    $kitsRoot = "${env:ProgramFiles(x86)}\Windows Kits\10\bin"
    $sdk = Get-ChildItem $kitsRoot -Directory -ErrorAction SilentlyContinue |
        Where-Object { Test-Path "$($_.FullName)\x64\makeappx.exe" } |
        Sort-Object {
            $parsed = $null
            if ([version]::TryParse($_.Name, [ref]$parsed)) { $parsed } else { [version]"0.0.0.0" }
        } -Descending |
        Select-Object -First 1
    if (-not $sdk) {
        Write-Error "Windows 10 SDK makeappx.exe not found under $kitsRoot"
    }
    $sdkBin = Join-Path $sdk.FullName "x64"
    $makeappx = Join-Path $sdkBin "makeappx.exe"
    $signtool = Join-Path $sdkBin "signtool.exe"
    $makepri = Join-Path $sdkBin "makepri.exe"
    Write-Host "[ok] Windows SDK: $($sdk.Name)" -ForegroundColor Green

    # -- 4. Stage files --
    $outDir = Join-Path $root "builds\store"
    $stageDir = Join-Path $outDir "_stage"
    if (Test-Path $stageDir) {
        Remove-Item $stageDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
    New-Item -ItemType Directory -Path $stageDir -Force | Out-Null

    Copy-Item $ExePath (Join-Path $stageDir "Ocy.exe")

    $licenseSrc = Join-Path $root "LICENSE"
    if (-not (Test-Path $licenseSrc)) {
        Write-Error "Missing LICENSE at $licenseSrc"
    }
    Copy-Item $licenseSrc (Join-Path $stageDir "LICENSE.txt")

    $stagedNotices = Join-Path $stageDir "THIRD-PARTY-NOTICES.txt"
    if ($NoticesPath) {
        if (-not (Test-Path $NoticesPath)) {
            Write-Error "Missing notices file at $NoticesPath"
        }
        Copy-Item $NoticesPath $stagedNotices
    } else {
        $python = if ($env:PYTHON) { $env:PYTHON } else { "python" }
        $noticesScript = Join-Path $root "rust\tools\notices.py"
        Write-Host "`n--- Generating THIRD-PARTY-NOTICES.txt ---" -ForegroundColor Cyan
        & $python $noticesScript --out $stagedNotices
        if ($LASTEXITCODE -ne 0) {
            Write-Error "notices.py failed"
        }
    }

    $assetsSrc = Join-Path $PSScriptRoot "assets"
    $assetsDst = Join-Path $stageDir "assets"
    if (-not (Test-Path $assetsSrc)) {
        Write-Error "Missing MSIX logo assets at $assetsSrc"
    }
    New-Item -ItemType Directory -Path $assetsDst -Force | Out-Null
    Copy-Item (Join-Path $assetsSrc "*") $assetsDst

    # -- 5. Resolve manifest placeholders and stamp AppxManifest.xml --
    $manifestPath = Join-Path $PSScriptRoot "AppxManifest.xml"
    $manifest = Get-Content $manifestPath -Raw

    if ($Store) {
        foreach ($required in @("OCY_STORE_IDENTITY_NAME", "OCY_STORE_PUBLISHER", "OCY_STORE_PUBLISHER_DISPLAY")) {
            if (-not (Get-Item "env:$required" -ErrorAction SilentlyContinue).Value) {
                Write-Error "-Store requires $required (see Partner Center -> Product management -> Product identity)"
            }
        }
        $identityName = $env:OCY_STORE_IDENTITY_NAME
        $publisher = $env:OCY_STORE_PUBLISHER
        $publisherDisplay = $env:OCY_STORE_PUBLISHER_DISPLAY
        $displayName = if ($env:OCY_STORE_DISPLAY_NAME) { $env:OCY_STORE_DISPLAY_NAME } else { "Ocy" }
    } elseif ($SignForSideload) {
        if (-not (Test-Path $SignForSideload)) {
            Write-Error "Sideload certificate not found at $SignForSideload"
        }
        $pw = if ($CertPassword) { $CertPassword } elseif ($env:OCY_CERT_PASSWORD) { $env:OCY_CERT_PASSWORD } else { "" }
        $cert = New-Object System.Security.Cryptography.X509Certificates.X509Certificate2($SignForSideload, $pw)
        $identityName = if ($env:OCY_STORE_IDENTITY_NAME) { $env:OCY_STORE_IDENTITY_NAME } else { "Ocy.SideloadTest" }
        $publisher = if ($env:OCY_STORE_PUBLISHER) { $env:OCY_STORE_PUBLISHER } else { $cert.Subject }
        $publisherDisplay = if ($env:OCY_STORE_PUBLISHER_DISPLAY) { $env:OCY_STORE_PUBLISHER_DISPLAY } else { "Ocy Local Test" }
        $displayName = if ($env:OCY_STORE_DISPLAY_NAME) { $env:OCY_STORE_DISPLAY_NAME } else { "Ocy" }
    } else {
        $identityName = if ($env:OCY_STORE_IDENTITY_NAME) { $env:OCY_STORE_IDENTITY_NAME } else { "Ocy.LocalTest" }
        $publisher = if ($env:OCY_STORE_PUBLISHER) { $env:OCY_STORE_PUBLISHER } else { "CN=OcyLocalTest" }
        $publisherDisplay = if ($env:OCY_STORE_PUBLISHER_DISPLAY) { $env:OCY_STORE_PUBLISHER_DISPLAY } else { "Ocy Local Test" }
        $displayName = if ($env:OCY_STORE_DISPLAY_NAME) { $env:OCY_STORE_DISPLAY_NAME } else { "Ocy" }
        Write-Host "[warn] Using placeholder local identity ($identityName). Pass -Store with Partner Center env vars for Store upload." -ForegroundColor Yellow
    }

    $replacements = @{
        "__IDENTITY_NAME__"          = [System.Security.SecurityElement]::Escape($identityName)
        "__PUBLISHER__"              = [System.Security.SecurityElement]::Escape($publisher)
        "__PUBLISHER_DISPLAY_NAME__" = [System.Security.SecurityElement]::Escape($publisherDisplay)
        "__DISPLAY_NAME__"           = [System.Security.SecurityElement]::Escape($displayName)
        "__VERSION__"                = $msixVersion
    }
    foreach ($entry in $replacements.GetEnumerator()) {
        $manifest = $manifest.Replace($entry.Key, $entry.Value)
    }
    if ($manifest -match '__[A-Z0-9_]+__') {
        Write-Error "Unreplaced manifest placeholder remains in AppxManifest.xml"
    }

    $stagedManifest = Join-Path $stageDir "AppxManifest.xml"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($stagedManifest, $manifest, $utf8NoBom)

    # -- 6. Generate resources.pri when makepri.exe is available --
    if (Test-Path $makepri) {
        Write-Host "`n--- Generating resources.pri ---" -ForegroundColor Cyan
        $priConfig = Join-Path $stageDir "priconfig.xml"
        $priOut = Join-Path $stageDir "resources.pri"
        & $makepri createconfig /cf $priConfig /dq en-US /o
        if ($LASTEXITCODE -ne 0) { Write-Error "makepri createconfig failed" }
        & $makepri new /pr $stageDir /cf $priConfig /mn $stagedManifest /of $priOut /o
        if ($LASTEXITCODE -ne 0) { Write-Error "makepri new failed" }
        Remove-Item $priConfig -Force -ErrorAction SilentlyContinue
    }

    # -- 7. Pack MSIX --
    $variant = if ($Store) { "store" } elseif ($SignForSideload) { "sideload" } else { "unsigned" }
    $msixPath = Join-Path $outDir "Ocy-$rawVersion-$variant.msix"
    if (Test-Path $msixPath) {
        Remove-Item $msixPath -Force
    }
    Write-Host "`n--- Packing $msixPath ---" -ForegroundColor Cyan
    & $makeappx pack /d $stageDir /p $msixPath /o
    if ($LASTEXITCODE -ne 0) {
        Write-Error "makeappx pack failed"
    }

    # -- 8. Optional Store symbol packages --
    if ($Store -and $PdbPath) {
        $resolvedPdb = Resolve-Path $PdbPath -ErrorAction Stop
        $pdbFile = Get-Item $resolvedPdb

        $appxsymPath = Join-Path $outDir "Ocy-$rawVersion-store.appxsym"
        $msixuploadPath = Join-Path $outDir "Ocy-$rawVersion-store.msixupload"
        $appxsymStage = Join-Path $outDir "_appxsym"
        $uploadStage = Join-Path $outDir "_msixupload"
        foreach ($path in @($appxsymStage, $uploadStage)) {
            if (Test-Path $path) {
                Remove-Item $path -Recurse -Force
            }
        }
        foreach ($path in @($appxsymPath, $msixuploadPath)) {
            if (Test-Path $path) {
                Remove-Item $path -Force
            }
        }

        New-Item -ItemType Directory -Path $appxsymStage -Force | Out-Null
        Copy-Item $pdbFile.FullName (Join-Path $appxsymStage "Ocy.pdb")
        # Microsoft Partner Center package upload accepts a .msixupload containing
        # the unsigned .msix and a .appxsym symbol package.
        # Format reference: https://learn.microsoft.com/en-us/windows/apps/publish/upload-app-packages
        # Compress-Archive only writes .zip: zip first, then rename to .appxsym.
        Compress-Archive -Path (Join-Path $appxsymStage "*") -DestinationPath "$appxsymPath.zip" -Force
        Move-Item -LiteralPath "$appxsymPath.zip" -Destination $appxsymPath -Force

        New-Item -ItemType Directory -Path $uploadStage -Force | Out-Null
        Copy-Item $msixPath (Join-Path $uploadStage (Split-Path $msixPath -Leaf))
        Copy-Item $appxsymPath (Join-Path $uploadStage (Split-Path $appxsymPath -Leaf))
        Compress-Archive -Path (Join-Path $uploadStage "*") -DestinationPath "$msixuploadPath.zip" -Force
        Move-Item -LiteralPath "$msixuploadPath.zip" -Destination $msixuploadPath -Force

        Remove-Item $appxsymStage -Recurse -Force
        Remove-Item $uploadStage -Recurse -Force
        Write-Host "[ok] Store symbols: $appxsymPath" -ForegroundColor Green
        Write-Host "[ok] Store upload package: $msixuploadPath" -ForegroundColor Green
    }

    # -- 9. Optional local sideload signing --
    if ($SignForSideload) {
        if (-not (Test-Path $signtool)) {
            Write-Error "signtool.exe not found at $signtool"
        }
        $pw = if ($CertPassword) { $CertPassword } elseif ($env:OCY_CERT_PASSWORD) { $env:OCY_CERT_PASSWORD } else { $null }
        Write-Host "`n--- Signing for local sideload testing ---" -ForegroundColor Cyan
        if ($pw) {
            & $signtool sign /fd SHA256 /f $SignForSideload /p $pw $msixPath
        } else {
            & $signtool sign /fd SHA256 /f $SignForSideload $msixPath
        }
        if ($LASTEXITCODE -ne 0) {
            Write-Error "signtool sign failed"
        }
        Write-Host "[ok] Signed sideload MSIX: $msixPath" -ForegroundColor Green
    } elseif ($Store) {
        Write-Host "[ok] Store MSIX (unsigned, ready for Partner Center): $msixPath" -ForegroundColor Green
    } else {
        Write-Host "[ok] Unsigned test MSIX: $msixPath" -ForegroundColor Green
    }
} finally {
    $stageDir = Join-Path $root "builds\store\_stage"
    if (Test-Path $stageDir) {
        Remove-Item $stageDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    Pop-Location
}
