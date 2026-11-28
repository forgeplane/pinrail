# Sets up a fresh Windows VM to build and test Pinrail. Windows starts it at
# the first sign-in, from the setup disc that create.sh makes.
#
# The script registers a task that runs it again, elevated, at the next
# sign-in until it has finished, so a restart part way through resumes it.
# Each step that has succeeded is recorded and skipped on the next run, and
# only one copy runs at a time. It runs in Windows PowerShell 5.1, which
# every Windows 11 install has, as an administrator.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# An error outside the steps ends the script. The window stays open so that
# the error can be read.
trap {
    Write-Host "The setup stopped: $_" -ForegroundColor Red
    Read-Host 'Press Enter to close'
    exit 1
}

$admin = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $admin.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'the setup needs an administrator: run it from PowerShell opened with "Run as administrator"'
}

# One copy at a time: a second one, as a sign-in can start while the first
# is still going, would race it through the same installs. A copy that
# ended without releasing the lock leaves it abandoned, which is taken.
$Lock = New-Object System.Threading.Mutex($false, 'Global\PinrailVMSetup')
try { $locked = $Lock.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $locked = $true }
if (-not $locked) {
    Write-Host 'The setup is already running in another window.'
    Start-Sleep 5
    exit 0
}

$State = 'C:\ProgramData\PinrailVM'
$Repo = 'C:\dev\pinrail'
New-Item -ItemType Directory -Force $State | Out-Null
Start-Transcript -Append -Path "$State\setup.log" | Out-Null

# A copy on the system disk, which keeps working when the disc is removed.
$Self = "$State\setup.ps1"
if ($PSCommandPath -ne $Self) {
    Copy-Item $PSCommandPath $Self -Force
    Get-ChildItem (Split-Path $PSCommandPath) -File |
        Where-Object { $_.Name -like 'utm-guest-tools-*.exe' -or $_.Name -eq 'authorized_keys' } |
        Copy-Item -Destination $State -Force
    # Files copied from a CD are read-only, which would stop later updates.
    Get-ChildItem $State -File | ForEach-Object { $_.IsReadOnly = $false }
}

# Runs the script at the account's next sign-in, elevated. Windows starts
# RunOnce entries unelevated, and again whenever Explorer restarts.
$Task = 'PinrailVMSetup'
$action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$Self`""
$trigger = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Highest
Register-ScheduledTask -TaskName $Task -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
# as earlier versions of this script registered it
Remove-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce' 'PinrailVMSetup' -ErrorAction SilentlyContinue

$Failed = @()

# Runs a step once. A step that fails is reported, and runs again the next
# time the script runs.
function Step([string]$Name, [scriptblock]$Body) {
    $done = Join-Path $State "$Name.done"
    if (Test-Path $done) { return }
    Write-Host "==> $Name" -ForegroundColor Cyan
    try {
        & $Body
        New-Item -ItemType File -Force $done | Out-Null
    } catch {
        Write-Host "    failed: $_" -ForegroundColor Red
        $script:Failed += $Name
    }
}

function Refresh-Path {
    $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
        [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Check-Exit([string]$What) {
    if ($LASTEXITCODE -ne 0) { throw "$What exited with $LASTEXITCODE" }
}

Step 'guest-tools' {
    # The network, display and clipboard drivers for UTM.
    $installer = Get-ChildItem $State -Filter 'utm-guest-tools-*.exe' | Select-Object -First 1
    if (-not $installer) { throw 'the UTM guest tools installer is missing' }
    Start-Process $installer.FullName -ArgumentList '/S' -Wait
}

Step 'system-settings' {
    powercfg /hibernate off
    powercfg /change standby-timeout-ac 0
    powercfg /change monitor-timeout-ac 0
    # Paths in node_modules and Cargo's target folder can exceed 260 characters.
    Set-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' 'LongPathsEnabled' 1 -Type DWord
    # Developer Mode lets programs create symbolic links without elevation.
    $unlock = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock'
    New-Item -Force $unlock | Out-Null
    Set-ItemProperty $unlock 'AllowDevelopmentWithoutDevLicense' 1 -Type DWord
    # Real-time scanning of build folders makes builds several times slower.
    Add-MpPreference -ExclusionPath 'C:\dev', "$env:USERPROFILE\.cargo",
        "$env:USERPROFILE\.rustup", "$env:LOCALAPPDATA\mise", "$env:LOCALAPPDATA\npm-cache"
}

Write-Host '==> Waiting for the network' -ForegroundColor Cyan
$online = $false
for ($i = 0; $i -lt 60 -and -not $online; $i++) {
    try {
        Invoke-WebRequest 'https://github.com' -Method Head -UseBasicParsing -TimeoutSec 10 | Out-Null
        $online = $true
    } catch { Start-Sleep 5 }
}
if (-not $online) {
    Write-Host 'The VM has no network. Restart it to try again.' -ForegroundColor Red
    Read-Host 'Press Enter to close'
    exit 1
}

Step 'winget' {
    # The App Installer, which provides winget, is registered some time after
    # the first sign-in. If it does not appear, install it from PowerShell
    # Gallery.
    for ($i = 0; $i -lt 24 -and -not (Get-Command winget -ErrorAction SilentlyContinue); $i++) {
        try { Add-AppxPackage -RegisterByFamilyName -MainPackage Microsoft.DesktopAppInstaller_8wekyb3d8bbwe } catch {}
        Start-Sleep 5
        Refresh-Path
    }
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Install-PackageProvider -Name NuGet -Force -Scope AllUsers | Out-Null
        Set-PSRepository PSGallery -InstallationPolicy Trusted
        Install-Module Microsoft.WinGet.Client -Force -Scope AllUsers
        Repair-WinGetPackageManager -AllUsers -Latest
        Refresh-Path
    }
    winget --version
    Check-Exit 'winget'
}

# winget's code for a package that is already installed and up to date.
$AlreadyInstalled = -1978335189

# Installs a package with winget, which can hang on a download: one that
# takes longer than $Minutes is stopped, with what it started, and tried
# once more.
function Invoke-Winget([string]$Id, [string[]]$Extra = @(), [int]$Minutes = 20) {
    $arguments = @('install', '--id', $Id, '--exact', '--source', 'winget', '--silent',
        '--disable-interactivity', '--accept-source-agreements', '--accept-package-agreements') + $Extra
    $line = ($arguments | ForEach-Object { if ($_ -match '\s') { '"' + $_ + '"' } else { $_ } }) -join ' '
    for ($try = 1; $try -le 2; $try++) {
        $start = New-Object System.Diagnostics.ProcessStartInfo (Get-Command winget).Source, $line
        $start.UseShellExecute = $false
        $winget = [System.Diagnostics.Process]::Start($start)
        if ($winget.WaitForExit($Minutes * 60 * 1000)) {
            $winget.WaitForExit()
            $code = $winget.ExitCode
            if ($code -ne 0 -and $code -ne $AlreadyInstalled) { throw "winget install $Id exited with $code" }
            return
        }
        Write-Host "    winget took more than $Minutes minutes; stopping it" -ForegroundColor Yellow
        taskkill /T /F /PID $winget.Id | Out-Null
    }
    throw "winget install $Id did not finish in $Minutes minutes, twice"
}

function Install-Package([string]$Id, [string[]]$Extra = @(), [int]$Minutes = 20) {
    Step "install-$Id" { Invoke-Winget $Id $Extra $Minutes }
}

Install-Package 'Git.Git'
Install-Package 'GitHub.cli'
Install-Package 'Microsoft.PowerShell'
Install-Package 'jdx.mise'
# The C++ build tools and the Windows SDK, which Rust uses to link programs,
# and Clang, which the ring crate needs on ARM64.
Install-Package 'Microsoft.VisualStudio.2022.BuildTools' @('--override', ('--wait --quiet --norestart --nocache ' +
    '--add Microsoft.VisualStudio.Workload.VCTools ' +
    '--add Microsoft.VisualStudio.Component.VC.Tools.ARM64 ' +
    '--add Microsoft.VisualStudio.Component.Windows11SDK.26100 ' +
    '--add Microsoft.VisualStudio.Component.VC.Llvm.Clang --includeRecommended')) 90
Install-Package 'Rustlang.Rustup'

Step 'clang' {
    # Clang from the build tools, which the ring crate's build needs for
    # ARM64 Windows, on the account's PATH, where cc-rs looks for it. The
    # component is added to an installation made before it was listed above.
    $tools = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools"
    $installer = Start-Process "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\setup.exe" -Wait -PassThru `
        -ArgumentList "modify --installPath `"$tools`" --add Microsoft.VisualStudio.Component.VC.Llvm.Clang --quiet --norestart --nocache"
    if ($installer.ExitCode -notin 0, 3010) { throw "the Visual Studio Installer exited with $($installer.ExitCode)" }
    # the processor's: a process emulated as x64 on ARM64, as Windows
    # PowerShell is when Git Bash starts it, is told x64 by every other source
    $arm64 = (Get-CimInstance Win32_Processor | Select-Object -First 1).Architecture -eq 12
    $arch = if ($arm64) { 'ARM64' } else { 'x64' }
    $bin = "$tools\VC\Tools\Llvm\$arch\bin"
    if (-not (Test-Path "$bin\clang.exe")) { throw "$bin has no clang.exe" }
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($userPath -split ';') -notcontains $bin) {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$bin", 'User')
    }
}
Refresh-Path

Step 'ssh-server' {
    # The OpenSSH server, which lets Claude Code on the Mac run builds and
    # tests in the VM. Its shell is Git Bash, so the commands are the same as
    # on macOS and Linux.
    $keys = "$State\authorized_keys"
    if (-not (Test-Path $keys)) {
        Write-Host '    The setup disc has no SSH key, so the SSH server is not set up.'
        return
    }
    # Microsoft's OpenSSH release from GitHub. Windows Update does not offer
    # the built-in OpenSSH server on every build, such as Insider builds.
    Invoke-Winget 'Microsoft.OpenSSH.Preview'
    Set-Service sshd -StartupType Automatic
    Start-Service sshd
    if (-not (Get-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -ErrorAction SilentlyContinue)) {
        New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server' `
            -Direction Inbound -Protocol TCP -LocalPort 22 -Action Allow | Out-Null
    }
    New-Item -Force 'HKLM:\SOFTWARE\OpenSSH' | Out-Null
    Set-ItemProperty 'HKLM:\SOFTWARE\OpenSSH' 'DefaultShell' "$env:ProgramFiles\Git\bin\bash.exe"
    # The server reads an administrator's keys from this file, and only when
    # nobody but the administrators and the system can change it.
    $adminKeys = "$env:ProgramData\ssh\administrators_authorized_keys"
    Copy-Item $keys $adminKeys -Force
    icacls $adminKeys /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null
    Check-Exit 'icacls'
}

Step 'ssh-path' {
    # Windows' OpenSSH gives commands only the machine's PATH, without the
    # account's own, where Rust, mise and winget's programs are. Git Bash
    # reads ~/.bashrc for an SSH command, so it adds the account's PATH from
    # the registry there, as it is at each sign-in.
    $bashrc = "$env:USERPROFILE\.bashrc"
    $marker = '# pinrail-vm: the account''s PATH'
    if ((Test-Path $bashrc) -and (Select-String -Quiet -SimpleMatch $marker $bashrc)) { return }
    $lines = @(
        $marker,
        'if [ -n "$SSH_CONNECTION" ]; then',
        '    pinrail_vm_path="$(MSYS_NO_PATHCONV=1 reg query ''HKCU\Environment'' /v Path 2>/dev/null | sed -n ''s/^ *Path *REG_[A-Z_]* *//Ip'' | tr -d ''\r'')"',
        '    # %Variables% in any case, as Windows names them, against bash''s',
        '    while [[ $pinrail_vm_path =~ %([A-Za-z_][A-Za-z0-9_]*)% ]]; do',
        '        pinrail_vm_name=${BASH_REMATCH[1]}',
        '        pinrail_vm_value=$(env | grep -i -m1 "^${pinrail_vm_name}=" | cut -d= -f2-)',
        '        # quoted, so that bash 5.2 takes & and \\ in the value as they are',
        '        pinrail_vm_path=${pinrail_vm_path//"%$pinrail_vm_name%"/"$pinrail_vm_value"}',
        '    done',
        '    [ -n "$pinrail_vm_path" ] && PATH="$PATH:$(cygpath -p "$pinrail_vm_path")"',
        '    unset pinrail_vm_path pinrail_vm_name pinrail_vm_value',
        'fi',
        ''
    )
    # UTF-8 without a byte order mark and with LF line ends, as bash needs,
    # on a line of its own after what the file ends with
    $text = $lines -join "`n"
    if ((Test-Path $bashrc) -and -not ((Get-Content -Raw $bashrc) -match "(^|`n)$")) { $text = "`n" + $text }
    [IO.File]::AppendAllText($bashrc, $text, (New-Object System.Text.UTF8Encoding $false))
}

Step 'rsync' {
    # rsync in Git Bash, which scripts/windows-vm/sync.sh copies the Mac's
    # checkout with. Git for Windows has none, so MSYS2's, whose runtime Git
    # Bash shares, goes in Git's usr\bin with the DLLs Git lacks. The
    # packages are pinned by SHA-256; Windows' tar unpacks zstd.
    $packages = [ordered]@{
        'rsync-3.5.2-2'      = 'e27c183d8af0ed250dc7502ba3b4aebe5c32086c4252fe4f16a4cff328c8e803'
        'libxxhash-0.8.4-1'  = 'ba784ad117626401cd5d3ec0cf70399f9de72a63755f96c28fa2032303f9c330'
        'libopenssl-4.1.0-1' = '71f093de4a002913f39783b79d18da33b63ce8baf825434218cc1d8bd40304fb'
        'popt-1.19-1'        = 'b402bd1d03815c5e28b7ab415d8e20f487eff2460ae0271a99674eeb0d77194f'
    }
    $work = Join-Path $env:TEMP 'pinrail-vm-rsync'
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force $work | Out-Null
    foreach ($name in $packages.Keys) {
        $file = Join-Path $work "$name-x86_64.pkg.tar.zst"
        Invoke-WebRequest "https://repo.msys2.org/msys/x86_64/$name-x86_64.pkg.tar.zst" -OutFile $file -UseBasicParsing
        $actual = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $packages[$name]) { throw "$name has SHA-256 $actual, not $($packages[$name])" }
        & "$env:SystemRoot\System32\tar.exe" -xf $file -C $work
        Check-Exit "tar of $name"
    }
    $bin = "$env:ProgramFiles\Git\usr\bin"
    foreach ($file in 'rsync.exe', 'msys-xxhash-0.dll', 'msys-crypto-4.dll', 'msys-popt-0.dll') {
        Copy-Item (Join-Path $work "usr\bin\$file") $bin -Force
    }
    Remove-Item -Recurse -Force $work
    & "$bin\rsync.exe" --version | Select-Object -First 1
    Check-Exit 'rsync --version'
}

Step 'mise-shims' {
    # mise's shims make the project's Node, Zig and nextest available in every
    # shell, including Git Bash.
    $shims = "$env:LOCALAPPDATA\mise\shims"
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($userPath -split ';') -notcontains $shims) {
        [Environment]::SetEnvironmentVariable('Path', "$userPath;$shims", 'User')
    }
    Refresh-Path
}

Step 'clone' {
    if (-not (Test-Path "$Repo\.git")) {
        New-Item -ItemType Directory -Force (Split-Path $Repo) | Out-Null
        git -c core.longpaths=true clone https://github.com/forgeplane/pinrail $Repo
        Check-Exit 'git clone'
    }
    git -C $Repo config core.longpaths true
}

Step 'toolchains' {
    Push-Location $Repo
    try {
        mise trust
        Check-Exit 'mise trust'
        mise install
        Check-Exit 'mise install'
        # The Rust version that rust-toolchain.toml pins.
        rustup toolchain install
        Check-Exit 'rustup toolchain install'
    } finally { Pop-Location }
}

Step 'project-setup' {
    # The repository's own setup, which installs the npm packages, builds the
    # plugins and installs Playwright's Chromium. Its scripts need Git Bash.
    $bash = "$env:ProgramFiles\Git\bin\bash.exe"
    & $bash -lc 'cd /c/dev/pinrail && mise run setup'
    Check-Exit 'mise run setup'
}

Stop-Transcript | Out-Null

if ($Failed.Count -gt 0) {
    Write-Host ''
    Write-Host ('These steps failed: ' + ($Failed -join ', ')) -ForegroundColor Red
    Write-Host "The log is in $State\setup.log. The failed steps run again at the next sign-in,"
    Write-Host "or when you run: powershell -ExecutionPolicy Bypass -File $Self"
} else {
    Unregister-ScheduledTask -TaskName $Task -Confirm:$false
    Write-Host ''
    Write-Host 'The VM is ready.' -ForegroundColor Green
    Write-Host "Pinrail is cloned in $Repo. To start the app in dev mode, open Git Bash and run:"
    Write-Host '  cd /c/dev/pinrail && mise run dev:desktop'
}
Read-Host 'Press Enter to close'
