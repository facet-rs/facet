@echo off
setlocal

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

if not exist "%SUBJECT_RUST_BIN%" (
    cargo build -p subject-rust
    if errorlevel 1 exit /b %ERRORLEVEL%
)

if not exist "%SUBJECT_GENERATED_PKG%" (
    where pnpm >nul 2>nul
    if errorlevel 1 (
        where corepack >nul 2>nul
        if errorlevel 1 (
            echo setup-vox-typescript: pnpm is required to run Vox TypeScript spec subjects 1>&2
            exit /b 127
        )
        corepack enable pnpm
    )

    pnpm --dir "%VOX_DIR%" install --frozen-lockfile
    if errorlevel 1 exit /b %ERRORLEVEL%
)

exit /b 0
