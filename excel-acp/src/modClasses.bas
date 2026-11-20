Attribute VB_Name = "modClasses"
' =============================================================================
'  ACP Excel - Module : classification des individus (clustering)
'  Classification ascendante hiérarchique (CAH) de Ward sur les coordonnées
'  des individus dans les premières dimensions de l'ACP, choix du nombre de
'  classes, consolidation par k-means, caractérisation des classes (v-tests),
'  dendrogramme et carte des classes.
' =============================================================================
Option Explicit

Private Const SEUIL_PREGROUPEMENT As Long = 2000   ' au-delà : pré-regroupement k-means
Private Const NB_PREGROUPES As Long = 100
Private Const K_MAX As Long = 8                    ' 8 couleurs distinctes au plus
Private Const NB_FEUILLES As Long = 20             ' feuilles affichées sur le dendrogramme

Private nCl As Long, qAx As Long, kAuto As Boolean, conso As Boolean
Private classe() As Long                 ' classe finale de chaque individu (1..nCl)
Private centre() As Double               ' centres des classes (nCl x qAx)
Private effCl() As Long
Private nU As Long, uniteInd() As Long, poidsU() As Double, classeU() As Long
Private fusA() As Long, fusB() As Long, fusH() As Double, ordreFus() As Long
Private hauteurs() As Double, nHaut As Long
Private numCl() As Long                  ' numérotation CAH -> numérotation finale
Private methodeTxt As String, nDeplaces As Long, nIterKM As Long
Private inertieInter As Double, inertieTotQ As Double
Private avertCl As String

' =============================================================================
'  Point d'entrée (bouton « Lancer la classification » de la feuille Accueil)
' =============================================================================
Public Sub LancerClassification()
    Dim wsA As Worksheet, t0 As Double, calc As XlCalculation, s As String, kDemande As Long
    t0 = Timer
    calc = Application.Calculation
    On Error GoTo Erreur
    avertCl = ""

    Set wsA = ObtenirFeuille(F_ACCUEIL)
    If wsA Is Nothing Then Err.Raise ERR_ACP, , "Feuille « Accueil » introuvable : exécutez la macro InstallerApplication."
    If Not ChargerStockage() Then
        Err.Raise ERR_ACP, , "Lancez d'abord l'ACP (bouton « Lancer l'ACP ») : la classification s'appuie sur ses résultats."
    End If
    If nInd < 4 Then Err.Raise ERR_ACP, , "Il faut au moins 4 individus pour une classification."

    ' Paramètres
    s = TexteCellule(wsA.Range(P_CL_AXES).Value)
    qAx = CLng(Val(s))
    If qAx < 1 Then
        qAx = nbRetenus
        If qAx < 2 Then qAx = 2
    End If
    If qAx > kCalc Then
        avertCl = avertCl & vbCrLf & "- Seules " & kCalc & " dimensions sont disponibles : la classification utilise les dimensions 1 à " & kCalc & "."
        qAx = kCalc
    End If
    s = TexteCellule(wsA.Range(P_CL_NB).Value)
    kDemande = CLng(Val(s))
    kAuto = (kDemande < 2)
    conso = (UCase$(TexteCellule(wsA.Range(P_CL_CONSO).Value)) <> "NON")

    Application.ScreenUpdating = False
    Application.Calculation = xlCalculationManual
    Application.Cursor = xlWait

    Classifier kDemande
    Application.StatusBar = "Classification : création de la feuille de résultats..."
    SupprimerClasses
    EcrireStockClasses
    EcrireFeuilleClasses

    AllerA ThisWorkbook.Worksheets(F_CLASSES), "A1"
    Application.Calculation = calc
    Application.Cursor = xlDefault
    Application.StatusBar = False
    Application.ScreenUpdating = True
    MsgBox "Classification terminée en " & Format$(Timer - t0, "0.0") & " s." & vbCrLf & vbCrLf & _
           nCl & " classes obtenues sur les " & qAx & " premières dimensions." & vbCrLf & _
           "Inertie expliquée par les classes : " & Format$(100# * Ratio(inertieInter, inertieTotQ), "0.0") & " %." & _
           IIf(Len(avertCl) > 0, vbCrLf & vbCrLf & "Remarques :" & avertCl, ""), vbInformation, "Classification"
    Exit Sub

Erreur:
    Application.Calculation = calc
    Application.Cursor = xlDefault
    Application.StatusBar = False
    Application.ScreenUpdating = True
    MsgBox Err.Description, vbExclamation, "Classification impossible"
End Sub

Private Function Ratio(ByVal a As Double, ByVal b As Double) As Double
    If b > 0 Then Ratio = a / b
End Function

Private Sub SupprimerClasses()
    Dim noms As Variant, i As Long, ws As Worksheet
    noms = Array(F_CLASSES, F_GRAPH_CL, F_GRAPH_CL2, F_STOCK_CL)
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

' =============================================================================
'  Calcul de la classification
' =============================================================================
Private Sub Classifier(ByVal kDemande As Long)
    Dim cU() As Double, i As Long, k As Long, r As Long, nm As Long
    Dim parent() As Long, numero() As Long, ra As Long, rb As Long
    Dim kMin As Long, kMax As Long, meilleur As Double, rapport As Double, negH() As Double
    Dim classeCAH() As Long

    ' 1) Unités de départ : les individus, ou des pré-groupes si les individus sont très nombreux
    Application.StatusBar = "Classification : préparation..."
    If nInd > SEUIL_PREGROUPEMENT Then
        PreRegrouper cU
        methodeTxt = "pré-regroupement des " & nInd & " individus en " & nU & " groupes (k-means), puis CAH de Ward sur ces groupes"
        avertCl = avertCl & vbCrLf & "- " & nInd & " individus : pré-regroupement en " & nU & " groupes avant la CAH."
    Else
        nU = nInd
        ReDim cU(1 To nU, 1 To qAx)
        ReDim poidsU(1 To nU)
        ReDim uniteInd(1 To nInd)
        For i = 1 To nInd
            uniteInd(i) = i
            poidsU(i) = 1
            For k = 1 To qAx
                cU(i, k) = matF(i, k)
            Next k
        Next i
        methodeTxt = "CAH de Ward sur les individus"
    End If
    If nU < 2 Then Err.Raise ERR_ACP, , "Pas assez d'individus distincts pour une classification."

    ' 2) CAH de Ward
    Application.StatusBar = "Classification : CAH de Ward sur " & nU & " éléments..."
    CAHWard cU
    nm = nU - 1
    ReDim negH(1 To nm)
    For r = 1 To nm
        negH(r) = -fusH(r)
    Next r
    ordreFus = OrdreDecroissant(negH, nm)          ' fusions par hauteur croissante
    nHaut = nm
    ReDim hauteurs(1 To nm)
    For r = 1 To nm
        hauteurs(r) = fusH(ordreFus(nm - r + 1)) / nInd   ' hauteurs(1) = dernière fusion (2 -> 1 classe)
    Next r

    ' 3) Nombre de classes
    kMax = K_MAX
    If kMax > nU Then kMax = nU
    If kAuto Then
        kMin = 3
        If kMax < 3 Then kMin = 2
        nCl = kMin
        meilleur = -1
        For k = kMin To kMax
            If k <= nm Then
                If hauteurs(k) > 0 Then rapport = hauteurs(k - 1) / hauteurs(k) Else rapport = 1E+300
                If rapport > meilleur Then
                    meilleur = rapport
                    nCl = k
                End If
            End If
        Next k
    Else
        nCl = kDemande
        If nCl > kMax Then
            avertCl = avertCl & vbCrLf & "- Nombre de classes ramené à " & kMax & " (maximum possible)."
            nCl = kMax
        End If
    End If

    ' 4) Coupure de l'arbre en nCl classes
    ReDim parent(1 To nU)
    For i = 1 To nU
        parent(i) = i
    Next i
    For r = 1 To nU - nCl
        ra = Trouver(parent, fusA(ordreFus(r)))
        rb = Trouver(parent, fusB(ordreFus(r)))
        If ra <> rb Then parent(rb) = ra
    Next r
    ReDim numero(1 To nU)
    ReDim classeU(1 To nU)
    k = 0
    For i = 1 To nU
        ra = Trouver(parent, i)
        If numero(ra) = 0 Then
            k = k + 1
            numero(ra) = k
        End If
        classeU(i) = numero(ra)
    Next i
    nCl = k
    ReDim classe(1 To nInd)
    For i = 1 To nInd
        classe(i) = classeU(uniteInd(i))
    Next i
    classeCAH = classe

    ' 5) Consolidation par k-means
    nDeplaces = 0
    nIterKM = 0
    If conso Then
        Application.StatusBar = "Classification : consolidation (k-means)..."
        Consolider
        For i = 1 To nInd
            If classe(i) <> classeCAH(i) Then nDeplaces = nDeplaces + 1
        Next i
    End If

    ' 6) Numérotation finale (classes ordonnées selon la dimension 1) et inerties
    Renumeroter
End Sub

Private Function Trouver(parent() As Long, ByVal i As Long) As Long
    Dim r As Long, t As Long
    r = i
    Do While parent(r) <> r
        r = parent(r)
    Loop
    Do While parent(i) <> r
        t = parent(i)
        parent(i) = r
        i = t
    Loop
    Trouver = r
End Function

' CAH de Ward par l'algorithme de la chaîne des plus proches voisins (O(n²))
' Distance de Ward : D(a,b) = wa.wb / (wa + wb) . ||ca - cb||² (augmentation d'inertie)
Private Sub CAHWard(cU() As Double)
    Dim D() As Double, w() As Double, actif() As Boolean, chaine() As Long
    Dim i As Long, j As Long, k As Long, a As Long, b As Long, lc As Long, nAct As Long, nm As Long
    Dim s As Double, x As Double, best As Double, dn As Double, fusion As Boolean

    ReDim D(1 To nU, 1 To nU)
    ReDim w(1 To nU)
    ReDim actif(1 To nU)
    ReDim chaine(1 To nU)
    ReDim fusA(1 To nU - 1)
    ReDim fusB(1 To nU - 1)
    ReDim fusH(1 To nU - 1)
    For i = 1 To nU
        w(i) = poidsU(i)
        actif(i) = True
    Next i
    For i = 1 To nU - 1
        For j = i + 1 To nU
            s = 0
            For k = 1 To qAx
                x = cU(i, k) - cU(j, k)
                s = s + x * x
            Next k
            D(i, j) = w(i) * w(j) / (w(i) + w(j)) * s
            D(j, i) = D(i, j)
        Next j
    Next i

    nAct = nU
    lc = 0
    nm = 0
    Do While nAct > 1
        If lc = 0 Then
            For i = 1 To nU
                If actif(i) Then Exit For
            Next i
            lc = 1
            chaine(1) = i
        End If
        a = chaine(lc)
        b = 0
        best = 1E+308
        If lc >= 2 Then
            b = chaine(lc - 1)
            best = D(a, b)
        End If
        For j = 1 To nU
            If actif(j) Then
                If j <> a Then
                    If D(a, j) < best Then
                        best = D(a, j)
                        b = j
                    End If
                End If
            End If
        Next j
        If b = 0 Then Err.Raise ERR_ACP, , "Erreur interne de la CAH (aucun voisin)."

        fusion = False
        If lc >= 2 Then
            If b = chaine(lc - 1) Then fusion = True
        End If
        If fusion Then
            nm = nm + 1
            fusA(nm) = a
            fusB(nm) = b
            fusH(nm) = best
            ' Formule de Lance-Williams pour Ward
            For k = 1 To nU
                If actif(k) Then
                    If k <> a And k <> b Then
                        dn = ((w(a) + w(k)) * D(a, k) + (w(b) + w(k)) * D(b, k) - w(k) * best) / (w(a) + w(b) + w(k))
                        D(a, k) = dn
                        D(k, a) = dn
                    End If
                End If
            Next k
            w(a) = w(a) + w(b)
            actif(b) = False
            nAct = nAct - 1
            lc = lc - 2
        Else
            lc = lc + 1
            chaine(lc) = b
        End If
    Loop
End Sub

' Pré-regroupement k-means (individus très nombreux)
Private Sub PreRegrouper(cU() As Double)
    Dim m As Long, i As Long, j As Long, k As Long, it As Long, jb As Long
    Dim ordre() As Long, v() As Double, c() As Double, somme() As Double, nb() As Long, aff() As Long, num() As Long
    Dim best As Double, d As Double, x As Double

    m = NB_PREGROUPES
    ReDim v(1 To nInd)
    For i = 1 To nInd
        v(i) = matF(i, 1)
    Next i
    ordre = OrdreDecroissantRapide(v, nInd)
    ReDim c(1 To m, 1 To qAx)
    For j = 1 To m
        i = ordre(1 + Int((j - 0.5) * nInd / m))
        For k = 1 To qAx
            c(j, k) = matF(i, k)
        Next k
    Next j

    ReDim aff(1 To nInd)
    For it = 1 To 10
        Application.StatusBar = "Classification : pré-regroupement (itération " & it & " / 10)..."
        ReDim somme(1 To m, 1 To qAx)
        ReDim nb(1 To m)
        For i = 1 To nInd
            best = 1E+308
            jb = 1
            For j = 1 To m
                d = 0
                For k = 1 To qAx
                    x = matF(i, k) - c(j, k)
                    d = d + x * x
                Next k
                If d < best Then
                    best = d
                    jb = j
                End If
            Next j
            aff(i) = jb
            nb(jb) = nb(jb) + 1
            For k = 1 To qAx
                somme(jb, k) = somme(jb, k) + matF(i, k)
            Next k
        Next i
        For j = 1 To m
            If nb(j) > 0 Then
                For k = 1 To qAx
                    c(j, k) = somme(j, k) / nb(j)
                Next k
            End If
        Next j
    Next it

    ReDim num(1 To m)
    nU = 0
    For j = 1 To m
        If nb(j) > 0 Then
            nU = nU + 1
            num(j) = nU
        End If
    Next j
    ReDim cU(1 To nU, 1 To qAx)
    ReDim poidsU(1 To nU)
    ReDim uniteInd(1 To nInd)
    For j = 1 To m
        If num(j) > 0 Then
            poidsU(num(j)) = nb(j)
            For k = 1 To qAx
                cU(num(j), k) = c(j, k)
            Next k
        End If
    Next j
    For i = 1 To nInd
        uniteInd(i) = num(aff(i))
    Next i
End Sub

' Centres des classes sur les qAx premières dimensions (centre inchangé si classe vide)
Private Sub CalculerCentres()
    Dim i As Long, k As Long, j As Long, somme() As Double
    ReDim somme(1 To nCl, 1 To qAx)
    ReDim effCl(1 To nCl)
    For i = 1 To nInd
        j = classe(i)
        effCl(j) = effCl(j) + 1
        For k = 1 To qAx
            somme(j, k) = somme(j, k) + matF(i, k)
        Next k
    Next i
    For j = 1 To nCl
        If effCl(j) > 0 Then
            For k = 1 To qAx
                centre(j, k) = somme(j, k) / effCl(j)
            Next k
        End If
    Next j
End Sub

Private Sub Consolider()
    Dim it As Long, i As Long, j As Long, k As Long, jb As Long, nChang As Long
    Dim best As Double, d As Double, x As Double
    ReDim centre(1 To nCl, 1 To qAx)
    CalculerCentres
    For it = 1 To 50
        nChang = 0
        For i = 1 To nInd
            jb = classe(i)
            best = 0
            For k = 1 To qAx
                x = matF(i, k) - centre(jb, k)
                best = best + x * x
            Next k
            For j = 1 To nCl
                If j <> classe(i) Then
                    d = 0
                    For k = 1 To qAx
                        x = matF(i, k) - centre(j, k)
                        d = d + x * x
                    Next k
                    If d < best Then
                        best = d
                        jb = j
                    End If
                End If
            Next j
            If jb <> classe(i) Then
                classe(i) = jb
                nChang = nChang + 1
            End If
        Next i
        nIterKM = it
        If nChang = 0 Then Exit For
        CalculerCentres
    Next it
End Sub

' Supprime les classes vides, numérote les classes selon la dimension 1, calcule les inerties
Private Sub Renumeroter()
    Dim j As Long, k As Long, i As Long, a As Long, b As Long, n As Long, tmp As Long
    Dim ordre() As Long, nouveau() As Long, ancienNCl As Long

    ReDim centre(1 To nCl, 1 To qAx)
    CalculerCentres
    ancienNCl = nCl
    ReDim ordre(1 To nCl)
    n = 0
    For j = 1 To nCl
        If effCl(j) > 0 Then
            n = n + 1
            ordre(n) = j
        End If
    Next j
    ' tri par coordonnée croissante sur la dimension 1
    For a = 1 To n - 1
        For b = a + 1 To n
            If centre(ordre(b), 1) < centre(ordre(a), 1) Then
                tmp = ordre(a): ordre(a) = ordre(b): ordre(b) = tmp
            End If
        Next b
    Next a
    ReDim nouveau(1 To ancienNCl)
    For a = 1 To n
        nouveau(ordre(a)) = a
    Next a
    numCl = nouveau
    For i = 1 To nInd
        classe(i) = nouveau(classe(i))
    Next i
    If n < nCl Then avertCl = avertCl & vbCrLf & "- " & (nCl - n) & " classe(s) vidée(s) par la consolidation."
    nCl = n
    ReDim centre(1 To nCl, 1 To qAx)
    CalculerCentres

    ' Inerties (dimensions 1 à qAx) : totale et inter-classes
    inertieTotQ = 0
    For i = 1 To nInd
        For k = 1 To qAx
            inertieTotQ = inertieTotQ + matF(i, k) ^ 2
        Next k
    Next i
    inertieTotQ = inertieTotQ / nInd
    inertieInter = 0
    For j = 1 To nCl
        For k = 1 To qAx
            inertieInter = inertieInter + effCl(j) * centre(j, k) ^ 2
        Next k
    Next j
    inertieInter = inertieInter / nInd
End Sub

' =============================================================================
'  Conservation des classes (pour redessiner la carte)
' =============================================================================
Private Sub EcrireStockClasses()
    Dim ws As Worksheet, t() As Variant, i As Long
    Set ws = ObtenirFeuille(F_STOCK_CL, True)
    ws.Cells.Clear
    ws.Visible = xlSheetHidden
    ws.Range("A1").Value = "nCl": ws.Range("B1").Value = nCl
    ws.Range("A2").Value = "q": ws.Range("B2").Value = qAx
    ws.Range("A3").Value = "nInd": ws.Range("B3").Value = nInd
    ReDim t(1 To nInd, 1 To 1)
    For i = 1 To nInd
        t(i, 1) = classe(i)
    Next i
    ws.Range("D1").Resize(nInd, 1).Value = t
End Sub

Private Function ChargerClasses() As Boolean
    Dim ws As Worksheet, t As Variant, i As Long
    Set ws = ObtenirFeuille(F_STOCK_CL)
    If ws Is Nothing Then Exit Function
    If TexteCellule(ws.Range("A1").Value) <> "nCl" Then Exit Function
    If CLng(ws.Range("B3").Value) <> nInd Then Exit Function
    nCl = CLng(ws.Range("B1").Value)
    qAx = CLng(ws.Range("B2").Value)
    If nCl < 1 Or nInd < 1 Then Exit Function
    t = ws.Range("D1").Resize(nInd, 1).Value
    ReDim classe(1 To nInd)
    For i = 1 To nInd
        classe(i) = CLng(t(i, 1))
    Next i
    ChargerClasses = True
End Function

' =============================================================================
'  Carte des individus colorés par classe (redessinable pour tout plan)
' =============================================================================
Public Sub DessinerCarteClasses(ws As Worksheet, ByVal ax As Long, ByVal ay As Long)
    Dim i As Long, j As Long, haut As Double, gauche As Double, ordre() As Long
    Dim fx() As Double, fy() As Double, montrer() As Boolean, score() As Double
    Dim cx() As Double, cy() As Double, nb() As Long, nomsC() As String

    If Not ChargerClasses() Then Err.Raise ERR_ACP, , "Classes introuvables : relancez la classification depuis la feuille Accueil."
    On Error Resume Next
    ws.ChartObjects("Individus").Delete
    On Error GoTo 0
    InitGraphiques F_GRAPH_CL
    ws.Rows(3).RowHeight = 22
    haut = ws.Rows(4).Top + 4
    gauche = ws.Columns("B").Left

    ReDim fx(1 To nInd): ReDim fy(1 To nInd): ReDim montrer(1 To nInd): ReDim score(1 To nInd)
    ReDim cx(1 To nCl): ReDim cy(1 To nCl): ReDim nb(1 To nCl): ReDim nomsC(1 To nCl)
    For i = 1 To nInd
        fx(i) = matF(i, ax)
        fy(i) = matF(i, ay)
        score(i) = ctrInd(i, ax) * lam(ax) + ctrInd(i, ay) * lam(ay)
        j = classe(i)
        nb(j) = nb(j) + 1
        cx(j) = cx(j) + fx(i)
        cy(j) = cy(j) + fy(i)
    Next i
    For j = 1 To nCl
        If nb(j) > 0 Then
            cx(j) = cx(j) / nb(j)
            cy(j) = cy(j) / nb(j)
        End If
        nomsC(j) = "Classe " & j
    Next j
    If nInd <= maxEtiq Then
        For i = 1 To nInd
            montrer(i) = True
        Next i
    ElseIf maxEtiq > 0 Then
        ordre = OrdreDecroissantRapide(score, nInd)
        For i = 1 To maxEtiq
            montrer(ordre(i)) = True
        Next i
    End If
    GraphIndividus ws, gauche, haut, 600, 470, nInd, fx, fy, etiq, montrer, classe, nomsC, nCl, cx, cy, _
                   TitreDim(ax), TitreDim(ay), "Carte des individus par classe", "Centres des classes"
End Sub

Private Function TitreDim(ByVal k As Long) As String
    TitreDim = "Dim." & k & " (" & Format$(Pct(k), "0.0") & " %)"
End Function

' =============================================================================
'  Feuille ACP_Classes
' =============================================================================
Private Sub EcrireFeuilleClasses()
    Dim ws As Worksheet, wsA As Worksheet, r As Long, i As Long, j As Long, k As Long, c As Long
    Dim tables As Collection, rng As Range, t() As Variant, e() As Variant
    Dim ax As Long, ay As Long, haut As Double, gauche As Double, nV As Long, nbC As Long
    Dim vt() As Double, moyC() As Double, moyG() As Double, nomsV() As String
    Dim d2() As Double, gris As Long, txt As String, kq As Long

    Set ws = ObtenirFeuille(F_CLASSES, True)
    Set wsA = ThisWorkbook.Worksheets(F_ACCUEIL)
    PreparerFeuille ws, "ACP - Classification des individus", _
                    "CAH de Ward sur les " & qAx & " premières dimensions de l'ACP" & IIf(conso, ", consolidée par k-means", "")
    Set tables = New Collection
    gris = RGB(110, 110, 110)

    ' --- Graphiques
    ax = CLng(Val(TexteCellule(wsA.Range(P_AXE_X).Value)))
    ay = CLng(Val(TexteCellule(wsA.Range(P_AXE_Y).Value)))
    If ax < 1 Or ax > kCalc Or ay < 1 Or ay > kCalc Or ax = ay Then
        ax = 1: ay = 2
    End If
    DessinerCarteClasses ws, ax, ay
    haut = ws.Rows(4).Top + 4
    gauche = ws.Columns("B").Left
    InitGraphiques F_GRAPH_CL2
    DessinerDendrogramme ws, gauche + 615, haut, 440, 230
    GraphGains ws, gauche + 615, haut + 240, 440, 230, hauteurs, nHaut, nCl
    r = LigneSous(ws, haut + 470) + 1

    ' --- 1. Résumé
    r = Section(ws, r, "1. Résumé de la classification")
    r = Paire(ws, r, "Nombre de classes", nCl & IIf(kAuto, " (choisi automatiquement : plus fort saut d'inertie entre 3 et 8 classes)", " (imposé)"))
    r = Paire(ws, r, "Dimensions utilisées", "Dim.1 à Dim." & qAx & " (" & NombreTexte(CumulPct(qAx), "0.0") & " % de l'inertie de l'ACP)")
    r = Paire(ws, r, "Méthode", methodeTxt)
    If conso Then
        r = Paire(ws, r, "Consolidation k-means", nDeplaces & " individu(s) réaffecté(s) en " & nIterKM & " itération(s)")
    Else
        r = Paire(ws, r, "Consolidation k-means", "non")
    End If
    r = Paire(ws, r, "Inertie inter-classes / totale", NombreTexte(100# * Ratio(inertieInter, inertieTotQ), "0.0") & _
                     " % (part de la variabilité expliquée par la partition)")
    r = r + 1

    ' --- 2. Description des classes
    r = Section(ws, r, "2. Les classes")
    kq = qAx
    If kq > 10 Then kq = 10
    ReDim d2(1 To nInd)
    For i = 1 To nInd
        For k = 1 To qAx
            d2(i) = d2(i) + (matF(i, k) - centre(classe(i), k)) ^ 2
        Next k
    Next i
    nbC = 4 + kq
    ReDim e(0 To nbC)
    e(1) = "Classe": e(2) = "Effectif": e(3) = "% des individus": e(4) = "Inertie intra-classe"
    For k = 1 To kq
        e(4 + k) = "Centre Dim." & k
    Next k
    ReDim t(1 To nCl, 1 To nbC)
    For j = 1 To nCl
        t(j, 1) = "Classe " & j
        t(j, 2) = effCl(j)
        t(j, 3) = 100# * effCl(j) / nInd
        t(j, 4) = 0
        For k = 1 To kq
            t(j, 4 + k) = centre(j, k)
        Next k
    Next j
    For i = 1 To nInd
        t(classe(i), 4) = t(classe(i), 4) + d2(i) / nInd
    Next i
    Set rng = Tableau(ws, r, 2, e, t, nCl, nbC, "0.000")
    rng.Cells(2, 2).Resize(nCl, 1).NumberFormat = "0"
    rng.Cells(2, 3).Resize(nCl, 1).NumberFormat = "0.0"
    For j = 1 To nCl
        rng.Cells(j + 1, 1).Interior.Color = CouleurSerie(j)
        If j = 4 Then rng.Cells(j + 1, 1).Font.Color = RGB(11, 11, 11) Else rng.Cells(j + 1, 1).Font.Color = RGB(255, 255, 255)
    Next j
    EchelleDivergente rng.Cells(2, 5).Resize(nCl, kq), -MaxAbsCentres(kq), MaxAbsCentres(kq)
    tables.Add rng
    r = r + nCl + 3

    ' --- 3. Caractérisation par les variables (v-tests)
    nV = nVar + nSup
    ReDim nomsV(1 To nV)
    ReDim vt(1 To nV, 1 To nCl)
    ReDim moyC(1 To nV, 1 To nCl)
    ReDim moyG(1 To nV)
    CalculerVTests nomsV, vt, moyC, moyG

    r = Section(ws, r, "3. Caractérisation des classes par les variables (v-tests)")
    ReDim e(0 To nCl + 1)
    e(1) = "Variable"
    For j = 1 To nCl
        e(j + 1) = "Classe " & j
    Next j
    ReDim t(1 To nV, 1 To nCl + 1)
    For i = 1 To nV
        t(i, 1) = "'" & nomsV(i)
        For j = 1 To nCl
            t(i, j + 1) = vt(i, j)
        Next j
    Next i
    Set rng = Tableau(ws, r, 2, e, t, nV, nCl + 1, "0.00")
    EchelleDivergente rng.Cells(2, 2).Resize(nV, nCl), -MaxAbsMat(vt, nV, nCl), MaxAbsMat(vt, nV, nCl)
    If nSup > 0 Then rng.Cells(nVar + 2, 1).Resize(nSup, 1).Font.Italic = True
    tables.Add rng
    r = r + nV + 2
    r = Ligne(ws, r, "v-test > 1,96 : moyenne de la classe significativement supérieure à la moyenne générale (risque 5 %) ; < -1,96 : inférieure." & _
                     IIf(nSup > 0, " Variables supplémentaires en italique.", ""), False, gris) + 1

    r = Section(ws, r, "Moyennes des variables par classe")
    ReDim e(0 To nCl + 2)
    e(1) = "Variable": e(2) = "Moyenne générale"
    For j = 1 To nCl
        e(j + 2) = "Classe " & j
    Next j
    ReDim t(1 To nV, 1 To nCl + 2)
    For i = 1 To nV
        t(i, 1) = "'" & nomsV(i)
        t(i, 2) = moyG(i)
        For j = 1 To nCl
            t(i, j + 2) = moyC(i, j)
        Next j
    Next i
    Set rng = Tableau(ws, r, 2, e, t, nV, nCl + 2, "0.00")
    tables.Add rng
    r = r + nV + 3

    ' --- 4. Interprétation
    r = Section(ws, r, "4. Portrait de chaque classe")
    For j = 1 To nCl
        r = PortraitClasse(ws, r, j, nomsV, vt, moyC, moyG, d2)
    Next j

    ' --- 5. Croisement avec la variable de groupe
    If nGroupes > 0 Then
        r = Section(ws, r, "5. Classes et groupes (effectifs)")
        ReDim e(0 To nGroupes + 2)
        e(1) = "Classe"
        For k = 1 To nGroupes
            e(k + 1) = nomsGroupes(k)
        Next k
        e(nGroupes + 2) = "Total"
        ReDim t(1 To nCl + 1, 1 To nGroupes + 2)
        For j = 1 To nCl
            t(j, 1) = "Classe " & j
            For k = 2 To nGroupes + 2
                t(j, k) = 0
            Next k
        Next j
        t(nCl + 1, 1) = "Total"
        For k = 2 To nGroupes + 2
            t(nCl + 1, k) = 0
        Next k
        For i = 1 To nInd
            If idxGroupe(i) >= 1 Then
                t(classe(i), idxGroupe(i) + 1) = t(classe(i), idxGroupe(i) + 1) + 1
                t(classe(i), nGroupes + 2) = t(classe(i), nGroupes + 2) + 1
                t(nCl + 1, idxGroupe(i) + 1) = t(nCl + 1, idxGroupe(i) + 1) + 1
                t(nCl + 1, nGroupes + 2) = t(nCl + 1, nGroupes + 2) + 1
            End If
        Next i
        Set rng = Tableau(ws, r, 2, e, t, nCl + 1, nGroupes + 2, "0")
        EchelleSequentielle rng.Cells(2, 2).Resize(nCl, nGroupes)
        rng.Rows(nCl + 2).Font.Italic = True
        tables.Add rng
        r = r + nCl + 4
    End If

    ' --- 6. Individus
    r = Section(ws, r, IIf(nGroupes > 0, "6", "5") & ". Classe de chaque individu")
    nbC = 4 + kq
    ReDim e(0 To nbC)
    e(1) = "Individu": e(2) = "Ligne (Données)": e(3) = "Classe": e(4) = "Distance² au centre"
    For k = 1 To kq
        e(4 + k) = "Coord. Dim." & k
    Next k
    ReDim t(1 To nInd, 1 To nbC)
    For i = 1 To nInd
        t(i, 1) = "'" & etiq(i)
        t(i, 2) = ligneSource(i)
        t(i, 3) = classe(i)
        t(i, 4) = d2(i)
        For k = 1 To kq
            t(i, 4 + k) = matF(i, k)
        Next k
    Next i
    Set rng = Tableau(ws, r, 2, e, t, nInd, nbC, "0.000")
    rng.Cells(2, 2).Resize(nInd, 2).NumberFormat = "0"
    rng.Cells(2, 3).Resize(nInd, 1).HorizontalAlignment = xlCenter
    tables.Add rng
    rng.AutoFilter
    r = r + nInd + 3
    r = Ligne(ws, r, "Astuce : copiez la colonne « Classe » à côté de vos données (même ordre que la colonne « Ligne (Données) ») pour réutiliser la classification.", False, gris)

    AjusterLargeurs ws, tables
    BarreAxes ws, ax, ay
    AllerA ws, "A1"
End Sub

Private Function CumulPct(ByVal q As Long) As Double
    Dim k As Long
    For k = 1 To q
        CumulPct = CumulPct + Pct(k)
    Next k
End Function

Private Function MaxAbsCentres(ByVal kq As Long) As Double
    Dim j As Long, k As Long
    For j = 1 To nCl
        For k = 1 To kq
            If Abs(centre(j, k)) > MaxAbsCentres Then MaxAbsCentres = Abs(centre(j, k))
        Next k
    Next j
    If MaxAbsCentres = 0 Then MaxAbsCentres = 1
End Function

Private Function MaxAbsMat(m() As Double, ByVal n1 As Long, ByVal n2 As Long) As Double
    Dim i As Long, j As Long
    For i = 1 To n1
        For j = 1 To n2
            If Abs(m(i, j)) > MaxAbsMat Then MaxAbsMat = Abs(m(i, j))
        Next j
    Next i
    If MaxAbsMat < 1.96 Then MaxAbsMat = 1.96
End Function

' v-test de la moyenne de chaque variable dans chaque classe
'   v = (moyenne classe - moyenne générale) / racine( s² / nk . (N - nk) / (N - 1) )
Private Sub CalculerVTests(nomsV() As String, vt() As Double, moyC() As Double, moyG() As Double)
    Dim v As Long, i As Long, j As Long, nT As Long, x As Double, present As Boolean
    Dim somme As Double, somme2 As Double, s2 As Double, sc() As Double, nc() As Long, den As Double

    For v = 1 To nVar + nSup
        If v <= nVar Then nomsV(v) = nomsVar(v) Else nomsV(v) = nomsSup(v - nVar) & " (supp.)"
        ReDim sc(1 To nCl)
        ReDim nc(1 To nCl)
        nT = 0: somme = 0: somme2 = 0
        For i = 1 To nInd
            If v <= nVar Then
                x = matX(i, v)
                present = True
            Else
                present = presSup(i, v - nVar)
                If present Then x = matSup(i, v - nVar)
            End If
            If present Then
                nT = nT + 1
                somme = somme + x
                somme2 = somme2 + x * x
                sc(classe(i)) = sc(classe(i)) + x
                nc(classe(i)) = nc(classe(i)) + 1
            End If
        Next i
        If nT > 0 Then
            moyG(v) = somme / nT
            s2 = somme2 / nT - moyG(v) ^ 2
            If s2 < 0 Then s2 = 0
        End If
        For j = 1 To nCl
            If nc(j) > 0 Then
                moyC(v, j) = sc(j) / nc(j)
                If nT > 1 And nc(j) < nT And s2 > 0 Then
                    den = Sqr(s2 / nc(j) * (nT - nc(j)) / (nT - 1))
                    If den > 0 Then vt(v, j) = (moyC(v, j) - moyG(v)) / den
                End If
            End If
        Next j
    Next v
End Sub

' Texte d'interprétation d'une classe
Private Function PortraitClasse(ws As Worksheet, ByVal r As Long, ByVal j As Long, nomsV() As String, vt() As Double, _
                                moyC() As Double, moyG() As Double, d2() As Double) As Long
    Dim nV As Long, v As Long, i As Long, c As Long, ordre() As Long, vals() As Double
    Dim hautes As String, basses As String, para As String, dist() As Double, nb As Long
    Dim g As Long, maxG As Long, nG() As Long, totG() As Long

    nV = nVar + nSup
    r = Ligne(ws, r, "Classe " & j & " : " & effCl(j) & " individu(s), " & NombreTexte(100# * effCl(j) / nInd, "0.0") & " %", True, CouleurSerie(j))

    ReDim vals(1 To nV)
    For v = 1 To nV
        vals(v) = vt(v, j)
    Next v
    ordre = OrdreDecroissant(vals, nV)
    c = 0
    For v = 1 To nV
        If vals(ordre(v)) >= 1.96 And c < 5 Then
            c = c + 1
            hautes = hautes & IIf(c > 1, ", ", "") & nomsV(ordre(v)) & " (" & NombreTexte(moyC(ordre(v), j)) & _
                     " contre " & NombreTexte(moyG(ordre(v))) & ")"
        End If
    Next v
    c = 0
    For v = nV To 1 Step -1
        If vals(ordre(v)) <= -1.96 And c < 5 Then
            c = c + 1
            basses = basses & IIf(c > 1, ", ", "") & nomsV(ordre(v)) & " (" & NombreTexte(moyC(ordre(v), j)) & _
                     " contre " & NombreTexte(moyG(ordre(v))) & ")"
        End If
    Next v
    If Len(hautes) > 0 Then r = Ligne(ws, r, "Valeurs élevées : " & hautes, False, -1, 1)
    If Len(basses) > 0 Then r = Ligne(ws, r, "Valeurs faibles : " & basses, False, -1, 1)
    If Len(hautes) + Len(basses) = 0 Then
        r = Ligne(ws, r, "Aucune variable ne distingue significativement cette classe de l'ensemble (classe « moyenne »).", False, -1, 1)
    End If

    ' Parangons : individus les plus proches du centre
    ReDim dist(1 To nInd)
    For i = 1 To nInd
        If classe(i) = j Then dist(i) = -d2(i) Else dist(i) = -1E+300
    Next i
    ordre = OrdreDecroissantRapide(dist, nInd)
    nb = effCl(j)
    If nb > 3 Then nb = 3
    para = ""
    For i = 1 To nb
        para = para & IIf(i > 1, ", ", "") & etiq(ordre(i))
    Next i
    r = Ligne(ws, r, "Individus les plus typiques (proches du centre) : " & para, False, RGB(90, 90, 90), 1)

    ' Groupe majoritaire
    If nGroupes > 0 Then
        ReDim nG(1 To nGroupes)
        ReDim totG(1 To nGroupes)
        For i = 1 To nInd
            If idxGroupe(i) >= 1 Then
                totG(idxGroupe(i)) = totG(idxGroupe(i)) + 1
                If classe(i) = j Then nG(idxGroupe(i)) = nG(idxGroupe(i)) + 1
            End If
        Next i
        maxG = 1
        For g = 2 To nGroupes
            If nG(g) > nG(maxG) Then maxG = g
        Next g
        If nG(maxG) > 0 And effCl(j) > 0 And totG(maxG) > 0 Then
            r = Ligne(ws, r, "Groupe majoritaire : " & nomsGroupes(maxG) & " (" & NombreTexte(100# * nG(maxG) / effCl(j), "0") & _
                             " % de la classe ; " & NombreTexte(100# * nG(maxG) / totG(maxG), "0") & " % de ce groupe)", False, RGB(90, 90, 90), 1)
        End If
    End If
    PortraitClasse = r + 1
End Function

' Dendrogramme : sommet de l'arbre (NB_FEUILLES feuilles au plus)
Private Sub DessinerDendrogramme(ws As Worksheet, ByVal g As Double, ByVal h As Double, ByVal l As Double, ByVal ht As Double)
    Dim nF As Long, i As Long, r As Long, m As Long, ra As Long, rb As Long, nNoeuds As Long
    Dim parent() As Long, noeudDe() As Long, gauche() As Long, droite() As Long
    Dim xN() As Double, yN() As Double, taille() As Double, clF() As Long
    Dim etiqF() As String, coulF() As Long, pile() As Long, sp As Long, pos As Long, coupe As Double

    nF = NB_FEUILLES
    If nF > nU Then nF = nU
    If nF < 2 Then Exit Sub
    nNoeuds = 2 * nF - 1
    ReDim gauche(1 To nNoeuds): ReDim droite(1 To nNoeuds)
    ReDim xN(1 To nNoeuds): ReDim yN(1 To nNoeuds)
    ReDim taille(1 To nF): ReDim clF(1 To nF)

    ' Feuilles : coupure de l'arbre en nF groupes
    ReDim parent(1 To nU)
    For i = 1 To nU
        parent(i) = i
    Next i
    For r = 1 To nU - nF
        ra = Trouver(parent, fusA(ordreFus(r)))
        rb = Trouver(parent, fusB(ordreFus(r)))
        If ra <> rb Then parent(rb) = ra
    Next r
    ReDim noeudDe(1 To nU)
    m = 0
    For i = 1 To nU
        ra = Trouver(parent, i)
        If noeudDe(ra) = 0 Then
            m = m + 1
            noeudDe(ra) = m
            clF(m) = classeU(i)
        End If
        taille(noeudDe(ra)) = taille(noeudDe(ra)) + poidsU(i)
    Next i

    ' Fusions restantes
    For r = nU - nF + 1 To nU - 1
        ra = Trouver(parent, fusA(ordreFus(r)))
        rb = Trouver(parent, fusB(ordreFus(r)))
        If ra <> rb Then
            m = m + 1
            gauche(m) = noeudDe(ra)
            droite(m) = noeudDe(rb)
            yN(m) = fusH(ordreFus(r)) / nInd
            parent(rb) = ra
            noeudDe(ra) = m
        End If
    Next r
    If m <> nNoeuds Then Exit Sub

    ' Ordre des feuilles : parcours en profondeur depuis la racine
    ReDim pile(1 To nNoeuds)
    sp = 1
    pile(1) = nNoeuds
    pos = 0
    Do While sp > 0
        m = pile(sp)
        sp = sp - 1
        If m <= nF Then
            pos = pos + 1
            xN(m) = pos
        Else
            sp = sp + 1: pile(sp) = droite(m)
            sp = sp + 1: pile(sp) = gauche(m)
        End If
    Loop
    For m = nF + 1 To nNoeuds
        xN(m) = (xN(gauche(m)) + xN(droite(m))) / 2
    Next m

    ReDim etiqF(1 To nF)
    ReDim coulF(1 To nF)
    For i = 1 To nF
        etiqF(i) = "n=" & Format$(taille(i), "0")
        If numCl(clF(i)) > 0 Then coulF(i) = CouleurSerie(numCl(clF(i))) Else coulF(i) = RGB(140, 140, 135)
    Next i

    If nCl >= 2 And nCl <= nHaut Then
        coupe = (hauteurs(nCl - 1) + hauteurs(nCl)) / 2
    ElseIf nCl >= 2 Then
        coupe = hauteurs(nCl - 1) / 2
    End If
    GraphDendrogramme ws, g, h, l, ht, nF, xN, yN, gauche, droite, etiqF, coulF, coupe
End Sub
