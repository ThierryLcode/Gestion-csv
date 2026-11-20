Attribute VB_Name = "modInstall"
' =============================================================================
'  ACP Excel - Analyse en Composantes Principales à partir d'un fichier CSV
'  Module : installation de l'interface (feuilles, paramètres, boutons)
' =============================================================================
Option Explicit

' --- Noms des feuilles -------------------------------------------------------
Public Const F_ACCUEIL As String = "Accueil"
Public Const F_DONNEES As String = "Données"
Public Const F_LISTES As String = "Listes"
Public Const F_SYNTHESE As String = "ACP_Synthèse"
Public Const F_VARIABLES As String = "ACP_Variables"
Public Const F_INDIVIDUS As String = "ACP_Individus"
Public Const F_GRAPH As String = "ACP_Graph"          ' ancienne version (supprimée si présente)
Public Const F_GRAPH_SYN As String = "ACP_GraphSyn"   ' données des graphiques (masquées)
Public Const F_GRAPH_VAR As String = "ACP_GraphVar"
Public Const F_GRAPH_IND As String = "ACP_GraphInd"
Public Const F_STOCK As String = "ACP_Stock"          ' résultats conservés pour redessiner

' Sélecteurs d'axes sur les feuilles ACP_Variables et ACP_Individus
Public Const CEL_AXE_X As String = "C3"
Public Const CEL_AXE_Y As String = "E3"

' --- Cellules de paramètres (feuille Accueil) --------------------------------
Public Const P_FICHIER As String = "C6"
Public Const P_SEPARATEUR As String = "C7"
Public Const P_DECIMAL As String = "C8"
Public Const P_ENCODAGE As String = "C9"
Public Const P_ENTETES As String = "C10"
Public Const P_TYPE_ACP As String = "C13"
Public Const P_MANQUANTES As String = "C14"
Public Const P_AXE_X As String = "C15"
Public Const P_AXE_Y As String = "C16"
Public Const P_NB_AXES As String = "C17"
Public Const P_MAX_ETIQ As String = "C18"

' --- Tableau des colonnes (feuille Accueil) ----------------------------------
Public Const LIGNE_ENTETE_COLONNES As Long = 23
Public Const COL_T_NUM As Long = 2
Public Const COL_T_NOM As Long = 3
Public Const COL_T_TYPE As Long = 4
Public Const COL_T_NUMERIQUES As Long = 5
Public Const COL_T_TEXTES As Long = 6
Public Const COL_T_VIDES As Long = 7
Public Const COL_T_ROLE As Long = 8

' --- Rôles possibles d'une colonne -------------------------------------------
Public Const ROLE_ACTIVE As String = "Active"
Public Const ROLE_SUPP As String = "Supplémentaire"
Public Const ROLE_ETIQ As String = "Étiquette des individus"
Public Const ROLE_GROUPE As String = "Groupe (couleur)"
Public Const ROLE_IGNOREE As String = "Ignorée"

Public Const ERR_ACP As Long = vbObjectError + 600

' Couleurs de l'interface
Public Function CouleurTitre() As Long
    CouleurTitre = RGB(13, 54, 107)
End Function

Public Function CouleurBouton() As Long
    CouleurBouton = RGB(42, 120, 214)
End Function

' =============================================================================
'  Point d'entrée : construit (ou reconstruit) l'interface du classeur.
' =============================================================================
Public Sub InstallerApplication()
    Dim wb As Workbook, i As Long, nom As String
    Set wb = ThisWorkbook

    Application.ScreenUpdating = False
    Application.DisplayAlerts = False

    ObtenirFeuille F_ACCUEIL, True
    ObtenirFeuille F_DONNEES, True
    ObtenirFeuille F_LISTES, True

    ' Supprime les feuilles vides par défaut (Feuil1, Sheet1...)
    For i = wb.Worksheets.Count To 1 Step -1
        nom = wb.Worksheets(i).Name
        If nom <> F_ACCUEIL And nom <> F_DONNEES And nom <> F_LISTES And Left$(nom, 4) <> "ACP_" Then
            If Application.WorksheetFunction.CountA(wb.Worksheets(i).Cells) = 0 _
               And wb.Worksheets(i).Shapes.Count = 0 Then
                wb.Worksheets(i).Delete
            End If
        End If
    Next i

    wb.Worksheets(F_ACCUEIL).Move Before:=wb.Worksheets(1)
    wb.Worksheets(F_DONNEES).Move After:=wb.Worksheets(F_ACCUEIL)

    PreparerListes
    ConstruireAccueil

    With wb.Worksheets(F_DONNEES)
        If Application.WorksheetFunction.CountA(.Cells) = 0 Then
            .Range("A1").Value = "Les données importées depuis le fichier CSV apparaîtront ici."
            .Range("A1").Font.Italic = True
            .Range("A1").Font.Color = RGB(120, 120, 120)
        End If
    End With

    wb.Worksheets(F_LISTES).Visible = xlSheetHidden
    AllerA wb.Worksheets(F_ACCUEIL), "A1"

    Application.DisplayAlerts = True
    Application.ScreenUpdating = True
End Sub

' Renvoie la feuille demandée (Nothing si absente), la crée si demandé.
Public Function ObtenirFeuille(ByVal nom As String, Optional ByVal creer As Boolean = False) As Worksheet
    Dim ws As Worksheet
    On Error Resume Next
    Set ws = ThisWorkbook.Worksheets(nom)
    On Error GoTo 0
    If ws Is Nothing And creer Then
        ' Worksheets.Add active la nouvelle feuille : on réactive ensuite la feuille d'origine
        Dim actif As Object
        Set actif = ActiveSheet
        Set ws = ThisWorkbook.Worksheets.Add(After:=ThisWorkbook.Worksheets(ThisWorkbook.Worksheets.Count))
        ws.Name = nom
        If Not actif Is Nothing Then
            On Error Resume Next
            actif.Activate
            On Error GoTo 0
        End If
    End If
    Set ObtenirFeuille = ws
End Function

' Active une feuille et sélectionne une cellule, sans jamais interrompre la macro
Public Sub AllerA(ws As Worksheet, ByVal adresse As String)
    On Error Resume Next
    If ws.Visible <> xlSheetVisible Then Exit Sub
    ws.Activate
    ws.Range(adresse).Select
    On Error GoTo 0
End Sub

' Dernière ligne / colonne non vide d'une feuille (0 si la feuille est vide)
Public Function DerniereLigne(ws As Worksheet) As Long
    Dim c As Range
    Set c = ws.Cells.Find(What:="*", After:=ws.Cells(1, 1), LookIn:=xlFormulas, _
                          SearchOrder:=xlByRows, SearchDirection:=xlPrevious)
    If c Is Nothing Then DerniereLigne = 0 Else DerniereLigne = c.Row
End Function

Public Function DerniereColonne(ws As Worksheet) As Long
    Dim c As Range
    Set c = ws.Cells.Find(What:="*", After:=ws.Cells(1, 1), LookIn:=xlFormulas, _
                          SearchOrder:=xlByColumns, SearchDirection:=xlPrevious)
    If c Is Nothing Then DerniereColonne = 0 Else DerniereColonne = c.Column
End Function

' -----------------------------------------------------------------------------
'  Listes de choix (feuille masquée « Listes »)
' -----------------------------------------------------------------------------
Private Sub PreparerListes()
    Dim ws As Worksheet, i As Long
    Set ws = ObtenirFeuille(F_LISTES, True)
    ws.Cells.Clear
    EcrireListe ws, 1, Array("Auto", "Point-virgule ( ; )", "Virgule ( , )", "Tabulation", "Barre verticale ( | )")
    EcrireListe ws, 2, Array("Auto", "Virgule ( , )", "Point ( . )")
    EcrireListe ws, 3, Array("Auto", "UTF-8", "Windows-1252 (ANSI)")
    EcrireListe ws, 4, Array("Oui", "Non")
    EcrireListe ws, 5, Array("Normée (centrée-réduite)", "Non normée (centrée)")
    EcrireListe ws, 6, Array("Exclure l'individu", "Remplacer par la moyenne")
    EcrireListe ws, 7, Array(ROLE_ACTIVE, ROLE_SUPP, ROLE_ETIQ, ROLE_GROUPE, ROLE_IGNOREE)
    For i = 1 To 20
        ws.Cells(i, 8).Value = i
    Next i
End Sub

Private Sub EcrireListe(ws As Worksheet, ByVal col As Long, valeurs As Variant)
    Dim i As Long
    For i = LBound(valeurs) To UBound(valeurs)
        ws.Cells(i - LBound(valeurs) + 1, col).Value = "'" & valeurs(i)
    Next i
End Sub

' Pose une liste déroulante pointant vers une colonne de la feuille Listes
Public Sub PoserListe(cible As Range, ByVal colListe As String, ByVal nb As Long)
    With cible.Validation
        .Delete
        .Add Type:=xlValidateList, AlertStyle:=xlValidAlertStop, _
             Formula1:="=" & F_LISTES & "!$" & colListe & "$1:$" & colListe & "$" & nb
        .IgnoreBlank = True
        .InCellDropdown = True
    End With
End Sub

' -----------------------------------------------------------------------------
'  Feuille Accueil
' -----------------------------------------------------------------------------
Private Sub ConstruireAccueil()
    Dim ws As Worksheet, i As Long
    Set ws = ObtenirFeuille(F_ACCUEIL, True)
    ws.Activate
    ws.Cells.Clear
    For i = ws.Shapes.Count To 1 Step -1
        ws.Shapes(i).Delete
    Next i
    On Error Resume Next
    ActiveWindow.DisplayGridlines = False
    On Error GoTo 0

    ws.Columns("A").ColumnWidth = 2
    ws.Columns("B").ColumnWidth = 38
    ws.Columns("C").ColumnWidth = 34
    ws.Columns("D").ColumnWidth = 30
    ws.Columns("E:G").ColumnWidth = 14
    ws.Columns("H").ColumnWidth = 26

    With ws.Range("B2")
        .Value = "Analyse en Composantes Principales (ACP) d'un fichier CSV"
        .Font.Size = 18
        .Font.Bold = True
        .Font.Color = CouleurTitre()
    End With
    With ws.Range("B3")
        .Value = "1) Importez un fichier CSV     2) Choisissez le rôle de chaque colonne (tableau ci-dessous)     3) Cliquez sur « Lancer l'ACP »"
        .Font.Italic = True
        .Font.Color = RGB(90, 90, 90)
    End With

    TitreSection ws, 5, "Import du fichier CSV"
    Libelle ws, 6, "Fichier importé", "(aucun)", False
    Libelle ws, 7, "Séparateur de champs", "Auto", True
    PoserListe ws.Range(P_SEPARATEUR), "A", 5
    Libelle ws, 8, "Séparateur décimal", "Auto", True
    PoserListe ws.Range(P_DECIMAL), "B", 3
    Libelle ws, 9, "Encodage du fichier", "Auto", True
    PoserListe ws.Range(P_ENCODAGE), "C", 3
    Libelle ws, 10, "Première ligne = noms des colonnes", "Oui", True
    PoserListe ws.Range(P_ENTETES), "D", 2

    TitreSection ws, 12, "Paramètres de l'ACP"
    Libelle ws, 13, "Type d'ACP", "Normée (centrée-réduite)", True
    PoserListe ws.Range(P_TYPE_ACP), "E", 2
    Libelle ws, 14, "Valeurs manquantes", "Exclure l'individu", True
    PoserListe ws.Range(P_MANQUANTES), "F", 2
    Libelle ws, 15, "Axe horizontal des graphiques", 1, True
    PoserListe ws.Range(P_AXE_X), "H", 20
    Libelle ws, 16, "Axe vertical des graphiques", 2, True
    PoserListe ws.Range(P_AXE_Y), "H", 20
    Libelle ws, 17, "Nombre d'axes détaillés dans les tableaux", 5, True
    PoserListe ws.Range(P_NB_AXES), "H", 20
    Libelle ws, 18, "Étiquettes max. sur le graphique des individus", 100, True
    With ws.Range(P_MAX_ETIQ).Validation
        .Delete
        .Add Type:=xlValidateWholeNumber, AlertStyle:=xlValidAlertStop, _
             Operator:=xlBetween, Formula1:="0", Formula2:="5000"
    End With
    ws.Range("C15:C18").HorizontalAlignment = xlLeft

    ' Aide sur les paramètres
    With ws.Range("D13")
        .Value = "Normée : variables d'unités différentes (cas usuel)."
    End With
    ws.Range("D14").Value = "Les cellules vides ou non numériques sont manquantes."
    ws.Range("D15").Value = "Plan initial des graphiques : modifiable ensuite"
    ws.Range("D16").Value = "sur les feuilles ACP_Variables et ACP_Individus."
    ws.Range("D17").Value = "Les 10 premiers axes au moins restent disponibles."
    ws.Range("D18").Value = "Au-delà, seuls les plus contributifs sont nommés."
    ws.Range("D13:D18").Font.Color = RGB(110, 110, 110)
    ws.Range("D13:D18").Font.Italic = True
    ws.Range("D13:D18").Font.Size = 9

    ' Boutons
    AjouterBouton ws, "E6:G7", "Importer un fichier CSV", "ImporterCSV", CouleurBouton()
    AjouterBouton ws, "E9:G10", "Actualiser la liste des colonnes", "ActualiserColonnes", RGB(86, 152, 231)
    AjouterBouton ws, "E13:G15", "Lancer l'ACP", "LancerACP", RGB(0, 131, 0)
    AjouterBouton ws, "E16:G17", "Effacer les résultats", "EffacerResultats", RGB(120, 120, 120)

    ' Tableau des colonnes
    TitreSection ws, 20, "Colonnes du fichier et rôle dans l'ACP"
    With ws.Range("B21")
        .Value = "Active : entre dans le calcul des axes  |  Supplémentaire : projetée sur le cercle sans influencer les axes  |  " & _
                 "Étiquette : nom des individus  |  Groupe : colore les individus (texte, 8 modalités max.)  |  Ignorée : non utilisée"
        .Font.Size = 9
        .Font.Color = RGB(90, 90, 90)
    End With
    AjouterBouton ws, "E22:F22", "Activer les numériques", "SelectionnerNumeriques", RGB(86, 152, 231)
    AjouterBouton ws, "G22:H22", "Tout ignorer", "ToutIgnorer", RGB(120, 120, 120)
    ws.Rows(22).RowHeight = 24

    EnteteTableauColonnes ws
End Sub

Public Sub EnteteTableauColonnes(ws As Worksheet)
    Dim r As Range
    Set r = ws.Range(ws.Cells(LIGNE_ENTETE_COLONNES, COL_T_NUM), ws.Cells(LIGNE_ENTETE_COLONNES, COL_T_ROLE))
    r.Value = Array("N°", "Colonne", "Type détecté", "Valeurs numériques", "Valeurs texte", "Vides", "Rôle dans l'ACP")
    r.Font.Bold = True
    r.Font.Color = RGB(255, 255, 255)
    r.Interior.Color = CouleurTitre()
    r.HorizontalAlignment = xlCenter
    ws.Cells(LIGNE_ENTETE_COLONNES, COL_T_NUM).ColumnWidth = 38
End Sub

Private Sub TitreSection(ws As Worksheet, ByVal ligne As Long, ByVal texte As String)
    With ws.Range(ws.Cells(ligne, 2), ws.Cells(ligne, 8))
        .Interior.Color = RGB(205, 226, 251)
        .Borders(xlEdgeBottom).Color = CouleurTitre()
    End With
    With ws.Cells(ligne, 2)
        .Value = texte
        .Font.Bold = True
        .Font.Size = 12
        .Font.Color = CouleurTitre()
    End With
End Sub

Private Sub Libelle(ws As Worksheet, ByVal ligne As Long, ByVal texte As String, ByVal valeur As Variant, ByVal saisie As Boolean)
    ws.Cells(ligne, 2).Value = texte
    ws.Cells(ligne, 3).Value = valeur
    If saisie Then
        With ws.Cells(ligne, 3)
            .Interior.Color = RGB(255, 249, 219)
            .Borders.Color = RGB(200, 200, 200)
            .Font.Color = RGB(13, 54, 107)
            .Locked = False
        End With
    Else
        ws.Cells(ligne, 3).Font.Color = RGB(90, 90, 90)
    End If
End Sub

Public Sub AjouterBouton(ws As Worksheet, ByVal adresse As String, ByVal texte As String, ByVal macro As String, ByVal couleur As Long)
    Dim r As Range, shp As Shape
    Set r = ws.Range(adresse)
    Set shp = ws.Shapes.AddShape(msoShapeRoundedRectangle, r.Left + 2, r.Top + 2, r.Width - 4, r.Height - 4)
    With shp
        .Name = "btn_" & macro
        .Fill.ForeColor.RGB = couleur
        .Line.Visible = msoFalse
        .Placement = xlMove
        With .TextFrame2
            .VerticalAnchor = msoAnchorMiddle
            .MarginLeft = 2
            .MarginRight = 2
            .TextRange.Text = texte
            .TextRange.Font.Size = 10
            .TextRange.Font.Bold = msoTrue
            .TextRange.Font.Fill.ForeColor.RGB = RGB(255, 255, 255)
            .TextRange.ParagraphFormat.Alignment = msoAlignCenter
        End With
        .OnAction = macro
    End With
End Sub

' Supprime les feuilles de résultats
Public Sub EffacerResultats()
    Application.ScreenUpdating = False
    SupprimerResultats
    ThisWorkbook.Worksheets(F_ACCUEIL).Activate
    Application.ScreenUpdating = True
End Sub

Public Sub SupprimerResultats()
    Dim noms As Variant, i As Long, ws As Worksheet
    noms = Array(F_SYNTHESE, F_VARIABLES, F_INDIVIDUS, F_GRAPH, F_GRAPH_SYN, F_GRAPH_VAR, F_GRAPH_IND, F_STOCK)
    Application.DisplayAlerts = False
    For i = LBound(noms) To UBound(noms)
        Set ws = ObtenirFeuille(CStr(noms(i)))
        If Not ws Is Nothing Then
            If ws.Visible <> xlSheetVisible Then ws.Visible = xlSheetVisible
            ws.Delete
        End If
    Next i
    Application.DisplayAlerts = True
End Sub
