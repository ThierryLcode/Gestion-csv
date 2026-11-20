# =============================================================================
#  Construit le classeur ACP_Excel.xlsm à partir des modules VBA du dossier src\
#  Prérequis : Windows + Microsoft Excel (2010 ou plus récent).
#  Lancement : double-cliquez sur construire_classeur.bat
#  En cas d'échec, le détail est écrit dans installation_journal.txt
# =============================================================================
$ErrorActionPreference = 'Stop'
$ici     = Split-Path -Parent $MyInvocation.MyCommand.Path
$src     = Join-Path $ici 'src'
$sortie  = Join-Path $ici 'ACP_Excel.xlsm'
$journal = Join-Path $ici 'installation_journal.txt'
# Seuls ces modules sont importés (les autres fichiers .bas éventuels sont ignorés)
$modules = @('modInstall.bas', 'modCSV.bas', 'modMatrice.bas', 'modGraphiques.bas', 'modACP.bas')

function Log($texte, $couleur = 'Gray') {
    Write-Host $texte -ForegroundColor $couleur
    Add-Content -Path $journal -Value $texte -Encoding UTF8
}
function Liberer($objet) {
    if ($objet) { try { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($objet) } catch {} }
}

Set-Content -Path $journal -Value "Journal d'installation ACP Excel - $(Get-Date)" -Encoding UTF8
Log "Dossier : $ici"

# 0) Vérifications préalables
foreach ($m in $modules) {
    if (-not (Test-Path (Join-Path $src $m))) {
        Log "ERREUR : fichier manquant src\$m (le dossier src doit être à côté de ce script)." 'Red'
        exit 1
    }
}
# Retire la marque « fichier téléchargé d'Internet » qui peut bloquer les fichiers
Get-ChildItem -Path $ici -Recurse -File | Unblock-File -ErrorAction SilentlyContinue

# 1) Version d'Excel (pour la clé de registre « Faire confiance au modèle objet VBA »)
try {
    $excel = New-Object -ComObject Excel.Application
} catch {
    Log "ERREUR : Microsoft Excel est introuvable sur cet ordinateur ($($_.Exception.Message))." 'Red'
    exit 1
}
$version = $excel.Version
Log "Excel version $version"
$excel.Quit(); Liberer $excel; $excel = $null
[GC]::Collect(); [GC]::WaitForPendingFinalizers()
Start-Sleep -Seconds 3

$cle = "HKCU:\Software\Microsoft\Office\$version\Excel\Security"
if (-not (Test-Path $cle)) { New-Item -Path $cle -Force | Out-Null }
$ancienne = (Get-ItemProperty -Path $cle -Name AccessVBOM -ErrorAction SilentlyContinue).AccessVBOM

$wb = $null
$code = 0
try {
    # Autorise temporairement l'accès au projet VBA (rétabli à la fin)
    Set-ItemProperty -Path $cle -Name AccessVBOM -Value 1 -Type DWord

    $excel = New-Object -ComObject Excel.Application
    $excel.Visible = $true
    $excel.DisplayAlerts = $false
    $wb = $excel.Workbooks.Add()

    try {
        $projet = $wb.VBProject
        [void]$projet.VBComponents.Count
    } catch {
        throw "Excel refuse l'accès au projet VBA. Dans Excel : Fichier > Options > Centre de gestion de la confidentialité > Paramètres > Paramètres des macros > cocher « Accès approuvé au modèle d'objet du projet VBA », puis relancez."
    }

    foreach ($m in $modules) {
        Log "  import du module $m"
        [void]$projet.VBComponents.Import((Join-Path $src $m))
    }

    Log "  construction de l'interface (macro InstallerApplication)"
    try {
        [void]$excel.Run("'" + $wb.Name + "'!InstallerApplication")
    } catch {
        throw "La macro InstallerApplication n'a pas pu s'exécuter : $($_.Exception.Message). Ouvrez l'éditeur VBA (Alt+F11) > Débogage > Compiler VBAProject pour voir la ligne en cause."
    }

    if (Test-Path $sortie) { Remove-Item $sortie -Force }
    $wb.SaveAs($sortie, 52)   # 52 = classeur prenant en charge les macros (.xlsm)
    $wb.Close($false)
    $wb = $null
    Log ""
    Log "Classeur créé : $sortie" 'Green'
    Log "Ouvrez-le et cliquez sur « Activer le contenu » pour autoriser les macros." 'Green'
}
catch {
    $code = 1
    Log ""
    Log "ÉCHEC : $($_.Exception.Message)" 'Red'
    Log "Détail enregistré dans $journal" 'Yellow'
    Log "Autre solution : méthode manuelle décrite dans README.md (import des 5 fichiers .bas avec Alt+F11)." 'Yellow'
}
finally {
    if ($wb) { try { $wb.Close($false) } catch {} }
    if ($excel) { try { $excel.Quit() } catch {} }
    Liberer $wb; Liberer $excel
    [GC]::Collect(); [GC]::WaitForPendingFinalizers()
    if ($null -eq $ancienne) {
        Remove-ItemProperty -Path $cle -Name AccessVBOM -ErrorAction SilentlyContinue
    } else {
        Set-ItemProperty -Path $cle -Name AccessVBOM -Value $ancienne -Type DWord
    }
}
exit $code
