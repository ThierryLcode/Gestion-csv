Attribute VB_Name = "modACP"
' =============================================================================
'  ACP Excel - Module : calcul de l'ACP et restitution (tableaux, graphiques,
'  interprétation automatique)
' =============================================================================
Option Explicit

' --- Données de l'analyse -----------------------------------------------------
Private nInd As Long, nVar As Long, nSup As Long
Private nomsVar() As String, nomsSup() As String
Private etiq() As String, ligneSource() As Long
Private matX() As Double, matZ() As Double
Private matSup() As Double, presSup() As Boolean
Private moy() As Double, ect() As Double, vMin() As Double, vMax() As Double, nManq() As Long
Private matR() As Double, matC() As Double
Private lam() As Double, vec() As Double
Private inertieTot As Double, nAxes As Long, kAff As Long, kCalc As Long
Private matF() As Double, dist2() As Double
Private coordVar() As Double, corVar() As Double, cos2Var() As Double, ctrVar() As Double
Private cos2Ind() As Double, ctrInd() As Double
Private corSup() As Double
Private normee As Boolean, imputer As Boolean
Private axeX As Long, axeY As Long, maxEtiq As Long
Private kmoOK As Boolean, kmoGlobal As Double, msa() As Double
Private bartOK As Boolean, bartChi2 As Double, bartDdl As Double, bartP As Double, logDet As Double
Private nGroupes As Long, nomsGroupes() As String, idxGroupe() As Long, effGroupe() As Long
Private baryG() As Double, eta2() As Double
Private nLignesTotal As Long, nExclues As Long, nImputees As Long
Private colEtiq As Long, colGroupe As Long, nomColGroupe As String
Private avertissements As String
Private nbKaiser As Long, nbBaton As Long, nb80 As Long, nbRetenus As Long

' Mise en forme
Private Const FMT_NB As String = "0.000"
Private Const FMT_PCT As String = "0.00"

' =============================================================================
'  Point d'entrée
' =============================================================================
Public Sub LancerACP()
    Dim t0 As Double, calc As XlCalculation
    t0 = Timer
    calc = Application.Calculation
    On Error GoTo Erreur
    avertissements = ""
    Application.ScreenUpdating = False
    Application.Calculation = xlCalculationManual
    Application.Cursor = xlWait

    Application.StatusBar = "ACP : lecture des données..."
    LireDonnees
    Application.StatusBar = "ACP : calculs..."
    Calculer
    Application.StatusBar = "ACP : création des tableaux et graphiques..."
    SupprimerResultats
    ObtenirFeuille F_SYNTHESE, True
    ObtenirFeuille F_VARIABLES, True
    ObtenirFeuille F_INDIVIDUS, True
    InitGraphiques
    Application.StatusBar = "ACP : variables..."
    EcrireVariables
    Application.StatusBar = "ACP : individus..."
    EcrireIndividus
    Application.StatusBar = "ACP : synthèse..."
    EcrireSynthese

    ThisWorkbook.Worksheets(F_SYNTHESE).Activate
    ThisWorkbook.Worksheets(F_SYNTHESE).Range("A1").Select
    Application.Calculation = calc
    Application.Cursor = xlDefault
    Application.StatusBar = False
    Application.ScreenUpdating = True
    MsgBox "ACP terminée en " & Format$(Timer - t0, "0.0") & " s." & vbCrLf & vbCrLf & _
           nInd & " individus, " & nVar & " variables actives." & vbCrLf & _
           "Plan " & axeX & "-" & axeY & " : " & Format$(Pct(axeX) + Pct(axeY), "0.0") & " % de l'inertie." & _
           IIf(Len(avertissements) > 0, vbCrLf & vbCrLf & "Remarques :" & avertissements, ""), _
           vbInformation, "ACP"
    Exit Sub

Erreur:
    Application.Calculation = calc
    Application.Cursor = xlDefault
    Application.StatusBar = False
    Application.ScreenUpdating = True
    MsgBox Err.Description, vbExclamation, "ACP impossible"
End Sub

Private Function Pct(ByVal k As Long) As Double
    If inertieTot > 0 Then Pct = 100# * lam(k) / inertieTot
End Function

Private Sub Avertir(ByVal texte As String)
    avertissements = avertissements & vbCrLf & "- " & texte
End Sub

' =============================================================================
'  1. Lecture des paramètres, des rôles et des données
' =============================================================================
Private Sub LireDonnees()
    Dim wsA As Worksheet, wsD As Worksheet
    Dim r As Long, role As String, colAct() As Long, colSup() As Long
    Dim derL As Long, derC As Long, v As Variant
    Dim i As Long, j As Long, k As Long, x As Double, s As String
    Dim valeurs() As Double, manque() As Boolean, garder() As Boolean
    Dim nbManq As Long, somme() As Double, nbPres() As Long
    Dim cles As Collection, g As String

    Set wsA = ObtenirFeuille(F_ACCUEIL)
    Set wsD = ObtenirFeuille(F_DONNEES)
    If wsA Is Nothing Or wsD Is Nothing Then Err.Raise ERR_ACP, , "Feuilles « Accueil » ou « Données » introuvables : exécutez la macro InstallerApplication."

    ' --- Rôles des colonnes
    nVar = 0: nSup = 0: colEtiq = 0: colGroupe = 0
    ReDim colAct(1 To 1)
    ReDim colSup(1 To 1)
    r = LIGNE_ENTETE_COLONNES + 1
    Do While Len(TexteCellule(wsA.Cells(r, COL_T_NOM).Value)) > 0
        role = TexteCellule(wsA.Cells(r, COL_T_ROLE).Value)
        j = CLng(Val(TexteCellule(wsA.Cells(r, COL_T_NUM).Value)))
        Select Case role
            Case ROLE_ACTIVE
                nVar = nVar + 1
                ReDim Preserve colAct(1 To nVar)
                colAct(nVar) = j
            Case ROLE_SUPP
                nSup = nSup + 1
                ReDim Preserve colSup(1 To nSup)
                colSup(nSup) = j
            Case ROLE_ETIQ
                If colEtiq = 0 Then colEtiq = j
            Case ROLE_GROUPE
                If colGroupe = 0 Then colGroupe = j
        End Select
        r = r + 1
    Loop
    If r = LIGNE_ENTETE_COLONNES + 1 Then Err.Raise ERR_ACP, , "Aucune colonne n'est listée : importez d'abord un fichier CSV (ou cliquez sur « Actualiser la liste des colonnes »)."
    If nVar < 2 Then Err.Raise ERR_ACP, , "Choisissez au moins 2 colonnes numériques avec le rôle « Active »."

    ' --- Paramètres
    normee = (InStr(1, TexteCellule(wsA.Range(P_TYPE_ACP).Value), "Non", vbTextCompare) = 0)
    imputer = (InStr(1, TexteCellule(wsA.Range(P_MANQUANTES).Value), "moyenne", vbTextCompare) > 0)
    axeX = CLng(Val(TexteCellule(wsA.Range(P_AXE_X).Value)))
    axeY = CLng(Val(TexteCellule(wsA.Range(P_AXE_Y).Value)))
    kAff = CLng(Val(TexteCellule(wsA.Range(P_NB_AXES).Value)))
    maxEtiq = CLng(Val(TexteCellule(wsA.Range(P_MAX_ETIQ).Value)))
    If axeX < 1 Then axeX = 1
    If axeY < 1 Then axeY = 2
    If axeX = axeY Then
        Avertir "Axes horizontal et vertical identiques : plan 1-2 utilisé."
        axeX = 1: axeY = 2
    End If
    If kAff < 1 Then kAff = 5
    If kAff > 20 Then kAff = 20
    If maxEtiq < 0 Then maxEtiq = 0

    ' --- Lecture de la feuille Données
    derL = DerniereLigne(wsD)
    derC = DerniereColonne(wsD)
    If derL < 2 Then Err.Raise ERR_ACP, , "La feuille « Données » est vide."
    For k = 1 To nVar
        If colAct(k) < 1 Or colAct(k) > derC Then Err.Raise ERR_ACP, , "La liste des colonnes ne correspond plus aux données : cliquez sur « Actualiser la liste des colonnes »."
    Next k
    For k = 1 To nSup
        If colSup(k) < 1 Or colSup(k) > derC Then Err.Raise ERR_ACP, , "La liste des colonnes ne correspond plus aux données : cliquez sur « Actualiser la liste des colonnes »."
    Next k
    If colEtiq > derC Then colEtiq = 0
    If colGroupe > derC Then colGroupe = 0
    v = wsD.Range(wsD.Cells(1, 1), wsD.Cells(derL, derC)).Value
    nLignesTotal = derL - 1

    ReDim nomsVar(1 To nVar)
    For k = 1 To nVar
        nomsVar(k) = TexteCellule(v(1, colAct(k)))
        If Len(nomsVar(k)) = 0 Then nomsVar(k) = "Colonne " & colAct(k)
    Next k
    If nSup > 0 Then
        ReDim nomsSup(1 To nSup)
        For k = 1 To nSup
            nomsSup(k) = TexteCellule(v(1, colSup(k)))
            If Len(nomsSup(k)) = 0 Then nomsSup(k) = "Colonne " & colSup(k)
        Next k
    End If
    If colGroupe > 0 Then nomColGroupe = TexteCellule(v(1, colGroupe))

    ' --- Valeurs actives et valeurs manquantes
    ReDim valeurs(1 To nLignesTotal, 1 To nVar)
    ReDim manque(1 To nLignesTotal, 1 To nVar)
    ReDim garder(1 To nLignesTotal)
    ReDim nManq(1 To nVar)
    For i = 1 To nLignesTotal
        nbManq = 0
        For k = 1 To nVar
            If LireNombre(v(i + 1, colAct(k)), x) Then
                valeurs(i, k) = x
            Else
                manque(i, k) = True
                nManq(k) = nManq(k) + 1
                nbManq = nbManq + 1
            End If
        Next k
        If imputer Then
            garder(i) = (nbManq < nVar)
        Else
            garder(i) = (nbManq = 0)
        End If
    Next i

    nInd = 0
    For i = 1 To nLignesTotal
        If garder(i) Then nInd = nInd + 1
    Next i
    nExclues = nLignesTotal - nInd
    If nInd < 3 Then
        Err.Raise ERR_ACP, , "Seulement " & nInd & " individu(s) sans valeur manquante : l'ACP demande au moins 3 individus." & _
                  IIf(imputer, "", vbCrLf & "Essayez l'option « Remplacer par la moyenne » pour les valeurs manquantes.")
    End If

    ' Moyennes des valeurs présentes (pour l'imputation)
    ReDim somme(1 To nVar)
    ReDim nbPres(1 To nVar)
    For i = 1 To nLignesTotal
        If garder(i) Then
            For k = 1 To nVar
                If Not manque(i, k) Then
                    somme(k) = somme(k) + valeurs(i, k)
                    nbPres(k) = nbPres(k) + 1
                End If
            Next k
        End If
    Next i

    ReDim matX(1 To nInd, 1 To nVar)
    ReDim etiq(1 To nInd)
    ReDim ligneSource(1 To nInd)
    If nSup > 0 Then
        ReDim matSup(1 To nInd, 1 To nSup)
        ReDim presSup(1 To nInd, 1 To nSup)
    End If
    nImputees = 0
    j = 0
    For i = 1 To nLignesTotal
        If garder(i) Then
            j = j + 1
            ligneSource(j) = i + 1
            For k = 1 To nVar
                If manque(i, k) Then
                    If nbPres(k) = 0 Then Err.Raise ERR_ACP, , "La variable « " & nomsVar(k) & " » ne contient aucune valeur numérique."
                    matX(j, k) = somme(k) / nbPres(k)
                    nImputees = nImputees + 1
                Else
                    matX(j, k) = valeurs(i, k)
                End If
            Next k
            For k = 1 To nSup
                If LireNombre(v(i + 1, colSup(k)), x) Then
                    matSup(j, k) = x
                    presSup(j, k) = True
                End If
            Next k
            s = ""
            If colEtiq > 0 Then s = TexteCellule(v(i + 1, colEtiq))
            If Len(s) = 0 Then s = "L" & (i + 1)
            etiq(j) = s
        End If
    Next i

    ' --- Groupes (8 couleurs au plus : au-delà, regroupement en « Autres »)
    nGroupes = 0
    If colGroupe > 0 Then
        Set cles = New Collection
        ReDim idxGroupe(1 To nInd)
        ReDim nomsGroupes(1 To 8)
        For j = 1 To nInd
            g = TexteCellule(v(ligneSource(j), colGroupe))
            If Len(g) = 0 Then g = "(vide)"
            If ExisteCle(cles, g) Then
                idxGroupe(j) = cles(g)
            Else
                nGroupes = nGroupes + 1
                cles.Add nGroupes, g
                idxGroupe(j) = nGroupes
                If nGroupes <= 8 Then nomsGroupes(nGroupes) = g
            End If
        Next j
        If nGroupes > 8 Then
            Avertir "La colonne de groupe compte " & nGroupes & " modalités : les modalités au-delà de la 7e sont regroupées en « Autres »."
            For j = 1 To nInd
                If idxGroupe(j) >= 8 Then idxGroupe(j) = 8
            Next j
            nomsGroupes(8) = "Autres"
            nGroupes = 8
        Else
            ReDim Preserve nomsGroupes(1 To nGroupes)
        End If
        ReDim effGroupe(1 To nGroupes)
        For j = 1 To nInd
            effGroupe(idxGroupe(j)) = effGroupe(idxGroupe(j)) + 1
        Next j
    End If

    If nExclues > 0 Then Avertir nExclues & " ligne(s) exclue(s) pour valeurs manquantes."
    If nImputees > 0 Then Avertir nImputees & " valeur(s) manquante(s) remplacée(s) par la moyenne."
End Sub

' =============================================================================
'  2. Calculs
' =============================================================================
Private Sub Calculer()
    Dim i As Long, j As Long, k As Long, a As Long, b As Long
    Dim s As Double, x As Double, mxAbs As Double, imax As Long
    Dim lamR() As Double, vecR() As Double, rinv() As Double
    Dim sR2 As Double, sA2 As Double, sR2j As Double, sA2j As Double, pc As Double
    Dim colF() As Double, colS() As Double, pres() As Boolean, cum As Double, baton As Double

    ' --- Statistiques descriptives
    ReDim moy(1 To nVar): ReDim ect(1 To nVar): ReDim vMin(1 To nVar): ReDim vMax(1 To nVar)
    For k = 1 To nVar
        s = 0
        vMin(k) = matX(1, k): vMax(k) = matX(1, k)
        For i = 1 To nInd
            s = s + matX(i, k)
            If matX(i, k) < vMin(k) Then vMin(k) = matX(i, k)
            If matX(i, k) > vMax(k) Then vMax(k) = matX(i, k)
        Next i
        moy(k) = s / nInd
        s = 0
        For i = 1 To nInd
            s = s + (matX(i, k) - moy(k)) ^ 2
        Next i
        ect(k) = Sqr(s / nInd)
        If ect(k) <= 0.000000000001 * (Abs(moy(k)) + 1) Then
            Err.Raise ERR_ACP, , "La variable « " & nomsVar(k) & " » est constante sur les individus retenus : passez-la en « Ignorée »."
        End If
    Next k

    ' --- Tableau centré (et réduit si ACP normée)
    ReDim matZ(1 To nInd, 1 To nVar)
    For i = 1 To nInd
        For k = 1 To nVar
            If normee Then
                matZ(i, k) = (matX(i, k) - moy(k)) / ect(k)
            Else
                matZ(i, k) = matX(i, k) - moy(k)
            End If
        Next k
    Next i

    ' --- Matrice diagonalisée (corrélations ou covariances) et corrélations
    ReDim matC(1 To nVar, 1 To nVar)
    ReDim matR(1 To nVar, 1 To nVar)
    For a = 1 To nVar
        For b = a To nVar
            s = 0
            For i = 1 To nInd
                s = s + matZ(i, a) * matZ(i, b)
            Next i
            matC(a, b) = s / nInd
            matC(b, a) = matC(a, b)
            If normee Then
                matR(a, b) = matC(a, b)
            Else
                matR(a, b) = matC(a, b) / (ect(a) * ect(b))
            End If
            matR(b, a) = matR(a, b)
        Next b
    Next a

    JacobiSym matC, nVar, lam, vec

    inertieTot = 0
    For k = 1 To nVar
        If lam(k) < 0 Then lam(k) = 0
        inertieTot = inertieTot + lam(k)
    Next k
    nAxes = 0
    For k = 1 To nVar
        If lam(k) > 0.000000001 * inertieTot Then nAxes = nAxes + 1
    Next k
    If nAxes < 2 Then Err.Raise ERR_ACP, , "Les variables actives sont parfaitement liées entre elles : un seul axe non nul, aucun plan factoriel n'est possible."
    If axeX > nAxes Or axeY > nAxes Then
        Avertir "Le plan demandé (" & axeX & "-" & axeY & ") dépasse le nombre d'axes non nuls (" & nAxes & ") : plan 1-2 utilisé."
        axeX = 1: axeY = 2
    End If
    If kAff > nAxes Then kAff = nAxes
    kCalc = kAff
    If axeX > kCalc Then kCalc = axeX
    If axeY > kCalc Then kCalc = axeY

    ' Orientation : la composante de plus grande valeur absolue est positive
    For k = 1 To nVar
        mxAbs = 0: imax = 1
        For j = 1 To nVar
            If Abs(vec(j, k)) > mxAbs Then mxAbs = Abs(vec(j, k)): imax = j
        Next j
        If vec(imax, k) < 0 Then
            For j = 1 To nVar
                vec(j, k) = -vec(j, k)
            Next j
        End If
    Next k

    ' --- Critères de choix du nombre d'axes
    nbKaiser = 0: nbBaton = 0: nb80 = 0
    For k = 1 To nVar
        If lam(k) > inertieTot / nVar + 0.0000000001 Then nbKaiser = nbKaiser + 1
    Next k
    For k = 1 To nAxes
        If Pct(k) > SeuilBaton(k) And nbBaton = k - 1 Then nbBaton = k
    Next k
    cum = 0
    For k = 1 To nVar
        cum = cum + Pct(k)
        If nb80 = 0 And cum >= 80 Then nb80 = k
    Next k
    nbRetenus = nbKaiser
    If nbRetenus < 2 Then nbRetenus = 2
    If nbRetenus > nAxes Then nbRetenus = nAxes

    ' --- Individus
    ReDim matF(1 To nInd, 1 To kCalc)
    ReDim dist2(1 To nInd)
    ReDim cos2Ind(1 To nInd, 1 To kCalc)
    ReDim ctrInd(1 To nInd, 1 To kCalc)
    For i = 1 To nInd
        s = 0
        For j = 1 To nVar
            s = s + matZ(i, j) * matZ(i, j)
        Next j
        dist2(i) = s
        For k = 1 To kCalc
            x = 0
            For j = 1 To nVar
                x = x + matZ(i, j) * vec(j, k)
            Next j
            matF(i, k) = x
            If dist2(i) > 1E-300 Then cos2Ind(i, k) = x * x / dist2(i)
            ctrInd(i, k) = 100# * x * x / (nInd * lam(k))
        Next k
    Next i

    ' --- Variables
    ReDim coordVar(1 To nVar, 1 To kCalc)
    ReDim corVar(1 To nVar, 1 To kCalc)
    ReDim cos2Var(1 To nVar, 1 To kCalc)
    ReDim ctrVar(1 To nVar, 1 To kCalc)
    For j = 1 To nVar
        For k = 1 To kCalc
            coordVar(j, k) = vec(j, k) * Sqr(lam(k))
            If normee Then
                corVar(j, k) = coordVar(j, k)
            Else
                corVar(j, k) = coordVar(j, k) / ect(j)
            End If
            cos2Var(j, k) = corVar(j, k) ^ 2
            ctrVar(j, k) = 100# * vec(j, k) ^ 2
        Next k
    Next j

    ' --- Variables supplémentaires : corrélations avec les axes
    If nSup > 0 Then
        ReDim corSup(1 To nSup, 1 To kCalc)
        ReDim colF(1 To nInd): ReDim colS(1 To nInd): ReDim pres(1 To nInd)
        For j = 1 To nSup
            For i = 1 To nInd
                colS(i) = matSup(i, j)
                pres(i) = presSup(i, j)
            Next i
            For k = 1 To kCalc
                For i = 1 To nInd
                    colF(i) = matF(i, k)
                Next i
                corSup(j, k) = CorrelationMasquee(colS, colF, pres, nInd)
            Next k
        Next j
    End If

    ' --- Groupes : barycentres et rapport de corrélation (eta²)
    If nGroupes > 0 Then
        ReDim baryG(1 To nGroupes, 1 To kCalc)
        ReDim eta2(1 To kCalc)
        For i = 1 To nInd
            For k = 1 To kCalc
                baryG(idxGroupe(i), k) = baryG(idxGroupe(i), k) + matF(i, k)
            Next k
        Next i
        For a = 1 To nGroupes
            For k = 1 To kCalc
                baryG(a, k) = baryG(a, k) / effGroupe(a)
            Next k
        Next a
        For k = 1 To kCalc
            s = 0
            For a = 1 To nGroupes
                s = s + effGroupe(a) * baryG(a, k) ^ 2
            Next a
            eta2(k) = (s / nInd) / lam(k)
        Next k
    End If

    ' --- Test de sphéricité de Bartlett et indice KMO (sur les corrélations)
    If normee Then
        lamR = lam
        vecR = vec
    Else
        JacobiSym matR, nVar, lamR, vecR
    End If
    bartOK = True
    logDet = 0
    For k = 1 To nVar
        If lamR(k) <= 0.0000000001 Then
            bartOK = False
            Exit For
        End If
        logDet = logDet + Log(lamR(k))
    Next k
    kmoOK = bartOK
    If bartOK Then
        bartDdl = nVar * (nVar - 1) / 2
        bartChi2 = -((nInd - 1) - (2 * nVar + 5) / 6) * logDet
        If bartChi2 < 0 Then bartChi2 = 0
        bartP = ProbaKhi2(bartChi2, bartDdl)

        ReDim rinv(1 To nVar, 1 To nVar)
        For a = 1 To nVar
            For b = a To nVar
                s = 0
                For k = 1 To nVar
                    s = s + vecR(a, k) * vecR(b, k) / lamR(k)
                Next k
                rinv(a, b) = s
                rinv(b, a) = s
            Next b
        Next a
        ReDim msa(1 To nVar)
        sR2 = 0: sA2 = 0
        For a = 1 To nVar
            sR2j = 0: sA2j = 0
            For b = 1 To nVar
                If a <> b Then
                    pc = -rinv(a, b) / Sqr(rinv(a, a) * rinv(b, b))
                    sR2j = sR2j + matR(a, b) ^ 2
                    sA2j = sA2j + pc ^ 2
                End If
            Next b
            If sR2j + sA2j > 0 Then msa(a) = sR2j / (sR2j + sA2j)
            sR2 = sR2 + sR2j
            sA2 = sA2 + sA2j
        Next a
        If sR2 + sA2 > 0 Then kmoGlobal = sR2 / (sR2 + sA2) Else kmoOK = False
    Else
        Avertir "La matrice des corrélations est singulière (variables redondantes ou plus de variables que d'individus) : test de Bartlett et KMO non calculables."
    End If
End Sub

' Seuil du bâton brisé (en %) pour l'axe k
Private Function SeuilBaton(ByVal k As Long) As Double
    Dim i As Long, s As Double
    For i = k To nVar
        s = s + 1# / i
    Next i
    SeuilBaton = 100# * s / nVar
End Function

Private Function ProbaKhi2(ByVal x As Double, ByVal ddl As Double) As Double
    On Error Resume Next
    ProbaKhi2 = -1
    ProbaKhi2 = Application.WorksheetFunction.ChiSq_Dist_RT(x, ddl)
    If Err.Number <> 0 Then
        Err.Clear
        ProbaKhi2 = Application.WorksheetFunction.ChiDist(x, ddl)
    End If
    On Error GoTo 0
End Function

' =============================================================================
'  Outils de mise en forme des feuilles de résultats
' =============================================================================
Private Sub PreparerFeuille(ws As Worksheet, ByVal titre As String, ByVal sousTitre As String)
    ws.Cells.Clear
    ws.Activate
    On Error Resume Next
    ActiveWindow.DisplayGridlines = False
    ActiveWindow.Zoom = 90
    On Error GoTo 0
    ws.Cells.Font.Name = "Calibri"
    ws.Cells.Font.Size = 10
    ws.Columns("A").ColumnWidth = 2
    With ws.Range("B1")
        .Value = titre
        .Font.Size = 16
        .Font.Bold = True
        .Font.Color = CouleurTitre()
    End With
    With ws.Range("B2")
        .Value = sousTitre
        .Font.Italic = True
        .Font.Color = RGB(110, 110, 110)
    End With
End Sub

Private Function Section(ws As Worksheet, ByVal ligne As Long, ByVal texte As String) As Long
    With ws.Range(ws.Cells(ligne, 2), ws.Cells(ligne, 14))
        .Borders(xlEdgeBottom).Color = CouleurTitre()
        .Borders(xlEdgeBottom).Weight = xlThin
    End With
    With ws.Cells(ligne, 2)
        .Value = texte
        .Font.Size = 12
        .Font.Bold = True
        .Font.Color = CouleurTitre()
    End With
    Section = ligne + 2
End Function

Private Function Ligne(ws As Worksheet, ByVal r As Long, ByVal texte As String, Optional ByVal gras As Boolean = False, _
                       Optional ByVal couleur As Long = -1, Optional ByVal retrait As Long = 0) As Long
    With ws.Cells(r, 2)
        .Value = "'" & Sym(texte)
        .Font.Bold = gras
        If couleur >= 0 Then .Font.Color = couleur
        .IndentLevel = retrait
    End With
    Ligne = r + 1
End Function

' Écrit un tableau (en-têtes + valeurs) et renvoie la plage complète
Private Function Tableau(ws As Worksheet, ByVal r As Long, ByVal c As Long, entetes As Variant, valeurs As Variant, _
                         ByVal nbL As Long, ByVal nbC As Long, Optional ByVal fmt As String = FMT_NB) As Range
    Dim h As Range, corps As Range, j As Long
    Set h = ws.Cells(r, c).Resize(1, nbC)
    For j = 1 To nbC
        h.Cells(1, j).Value = "'" & entetes(j)
    Next j
    With h
        .Font.Bold = True
        .Font.Color = RGB(255, 255, 255)
        .Interior.Color = CouleurTitre()
        .HorizontalAlignment = xlCenter
        .VerticalAlignment = xlCenter
        .WrapText = True
    End With
    Set corps = ws.Cells(r + 1, c).Resize(nbL, nbC)
    corps.Value = valeurs
    If nbC > 1 Then corps.Offset(0, 1).Resize(nbL, nbC - 1).NumberFormat = fmt
    corps.Columns(1).Font.Bold = True
    With corps.Borders
        .LineStyle = xlContinuous
        .Color = RGB(225, 224, 217)
    End With
    Set Tableau = ws.Cells(r, c).Resize(nbL + 1, nbC)
End Function

' Échelle de couleurs à 3 niveaux (rouge - gris neutre - bleu)
Private Sub EchelleDivergente(rng As Range, ByVal vMinE As Double, ByVal vMaxE As Double)
    Dim cs As ColorScale
    Set cs = rng.FormatConditions.AddColorScale(ColorScaleType:=3)
    With cs.ColorScaleCriteria(1)
        .Type = xlConditionValueNumber
        .Value = vMinE
        .FormatColor.Color = RGB(227, 73, 72)
    End With
    With cs.ColorScaleCriteria(2)
        .Type = xlConditionValueNumber
        .Value = 0
        .FormatColor.Color = RGB(240, 239, 236)
    End With
    With cs.ColorScaleCriteria(3)
        .Type = xlConditionValueNumber
        .Value = vMaxE
        .FormatColor.Color = RGB(42, 120, 214)
    End With
End Sub

' Échelle séquentielle (blanc -> bleu)
Private Sub EchelleSequentielle(rng As Range)
    Dim cs As ColorScale
    Set cs = rng.FormatConditions.AddColorScale(ColorScaleType:=2)
    With cs.ColorScaleCriteria(1)
        .Type = xlConditionValueLowestValue
        .FormatColor.Color = RGB(252, 252, 251)
    End With
    With cs.ColorScaleCriteria(2)
        .Type = xlConditionValueHighestValue
        .FormatColor.Color = RGB(110, 167, 236)
    End With
End Sub

Private Sub AjusterLargeurs(ws As Worksheet, tables As Collection, Optional ByVal largeurMax As Double = 40)
    Dim larg() As Double, t As Variant, c As Long, rng As Range, nC As Long
    nC = 1
    For Each t In tables
        Set rng = t
        If rng.Column + rng.Columns.Count - 1 > nC Then nC = rng.Column + rng.Columns.Count - 1
    Next t
    ReDim larg(1 To nC)
    For Each t In tables
        Set rng = t
        rng.Rows(1).WrapText = False
        rng.Columns.AutoFit
        For c = 1 To rng.Columns.Count
            If rng.Columns(c).ColumnWidth > larg(rng.Column + c - 1) Then larg(rng.Column + c - 1) = rng.Columns(c).ColumnWidth
        Next c
        rng.Rows(1).WrapText = True
    Next t
    For c = 2 To nC
        If larg(c) > 0 Then
            If larg(c) > largeurMax Then larg(c) = largeurMax
            If larg(c) < 9 Then larg(c) = 9
            ws.Columns(c).ColumnWidth = larg(c) + 1
        End If
    Next c
End Sub

' Première ligne située sous une position verticale (en points)
Private Function LigneSous(ws As Worksheet, ByVal y As Double) As Long
    Dim r As Long
    r = 1
    Do While ws.Rows(r).Top < y
        r = r + 1
    Loop
    LigneSous = r + 1
End Function

Private Function EnteteAxes(ByVal prefixe As String, ByVal k As Long) As String
    EnteteAxes = prefixe & "Dim." & k
End Function

Private Function TitreAxe(ByVal k As Long) As String
    TitreAxe = "Dim." & k & " (" & Format$(Pct(k), "0.0") & " %)"
End Function

Private Function NombreTexte(ByVal x As Double, Optional ByVal fmt As String = "0.00") As String
    NombreTexte = Format$(x, fmt)
End Function

Private Function Signe(ByVal x As Double, Optional ByVal fmt As String = "0.00") As String
    Signe = Format$(x, "+" & fmt & ";-" & fmt & ";" & fmt)
End Function

' =============================================================================
'  3. Feuille Variables
' =============================================================================
Private Sub EcrireVariables()
    Dim ws As Worksheet, r As Long, i As Long, j As Long, k As Long, tables As Collection
    Dim t() As Variant, e() As Variant, rng As Range, debutTables As Long
    Dim cx() As Double, cy() As Double, qual() As Double, sx() As Double, sy() As Double
    Dim cX2() As Double, cY2() As Double, hautGraph As Double

    Set ws = ThisWorkbook.Worksheets(F_VARIABLES)
    PreparerFeuille ws, "ACP - Variables", "Cercle des corrélations, contributions, qualité de représentation et statistiques des variables"
    Set tables = New Collection

    ' --- Graphiques en haut de la feuille
    hautGraph = ws.Rows(4).Top
    ReDim cx(1 To nVar): ReDim cy(1 To nVar): ReDim qual(1 To nVar)
    ReDim cX2(1 To nVar): ReDim cY2(1 To nVar)
    For j = 1 To nVar
        cx(j) = corVar(j, axeX)
        cy(j) = corVar(j, axeY)
        qual(j) = cos2Var(j, axeX) + cos2Var(j, axeY)
        cX2(j) = ctrVar(j, axeX)
        cY2(j) = ctrVar(j, axeY)
    Next j
    If nSup > 0 Then
        ReDim sx(1 To nSup): ReDim sy(1 To nSup)
        For j = 1 To nSup
            sx(j) = corSup(j, axeX)
            sy(j) = corSup(j, axeY)
        Next j
    Else
        ReDim sx(1 To 1): ReDim sy(1 To 1): ReDim nomsSup(1 To 1)
    End If
    GraphCercle ws, ws.Range("B1").Left, hautGraph, 460, nomsVar, cx, cy, qual, nVar, nomsSup, sx, sy, nSup, TitreAxe(axeX), TitreAxe(axeY)
    GraphContributions ws, ws.Range("B1").Left + 475, hautGraph, 470, 225, nomsVar, cX2, nVar, 30, 100# / nVar, _
                       "Contributions des variables à la Dim." & axeX, "ContribVarX"
    GraphContributions ws, ws.Range("B1").Left + 475, hautGraph + 235, 470, 225, nomsVar, cY2, nVar, 30, 100# / nVar, _
                       "Contributions des variables à la Dim." & axeY, "ContribVarY"
    r = LigneSous(ws, hautGraph + 470)
    With ws.Cells(r - 1, 2)
        .Value = "'" & Sym("Flèches foncées : variables bien représentées sur le plan (cos² {>=} 0,5). Barres foncées : contribution supérieure à la moyenne (ligne pointillée).")
        .Font.Italic = True
        .Font.Size = 9
        .Font.Color = RGB(110, 110, 110)
    End With
    r = r + 1
    debutTables = r

    ' --- Statistiques descriptives
    r = Section(ws, r, "Statistiques descriptives des variables actives")
    ReDim t(1 To nVar, 1 To 8)
    For j = 1 To nVar
        t(j, 1) = "'" & nomsVar(j)
        t(j, 2) = moy(j)
        t(j, 3) = ect(j)
        t(j, 4) = vMin(j)
        t(j, 5) = vMax(j)
        If Abs(moy(j)) > 1E-300 Then t(j, 6) = 100# * ect(j) / Abs(moy(j)) Else t(j, 6) = Empty
        t(j, 7) = nManq(j)
        If kmoOK Then t(j, 8) = msa(j) Else t(j, 8) = "-"
    Next j
    Set rng = Tableau(ws, r, 2, Array("", "Variable", "Moyenne", "Écart-type", "Minimum", "Maximum", "Coef. de variation (%)", "Valeurs manquantes (fichier)", "MSA (KMO)"), t, nVar, 8)
    rng.Columns(7).Offset(1).Resize(nVar).NumberFormat = "0"
    rng.Columns(6).Offset(1).Resize(nVar).NumberFormat = "0.0"
    tables.Add rng
    r = r + nVar + 2
    r = Ligne(ws, r - 1, "Écart-type calculé avec le diviseur n (convention de l'ACP). MSA : adéquation de chaque variable à l'analyse (> 0,5 souhaitable).", False, RGB(110, 110, 110)) + 1

    ' --- Matrice des corrélations
    r = Section(ws, r, "Matrice des corrélations (Pearson)")
    ReDim e(1 To nVar + 1)
    e(1) = ""
    For j = 1 To nVar
        e(j + 1) = nomsVar(j)
    Next j
    ReDim t(1 To nVar, 1 To nVar + 1)
    For i = 1 To nVar
        t(i, 1) = "'" & nomsVar(i)
        For j = 1 To nVar
            t(i, j + 1) = matR(i, j)
        Next j
    Next i
    Set rng = Tableau(ws, r, 2, PrefixeVide(e), t, nVar, nVar + 1, "0.00")
    EchelleDivergente rng.Offset(1, 1).Resize(nVar, nVar), -1, 1
    tables.Add rng
    r = r + nVar + 3

    ' --- Coordonnées / corrélations
    If normee Then
        r = Section(ws, r, "Coordonnées des variables (= corrélations variable-axe)")
    Else
        r = Section(ws, r, "Coordonnées des variables (covariances variable-axe)")
    End If
    Set rng = TableauVarAxes(ws, r, coordVar, nomsVar, nVar, "", FMT_NB)
    EchelleDivergente rng.Offset(1, 1).Resize(nVar, kAff), -MaxAbs(coordVar, nVar, kAff), MaxAbs(coordVar, nVar, kAff)
    tables.Add rng
    r = r + nVar + 3
    If Not normee Then
        r = Section(ws, r, "Corrélations des variables avec les axes")
        Set rng = TableauVarAxes(ws, r, corVar, nomsVar, nVar, "", FMT_NB)
        EchelleDivergente rng.Offset(1, 1).Resize(nVar, kAff), -1, 1
        tables.Add rng
        r = r + nVar + 3
    End If

    ' --- Qualité de représentation
    r = Section(ws, r, "Qualité de représentation des variables (cos²)")
    ReDim t(1 To nVar, 1 To kAff + 2)
    For j = 1 To nVar
        t(j, 1) = "'" & nomsVar(j)
        For k = 1 To kAff
            t(j, k + 1) = cos2Var(j, k)
        Next k
        t(j, kAff + 2) = cos2Var(j, axeX) + cos2Var(j, axeY)
    Next j
    Set rng = Tableau(ws, r, 2, EntetesAxes("Variable", "", "Plan " & axeX & "-" & axeY), t, nVar, kAff + 2, FMT_NB)
    EchelleSequentielle rng.Offset(1, 1).Resize(nVar, kAff + 1)
    tables.Add rng
    r = r + nVar + 3

    ' --- Contributions
    r = Section(ws, r, "Contributions des variables à la construction des axes (%)")
    ReDim t(1 To nVar + 1, 1 To kAff + 1)
    For j = 1 To nVar
        t(j, 1) = "'" & nomsVar(j)
        For k = 1 To kAff
            t(j, k + 1) = ctrVar(j, k)
        Next k
    Next j
    t(nVar + 1, 1) = "Total"
    For k = 1 To kAff
        t(nVar + 1, k + 1) = 100
    Next k
    Set rng = Tableau(ws, r, 2, EntetesAxes("Variable", "", ""), t, nVar + 1, kAff + 1, FMT_PCT)
    EchelleSequentielle rng.Offset(1, 1).Resize(nVar, kAff)
    rng.Rows(nVar + 2).Font.Italic = True
    tables.Add rng
    r = r + nVar + 3
    r = Ligne(ws, r - 1, "Une variable contribue significativement à un axe lorsque sa contribution dépasse la moyenne 100 / p = " & _
                         Format$(100# / nVar, "0.0") & " %.", False, RGB(110, 110, 110)) + 1

    ' --- Vecteurs propres
    r = Section(ws, r, "Vecteurs propres (coefficients des composantes principales)")
    Set rng = TableauVarAxes(ws, r, vec, nomsVar, nVar, "", "0.0000")
    tables.Add rng
    r = r + nVar + 3

    ' --- Variables supplémentaires
    If nSup > 0 Then
        r = Section(ws, r, "Variables supplémentaires : corrélations avec les axes")
        Set rng = TableauVarAxes(ws, r, corSup, nomsSup, nSup, "", FMT_NB)
        EchelleDivergente rng.Offset(1, 1).Resize(nSup, kAff), -1, 1
        tables.Add rng
        r = r + nSup + 3
    End If

    AjusterLargeurs ws, tables
    ws.Range("A1").Select
End Sub

Private Function PrefixeVide(e() As Variant) As Variant
    Dim r() As Variant, i As Long
    ReDim r(0 To UBound(e))
    r(0) = ""
    For i = 1 To UBound(e)
        r(i) = e(i)
    Next i
    PrefixeVide = r
End Function

' En-têtes : (index 0 inutilisé) premier libellé, Dim.1..Dim.kAff, colonne finale optionnelle
Private Function EntetesAxes(ByVal premier As String, ByVal prefixe As String, ByVal dernier As String) As Variant
    Dim e() As Variant, k As Long, n As Long
    n = kAff + 1
    If Len(dernier) > 0 Then n = n + 1
    ReDim e(0 To n)
    e(1) = premier
    For k = 1 To kAff
        e(k + 1) = prefixe & "Dim." & k
    Next k
    If Len(dernier) > 0 Then e(n) = dernier
    EntetesAxes = e
End Function

Private Function TableauVarAxes(ws As Worksheet, ByVal r As Long, m() As Double, noms() As String, ByVal n As Long, _
                                ByVal prefixe As String, ByVal fmt As String) As Range
    Dim t() As Variant, j As Long, k As Long
    ReDim t(1 To n, 1 To kAff + 1)
    For j = 1 To n
        t(j, 1) = "'" & noms(j)
        For k = 1 To kAff
            t(j, k + 1) = m(j, k)
        Next k
    Next j
    Set TableauVarAxes = Tableau(ws, r, 2, EntetesAxes("Variable", prefixe, ""), t, n, kAff + 1, fmt)
End Function

Private Function MaxAbs(m() As Double, ByVal n As Long, ByVal kk As Long) As Double
    Dim i As Long, k As Long
    For i = 1 To n
        For k = 1 To kk
            If Abs(m(i, k)) > MaxAbs Then MaxAbs = Abs(m(i, k))
        Next k
    Next i
    If MaxAbs = 0 Then MaxAbs = 1
End Function

' =============================================================================
'  4. Feuille Individus
' =============================================================================
Private Sub EcrireIndividus()
    Dim ws As Worksheet, r As Long, i As Long, k As Long, c As Long, nbC As Long, tables As Collection
    Dim t() As Variant, e() As Variant, rng As Range, hautGraph As Double
    Dim fx() As Double, fy() As Double, montrer() As Boolean, score() As Double, ordre() As Long
    Dim cX2() As Double, cY2() As Double, bx() As Double, by() As Double, nbM As Long
    Dim avecGroupe As Boolean

    Set ws = ThisWorkbook.Worksheets(F_INDIVIDUS)
    PreparerFeuille ws, "ACP - Individus", "Carte factorielle, coordonnées, qualité de représentation et contributions des individus"
    Set tables = New Collection
    avecGroupe = (nGroupes > 0)

    ' --- Choix des individus à nommer : les plus contributifs au plan
    ReDim fx(1 To nInd): ReDim fy(1 To nInd): ReDim montrer(1 To nInd): ReDim score(1 To nInd)
    ReDim cX2(1 To nInd): ReDim cY2(1 To nInd)
    For i = 1 To nInd
        fx(i) = matF(i, axeX)
        fy(i) = matF(i, axeY)
        cX2(i) = ctrInd(i, axeX)
        cY2(i) = ctrInd(i, axeY)
        score(i) = ctrInd(i, axeX) * lam(axeX) + ctrInd(i, axeY) * lam(axeY)
    Next i
    If nInd <= maxEtiq Then
        For i = 1 To nInd
            montrer(i) = True
        Next i
    ElseIf maxEtiq > 0 Then
        ordre = OrdreDecroissantRapide(score, nInd)
        nbM = maxEtiq
        For i = 1 To nbM
            montrer(ordre(i)) = True
        Next i
        Avertir "Graphique des individus : seuls les " & maxEtiq & " individus les plus contributifs au plan sont nommés."
    End If
    If avecGroupe Then
        ReDim bx(1 To nGroupes): ReDim by(1 To nGroupes)
        For k = 1 To nGroupes
            bx(k) = baryG(k, axeX)
            by(k) = baryG(k, axeY)
        Next k
    Else
        ReDim bx(1 To 1): ReDim by(1 To 1): ReDim idxGroupe(1 To nInd): ReDim nomsGroupes(1 To 1)
    End If

    hautGraph = ws.Rows(4).Top
    GraphIndividus ws, ws.Range("B1").Left, hautGraph, 600, 470, nInd, fx, fy, etiq, montrer, idxGroupe, nomsGroupes, nGroupes, _
                   bx, by, TitreAxe(axeX), TitreAxe(axeY)
    GraphContributions ws, ws.Range("B1").Left + 615, hautGraph, 440, 230, etiq, cX2, nInd, 15, 100# / nInd, _
                       "Individus les plus contributifs à la Dim." & axeX, "ContribIndX"
    GraphContributions ws, ws.Range("B1").Left + 615, hautGraph + 240, 440, 230, etiq, cY2, nInd, 15, 100# / nInd, _
                       "Individus les plus contributifs à la Dim." & axeY, "ContribIndY"
    r = LigneSous(ws, hautGraph + 470) + 1

    ' --- Groupes
    If avecGroupe Then
        r = Section(ws, r, "Groupes (« " & nomColGroupe & " ») : barycentres sur les axes")
        ReDim t(1 To nGroupes + 1, 1 To kAff + 2)
        For k = 1 To nGroupes
            t(k, 1) = "'" & nomsGroupes(k)
            t(k, 2) = effGroupe(k)
            For c = 1 To kAff
                t(k, c + 2) = baryG(k, c)
            Next c
        Next k
        t(nGroupes + 1, 1) = "Eta² (rapport de corrélation)"
        t(nGroupes + 1, 2) = Empty
        For c = 1 To kAff
            t(nGroupes + 1, c + 2) = eta2(c)
        Next c
        ReDim e(0 To kAff + 2)
        e(1) = "Groupe": e(2) = "Effectif"
        For c = 1 To kAff
            e(c + 2) = "Dim." & c
        Next c
        Set rng = Tableau(ws, r, 2, e, t, nGroupes + 1, kAff + 2, FMT_NB)
        rng.Columns(2).Offset(1).Resize(nGroupes).NumberFormat = "0"
        rng.Rows(nGroupes + 2).Font.Italic = True
        tables.Add rng
        r = r + nGroupes + 3
        r = Ligne(ws, r - 1, "Eta² : part de la variance de l'axe expliquée par les groupes (0 = aucune différence, 1 = groupes parfaitement séparés).", False, RGB(110, 110, 110)) + 1
    End If

    ' --- Tableau des individus
    r = Section(ws, r, "Coordonnées, qualité de représentation (cos²) et contributions (%) des individus")
    nbC = 3 + 3 * kAff + 1
    If avecGroupe Then nbC = nbC + 1
    ReDim e(0 To nbC)
    c = 1
    e(c) = "Individu": c = c + 1
    If avecGroupe Then e(c) = nomColGroupe: c = c + 1
    e(c) = "Ligne (Données)": c = c + 1
    e(c) = "Distance² au centre": c = c + 1
    For k = 1 To kAff
        e(c) = "Coord. Dim." & k: c = c + 1
    Next k
    For k = 1 To kAff
        e(c) = "cos² Dim." & k: c = c + 1
    Next k
    For k = 1 To kAff
        e(c) = "Ctr Dim." & k & " (%)": c = c + 1
    Next k
    e(c) = "Qualité plan " & axeX & "-" & axeY

    ReDim t(1 To nInd, 1 To nbC)
    For i = 1 To nInd
        c = 1
        t(i, c) = "'" & etiq(i): c = c + 1
        If avecGroupe Then t(i, c) = "'" & nomsGroupes(idxGroupe(i)): c = c + 1
        t(i, c) = ligneSource(i): c = c + 1
        t(i, c) = dist2(i): c = c + 1
        For k = 1 To kAff
            t(i, c) = matF(i, k): c = c + 1
        Next k
        For k = 1 To kAff
            t(i, c) = cos2Ind(i, k): c = c + 1
        Next k
        For k = 1 To kAff
            t(i, c) = ctrInd(i, k): c = c + 1
        Next k
        t(i, c) = cos2Ind(i, axeX) + cos2Ind(i, axeY)
    Next i
    Set rng = Tableau(ws, r, 2, e, t, nInd, nbC, FMT_NB)
    c = 2
    If avecGroupe Then c = 3
    ' c = colonne « Ligne (Données) » ; puis distance², coordonnées, cos², contributions
    rng.Cells(2, c).Resize(nInd, 1).NumberFormat = "0"
    rng.Cells(2, c + 2 + 2 * kAff).Resize(nInd, kAff).NumberFormat = FMT_PCT
    If nInd <= 20000 Then
        EchelleDivergente rng.Cells(2, c + 2).Resize(nInd, kAff), -MaxAbs(matF, nInd, kAff), MaxAbs(matF, nInd, kAff)
        EchelleSequentielle rng.Cells(2, c + 2 + 2 * kAff).Resize(nInd, kAff)
    End If
    tables.Add rng
    rng.AutoFilter
    AjusterLargeurs ws, tables
    ws.Range("A1").Select
End Sub

' =============================================================================
'  5. Feuille Synthèse : tableaux clés et interprétation automatique
' =============================================================================
Private Sub EcrireSynthese()
    Dim ws As Worksheet, r As Long, k As Long, j As Long, i As Long, rng As Range, t() As Variant
    Dim cum As Double, rTab As Long, nAff As Long, hautG As Double, texte As String
    Dim gris As Long, bleu As Long, liste As String, nb As Long, a As Long, b As Long
    Dim wsA As Worksheet, pcts() As Double

    Set ws = ThisWorkbook.Worksheets(F_SYNTHESE)
    Set wsA = ThisWorkbook.Worksheets(F_ACCUEIL)
    PreparerFeuille ws, "ACP - Synthèse et interprétation", "Analyse générée le " & Format$(Now, "dd/mm/yyyy à hh:nn") & _
                    "  -  fichier : " & TexteCellule(wsA.Range(P_FICHIER).Value)
    ws.Columns("B").ColumnWidth = 30
    ws.Columns("C:G").ColumnWidth = 14
    gris = RGB(110, 110, 110)
    bleu = CouleurTitre()
    r = 4

    ' --- 1. Données
    r = Section(ws, r, "1. Données analysées")
    r = Paire(ws, r, "Individus analysés", nInd & " sur " & nLignesTotal & " lignes")
    If nExclues > 0 Then r = Paire(ws, r, "Lignes exclues (valeurs manquantes)", CStr(nExclues))
    If nImputees > 0 Then r = Paire(ws, r, "Valeurs remplacées par la moyenne", CStr(nImputees))
    r = Paire(ws, r, "Variables actives (" & nVar & ")", Join(nomsVar, ", "))
    If nSup > 0 Then r = Paire(ws, r, "Variables supplémentaires (" & nSup & ")", Join(nomsSup, ", "))
    If colEtiq > 0 Then r = Paire(ws, r, "Étiquette des individus", TexteCellule(ThisWorkbook.Worksheets(F_DONNEES).Cells(1, colEtiq).Value))
    If nGroupes > 0 Then r = Paire(ws, r, "Variable de groupe", nomColGroupe & " (" & nGroupes & " groupes)")
    If normee Then
        r = Paire(ws, r, "Type d'ACP", "Normée : variables centrées-réduites, diagonalisation de la matrice des corrélations (inertie totale = " & nVar & ")")
    Else
        r = Paire(ws, r, "Type d'ACP", "Non normée : variables centrées, diagonalisation de la matrice des covariances (inertie totale = " & NombreTexte(inertieTot, "0.000") & ")")
    End If
    r = r + 1

    ' --- 2. Adéquation
    r = Section(ws, r, "2. Les données se prêtent-elles à une ACP ?")
    If bartOK Then
        r = Paire(ws, r, "Test de sphéricité de Bartlett", "Khi² = " & NombreTexte(bartChi2, "0.00") & " ; ddl = " & bartDdl & " ; p-value = " & FormatP(bartP))
        If bartP >= 0 And bartP < 0.05 Then
            r = Ligne(ws, r, "-> Les corrélations entre variables sont significativement non nulles (p < 0,05) : la recherche d'axes communs a un sens.", False, bleu, 1)
        Else
            r = Ligne(ws, r, "-> On ne peut pas rejeter l'hypothèse de variables non corrélées : l'ACP risque de résumer peu d'information.", False, RGB(227, 73, 72), 1)
        End If
    End If
    If kmoOK Then
        r = Paire(ws, r, "Indice KMO (Kaiser-Meyer-Olkin)", NombreTexte(kmoGlobal, "0.000") & " - " & AppreciationKMO(kmoGlobal))
        liste = ""
        For j = 1 To nVar
            If msa(j) < 0.5 Then liste = liste & IIf(Len(liste) > 0, ", ", "") & nomsVar(j) & " (" & NombreTexte(msa(j), "0.00") & ")"
        Next j
        If Len(liste) > 0 Then
            r = Ligne(ws, r, "-> Variables peu adaptées (MSA < 0,5), à envisager de retirer : " & liste, False, RGB(227, 73, 72), 1)
        Else
            r = Ligne(ws, r, "-> Toutes les variables ont un indice MSA {>=} 0,5.", False, bleu, 1)
        End If
    End If
    If Not bartOK Then
        r = Ligne(ws, r, "Matrice des corrélations singulière : Bartlett et KMO non calculables (variables redondantes ou n {<=} p).", False, gris)
    End If
    ' Corrélations fortes
    liste = ""
    nb = 0
    For a = 1 To nVar - 1
        For b = a + 1 To nVar
            If Abs(matR(a, b)) >= 0.7 Then
                nb = nb + 1
                If nb <= 12 Then liste = liste & IIf(Len(liste) > 0, " ; ", "") & nomsVar(a) & " / " & nomsVar(b) & " (" & Signe(matR(a, b)) & ")"
            End If
        Next b
    Next a
    If nb > 0 Then
        r = Paire(ws, r, "Corrélations fortes (|r| {>=} 0,7)", nb & " paire(s) : " & liste & IIf(nb > 12, " ...", ""))
    Else
        r = Paire(ws, r, "Corrélations fortes (|r| {>=} 0,7)", "aucune")
    End If
    r = r + 1

    ' --- 3. Valeurs propres
    r = Section(ws, r, "3. Valeurs propres et inertie expliquée")
    nAff = nVar
    If nAff > 30 Then nAff = 30
    ReDim t(1 To nAff, 1 To 5)
    cum = 0
    For k = 1 To nAff
        cum = cum + Pct(k)
        t(k, 1) = "'Dim." & k
        t(k, 2) = lam(k)
        t(k, 3) = Pct(k)
        t(k, 4) = cum
        t(k, 5) = SeuilBaton(k)
    Next k
    rTab = r
    Set rng = Tableau(ws, r, 2, Array("", "Axe", "Valeur propre", "% d'inertie", "% cumulé", "Bâton brisé (%)"), t, nAff, 5, "0.00")
    rng.Columns(2).Offset(1).Resize(nAff).NumberFormat = "0.000"
    With rng.Columns(3).Offset(1).Resize(nAff).FormatConditions.AddDatabar
        .BarColor.Color = RGB(134, 182, 239)
        .MinPoint.Modify newtype:=xlConditionValueNumber, newvalue:=0
        .MaxPoint.Modify newtype:=xlConditionValueNumber, newvalue:=100
    End With
    For k = 1 To nbRetenus
        rng.Rows(k + 1).Font.Bold = True
    Next k
    hautG = ws.Rows(rTab).Top
    pcts = PctTableau()
    GraphEboulis ws, ws.Columns("H").Left, hautG, 480, 260, pcts, nAxes, nbRetenus
    r = r + nAff + 1
    If ws.Rows(r).Top < hautG + 270 Then r = LigneSous(ws, hautG + 270)
    r = Ligne(ws, r, "Valeur propre = variance de la composante principale. Bâton brisé : part d'inertie attendue si l'information était répartie au hasard.", False, gris)
    r = r + 1

    ' --- 4. Nombre d'axes
    r = Section(ws, r, "4. Combien d'axes retenir ?")
    If normee Then
        r = Paire(ws, r, "Critère de Kaiser (valeur propre > 1)", nbKaiser & " axe(s)")
    Else
        r = Paire(ws, r, "Critère de Kaiser (valeur propre > moyenne)", nbKaiser & " axe(s)")
    End If
    r = Paire(ws, r, "Critère du bâton brisé", nbBaton & " axe(s)")
    If nb80 > 0 Then r = Paire(ws, r, "Axes pour atteindre 80 % d'inertie", nb80 & " axe(s)")
    texte = "On retient " & nbRetenus & " axes, qui restituent " & NombreTexte(Cumul(nbRetenus), "0.0") & " % de l'inertie totale."
    If nbKaiser < 2 Then texte = texte & " (Kaiser n'en désigne que " & nbKaiser & " ; le premier plan est tout de même étudié.)"
    r = Ligne(ws, r, "-> " & texte, True, bleu, 1)
    r = Ligne(ws, r, "Le premier plan (Dim.1-Dim.2) restitue " & NombreTexte(Pct(1) + Pct(2), "0.0") & " % de l'inertie : " & _
                     AppreciationPlan(Pct(1) + Pct(2)), False, -1, 1)
    r = r + 1

    ' --- 5. Interprétation des axes
    r = Section(ws, r, "5. Interprétation des axes")
    For k = 1 To kAff
        r = InterpreterAxe(ws, r, k)
    Next k

    ' --- 6. Plan factoriel affiché
    r = Section(ws, r, "6. Plan factoriel Dim." & axeX & " - Dim." & axeY & " (graphiques)")
    r = Ligne(ws, r, "Ce plan restitue " & NombreTexte(Pct(axeX) + Pct(axeY), "0.0") & " % de l'inertie totale.", False, -1)
    liste = ""
    For j = 1 To nVar
        If cos2Var(j, axeX) + cos2Var(j, axeY) >= 0.5 Then liste = liste & IIf(Len(liste) > 0, ", ", "") & nomsVar(j) & " (" & NombreTexte(cos2Var(j, axeX) + cos2Var(j, axeY)) & ")"
    Next j
    If Len(liste) = 0 Then liste = "aucune"
    r = Ligne(ws, r, "Variables bien représentées (cos² {>=} 0,5) : " & liste, False, -1, 1)
    liste = ""
    For j = 1 To nVar
        If cos2Var(j, axeX) + cos2Var(j, axeY) < 0.3 Then liste = liste & IIf(Len(liste) > 0, ", ", "") & nomsVar(j) & " (" & NombreTexte(cos2Var(j, axeX) + cos2Var(j, axeY)) & ")"
    Next j
    If Len(liste) > 0 Then
        r = Ligne(ws, r, "Variables mal représentées (cos² < 0,3), à interpréter avec prudence sur ce plan : " & liste, False, -1, 1)
    End If
    nb = 0
    For i = 1 To nInd
        If cos2Ind(i, axeX) + cos2Ind(i, axeY) < 0.3 Then nb = nb + 1
    Next i
    r = Ligne(ws, r, nb & " individu(s) sur " & nInd & " sont mal représentés sur ce plan (cos² < 0,3) : leur position est à interpréter avec prudence.", False, -1, 1)
    If nSup > 0 Then
        For j = 1 To nSup
            r = Ligne(ws, r, "Variable supplémentaire « " & nomsSup(j) & " » : r = " & Signe(corSup(j, axeX)) & " avec Dim." & axeX & _
                             ", r = " & Signe(corSup(j, axeY)) & " avec Dim." & axeY & ".", False, -1, 1)
        Next j
    End If
    If nGroupes > 0 Then
        r = Ligne(ws, r, "Groupes (« " & nomColGroupe & " ») : eta² = " & NombreTexte(eta2(axeX), "0.00") & " sur Dim." & axeX & _
                         ", " & NombreTexte(eta2(axeY), "0.00") & " sur Dim." & axeY & " (" & AppreciationEta(Application.WorksheetFunction.Max(eta2(axeX), eta2(axeY))) & ").", False, -1, 1)
        For a = 1 To nGroupes
            r = Ligne(ws, r, nomsGroupes(a) & " (" & effGroupe(a) & " ind.) : barycentre (" & Signe(baryG(a, axeX)) & " ; " & Signe(baryG(a, axeY)) & ")", False, gris, 2)
        Next a
    End If
    r = r + 1

    ' --- 7. Remarques
    If Len(avertissements) > 0 Then
        r = Section(ws, r, "7. Remarques sur le traitement")
        Dim lignesAv As Variant
        lignesAv = Split(Mid$(avertissements, Len(vbCrLf) + 1), vbCrLf)
        For i = LBound(lignesAv) To UBound(lignesAv)
            r = Ligne(ws, r, CStr(lignesAv(i)), False, gris)
        Next i
        r = r + 1
    End If

    ' --- Guide de lecture
    r = Section(ws, r, "Guide de lecture")
    r = Ligne(ws, r, "Cercle des corrélations : une flèche proche du cercle indique une variable bien représentée ; deux flèches proches = variables corrélées positivement,", False, gris)
    r = Ligne(ws, r, "opposées = corrélées négativement, orthogonales = non corrélées (à condition qu'elles soient bien représentées).", False, gris, 1)
    r = Ligne(ws, r, "Carte des individus : deux individus proches ont des profils semblables (s'ils sont bien représentés : cos² élevé).", False, gris)
    r = Ligne(ws, r, "Un individu situé du côté d'une flèche présente des valeurs élevées pour cette variable ; du côté opposé, des valeurs faibles.", False, gris, 1)
    r = Ligne(ws, r, "Contribution : part (en %) d'une variable ou d'un individu dans la construction d'un axe. cos² : qualité de représentation (0 à 1).", False, gris)
    r = Ligne(ws, r, "Détails : feuilles « " & F_VARIABLES & " » et « " & F_INDIVIDUS & " ».", False, gris)

    ws.Range("A1").Select
End Sub

' Remplace les jetons {>=} et {<=} par les symboles Unicode correspondants
Private Function Sym(ByVal s As String) As String
    Sym = Replace(Replace(s, "{>=}", ChrW(8805)), "{<=}", ChrW(8804))
End Function

Private Function Paire(ws As Worksheet, ByVal r As Long, ByVal libelle As String, ByVal valeur As String) As Long
    ws.Cells(r, 2).Value = "'" & Sym(libelle)
    ws.Cells(r, 2).Font.Color = RGB(90, 90, 90)
    ws.Cells(r, 3).Value = "'" & valeur
    ws.Cells(r, 3).Font.Bold = True
    Paire = r + 1
End Function

Private Function PctTableau() As Double()
    Dim t() As Double, k As Long
    ReDim t(1 To nVar)
    For k = 1 To nVar
        t(k) = Pct(k)
    Next k
    PctTableau = t
End Function

Private Function Cumul(ByVal kk As Long) As Double
    Dim k As Long
    For k = 1 To kk
        Cumul = Cumul + Pct(k)
    Next k
End Function

Private Function FormatP(ByVal p As Double) As String
    If p < 0 Then
        FormatP = "n.c."
    ElseIf p < 0.0001 Then
        FormatP = "< 0,0001"
    Else
        FormatP = Format$(p, "0.0000")
    End If
End Function

Private Function AppreciationKMO(ByVal x As Double) As String
    If x >= 0.9 Then
        AppreciationKMO = "excellent"
    ElseIf x >= 0.8 Then
        AppreciationKMO = "très bon"
    ElseIf x >= 0.7 Then
        AppreciationKMO = "bon"
    ElseIf x >= 0.6 Then
        AppreciationKMO = "moyen"
    ElseIf x >= 0.5 Then
        AppreciationKMO = "médiocre, mais acceptable"
    Else
        AppreciationKMO = "insuffisant : les variables partagent peu d'information commune"
    End If
End Function

Private Function AppreciationPlan(ByVal p As Double) As String
    If p >= 70 Then
        AppreciationPlan = "très bonne représentation, l'essentiel de l'information est visible sur les graphiques."
    ElseIf p >= 50 Then
        AppreciationPlan = "représentation satisfaisante, compléter par l'étude des axes suivants si besoin."
    Else
        AppreciationPlan = "représentation partielle, les axes suivants portent une part importante de l'information."
    End If
End Function

Private Function AppreciationEta(ByVal e As Double) As String
    If e >= 0.5 Then
        AppreciationEta = "groupes nettement séparés"
    ElseIf e >= 0.2 Then
        AppreciationEta = "groupes partiellement séparés"
    Else
        AppreciationEta = "groupes peu séparés"
    End If
End Function

' Texte d'interprétation d'un axe
Private Function InterpreterAxe(ws As Worksheet, ByVal r As Long, ByVal k As Long) As Long
    Dim ordre() As Long, valeurs() As Double, j As Long, i As Long, n As Long
    Dim pos As String, neg As String, nbPos As Long, nbNeg As Long, seuil As Double
    Dim contrib As String, scoreInd() As Double, ordreInd() As Long, haut As String, bas As String
    Dim nbI As Long, texte As String, c As Long

    r = Ligne(ws, r, "Dim." & k & " : valeur propre " & NombreTexte(lam(k), "0.000") & ", " & NombreTexte(Pct(k), "0.0") & _
                     " % de l'inertie (cumul " & NombreTexte(Cumul(k), "0.0") & " %)", True, CouleurTitre())

    ' Variables triées par corrélation
    ReDim valeurs(1 To nVar)
    For j = 1 To nVar
        valeurs(j) = corVar(j, k)
    Next j
    ordre = OrdreDecroissant(valeurs, nVar)
    seuil = 0.5
    For j = 1 To nVar
        If valeurs(ordre(j)) >= seuil Then
            nbPos = nbPos + 1
            pos = pos & IIf(nbPos > 1, ", ", "") & nomsVar(ordre(j)) & " (" & Signe(valeurs(ordre(j))) & ")"
        End If
    Next j
    For j = nVar To 1 Step -1
        If valeurs(ordre(j)) <= -seuil Then
            nbNeg = nbNeg + 1
            neg = neg & IIf(nbNeg > 1, ", ", "") & nomsVar(ordre(j)) & " (" & Signe(valeurs(ordre(j))) & ")"
        End If
    Next j

    If nbPos + nbNeg = 0 Then
        ' aucune corrélation forte : on cite les deux plus fortes
        pos = ""
        For j = 1 To nVar
            valeurs(j) = Abs(corVar(j, k))
        Next j
        ordre = OrdreDecroissant(valeurs, nVar)
        For j = 1 To Application.WorksheetFunction.Min(2, nVar)
            pos = pos & IIf(j > 1, ", ", "") & nomsVar(ordre(j)) & " (" & Signe(corVar(ordre(j), k)) & ")"
        Next j
        r = Ligne(ws, r, "Aucune variable n'est fortement corrélée (|r| {>=} 0,5) à cet axe ; les plus liées sont : " & pos & ". Axe difficile à interpréter.", False, -1, 1)
    Else
        If nbPos > 0 Then r = Ligne(ws, r, "Côté positif (r {>=} 0,5) : " & pos, False, -1, 1)
        If nbNeg > 0 Then r = Ligne(ws, r, "Côté négatif (r {<=} -0,5) : " & neg, False, -1, 1)
        If nbPos > 0 And nbNeg > 0 Then
            texte = "Cet axe oppose les individus ayant de fortes valeurs de " & NomsCourts(pos) & " à ceux ayant de fortes valeurs de " & NomsCourts(neg) & "."
        ElseIf nbPos >= 2 And k = 1 And nbPos >= nVar * 0.6 Then
            texte = "Toutes les variables liées à l'axe vont dans le même sens : c'est un « effet taille », l'axe classe les individus selon leur niveau général (" & NomsCourts(pos) & ")."
        ElseIf nbPos > 0 Then
            texte = "Les individus à droite (coordonnée positive) ont des valeurs élevées de " & NomsCourts(pos) & ", ceux à gauche des valeurs faibles."
        Else
            texte = "Les individus à coordonnée négative ont des valeurs élevées de " & NomsCourts(neg) & "."
        End If
        r = Ligne(ws, r, "-> " & texte, False, CouleurTitre(), 1)
    End If

    ' Variables contributives
    contrib = ""
    For j = 1 To nVar
        valeurs(j) = ctrVar(j, k)
    Next j
    ordre = OrdreDecroissant(valeurs, nVar)
    c = 0
    For j = 1 To nVar
        If valeurs(ordre(j)) > 100# / nVar Then
            c = c + 1
            contrib = contrib & IIf(c > 1, ", ", "") & nomsVar(ordre(j)) & " (" & NombreTexte(valeurs(ordre(j)), "0.0") & " %)"
        End If
    Next j
    If Len(contrib) > 0 Then r = Ligne(ws, r, "Variables contribuant le plus (> " & NombreTexte(100# / nVar, "0.0") & " %) : " & contrib, False, RGB(90, 90, 90), 1)

    ' Individus extrêmes et contributifs
    ReDim scoreInd(1 To nInd)
    For i = 1 To nInd
        scoreInd(i) = matF(i, k)
    Next i
    ordreInd = OrdreDecroissantRapide(scoreInd, nInd)
    nbI = 3
    If nInd < 6 Then nbI = 1
    haut = "": bas = ""
    For i = 1 To nbI
        haut = haut & IIf(i > 1, ", ", "") & etiq(ordreInd(i)) & " (" & Signe(matF(ordreInd(i), k)) & ")"
        bas = bas & IIf(i > 1, ", ", "") & etiq(ordreInd(nInd - i + 1)) & " (" & Signe(matF(ordreInd(nInd - i + 1), k)) & ")"
    Next i
    r = Ligne(ws, r, "Individus les plus à l'extrémité positive : " & haut & " ; négative : " & bas, False, RGB(90, 90, 90), 1)
    For i = 1 To nInd
        scoreInd(i) = ctrInd(i, k)
    Next i
    ordreInd = OrdreDecroissantRapide(scoreInd, nInd)
    contrib = ""
    For i = 1 To Application.WorksheetFunction.Min(3, nInd)
        contrib = contrib & IIf(i > 1, ", ", "") & etiq(ordreInd(i)) & " (" & NombreTexte(ctrInd(ordreInd(i), k), "0.0") & " %)"
    Next i
    r = Ligne(ws, r, "Individus les plus contributifs : " & contrib & IIf(ctrInd(ordreInd(1), k) > 25, "  -> attention, un individu pèse fortement sur cet axe (atypique ?).", ""), False, RGB(90, 90, 90), 1)
    If nGroupes > 0 Then
        r = Ligne(ws, r, "Séparation des groupes sur cet axe : eta² = " & NombreTexte(eta2(k), "0.00") & " (" & AppreciationEta(eta2(k)) & ").", False, RGB(90, 90, 90), 1)
    End If
    InterpreterAxe = r + 1
End Function

' "A (+0,91), B (+0,85)" -> "A, B" (3 noms au plus)
Private Function NomsCourts(ByVal liste As String) As String
    Dim morceaux As Variant, i As Long, p As Long, s As String, n As Long
    morceaux = Split(liste, "), ")
    For i = LBound(morceaux) To UBound(morceaux)
        p = InStrRev(morceaux(i), " (")
        If p > 0 Then
            n = n + 1
            If n <= 3 Then s = s & IIf(n > 1, ", ", "") & Left$(morceaux(i), p - 1)
        End If
    Next i
    If n > 3 Then s = s & "..."
    NomsCourts = s
End Function
