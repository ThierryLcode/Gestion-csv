Attribute VB_Name = "modCSV"
' =============================================================================
'  ACP Excel - Module : import du fichier CSV et choix des colonnes
' =============================================================================
Option Explicit

' =============================================================================
'  Importer un fichier CSV dans la feuille « Données »
' =============================================================================
Public Sub ImporterCSV()
    Dim fichier As Variant, texte As String, sep As String, dec As String
    Dim brut As Variant, donnees() As Variant
    Dim nL As Long, nC As Long, nbData As Long, debut As Long
    Dim i As Long, j As Long, ok As Boolean, x As Double, s As String
    Dim entetes As Boolean, encodage As String, noms As Collection
    Dim wsA As Worksheet, wsD As Worksheet, nbLignesAffichees As Long

    Set wsA = ObtenirFeuille(F_ACCUEIL)
    If wsA Is Nothing Then
        MsgBox "Exécutez d'abord la macro InstallerApplication.", vbExclamation
        Exit Sub
    End If

    fichier = Application.GetOpenFilename( _
        "Fichiers CSV (*.csv;*.txt;*.tsv),*.csv;*.txt;*.tsv,Tous les fichiers (*.*),*.*", 1, _
        "Choisir le fichier CSV à analyser")
    If VarType(fichier) = vbBoolean Then Exit Sub

    On Error GoTo Erreur
    Application.ScreenUpdating = False
    Application.StatusBar = "Lecture du fichier..."

    texte = LireFichierTexte(CStr(fichier), CStr(wsA.Range(P_ENCODAGE).Value), encodage)
    texte = Replace(texte, vbCrLf, vbLf)
    texte = Replace(texte, vbCr, vbLf)
    If Len(Trim$(Replace(texte, vbLf, ""))) = 0 Then Err.Raise ERR_ACP, , "Le fichier est vide."

    sep = ChoisirSeparateur(CStr(wsA.Range(P_SEPARATEUR).Value), texte)
    Application.StatusBar = "Découpage des lignes..."
    brut = ParserCSV(texte, sep)
    texte = vbNullString
    nL = UBound(brut, 1)
    nC = UBound(brut, 2)
    dec = ChoisirDecimal(CStr(wsA.Range(P_DECIMAL).Value), sep, brut)
    entetes = (UCase$(Trim$(CStr(wsA.Range(P_ENTETES).Value))) <> "NON")

    If entetes Then
        debut = 2
        nbData = nL - 1
    Else
        debut = 1
        nbData = nL
    End If
    If nbData < 1 Then Err.Raise ERR_ACP, , "Le fichier ne contient aucune ligne de données."
    If nbData + 1 > ThisWorkbook.Worksheets(F_DONNEES).Rows.Count Then
        Err.Raise ERR_ACP, , "Le fichier contient trop de lignes pour une feuille Excel (" & nbData & ")."
    End If
    If nC > ThisWorkbook.Worksheets(F_DONNEES).Columns.Count Then
        Err.Raise ERR_ACP, , "Le fichier contient trop de colonnes pour une feuille Excel (" & nC & ")."
    End If

    Application.StatusBar = "Conversion des nombres..."
    ReDim donnees(1 To nbData + 1, 1 To nC)
    Set noms = New Collection
    For j = 1 To nC
        s = ""
        If entetes Then s = Trim$(CStr(brut(1, j)))
        If Len(s) = 0 Then s = "Colonne " & j
        donnees(1, j) = "'" & NomUnique(s, noms)
    Next j
    For i = debut To nL
        For j = 1 To nC
            s = Trim$(CStr(brut(i, j)))
            If Len(s) = 0 Then
                donnees(i - debut + 2, j) = Empty
            Else
                x = ConvertirNombre(s, dec, ok)
                If ok Then
                    donnees(i - debut + 2, j) = x
                Else
                    donnees(i - debut + 2, j) = "'" & Left$(s, 32000)
                End If
            End If
        Next j
        If (i Mod 20000) = 0 Then Application.StatusBar = "Conversion des nombres... ligne " & i & " / " & nL
    Next i
    brut = Empty

    Application.StatusBar = "Écriture dans la feuille Données..."
    Set wsD = ObtenirFeuille(F_DONNEES, True)
    wsD.Cells.Clear
    wsD.Range("A1").Resize(nbData + 1, nC).Value = donnees
    Erase donnees
    With wsD.Range("A1").Resize(1, nC)
        .Font.Bold = True
        .Font.Color = RGB(255, 255, 255)
        .Interior.Color = CouleurTitre()
    End With
    nbLignesAffichees = nbData + 1
    If nbLignesAffichees > 300 Then nbLignesAffichees = 300
    If nC <= 300 Then wsD.Range("A1").Resize(nbLignesAffichees, nC).Columns.AutoFit

    wsA.Range(P_FICHIER).Value = CStr(fichier)
    wsA.Range("D7").Value = "détecté : " & NomSeparateur(sep)
    wsA.Range("D8").Value = "détecté : " & IIf(dec = ",", "virgule", "point")
    wsA.Range("D9").Value = "détecté : " & encodage
    With wsA.Range("D7:D9")
        .Font.Size = 9
        .Font.Italic = True
        .Font.Color = RGB(110, 110, 110)
    End With

    ActualiserColonnes
    Application.StatusBar = False
    Application.ScreenUpdating = True
    wsA.Activate
    MsgBox "Import terminé : " & Format$(nbData, "#,##0") & " lignes et " & nC & " colonnes." & vbCrLf & vbCrLf & _
           "Séparateur : " & NomSeparateur(sep) & "   |   Décimales : " & IIf(dec = ",", "virgule", "point") & _
           "   |   Encodage : " & encodage & vbCrLf & vbCrLf & _
           "Vérifiez le rôle de chaque colonne puis cliquez sur « Lancer l'ACP ».", vbInformation, "ACP"
    Exit Sub

Erreur:
    Application.StatusBar = False
    Application.ScreenUpdating = True
    MsgBox "Import impossible : " & Err.Description, vbCritical, "ACP"
End Sub

Private Function NomSeparateur(ByVal sep As String) As String
    Select Case sep
        Case ";": NomSeparateur = "point-virgule"
        Case ",": NomSeparateur = "virgule"
        Case vbTab: NomSeparateur = "tabulation"
        Case "|": NomSeparateur = "barre verticale"
        Case Else: NomSeparateur = sep
    End Select
End Function

Private Function NomUnique(ByVal s As String, noms As Collection) As String
    Dim t As String, k As Long
    t = s
    k = 1
    Do While ExisteCle(noms, t)
        k = k + 1
        t = s & " (" & k & ")"
    Loop
    noms.Add t, t
    NomUnique = t
End Function

Public Function ExisteCle(c As Collection, ByVal cle As String) As Boolean
    Dim v As Variant
    On Error Resume Next
    v = c.Item(cle)
    ExisteCle = (Err.Number = 0)
    Err.Clear
End Function

' -----------------------------------------------------------------------------
'  Lecture du fichier avec détection de l'encodage
' -----------------------------------------------------------------------------
Private Function LireFichierTexte(ByVal chemin As String, ByVal choix As String, ByRef utilise As String) As String
    Dim octets() As Byte, ff As Integer, taille As Long, charset As String, t As String

    ff = FreeFile
    Open chemin For Binary Access Read Shared As #ff
    taille = LOF(ff)
    If taille = 0 Then
        Close #ff
        utilise = "-"
        Exit Function
    End If
    ReDim octets(0 To taille - 1)
    Get #ff, , octets
    Close #ff

    If taille >= 2 And octets(0) = &HFF And octets(1) = &HFE Then
        charset = "unicode"
        utilise = "UTF-16"
    ElseIf InStr(1, choix, "UTF", vbTextCompare) > 0 Then
        charset = "utf-8"
        utilise = "UTF-8"
    ElseIf InStr(1, choix, "1252", vbTextCompare) > 0 Then
        charset = "windows-1252"
        utilise = "Windows-1252"
    ElseIf EstUtf8(octets) Then
        charset = "utf-8"
        utilise = "UTF-8"
    Else
        charset = "windows-1252"
        utilise = "Windows-1252"
    End If

    t = DecoderOctets(octets, charset)
    If Len(t) > 0 Then
        If AscW(Left$(t, 1)) = &HFEFF Then t = Mid$(t, 2)
    End If
    LireFichierTexte = t
End Function

' Vérifie que les octets forment de l'UTF-8 valide
Private Function EstUtf8(ByRef b() As Byte) As Boolean
    Dim i As Long, n As Long, k As Long, suite As Long, c As Long
    n = UBound(b) + 1
    If n >= 3 Then
        If b(0) = &HEF And b(1) = &HBB And b(2) = &HBF Then
            EstUtf8 = True
            Exit Function
        End If
    End If
    i = 0
    Do While i < n
        c = b(i)
        If c < &H80 Then
            suite = 0
        ElseIf c >= &HC2 And c <= &HDF Then
            suite = 1
        ElseIf c >= &HE0 And c <= &HEF Then
            suite = 2
        ElseIf c >= &HF0 And c <= &HF4 Then
            suite = 3
        Else
            Exit Function
        End If
        For k = 1 To suite
            If i + k >= n Then Exit Function
            If b(i + k) < &H80 Or b(i + k) > &HBF Then Exit Function
        Next k
        i = i + suite + 1
    Loop
    EstUtf8 = True
End Function

Private Function DecoderOctets(ByRef b() As Byte, ByVal charset As String) As String
    Dim st As Object
    On Error GoTo SansADO
    Set st = CreateObject("ADODB.Stream")
    st.Type = 1
    st.Open
    st.Write b
    st.Position = 0
    st.Type = 2
    st.Charset = charset
    DecoderOctets = st.ReadText(-1)
    st.Close
    Exit Function
SansADO:
    ' ADODB indisponible : décodage selon la page de code du système
    If charset = "unicode" Then
        DecoderOctets = CStr(b)
    Else
        DecoderOctets = StrConv(b, vbUnicode)
    End If
End Function

' -----------------------------------------------------------------------------
'  Détection du séparateur de champs et du séparateur décimal
' -----------------------------------------------------------------------------
Private Function ChoisirSeparateur(ByVal choix As String, ByRef texte As String) As String
    Dim ligne As String, p As Long, candidats As Variant, i As Long, nb As Long, meilleur As Long

    If InStr(1, choix, "Point-virgule", vbTextCompare) > 0 Then
        ChoisirSeparateur = ";": Exit Function
    ElseIf InStr(1, choix, "Virgule", vbTextCompare) > 0 Then
        ChoisirSeparateur = ",": Exit Function
    ElseIf InStr(1, choix, "Tabulation", vbTextCompare) > 0 Then
        ChoisirSeparateur = vbTab: Exit Function
    ElseIf InStr(1, choix, "Barre", vbTextCompare) > 0 Then
        ChoisirSeparateur = "|": Exit Function
    End If

    p = InStr(1, texte, vbLf)
    If p = 0 Then ligne = texte Else ligne = Left$(texte, p - 1)
    candidats = Array(";", vbTab, ",", "|")
    ChoisirSeparateur = ";"
    meilleur = 0
    For i = LBound(candidats) To UBound(candidats)
        nb = CompterHorsGuillemets(ligne, CStr(candidats(i)))
        If nb > meilleur Then
            meilleur = nb
            ChoisirSeparateur = candidats(i)
        End If
    Next i
End Function

Private Function CompterHorsGuillemets(ByVal ligne As String, ByVal c As String) As Long
    Dim i As Long, dedans As Boolean, ch As String
    For i = 1 To Len(ligne)
        ch = Mid$(ligne, i, 1)
        If ch = """" Then
            dedans = Not dedans
        ElseIf ch = c And Not dedans Then
            CompterHorsGuillemets = CompterHorsGuillemets + 1
        End If
    Next i
End Function

Private Function ChoisirDecimal(ByVal choix As String, ByVal sep As String, ByRef brut As Variant) As String
    Dim i As Long, j As Long, s As String, nV As Long, nP As Long, ok As Boolean, x As Double, fin As Long

    If InStr(1, choix, "Virgule", vbTextCompare) > 0 Then
        ChoisirDecimal = ",": Exit Function
    ElseIf InStr(1, choix, "Point", vbTextCompare) > 0 Then
        ChoisirDecimal = ".": Exit Function
    End If
    If sep = "," Then
        ChoisirDecimal = ".": Exit Function
    End If

    fin = UBound(brut, 1)
    If fin > 1000 Then fin = 1000
    For i = 1 To fin
        For j = 1 To UBound(brut, 2)
            s = Trim$(CStr(brut(i, j)))
            If InStr(s, ",") > 0 And InStr(s, ".") = 0 Then
                x = ConvertirNombre(s, ",", ok)
                If ok Then nV = nV + 1
            ElseIf InStr(s, ".") > 0 And InStr(s, ",") = 0 Then
                x = ConvertirNombre(s, ".", ok)
                If ok Then nP = nP + 1
            End If
        Next j
    Next i
    If nV > nP Then ChoisirDecimal = "," Else ChoisirDecimal = "."
End Function

' -----------------------------------------------------------------------------
'  Conversion texte -> nombre, indépendante des paramètres régionaux
' -----------------------------------------------------------------------------
Public Function ConvertirNombre(ByVal s As String, ByVal dec As String, ByRef ok As Boolean) As Double
    Dim t As String
    ok = False
    t = Replace(s, ChrW(160), "")
    t = Replace(t, ChrW(8239), "")
    t = Replace(t, " ", "")
    t = Replace(t, vbTab, "")
    If Len(t) = 0 Then Exit Function
    If Right$(t, 1) = "%" Then t = Left$(t, Len(t) - 1)
    If Len(t) = 0 Then Exit Function

    If dec = "," Then
        If InStr(t, ",") > 0 Then
            t = Replace(t, ".", "")
            t = Replace(t, ",", ".")
        End If
    Else
        If InStr(t, ".") > 0 Then
            t = Replace(t, ",", "")
        Else
            t = Replace(t, ",", ".")
        End If
    End If

    t = UCase$(t)
    If Not FormeNumeriqueValide(t) Then Exit Function
    On Error GoTo Invalide
    ConvertirNombre = Val(t)
    ok = True
    Exit Function
Invalide:
    ok = False
    ConvertirNombre = 0
End Function

Private Function FormeNumeriqueValide(ByVal t As String) As Boolean
    Dim i As Long, n As Long, ch As String, debut As Long
    Dim chiffres As Long, chiffresExpo As Long, point As Boolean, expo As Boolean

    n = Len(t)
    If n = 0 Then Exit Function
    debut = 1
    ch = Left$(t, 1)
    If ch = "+" Or ch = "-" Then debut = 2
    i = debut
    Do While i <= n
        ch = Mid$(t, i, 1)
        If ch >= "0" And ch <= "9" Then
            If expo Then chiffresExpo = chiffresExpo + 1 Else chiffres = chiffres + 1
        ElseIf ch = "." Then
            If point Or expo Then Exit Function
            point = True
        ElseIf ch = "E" Then
            If expo Or chiffres = 0 Then Exit Function
            expo = True
            If i < n Then
                If Mid$(t, i + 1, 1) = "+" Or Mid$(t, i + 1, 1) = "-" Then i = i + 1
            End If
        Else
            Exit Function
        End If
        i = i + 1
    Loop
    If chiffres = 0 Then Exit Function
    If expo And chiffresExpo = 0 Then Exit Function
    FormeNumeriqueValide = True
End Function

' Valeur numérique d'une cellule (nombre ou texte convertible)
Public Function LireNombre(ByVal v As Variant, ByRef x As Double) As Boolean
    Dim ok As Boolean
    If IsError(v) Or IsEmpty(v) Then Exit Function
    Select Case VarType(v)
        Case vbDouble, vbSingle, vbInteger, vbLong, vbCurrency, vbDecimal, vbByte
            x = CDbl(v)
            LireNombre = True
        Case vbString
            x = ConvertirNombre(CStr(v), ",", ok)
            LireNombre = ok
    End Select
End Function

Public Function TexteCellule(ByVal v As Variant) As String
    If IsError(v) Then
        TexteCellule = ""
    ElseIf IsEmpty(v) Or IsNull(v) Then
        TexteCellule = ""
    Else
        TexteCellule = Trim$(CStr(v))
    End If
End Function

' -----------------------------------------------------------------------------
'  Découpage CSV (RFC 4180 : champs entre guillemets, "" = guillemet,
'  retours à la ligne autorisés dans les champs entre guillemets)
'  Le texte doit utiliser vbLf comme fin de ligne.
' -----------------------------------------------------------------------------
Private Function ParserCSV(ByRef t As String, ByVal sep As String) As Variant
    Dim lignes As Collection, champs() As String, nb As Long, maxCols As Long
    Dim i As Long, L As Long, j As Long, fin As Long, pSep As Long, pLf As Long
    Dim valeur As String, ligne() As String, k As Long, res() As Variant, ligneVar As Variant, r As Long

    Set lignes = New Collection
    L = Len(t)
    i = 1
    nb = 0
    ReDim champs(0 To 31)

    Do
        If i <= L And Mid$(t, i, 1) = """" Then
            ' Champ entre guillemets
            j = i + 1
            Do
                j = InStr(j, t, """")
                If j = 0 Then
                    j = L + 1
                    Exit Do
                End If
                If Mid$(t, j + 1, 1) = """" Then
                    j = j + 2
                Else
                    Exit Do
                End If
            Loop
            valeur = Replace(Mid$(t, i + 1, j - i - 1), """""", """")
            i = j + 1
            fin = ProchainDelimiteur(t, i, sep, L)
            If fin > i Then valeur = valeur & Mid$(t, i, fin - i)
            i = fin
        Else
            fin = ProchainDelimiteur(t, i, sep, L)
            valeur = Mid$(t, i, fin - i)
            i = fin
        End If

        If nb > UBound(champs) Then ReDim Preserve champs(0 To 2 * UBound(champs) + 1)
        champs(nb) = valeur
        nb = nb + 1

        If i > L Then
            AjouterLigne lignes, champs, nb, maxCols
            Exit Do
        ElseIf Mid$(t, i, 1) = sep Then
            i = i + 1
        Else
            AjouterLigne lignes, champs, nb, maxCols
            nb = 0
            i = i + 1
            If i > L Then Exit Do
        End If
    Loop

    If lignes.Count = 0 Then Err.Raise ERR_ACP, , "Aucune ligne lisible dans le fichier."
    ReDim res(1 To lignes.Count, 1 To maxCols)
    r = 0
    For Each ligneVar In lignes
        r = r + 1
        For k = 0 To UBound(ligneVar)
            res(r, k + 1) = ligneVar(k)
        Next k
    Next ligneVar
    ParserCSV = res
End Function

Private Function ProchainDelimiteur(ByRef t As String, ByVal i As Long, ByVal sep As String, ByVal L As Long) As Long
    Dim pSep As Long, pLf As Long
    pSep = InStr(i, t, sep)
    pLf = InStr(i, t, vbLf)
    If pSep = 0 Then pSep = L + 1
    If pLf = 0 Then pLf = L + 1
    If pSep < pLf Then ProchainDelimiteur = pSep Else ProchainDelimiteur = pLf
End Function

Private Sub AjouterLigne(lignes As Collection, champs() As String, ByVal nb As Long, ByRef maxCols As Long)
    Dim ligne() As String, k As Long
    If nb = 0 Then Exit Sub
    If nb = 1 And Len(Trim$(champs(0))) = 0 Then Exit Sub   ' ligne vide
    ReDim ligne(0 To nb - 1)
    For k = 0 To nb - 1
        ligne(k) = champs(k)
    Next k
    lignes.Add ligne
    If nb > maxCols Then maxCols = nb
End Sub

' =============================================================================
'  Tableau des colonnes de la feuille Accueil
' =============================================================================
Public Sub ActualiserColonnes()
    Dim wsA As Worksheet, wsD As Worksheet, derL As Long, derC As Long
    Dim v As Variant, anciens As Collection, r As Long, i As Long, j As Long
    Dim nNum As Long, nTxt As Long, nVide As Long, nDist As Long, distincts As Collection
    Dim t() As Variant, typ As String, role As String, nom As String, x As Double
    Dim etiquetteChoisie As Boolean, ecranActif As Boolean, plage As Range

    Set wsA = ObtenirFeuille(F_ACCUEIL)
    Set wsD = ObtenirFeuille(F_DONNEES)
    If wsA Is Nothing Or wsD Is Nothing Then
        MsgBox "Exécutez d'abord la macro InstallerApplication.", vbExclamation
        Exit Sub
    End If
    derL = DerniereLigne(wsD)
    derC = DerniereColonne(wsD)
    If derL < 2 Or derC < 1 Then
        MsgBox "La feuille « Données » ne contient pas de données (une ligne d'en-têtes puis au moins une ligne).", vbExclamation
        Exit Sub
    End If

    ecranActif = Application.ScreenUpdating
    Application.ScreenUpdating = False

    ' Mémorise les rôles déjà choisis (par nom de colonne)
    Set anciens = New Collection
    r = LIGNE_ENTETE_COLONNES + 1
    Do While Len(TexteCellule(wsA.Cells(r, COL_T_NOM).Value)) > 0
        nom = TexteCellule(wsA.Cells(r, COL_T_NOM).Value)
        If Not ExisteCle(anciens, nom) Then anciens.Add TexteCellule(wsA.Cells(r, COL_T_ROLE).Value), nom
        r = r + 1
    Loop

    With wsA.Range(wsA.Cells(LIGNE_ENTETE_COLONNES + 1, COL_T_NUM), wsA.Cells(wsA.Rows.Count, COL_T_ROLE))
        .Clear
        .Validation.Delete
    End With
    EnteteTableauColonnes wsA

    v = wsD.Range(wsD.Cells(1, 1), wsD.Cells(derL, derC)).Value
    ReDim t(1 To derC, 1 To 7)
    For j = 1 To derC
        nNum = 0: nTxt = 0: nVide = 0: nDist = 0
        Set distincts = New Collection
        For i = 2 To derL
            If IsError(v(i, j)) Then
                nVide = nVide + 1
            ElseIf IsEmpty(v(i, j)) Then
                nVide = nVide + 1
            ElseIf LireNombre(v(i, j), x) Then
                nNum = nNum + 1
            ElseIf Len(Trim$(CStr(v(i, j)))) = 0 Then
                nVide = nVide + 1
            Else
                nTxt = nTxt + 1
                If nDist <= 1000 Then
                    If Not ExisteCle(distincts, Trim$(CStr(v(i, j)))) Then
                        distincts.Add 1, Trim$(CStr(v(i, j)))
                        nDist = nDist + 1
                    End If
                End If
            End If
        Next i

        nom = TexteCellule(v(1, j))
        If Len(nom) = 0 Then nom = "Colonne " & j
        If nNum > 0 And nTxt = 0 Then
            typ = "Numérique"
        ElseIf nNum = 0 And nTxt > 0 Then
            typ = "Texte (" & nDist & IIf(nDist > 1000, "+", "") & " valeurs distinctes)"
        ElseIf nNum > 0 Then
            typ = "Mixte (" & Format$(nNum / (nNum + nTxt), "0%") & " numérique)"
        Else
            typ = "Vide"
        End If

        role = ""
        If ExisteCle(anciens, nom) Then role = anciens(nom)
        If Len(role) = 0 Then
            If nNum > 0 And nNum >= 0.9 * (nNum + nTxt) Then
                role = ROLE_ACTIVE
            ElseIf nTxt > 0 And nNum = 0 And nVide = 0 And nDist = nTxt And Not etiquetteChoisie Then
                role = ROLE_ETIQ
                etiquetteChoisie = True
            Else
                role = ROLE_IGNOREE
            End If
        End If

        t(j, 1) = j
        t(j, 2) = "'" & nom
        t(j, 3) = typ
        t(j, 4) = nNum
        t(j, 5) = nTxt
        t(j, 6) = nVide
        t(j, 7) = role
    Next j

    Set plage = wsA.Cells(LIGNE_ENTETE_COLONNES + 1, COL_T_NUM).Resize(derC, 7)
    plage.Value = t
    plage.Borders.Color = RGB(210, 210, 210)
    plage.Columns(1).HorizontalAlignment = xlCenter
    plage.Columns(4).Resize(, 3).HorizontalAlignment = xlCenter
    With plage.Columns(7)
        .Interior.Color = RGB(255, 249, 219)
        .Locked = False
    End With
    PoserListe plage.Columns(7), "G", 5
    With plage.Columns(7).FormatConditions
        .Delete
        With .Add(Type:=xlCellValue, Operator:=xlEqual, Formula1:="=""" & ROLE_ACTIVE & """")
            .Interior.Color = RGB(205, 238, 205)
            .Font.Bold = True
        End With
        With .Add(Type:=xlCellValue, Operator:=xlEqual, Formula1:="=""" & ROLE_IGNOREE & """")
            .Font.Color = RGB(150, 150, 150)
        End With
    End With

    Application.ScreenUpdating = ecranActif
End Sub

' Passe en « Active » toutes les colonnes numériques (ou mixtes) ignorées
Public Sub SelectionnerNumeriques()
    Dim ws As Worksheet, r As Long, typ As String
    Set ws = ObtenirFeuille(F_ACCUEIL)
    r = LIGNE_ENTETE_COLONNES + 1
    Do While Len(TexteCellule(ws.Cells(r, COL_T_NOM).Value)) > 0
        typ = TexteCellule(ws.Cells(r, COL_T_TYPE).Value)
        If (Left$(typ, 9) = "Numérique" Or Left$(typ, 5) = "Mixte") _
           And TexteCellule(ws.Cells(r, COL_T_ROLE).Value) = ROLE_IGNOREE Then
            ws.Cells(r, COL_T_ROLE).Value = ROLE_ACTIVE
        End If
        r = r + 1
    Loop
End Sub

' Passe toutes les colonnes actives ou supplémentaires en « Ignorée »
Public Sub ToutIgnorer()
    Dim ws As Worksheet, r As Long, role As String
    Set ws = ObtenirFeuille(F_ACCUEIL)
    r = LIGNE_ENTETE_COLONNES + 1
    Do While Len(TexteCellule(ws.Cells(r, COL_T_NOM).Value)) > 0
        role = TexteCellule(ws.Cells(r, COL_T_ROLE).Value)
        If role = ROLE_ACTIVE Or role = ROLE_SUPP Then ws.Cells(r, COL_T_ROLE).Value = ROLE_IGNOREE
        r = r + 1
    Loop
End Sub
