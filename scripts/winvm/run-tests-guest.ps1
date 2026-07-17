# Runs the cross-built nextest archive on the guest. Invoked over SSH by
# run-tests.sh. Any extra args are forwarded to `nextest run` (default is the
# CI exclusion set below).
#
# We call tools by absolute path and fix up $env:Path in-process rather than
# trusting the machine PATH — sshd snapshots its environment at service start,
# so PATH edits from provisioning aren't visible to SSH sessions.
param([Parameter(ValueFromRemainingArguments = $true)] $Extra)

$ErrorActionPreference = 'Stop'
$root    = 'C:\facet'
$nextest = Join-Path $root 'bin\cargo-nextest.exe'
$archive = Join-Path $root 'facet-win.tar.zst'
$src     = Join-Path $root 'src'

$nodeDir = Get-ChildItem $root -Directory -Filter 'node-*-win-x64' | Select-Object -First 1
if ($nodeDir) { $env:Path = "$($nodeDir.FullName);$env:Path" }
$env:Path = "$root\bin;$env:Path"

# Match the CI Windows lane (test-platforms.yml).
$env:PROPTEST_CASES = '0'
$filter = 'not (binary(postgres_integration) | binary(query_integration) | test(swift) | test(typescript))'

& $nextest nextest run `
    --archive-file $archive `
    --workspace-remap $src `
    --no-fail-fast `
    -E $filter `
    @Extra

exit $LASTEXITCODE
