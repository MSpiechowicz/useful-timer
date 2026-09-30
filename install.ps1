# Per-user installer; works with Windows PowerShell 5.1 and PowerShell 7.
param(
    [string]$Version = '',
    [string]$InstallDir = '',
    [switch]$Help,
    # In-app updater: wait for normal exit (including settings save), then relaunch.
    [int]$WaitForProcessId = 0,
    [switch]$Restart,
    [switch]$SelfRemove,
    [string]$ReadyFile = ''
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($Help) {
    Write-Output 'Usage: install.ps1 [-Version VERSION] [-InstallDir ABSOLUTE_PATH]'
    return
}
if ($env:OS -ne 'Windows_NT') { throw 'This installer requires Windows.' }
$architecture = $env:PROCESSOR_ARCHITEW6432
if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }
if ($architecture -ne 'AMD64') { throw "Unsupported Windows architecture: $architecture. Releases support x86-64." }
if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\Useful Timer' }
if (-not [IO.Path]::IsPathRooted($InstallDir)) { throw '-InstallDir must be an absolute path.' }
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
$work = $null
$staged = $null
$destination = Join-Path $InstallDir 'useful-timer.exe'
$handoffStarted = $false
try {
    # Windows PowerShell 5.1 may otherwise negotiate obsolete TLS versions.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $repo = 'https://github.com/MSpiechowicz/useful-timer'
    if (-not $Version) {
        try {
            $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/MSpiechowicz/useful-timer/releases/latest' -Headers @{ 'User-Agent' = 'useful-timer-installer' }
            $Version = $release.tag_name
        } catch {
            throw "Cannot resolve the latest release. Check your connection and whether a release has been published. $($_.Exception.Message)"
        }
    }
    $Version = $Version -replace '^v', ''
    if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Invalid release version: $Version" }
    $asset = "useful-timer-$Version-x86_64-pc-windows-msvc.zip"
    $work = Join-Path ([IO.Path]::GetTempPath()) ('useful-timer-' + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $work | Out-Null
    Write-Output "Downloading Useful Timer $Version (Windows x86-64)..."
    foreach ($file in @($asset, "$asset.sha256")) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$repo/releases/download/v$Version/$file" -OutFile (Join-Path $work $file)
        } catch {
            throw "Cannot download $file. This release must include the archive and its checksum. $($_.Exception.Message)"
        }
    }
    $checksum = (Get-Content -LiteralPath (Join-Path $work "$asset.sha256") -Raw).Trim()
    if ($checksum -notmatch ('^([0-9a-fA-F]{64})\s+' + [regex]::Escape($asset) + '$')) { throw 'Invalid checksum file.' }
    $expected = $Matches[1]
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $work $asset)).Hash
    if ($actual -ne $expected) { throw 'SHA-256 checksum mismatch; nothing was installed.' }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $work $asset))
    try {
        foreach ($file in @('useful-timer.exe', 'LICENSE', 'README.md')) {
            $entry = $zip.GetEntry($file)
            if (-not $entry -or $entry.Length -eq 0) { throw "Missing or empty archive member: $file" }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $work $file))
        }
    } finally { $zip.Dispose() }

    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    $staged = Join-Path $InstallDir ('.useful-timer-' + [Guid]::NewGuid() + '.exe')
    Copy-Item -LiteralPath (Join-Path $work 'useful-timer.exe') -Destination $staged
    # Verify and stage while the app remains open, so checksum failures can be
    # shown as toasts. Only request shutdown once replacement is ready.
    if ($ReadyFile) { [IO.File]::WriteAllText($ReadyFile, 'verified') }
    $handoffStarted = $true
    if ($WaitForProcessId -gt 0) {
        $running = Get-Process -Id $WaitForProcessId -ErrorAction SilentlyContinue
        if ($running) { $running | Wait-Process -ErrorAction Stop }
    }
    try {
        if (Test-Path -LiteralPath $destination) {
            [IO.File]::Replace($staged, $destination, [NullString]::Value)
        } else {
            [IO.File]::Move($staged, $destination)
        }
    } catch {
        throw "Cannot replace Useful Timer. Close the app before updating. $($_.Exception.Message)"
    }
    $staged = $null
    foreach ($file in @('LICENSE', 'README.md')) {
        Copy-Item -LiteralPath (Join-Path $work $file) -Destination $InstallDir -Force
    }
    $programs = [Environment]::GetFolderPath('Programs')
    New-Item -ItemType Directory -Force -Path $programs | Out-Null
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut((Join-Path $programs 'Useful Timer.lnk'))
    $shortcut.TargetPath = $destination
    $shortcut.WorkingDirectory = $InstallDir
    $shortcut.Description = 'Animated desktop countdown timers'
    $shortcut.Save()

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($userPath -split ';' | Where-Object { $_ })
    if (@($entries | ForEach-Object { $_.TrimEnd('\') }) -notcontains $InstallDir.TrimEnd('\')) {
        [Environment]::SetEnvironmentVariable('Path', (($entries + $InstallDir) -join ';'), 'User')
    }
    if (($env:Path -split ';').TrimEnd('\') -notcontains $InstallDir.TrimEnd('\')) {
        $env:Path = "$InstallDir;$env:Path"
    }
    Write-Output "Installed: $destination"
    Write-Output 'Open Useful Timer from the Start menu, or run useful-timer in a new terminal. Unsigned releases may trigger SmartScreen; verify the source before approving.'
} catch {
    if (-not $Restart -or -not $handoffStarted) { throw }
    # The parent GUI has closed; report failure visibly and reopen the existing app.
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show(
        "Useful Timer could not be updated. Your saved settings have not been removed.`n`n$($_.Exception.Message)",
        'Useful Timer update failed'
    ) | Out-Null
} finally {
    if ($staged -and (Test-Path -LiteralPath $staged)) { Remove-Item -LiteralPath $staged -Force }
    if ($work -and (Test-Path -LiteralPath $work)) { Remove-Item -LiteralPath $work -Recurse -Force }
    if ($ReadyFile -and (Test-Path -LiteralPath $ReadyFile)) { Remove-Item -LiteralPath $ReadyFile -Force }
    if ($SelfRemove) { Remove-Item -LiteralPath $PSCommandPath -Force }
}
if ($Restart -and (Test-Path -LiteralPath $destination)) {
    Start-Process -FilePath $destination -WorkingDirectory $InstallDir
}
