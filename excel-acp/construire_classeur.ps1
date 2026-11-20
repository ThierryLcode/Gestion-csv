# =============================================================================
#  Construit le classeur ACP_Excel.xlsm à partir des modules VBA du dossier src\
#  Prérequis : Windows + Microsoft Excel (2010 ou plus récent).
#  Lancement : double-cliquez sur construire_classeur.bat
# =============================================================================
$ErrorActionPreference = 'Stop'
$ici    = Split-Path -Parent $MyInvocation.MyCommand.Path
$src    = Join-Path $ici 'src'
$sortie = Join-Path $ici 'ACP_Excel.xlsm'

function Liberer($objet) {
    if ($objet) { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($objet) }
}

Write-Host "Construction de $sortie"

# 1) Version d'Excel (pour la clé de registre « Faire confiance au modèle objet VBA »)
$excel = New-Object -ComObject Excel.Application
$version = $excel.Version
$excel.Quit(); Liberer $excel; $excel = $null
[GC]::Collect(); [GC]::WaitForPendingFinalizers()
Start-Sleep -Seconds 2

$cle = "HKCU:\Software\Microsoft\Office\$version\Excel\Security"
if (-not (Test-Path $cle)) { New-Item -Path $cle -Force | Out-Null }
$ancienne = (Get-ItemProperty -Path $cle -Name AccessVBOM -ErrorAction SilentlyContinue).AccessVBOM

$wb = $null
try {
    # Autorise temporairement l'accès au projet VBA (rétabli à la fin)
    Set-ItemProperty -Path $cle -Name AccessVBOM -Value 1 -Type DWord

    $excel = New-Object -ComObject Excel.Application
    $excel.Visible = $false
    $excel.DisplayAlerts = $false
    $wb = $excel.Workbooks.Add()

    Get-ChildItem -Path $src -Filter *.bas | Sort-Object Name | ForEach-Object {
        Write-Host "  import du module $($_.Name)"
        [void]$wb.VBProject.VBComponents.Import($_.FullName)
    }

    Write-Host "  construction de l'interface"
    [void]$excel.Run("InstallerApplication")

    if (Test-Path $sortie) { Remove-Item $sortie -Force }
    $wb.SaveAs($sortie, 52)   # 52 = classeur prenant en charge les macros (.xlsm)
    $wb.Close($false)
    Write-Host ""
    Write-Host "Classeur créé : $sortie" -ForegroundColor Green
    Write-Host "Ouvrez-le et cliquez sur « Activer le contenu » pour autoriser les macros."
}
catch {
    Write-Host ""
    Write-Host "ÉCHEC : $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "Si l'accès au projet VBA est refusé : Excel > Fichier > Options > Centre de gestion de la confidentialité >"
    Write-Host "Paramètres > Paramètres des macros > cocher « Accès approuvé au modèle d'objet du projet VBA »,"
    Write-Host "ou suivez la méthode manuelle décrite dans README.md."
    exit 1
}
finally {
    if ($excel) { $excel.Quit() }
    Liberer $wb; Liberer $excel
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($null -eq $ancienne) {
        Remove-ItemProperty -Path $cle -Name AccessVBOM -ErrorAction SilentlyContinue
    } else {
        Set-ItemProperty -Path $cle -Name AccessVBOM -Value $ancienne -Type DWord
    }
}
