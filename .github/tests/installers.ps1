# Local release fixtures replace only network access. Windows installation uses real OS APIs.
$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$installer = Join-Path $root 'install.ps1'
$isWindowsHost = [Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT
$work = Join-Path ([IO.Path]::GetTempPath()) ('timer install ' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
$fixtures = Join-Path $work 'fixtures'
$package = Join-Path $work 'package'
New-Item -ItemType Directory -Path $fixtures, $package | Out-Null
$destination = Join-Path $work 'installed app'
$asset = 'useful-timer-1.2.3-x86_64-pc-windows-msvc.zip'
$oldOS = $env:OS
$oldArch = $env:PROCESSOR_ARCHITECTURE
$oldArch64 = $env:PROCESSOR_ARCHITEW6432
$oldPath = $env:Path
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$shortcutPath = $null
$shortcutBackup = $null
if ($isWindowsHost) {
    $shortcutPath = Join-Path ([Environment]::GetFolderPath('Programs')) 'Useful Timer.lnk'
    if (Test-Path -LiteralPath $shortcutPath) { $shortcutBackup = [IO.File]::ReadAllBytes($shortcutPath) }
}
function Assert($condition, $message) {
    if (-not $condition) { throw $message }
}
function Invoke-RestMethod { return @{ tag_name = 'v1.2.3' } }
function Invoke-WebRequest {
    param($Uri, $OutFile, [switch]$UseBasicParsing)
    Copy-Item -LiteralPath (Join-Path $fixtures ($Uri.Split('/')[-1])) -Destination $OutFile
}
function New-Fixture([string]$executable = '', [switch]$OmitExecutable) {
    Remove-Item -Path (Join-Path $package '*') -Force -ErrorAction SilentlyContinue
    if (-not $OmitExecutable) {
        if ($executable) { Copy-Item -LiteralPath $executable -Destination (Join-Path $package 'useful-timer.exe') }
        else { [IO.File]::WriteAllBytes((Join-Path $package 'useful-timer.exe'), [byte[]](1, 2, 3)) }
    }
    Set-Content -LiteralPath (Join-Path $package 'LICENSE') 'Fixture license'
    Set-Content -LiteralPath (Join-Path $package 'README.md') 'Fixture instructions'
    Compress-Archive -Path (Join-Path $package '*') -DestinationPath (Join-Path $fixtures $asset) -Force
    $hash = (Get-FileHash -Algorithm SHA256 (Join-Path $fixtures $asset)).Hash.ToLowerInvariant()
    "$hash  $asset" | Set-Content -LiteralPath (Join-Path $fixtures "$asset.sha256") -Encoding ascii
}
function Expect-Failure([string]$reason) {
    $failed = $false
    $ready = Join-Path $work 'failed-update.ready'
    try { & $installer -InstallDir $destination -Version 1.2.3 -WaitForProcessId $PID -ReadyFile $ready -Restart }
    catch { $failed = $true; Write-Output "PASS: $reason ($($_.Exception.Message))" }
    Assert $failed "Expected installation to fail: $reason"
    Assert ((Get-FileHash -LiteralPath (Join-Path $destination 'useful-timer.exe')).Hash -eq $script:before) 'Failed install replaced existing executable'
    Assert (-not (Test-Path -LiteralPath $ready)) 'Failed verification requested app shutdown'
}
try {
    $env:OS = 'Windows_NT'
    $env:PROCESSOR_ARCHITECTURE = 'AMD64'
    $env:PROCESSOR_ARCHITEW6432 = ''
    New-Item -ItemType Directory -Path $destination | Out-Null
    Set-Content -LiteralPath (Join-Path $destination 'useful-timer.exe') 'Existing installation'
    $script:before = (Get-FileHash -LiteralPath (Join-Path $destination 'useful-timer.exe')).Hash
    New-Fixture
    Set-Content -LiteralPath (Join-Path $fixtures $asset) 'Corrupted archive'
    Expect-Failure 'checksum mismatch preserves existing installation'
    New-Fixture
    Remove-Item -LiteralPath (Join-Path $fixtures "$asset.sha256")
    Expect-Failure 'missing checksum preserves existing installation'
    New-Fixture -OmitExecutable
    Expect-Failure 'missing executable preserves existing installation'
    New-Fixture
    $env:PROCESSOR_ARCHITECTURE = 'ARM64'
    Expect-Failure 'unsupported architecture'
    $env:PROCESSOR_ARCHITECTURE = 'AMD64'
    if ($isWindowsHost) {
        # Standalone Windows executables let us actually launch the installed fixture.
        New-Fixture (Join-Path $env:WINDIR 'System32\whoami.exe')
        & $installer -InstallDir $destination
        $actual = & (Join-Path $destination 'useful-timer.exe')
        Assert ($LASTEXITCODE -eq 0 -and $actual -eq (& whoami.exe)) 'Installed executable did not launch'
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($shortcutPath)
        Assert ($shortcut.TargetPath -eq (Join-Path $destination 'useful-timer.exe')) 'Start menu shortcut points to wrong executable'
        New-Fixture (Join-Path $env:WINDIR 'System32\hostname.exe')
        & $installer -Version v1.2.3 -InstallDir $destination
        $actual = & (Join-Path $destination 'useful-timer.exe')
        Assert ($LASTEXITCODE -eq 0 -and $actual -eq (& hostname.exe)) 'Updated executable did not launch'
        $entries = @([Environment]::GetEnvironmentVariable('Path', 'User') -split ';' | Where-Object { $_ -eq $destination })
        Assert ($entries.Count -eq 1) 'Installer duplicated or omitted user PATH entry'
        Write-Output 'PASS: Windows install, launch, shortcut, update, and idempotent user PATH'

        # In-app handoff must finish only after the parent exits, then remove its
        # temporary helper. Use a real process rather than mocking Wait-Process.
        $helper = Join-Path $work 'updater helper.ps1'
        Copy-Item -LiteralPath $installer -Destination $helper
        $ready = Join-Path $work 'verified-update.ready'
        $quotedReady = $ready.Replace("'", "''")
        $parentCode = "`$deadline = (Get-Date).AddSeconds(30); while (-not (Test-Path -LiteralPath '$quotedReady')) { if ((Get-Date) -gt `$deadline) { exit 2 }; Start-Sleep -Milliseconds 20 }; if ((Get-Content -LiteralPath '$quotedReady' -Raw) -ne 'verified') { exit 3 }; exit 0"
        $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($parentCode))
        $parent = Start-Process powershell.exe -ArgumentList '-NoProfile', '-EncodedCommand', $encoded -PassThru -WindowStyle Hidden
        try {
            & $helper -Version 1.2.3 -InstallDir $destination -WaitForProcessId $parent.Id -ReadyFile $ready -SelfRemove
            $parent.Refresh()
            Assert $parent.HasExited 'Update finished before the parent exited'
            Assert ($parent.ExitCode -eq 0) 'Parent did not receive verified download before exiting'
            Assert (-not (Test-Path -LiteralPath $ready)) 'Updater left its readiness marker behind'
            Assert (-not (Test-Path -LiteralPath $helper)) 'Updater left its temporary script behind'
            $actual = & (Join-Path $destination 'useful-timer.exe')
            Assert ($LASTEXITCODE -eq 0 -and $actual -eq (& hostname.exe)) 'Handoff did not leave a runnable app'
            Write-Output 'PASS: updater waits for parent exit and cleans up its helper'
        } finally {
            if (-not $parent.HasExited) { $parent.Kill() }
            $parent.Dispose()
        }
    } else {
        Write-Output 'SKIP: native Windows shortcut, PATH, and launch checks require Windows'
    }
} finally {
    if ($isWindowsHost) {
        [Environment]::SetEnvironmentVariable('Path', $userPath, 'User')
        if ($null -ne $shortcutBackup) { [IO.File]::WriteAllBytes($shortcutPath, $shortcutBackup) }
        elseif (Test-Path -LiteralPath $shortcutPath) { Remove-Item -LiteralPath $shortcutPath -Force }
    }
    $env:OS = $oldOS
    $env:PROCESSOR_ARCHITECTURE = $oldArch
    $env:PROCESSOR_ARCHITEW6432 = $oldArch64
    $env:Path = $oldPath
    Remove-Item -LiteralPath $work -Recurse -Force
}
