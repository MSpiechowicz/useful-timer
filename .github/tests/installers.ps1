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
    try { & $installer -InstallDir $destination }
    catch { $failed = $true; Write-Output "PASS: $reason ($($_.Exception.Message))" }
    Assert $failed "Expected installation to fail: $reason"
    Assert ((Get-FileHash -LiteralPath (Join-Path $destination 'useful-timer.exe')).Hash -eq $script:before) 'Failed install replaced existing executable'
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
