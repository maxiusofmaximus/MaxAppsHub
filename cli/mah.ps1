# mah: CLI de MaxApps Hub por adb (sin tocar la pantalla del móvil).
#   mah list
#   mah channel <pkg> alfa|estable
#   mah check [pkg]
#   mah update <pkg> [--confirm] [--force]
#   mah status
# Variables: ADB (ruta a adb), ANDROID_SERIAL (dispositivo).

[Console]::OutputEncoding = [Text.Encoding]::UTF8

$ADB = if ($env:ADB) { $env:ADB } else { 'C:\Users\Max\AppData\Local\Android\Sdk\platform-tools\adb.exe' }
$SERIAL = if ($env:ANDROID_SERIAL) { $env:ANDROID_SERIAL } else { '10507543AG103805' }
$RESULT = '/sdcard/Android/data/com.maxapps.hub/files/cli-result.json'
$WINDOW_DUMP = '/sdcard/mah-window.xml'

function Fail([string]$msg) {
    Write-Error $msg
    exit 1
}

# Lee cli-result.json del móvil. Devuelve $null si no existe o no es JSON válido.
function Read-Result {
    $raw = & $ADB -s $SERIAL exec-out cat $RESULT 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $raw) { return $null }
    try { return ($raw | Out-String | ConvertFrom-Json) } catch { return $null }
}

# Lanza el broadcast y espera a que cli-result.json tenga un `t` nuevo.
function Invoke-Cli([string]$cmd, [hashtable]$extras, [int]$timeoutSec) {
    $before = Read-Result
    $prevT = if ($before) { [long]$before.t } else { 0 }

    $bcast = @('shell', 'am', 'broadcast', '-a', 'com.maxapps.hub.CLI',
               '-n', 'com.maxapps.hub/.CliReceiver', '--es', 'cmd', $cmd)
    foreach ($k in $extras.Keys) { $bcast += @('--es', $k, [string]$extras[$k]) }
    & $ADB -s $SERIAL @bcast | Out-Null

    $deadline = (Get-Date).AddSeconds($timeoutSec)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 800
        $r = Read-Result
        if ($r -and [long]$r.t -gt $prevT) { return $r }
    }
    Fail "Sin respuesta del Hub tras ${timeoutSec}s (¿la app está instalada y el móvil conectado?)."
}

function Show-Result($r) {
    if (-not $r.ok) { Fail "Error: $($r.error)" }
    return $r.data
}

function Show-Apps($apps) {
    $apps | ForEach-Object {
        [pscustomobject]@{
            App = $_.name
            Paquete = $_.packageId
            Instalada = $_.installed
            Canal = $_.channel
            Ultima = $_.latest
            Actualizar = $_.updateAvailable
        }
    } | Format-Table -AutoSize
}

# --- Confirmación del instalador del sistema (solo con --confirm) ---

function Get-WindowXml {
    & $ADB -s $SERIAL shell uiautomator dump $WINDOW_DUMP *> $null
    if ($LASTEXITCODE -ne 0) { return $null }
    $raw = & $ADB -s $SERIAL exec-out cat $WINDOW_DUMP 2>$null
    if ($LASTEXITCODE -ne 0 -or -not $raw) { return $null }
    return ($raw | Out-String)
}

# Busca un botón del instalador (paquete *packageinstaller*) cuyo texto encaje con $pattern.
function Find-InstallerButton([string]$xml, [string]$pattern) {
    try { [xml]$doc = $xml } catch { return $null }
    foreach ($n in $doc.SelectNodes('//node')) {
        if ($n.package -notmatch 'packageinstaller') { continue }
        $label = if ($n.text) { $n.text } else { $n.'content-desc' }
        if ($label -notmatch $pattern) { continue }
        if ($n.bounds -match '\[(\d+),(\d+)\]\[(\d+),(\d+)\]') {
            $x = [math]::Floor(([int]$matches[1] + [int]$matches[3]) / 2)
            $y = [math]::Floor(([int]$matches[2] + [int]$matches[4]) / 2)
            return [pscustomobject]@{ Label = $label; X = $x; Y = $y }
        }
    }
    return $null
}

function Find-InstallButton([string]$xml) { Find-InstallerButton $xml '^(instalar|actualizar|install|update)$' }

# Una pantalla «Se instaló la app» que quedó de una instalación anterior tapa el diálogo nuevo
# y deja `mah status` en «instalador»: se cierra con «Listo» y se sigue esperando.
function Find-StaleDoneButton([string]$xml) { Find-InstallerButton $xml '^(listo|hecho|done)$' }

function Confirm-Installer {
    $deadline = (Get-Date).AddSeconds(90)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 2
        $xml = Get-WindowXml
        if ($xml) {
            $btn = Find-InstallButton $xml
            if ($btn) {
                & $ADB -s $SERIAL shell input tap $btn.X $btn.Y | Out-Null
                Write-Host "Pulsado «$($btn.Label)» en el instalador del sistema."
                return
            }
            $stale = Find-StaleDoneButton $xml
            if ($stale) {
                & $ADB -s $SERIAL shell input tap $stale.X $stale.Y | Out-Null
                Write-Host "Cerrada una pantalla vieja del instalador («$($stale.Label)»); sigo esperando el diálogo nuevo."
            }
        }
    }
    Write-Warning 'El instalador no mostró «Instalar»/«Actualizar» en 90 s. Confírmalo a mano en el móvil.'
}

# --- Comandos ---

$argv = @($args)
$confirm = [bool]($argv | Where-Object { $_ -in '--confirm', '-confirm' })
$force = [bool]($argv | Where-Object { $_ -in '--force', '-force' })
$pos = @($argv | Where-Object { $_ -notin '--confirm', '-confirm', '--force', '-force' })
$cmd = if ($pos.Count -gt 0) { $pos[0] } else { 'help' }

switch ($cmd) {
    'list' {
        $data = Show-Result (Invoke-Cli 'list' @{} 90)
        Show-Apps $data.apps
    }
    'check' {
        $extras = if ($pos.Count -gt 1) { @{ pkg = $pos[1] } } else { @{} }
        $data = Show-Result (Invoke-Cli 'check' $extras 90)
        Show-Apps $data.apps
    }
    'channel' {
        if ($pos.Count -lt 3 -or $pos[2] -notin 'alfa', 'estable') { Fail 'Uso: mah channel <pkg> alfa|estable' }
        $data = Show-Result (Invoke-Cli 'channel' @{ pkg = $pos[1]; value = $pos[2] } 30)
        Write-Host "Canal de $($data.packageId): $($data.channel)"
    }
    'update' {
        if ($pos.Count -lt 2) { Fail 'Uso: mah update <pkg> [--confirm] [--force]' }
        # Android bloquea abrir el instalador desde segundo plano: el Hub debe estar al frente.
        & $ADB -s $SERIAL shell monkey -p com.maxapps.hub -c android.intent.category.LAUNCHER 1 *> $null
        Start-Sleep -Seconds 2
        $data = Show-Result (Invoke-Cli 'update' @{ pkg = $pos[1]; force = $(if ($force) { '1' } else { '0' }) } 30)
        Write-Host "Descarga lanzada: $($data.packageId) $($data.version)"
        if ($confirm) { Confirm-Installer }
        else { Write-Host 'Sigue el progreso con «mah status». El instalador del sistema pide confirmación en el móvil.' }
    }
    'status' {
        $data = Show-Result (Invoke-Cli 'status' @{} 30)
        $data | ConvertTo-Json -Depth 5
    }
    default {
        Write-Host @'
mah list                         estado de todas las apps (canal, instalada, última)
mah channel <pkg> alfa|estable   cambia el canal de una app
mah check [pkg]                  consulta GitHub (todas o una)
mah update <pkg> [--confirm] [--force]     descarga e instala; --confirm pulsa Instalar/Actualizar; --force salta la comparación de versiones
mah status                       progreso de la última descarga/instalación
'@
    }
}
