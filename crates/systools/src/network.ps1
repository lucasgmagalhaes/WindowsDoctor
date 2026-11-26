# Teste de rede sob demanda. Somente leitura. Escreve um objeto JSON (UTF-8).
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

$route = Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object RouteMetric, InterfaceMetric | Select-Object -First 1
$gateway = if ($route) { [string]$route.NextHop } else { $null }
$adapter = if ($route) { Get-NetAdapter -InterfaceIndex $route.InterfaceIndex } else { $null }

function Measure-Ping([string]$target, [string]$label) {
    $ping = [System.Net.NetworkInformation.Ping]::new()
    $times = @(); $sent = 20
    for ($i = 0; $i -lt $sent; $i++) {
        try {
            $r = $ping.Send($target, 1000)
            if ($r.Status -eq 'Success') { $times += [int]$r.RoundtripTime }
        } catch {}
        Start-Sleep -Milliseconds 100
    }
    $avg = $null; $max = $null
    if ($times.Count -gt 0) {
        $avg = [math]::Round(($times | Measure-Object -Average).Average, 1)
        $max = ($times | Measure-Object -Maximum).Maximum
    }
    [pscustomobject]@{ host = $target; label = $label; sent = $sent; received = $times.Count; avgMs = $avg; maxMs = $max }
}

$targets = @()
if ($gateway) { $targets += Measure-Ping $gateway 'Roteador' }
$targets += Measure-Ping '1.1.1.1' 'Internet (Cloudflare)'
$targets += Measure-Ping '8.8.8.8' 'Internet (Google)'

$dnsMs = $null
$sw = [Diagnostics.Stopwatch]::StartNew()
$ok = $false
try { $null = [System.Net.Dns]::GetHostAddresses('www.microsoft.com'); $ok = $true } catch {}
$sw.Stop()
if ($ok) { $dnsMs = [math]::Round($sw.Elapsed.TotalMilliseconds, 0) }

$dnsServers = @()
if ($route) {
    $dnsServers = @((Get-DnsClientServerAddress -InterfaceIndex $route.InterfaceIndex -AddressFamily IPv4).ServerAddresses)
}

# Wi-Fi: a linha com porcentagem de "netsh wlan show interfaces" é o sinal (em qualquer idioma).
$wifiSignal = $null; $ssid = $null
if ($adapter -and ($adapter.MediaType -match '802\.11' -or $adapter.PhysicalMediaType -match '802\.11')) {
    foreach ($line in (netsh wlan show interfaces)) {
        if ($line -match '^\s*SSID\s*:\s*(.+)$') { $ssid = $Matches[1].Trim() }
        elseif ($line -match ':\s*(\d{1,3})\s*%\s*$') { $wifiSignal = [int]$Matches[1] }
    }
}

[pscustomobject]@{
    gateway     = $gateway
    adapter     = if ($adapter) { [string]$adapter.InterfaceDescription } else { $null }
    linkSpeed   = if ($adapter) { [string]$adapter.LinkSpeed } else { $null }
    wifi        = [bool]($null -ne $wifiSignal)
    wifiSignal  = $wifiSignal
    ssid        = $ssid
    targets     = $targets
    dnsMs       = $dnsMs
    dnsServers  = $dnsServers
} | ConvertTo-Json -Depth 4 -Compress
