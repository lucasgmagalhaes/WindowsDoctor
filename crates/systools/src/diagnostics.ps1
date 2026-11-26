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
    @{ LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000 },
    @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WER-SystemErrorReporting'; Id = 1001 },
    @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WindowsUpdateClient'; Id = 20 }
)
$since = $now.AddDays(-30)
$events = @(foreach ($f in $filters) {
    $f.StartTime = $since
    try {
        Get-WinEvent -FilterHashtable $f -MaxEvents 200 -ErrorAction Stop |
            Where-Object { $_.Level -ge 1 -and $_.Level -le 3 } |
            ForEach-Object {
                $detail = ''
                $p = $_.Properties
                if ($_.Id -eq 1000 -and $p.Count -gt 0) { $detail = [string]$p[0].Value }
                # Tela azul: o primeiro campo é o código de parada com os parâmetros.
                if ($_.Id -eq 1001 -and $p.Count -gt 0) { $detail = [string]$p[0].Value }
                # Windows Update: código de erro | título da atualização.
                if ($_.Id -eq 20 -and $p.Count -gt 1) { $detail = ([string]$p[0].Value) + ' | ' + ([string]$p[1].Value) }
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
    # Contadores SMART: exigem administrador; ficam nulos sem ele.
    $r = $_ | Get-StorageReliabilityCounter
    $temp = $null; $wear = $null; $hours = $null; $rerr = $null; $werr = $null
    if ($r) {
        if ($r.Temperature) { $temp = [int]$r.Temperature }
        if ($null -ne $r.Wear) { $wear = [int]$r.Wear }
        if ($r.PowerOnHours) { $hours = [uint64]$r.PowerOnHours }
        if ($null -ne $r.ReadErrorsUncorrected) { $rerr = [uint64]$r.ReadErrorsUncorrected }
        if ($null -ne $r.WriteErrorsUncorrected) { $werr = [uint64]$r.WriteErrorsUncorrected }
    }
    [pscustomobject]@{
        name         = [string]$_.FriendlyName
        mediaType    = [string]$_.MediaType
        health       = [string]$_.HealthStatus
        size         = [uint64]$_.Size
        temperature  = $temp
        wear         = $wear
        powerOnHours = $hours
        readErrors   = $rerr
        writeErrors  = $werr
    }
})

# Dispositivos com problema no Gerenciador de Dispositivos (45 = desconectado, ignorado).
$devices = @(Get-CimInstance Win32_PnPEntity -Filter 'ConfigManagerErrorCode <> 0' |
    Where-Object { $_.ConfigManagerErrorCode -ne 45 } | ForEach-Object {
        [pscustomobject]@{
            name  = [string]$_.Name
            code  = [int]$_.ConfigManagerErrorCode
            class = [string]$_.PNPClass
        }
    })

# Minidumps de tela azul (a pasta costuma exigir administrador).
$minidumps = @(Get-ChildItem "$env:SystemRoot\Minidump" -Filter *.dmp | Sort-Object LastWriteTime -Descending |
    Select-Object -First 10 | ForEach-Object {
        [pscustomobject]@{ name = $_.Name; time = $_.LastWriteTime.ToString('yyyy-MM-dd HH:mm') }
    })

# Temperaturas: zonas térmicas ACPI (aproximadas, exigem administrador) e nvidia-smi.
$temps = @()
$temps += @(Get-CimInstance -Namespace root\wmi -ClassName MSAcpi_ThermalZoneTemperature | ForEach-Object {
    [pscustomobject]@{ label = 'Zona térmica ACPI (' + ([string]$_.InstanceName).Split('\')[-1] + ')'; celsius = [math]::Round($_.CurrentTemperature / 10 - 273.15, 1) }
})
$smi = Get-Command nvidia-smi -ErrorAction SilentlyContinue
if ($smi) {
    $temps += @(& $smi.Source --query-gpu=name,temperature.gpu --format=csv,noheader,nounits | ForEach-Object {
        $parts = $_ -split ','
        if ($parts.Count -ge 2) { [pscustomobject]@{ label = $parts[0].Trim(); celsius = [double]$parts[1].Trim() } }
    })
}

$pendingReboot = (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired') -or
    (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending')

# Bateria (notebooks): capacidade original x atual pelo relatório do powercfg.
$battery = $null
if (Get-CimInstance Win32_Battery) {
    $xmlPath = Join-Path $env:TEMP 'windowsdoctor-battery.xml'
    powercfg /batteryreport /xml /output $xmlPath | Out-Null
    if (Test-Path $xmlPath) {
        $b = ([xml](Get-Content $xmlPath -Raw)).BatteryReport.Batteries.Battery | Select-Object -First 1
        if ($b) {
            $cycles = $null
            if ([string]$b.CycleCount -match '^\d+$') { $cycles = [int]$b.CycleCount }
            $battery = [pscustomobject]@{
                name               = ([string]$b.Manufacturer + ' ' + [string]$b.Id).Trim()
                designCapacity     = [uint64]$b.DesignCapacity
                fullChargeCapacity = [uint64]$b.FullChargeCapacity
                cycleCount         = $cycles
            }
        }
        Remove-Item $xmlPath -Force
    }
}

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
    devices      = $devices
    minidumps    = $minidumps
    temps        = $temps
    pendingReboot = [bool]$pendingReboot
    battery      = $battery
} | ConvertTo-Json -Depth 4 -Compress
