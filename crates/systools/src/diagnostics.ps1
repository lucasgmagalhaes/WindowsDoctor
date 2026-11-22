# Coleta de dados para o diagnostico. Somente leitura.
# Escreve um unico objeto JSON (UTF-8) na saida padrao; o Rust interpreta.
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$now = Get-Date

$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1

$gpus = @(Get-CimInstance Win32_VideoController | ForEach-Object {
    $date = ''
    $age = $null
    if ($_.DriverDate) {
        $date = $_.DriverDate.ToString('yyyy-MM-dd')
        $age = [int]($now - $_.DriverDate).TotalDays
    }
    [pscustomobject]@{
        name          = [string]$_.Name
        driverVersion = [string]$_.DriverVersion
        driverDate    = $date
        driverAgeDays = $age
        refreshRate   = [int]$_.CurrentRefreshRate
        width         = [int]$_.CurrentHorizontalResolution
        height        = [int]$_.CurrentVerticalResolution
    }
})

$decode = { param($a) if ($a) { -join ($a | Where-Object { $_ -ne 0 } | ForEach-Object { [char]$_ }) } else { '' } }
$conns = @(Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorConnectionParams)
$monitors = @(Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorID | ForEach-Object {
    $m = $_
    $c = $conns | Where-Object { $_.InstanceName -eq $m.InstanceName } | Select-Object -First 1
    $tech = $null
    if ($c) { $tech = [int64]$c.VideoOutputTechnology }
    [pscustomobject]@{
        manufacturer = (& $decode $m.ManufacturerName)
        name         = (& $decode $m.UserFriendlyName)
        connection   = $tech
    }
})

$filters = @(
    @{ LogName = 'System'; ProviderName = 'Display'; Id = 4101 },
    @{ LogName = 'System'; ProviderName = 'nvlddmkm'; Id = 13, 14, 153 },
    @{ LogName = 'System'; ProviderName = 'amdkmdag' },
    @{ LogName = 'System'; ProviderName = 'amdwddmg' },
    @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WHEA-Logger' },
    @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Kernel-Power'; Id = 41 },
    @{ LogName = 'System'; ProviderName = 'disk'; Id = 7, 11, 51, 153 },
    @{ LogName = 'System'; ProviderName = 'Ntfs'; Id = 55, 98 },
    @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Ntfs'; Id = 55, 98 },
    @{ LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000 }
)
$since = $now.AddDays(-30)
$events = @(foreach ($f in $filters) {
    $f.StartTime = $since
    try {
        Get-WinEvent -FilterHashtable $f -MaxEvents 200 -ErrorAction Stop |
            Where-Object { $_.Level -ge 1 -and $_.Level -le 3 } |
            ForEach-Object {
                $detail = ''
                if ($_.Id -eq 1000 -and $_.Properties.Count -gt 0) { $detail = [string]$_.Properties[0].Value }
                [pscustomobject]@{
                    provider = $_.ProviderName
                    id       = $_.Id
                    ageHours = [math]::Round(($now - $_.TimeCreated).TotalHours, 1)
                    time     = $_.TimeCreated.ToString('yyyy-MM-dd HH:mm')
                    detail   = $detail
                }
            }
    } catch {}
})

$disks = @(Get-PhysicalDisk | ForEach-Object {
    [pscustomobject]@{
        name      = [string]$_.FriendlyName
        mediaType = [string]$_.MediaType
        health    = [string]$_.HealthStatus
        size      = [uint64]$_.Size
    }
})

$sys = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='$env:SystemDrive'"
$dwm = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\Dwm'
$power = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Power'

[pscustomobject]@{
    os           = "$($os.Caption) $($os.Version)"
    cpu          = ([string]$cpu.Name).Trim()
    ramTotal     = [uint64]$os.TotalVisibleMemorySize * 1024
    ramFree      = [uint64]$os.FreePhysicalMemory * 1024
    uptimeSecs   = [int64]($now - $os.LastBootUpTime).TotalSeconds
    gpus         = $gpus
    monitors     = $monitors
    events       = $events
    disks        = $disks
    systemDrive  = [string]$env:SystemDrive
    systemTotal  = [uint64]$sys.Size
    systemFree   = [uint64]$sys.FreeSpace
    mpoDisabled  = ($dwm.OverlayTestMode -eq 5)
    fastStartup  = ($power.HiberbootEnabled -eq 1)
} | ConvertTo-Json -Depth 4 -Compress
