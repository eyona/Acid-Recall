# Pinned GPLv3 ASIO SDK for Windows builds. Dot-source to keep the environment locally.
$ErrorActionPreference = 'Stop'
$asioBuildRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\.build-tools\asio'))
$asioArchive = Join-Path $asioBuildRoot 'ASIO-SDK_2.3.4_2025-10-15.zip'
$asioSdkRoot = Join-Path $asioBuildRoot 'sdk\ASIOSDK'
New-Item -ItemType Directory -Force -Path $asioBuildRoot | Out-Null
if (-not (Test-Path -LiteralPath $asioArchive)) {
    Invoke-WebRequest -Uri 'https://download.steinberg.net/sdk_downloads/ASIO-SDK_2.3.4_2025-10-15.zip' -OutFile $asioArchive
}
$asioExpectedHash = 'D5EBF0C20DD2C5F43771FD0C1418F4B361BF52434EE670097CFA6B3A335E2ECA'
if ((Get-FileHash -LiteralPath $asioArchive -Algorithm SHA256).Hash -ne $asioExpectedHash) {
    throw 'ASIO SDK archive checksum mismatch.'
}
if (-not (Test-Path -LiteralPath (Join-Path $asioSdkRoot 'common\asio.h'))) {
    Expand-Archive -LiteralPath $asioArchive -DestinationPath (Join-Path $asioBuildRoot 'sdk') -Force
}
$env:CPAL_ASIO_DIR = $asioSdkRoot
if (-not $env:LIBCLANG_PATH) {
    foreach ($asioClangDir in @('C:\Program Files\LLVM\bin', 'C:\Program Files\LLVM\lib',
        (Join-Path $PSScriptRoot '..\.build-tools\libclang\clang\native'))) {
        if (Test-Path -LiteralPath (Join-Path $asioClangDir 'libclang.dll')) {
            $env:LIBCLANG_PATH = $asioClangDir
            break
        }
    }
}
if (-not $env:LIBCLANG_PATH) {
    $asioClangPackage = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\.build-tools\libclang'))
    & python -m pip install --no-deps --no-cache-dir --target $asioClangPackage libclang==18.1.1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot prepare LLVM; set LIBCLANG_PATH to an installed libclang.dll.' }
    $env:LIBCLANG_PATH = Join-Path $asioClangPackage 'clang\native'
}
if ($env:GITHUB_ENV) {
    "CPAL_ASIO_DIR=$env:CPAL_ASIO_DIR" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
    "LIBCLANG_PATH=$env:LIBCLANG_PATH" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
}
Write-Output 'Pinned ASIO SDK and LLVM configured.'
