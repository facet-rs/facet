@echo off
setlocal EnableExtensions EnableDelayedExpansion

set "SCRIPT_DIR=%~dp0"
set "REPO_ROOT=%SCRIPT_DIR%.."
set "VOX_DIR=%REPO_ROOT%\vox"
set "SUBJECT_GENERATED_PKG=%VOX_DIR%\typescript\subject\node_modules\@bearcove\vox-generated\package.json"
if defined CARGO_TARGET_DIR (
    set "TARGET_DIR=%CARGO_TARGET_DIR%"
) else (
    set "TARGET_DIR=%REPO_ROOT%\target"
)
set "SUBJECT_RUST_BIN=%TARGET_DIR%\debug\subject-rust.exe"
if defined VOX_PNPM_VERSION (
    set "PNPM_VERSION=%VOX_PNPM_VERSION%"
) else (
    set "PNPM_VERSION=11.7.0"
)

if not exist "%SUBJECT_RUST_BIN%" (
    cargo build -p subject-rust
    if errorlevel 1 exit /b %ERRORLEVEL%
)

if not exist "%SUBJECT_GENERATED_PKG%" (
    if defined PNPM (
        set "PNPM_CMD=!PNPM!"
    ) else (
        set "PNPM_CMD="
    )

    if not defined PNPM_CMD (
        where pnpm >nul 2>nul
        if not errorlevel 1 set "PNPM_CMD=pnpm"
    )

    if not defined PNPM_CMD (
        where corepack >nul 2>nul
        if not errorlevel 1 (
            corepack prepare "pnpm@!PNPM_VERSION!" --activate
            if errorlevel 1 exit /b %ERRORLEVEL%
            where pnpm >nul 2>nul
            if not errorlevel 1 set "PNPM_CMD=pnpm"
        )
    )

    if not defined PNPM_CMD (
        where npm >nul 2>nul
        if not errorlevel 1 (
            set "LOCAL_PNPM_ROOT=!TARGET_DIR!\vox-pnpm"
            set "PNPM_CMD=!TARGET_DIR!\vox-pnpm\node_modules\.bin\pnpm.cmd"
            if not exist "!PNPM_CMD!" (
                npm install --prefix "!LOCAL_PNPM_ROOT!" "pnpm@!PNPM_VERSION!"
                if errorlevel 1 exit /b %ERRORLEVEL%
            )
        )
    )

    if not defined PNPM_CMD (
        echo setup-vox-typescript: pnpm is required; install pnpm, corepack, or npm 1>&2
        exit /b 127
    )

    "!PNPM_CMD!" --dir "%VOX_DIR%" install --frozen-lockfile
    if errorlevel 1 exit /b %ERRORLEVEL%
)

if defined NEXTEST_ENV (
    set "NODE_DIR_FOR_NEXTEST="
    for /f "delims=" %%I in ('where node 2^>nul') do (
        if not defined NODE_DIR_FOR_NEXTEST set "NODE_DIR_FOR_NEXTEST=%%~dpI"
    )
    if defined NODE_DIR_FOR_NEXTEST >>"!NEXTEST_ENV!" echo PATH=!NODE_DIR_FOR_NEXTEST!;!PATH!
)

exit /b 0
