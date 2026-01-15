@echo off
REM Exit with 1 if NEXTEST_ENV isn't defined
if "%NEXTEST_ENV%"=="" exit /b 1

REM Disable colors for consistent snapshot tests
echo NO_COLOR=1>> "%NEXTEST_ENV%"
