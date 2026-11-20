Attribute VB_Name = "modGraphiques"
' =============================================================================
'  ACP Excel - Module : graphiques
'  Les données des graphiques sont écrites dans la feuille masquée ACP_Graph.
' =============================================================================
Option Explicit

Private wsGraph As Worksheet
Private colLibre As Long

Private Const PI_ As Double = 3.14159265358979

' Palette catégorielle (ordre fixe, 8 teintes validées pour daltonisme)
Public Function CouleurSerie(ByVal i As Long) As Long
    Select Case ((i - 1) Mod 8) + 1
        Case 1: CouleurSerie = RGB(42, 120, 214)    ' bleu
        Case 2: CouleurSerie = RGB(235, 104, 52)    ' orange
        Case 3: CouleurSerie = RGB(27, 175, 122)    ' turquoise
        Case 4: CouleurSerie = RGB(237, 161, 0)     ' jaune
        Case 5: CouleurSerie = RGB(232, 123, 164)   ' magenta
        Case 6: CouleurSerie = RGB(0, 131, 0)       ' vert
        Case 7: CouleurSerie = RGB(98, 80, 214)     ' violet
        Case 8: CouleurSerie = RGB(227, 73, 72)     ' rouge
    End Select
End Function

Private Function EncreSecondaire() As Long
    EncreSecondaire = RGB(90, 90, 85)
End Function

Private Function CouleurGrille() As Long
    CouleurGrille = RGB(225, 224, 217)
End Function

Public Sub InitGraphiques()
    Set wsGraph = ObtenirFeuille(F_GRAPH, True)
    wsGraph.Cells.Clear
    wsGraph.Visible = xlSheetHidden
    colLibre = 1
End Sub

' Réserve un bloc de cellules dans la feuille masquée
Private Function Zone(ByVal nL As Long, ByVal nC As Long) As Range
    If nL < 1 Then nL = 1
    Set Zone = wsGraph.Cells(1, colLibre).Resize(nL, nC)
    colLibre = colLibre + nC + 1
End Function

Private Function NouveauGraphique(ws As Worksheet, ByVal gauche As Double, ByVal haut As Double, _
                                  ByVal largeur As Double, ByVal hauteur As Double, ByVal nom As String, _
                                  ByVal typ As XlChartType) As Chart
    Dim co As ChartObject
    Set co = ws.ChartObjects.Add(gauche, haut, largeur, hauteur)
    co.Name = nom
    Do While co.Chart.SeriesCollection.Count > 0
        co.Chart.SeriesCollection(1).Delete
    Loop
    On Error Resume Next
    co.Chart.ChartType = typ
    On Error GoTo 0
    With co.Chart
        .ChartArea.Format.Line.Visible = msoFalse
        .ChartArea.Font.Name = "Calibri"
        .ChartArea.Font.Size = 9
        .ChartArea.Font.Color = EncreSecondaire()
    End With
    Set NouveauGraphique = co.Chart
End Function

Private Sub Titre(ch As Chart, ByVal texte As String)
    ch.HasTitle = True
    ch.ChartTitle.Text = texte
    ch.ChartTitle.Font.Size = 12
    ch.ChartTitle.Font.Bold = True
    ch.ChartTitle.Font.Color = RGB(30, 30, 30)
End Sub

Private Sub TitreAxe(ax As Axis, ByVal texte As String)
    ax.HasTitle = True
    ax.AxisTitle.Text = texte
    ax.AxisTitle.Font.Size = 9
    ax.AxisTitle.Font.Bold = False
    ax.AxisTitle.Font.Color = EncreSecondaire()
End Sub

' Axes croisés à l'origine, étiquettes en bord de graphique
Private Sub AxesFactoriels(ch As Chart, ByVal titreX As String, ByVal titreY As String)
    Dim ax As Axis, t As Variant
    For Each t In Array(xlCategory, xlValue)
        Set ax = ch.Axes(t)
        ax.CrossesAt = 0
        ax.TickLabelPosition = xlTickLabelPositionLow
        ax.MajorTickMark = xlTickMarkNone
        ax.Format.Line.ForeColor.RGB = RGB(140, 140, 135)
        ax.Format.Line.Weight = 0.75
        ax.HasMajorGridlines = True
        ax.MajorGridlines.Format.Line.ForeColor.RGB = CouleurGrille()
        ax.MajorGridlines.Format.Line.Weight = 0.5
    Next t
    TitreAxe ch.Axes(xlCategory), titreX
    TitreAxe ch.Axes(xlValue), titreY
End Sub

Private Sub ZoneTraceCarree(ch As Chart)
    Dim d As Double
    On Error Resume Next
    With ch.PlotArea
        d = .InsideWidth
        If .InsideHeight < d Then d = .InsideHeight
        .InsideWidth = d
        .InsideHeight = d
        .InsideLeft = (ch.ChartArea.Width - d) / 2
    End With
    On Error GoTo 0
End Sub

Private Function SerieXY(ch As Chart, xR As Range, yR As Range, ByVal nom As String, ByVal typ As XlChartType) As Series
    Dim s As Series
    Set s = ch.SeriesCollection.NewSeries
    s.ChartType = typ
    s.Name = "=""" & Replace(Left$(nom, 200), """", "'") & """"
    s.XValues = xR
    s.Values = yR
    Set SerieXY = s
End Function

Private Sub Marqueurs(s As Series, ByVal couleur As Long, ByVal taille As Long, ByVal style As XlMarkerStyle)
    s.MarkerStyle = style
    s.MarkerSize = taille
    s.MarkerBackgroundColor = couleur
    s.MarkerForegroundColor = RGB(255, 255, 255)
    s.Format.Line.Visible = msoFalse
End Sub

Private Function TexteCel(ByVal s As String) As String
    TexteCel = "'" & s
End Function

' =============================================================================
'  Éboulis des valeurs propres (% d'inertie par axe)
' =============================================================================
Public Sub GraphEboulis(ws As Worksheet, ByVal g As Double, ByVal h As Double, ByVal l As Double, ByVal ht As Double, _
                        pct() As Double, ByVal nAx As Long, ByVal nbRetenus As Long)
    Dim ch As Chart, z As Range, i As Long, s As Series, t() As Variant
    If nAx > 20 Then nAx = 20
    Set z = Zone(nAx, 2)
    ReDim t(1 To nAx, 1 To 2)
    For i = 1 To nAx
        t(i, 1) = TexteCel("Dim." & i)
        t(i, 2) = pct(i)
    Next i
    z.Value = t

    Set ch = NouveauGraphique(ws, g, h, l, ht, "Eboulis", xlColumnClustered)
    Set s = ch.SeriesCollection.NewSeries
    s.Name = "=""% d'inertie"""
    s.XValues = z.Columns(1)
    s.Values = z.Columns(2)
    s.ChartType = xlColumnClustered
    s.Format.Fill.ForeColor.RGB = RGB(134, 182, 239)
    s.Format.Line.Visible = msoFalse
    For i = 1 To nAx
        If i <= nbRetenus Then s.Points(i).Format.Fill.ForeColor.RGB = CouleurSerie(1)
    Next i
    s.HasDataLabels = True
    s.DataLabels.NumberFormat = "0.0"
    s.DataLabels.Font.Size = 8
    s.DataLabels.Font.Color = EncreSecondaire()
    ch.ChartGroups(1).GapWidth = 40
    ch.HasLegend = False
    Titre ch, "Éboulis des valeurs propres (% d'inertie expliquée)"
    TitreAxe ch.Axes(xlValue), "% de l'inertie totale"
    ch.Axes(xlValue).MinimumScale = 0
    ch.Axes(xlValue).HasMajorGridlines = True
    ch.Axes(xlValue).MajorGridlines.Format.Line.ForeColor.RGB = CouleurGrille()
    ch.Axes(xlValue).Format.Line.Visible = msoFalse
End Sub

' =============================================================================
'  Cercle des corrélations
' =============================================================================
Public Sub GraphCercle(ws As Worksheet, ByVal g As Double, ByVal h As Double, ByVal taille As Double, _
                       noms() As String, cx() As Double, cy() As Double, qual() As Double, ByVal nV As Long, _
                       nomsS() As String, sx() As Double, sy() As Double, ByVal nS As Long, _
                       ByVal titreX As String, ByVal titreY As String)
    Dim ch As Chart, z As Range, i As Long, s As Series, t() As Variant, coul As Long, nMax As Long

    Set ch = NouveauGraphique(ws, g, h, taille, taille, "Cercle", xlXYScatterLinesNoMarkers)

    ' Cercle unité
    Set z = Zone(101, 2)
    ReDim t(1 To 101, 1 To 2)
    For i = 0 To 100
        t(i + 1, 1) = Cos(2 * PI_ * i / 100)
        t(i + 1, 2) = Sin(2 * PI_ * i / 100)
    Next i
    z.Value = t
    Set s = SerieXY(ch, z.Columns(1), z.Columns(2), "Cercle", xlXYScatterLinesNoMarkers)
    s.Format.Line.ForeColor.RGB = RGB(160, 160, 155)
    s.Format.Line.Weight = 1

    ' Flèches des variables actives (250 séries max. par graphique)
    nMax = nV
    If nMax > 220 Then nMax = 220
    Set z = Zone(2 * nMax, 2)
    ReDim t(1 To 2 * nMax, 1 To 2)
    For i = 1 To nMax
        t(2 * i - 1, 1) = 0: t(2 * i - 1, 2) = 0
        t(2 * i, 1) = cx(i): t(2 * i, 2) = cy(i)
    Next i
    z.Value = t
    For i = 1 To nMax
        If qual(i) >= 0.5 Then coul = RGB(28, 92, 171) Else coul = RGB(134, 182, 239)
        Set s = SerieXY(ch, z.Cells(2 * i - 1, 1).Resize(2, 1), z.Cells(2 * i - 1, 2).Resize(2, 1), noms(i), xlXYScatterLinesNoMarkers)
        With s.Format.Line
            .ForeColor.RGB = coul
            .Weight = 1.75
            .EndArrowheadStyle = msoArrowheadTriangle
        End With
        s.Points(2).HasDataLabel = True
        With s.Points(2).DataLabel
            .Text = noms(i)
            If cx(i) >= 0 Then .Position = xlLabelPositionRight Else .Position = xlLabelPositionLeft
            .Font.Size = 9
            .Font.Color = RGB(30, 30, 30)
        End With
    Next i

    ' Variables supplémentaires (pointillés)
    If nS > 0 Then
        If nS > 30 Then nS = 30
        Set z = Zone(2 * nS, 2)
        ReDim t(1 To 2 * nS, 1 To 2)
        For i = 1 To nS
            t(2 * i - 1, 1) = 0: t(2 * i - 1, 2) = 0
            t(2 * i, 1) = sx(i): t(2 * i, 2) = sy(i)
        Next i
        z.Value = t
        For i = 1 To nS
            Set s = SerieXY(ch, z.Cells(2 * i - 1, 1).Resize(2, 1), z.Cells(2 * i - 1, 2).Resize(2, 1), nomsS(i) & " (supp.)", xlXYScatterLinesNoMarkers)
            With s.Format.Line
                .ForeColor.RGB = CouleurSerie(2)
                .Weight = 1.5
                .DashStyle = msoLineDash
                .EndArrowheadStyle = msoArrowheadTriangle
            End With
            s.Points(2).HasDataLabel = True
            With s.Points(2).DataLabel
                .Text = nomsS(i)
                If sx(i) >= 0 Then .Position = xlLabelPositionRight Else .Position = xlLabelPositionLeft
                .Font.Size = 9
                .Font.Italic = True
                .Font.Color = RGB(90, 90, 85)
            End With
        Next i
    End If

    ch.HasLegend = False
    Titre ch, "Cercle des corrélations"
    With ch.Axes(xlCategory)
        .MinimumScale = -1.15
        .MaximumScale = 1.15
        .MajorUnit = 0.5
        .TickLabels.NumberFormat = "0.0"
    End With
    With ch.Axes(xlValue)
        .MinimumScale = -1.15
        .MaximumScale = 1.15
        .MajorUnit = 0.5
        .TickLabels.NumberFormat = "0.0"
    End With
    AxesFactoriels ch, titreX, titreY
    ZoneTraceCarree ch
End Sub

' =============================================================================
'  Carte des individus (couleur par groupe, barycentres des groupes)
' =============================================================================
Public Sub GraphIndividus(ws As Worksheet, ByVal g As Double, ByVal h As Double, ByVal l As Double, ByVal ht As Double, _
                          ByVal nInd As Long, fx() As Double, fy() As Double, lbl() As String, montrer() As Boolean, _
                          idxG() As Long, nomsG() As String, ByVal nG As Long, bx() As Double, by() As Double, _
                          ByVal titreX As String, ByVal titreY As String)
    Dim ch As Chart, z As Range, i As Long, k As Long, gr As Long, s As Series, t() As Variant
    Dim debut() As Long, effectif() As Long, pos() As Long, ordre() As Long, nbGr As Long
    Dim ligne As Long, nbEtiq As Long, mx As Double, my As Double, coul As Long

    Set ch = NouveauGraphique(ws, g, h, l, ht, "Individus", xlXYScatter)

    ' Regroupe les individus par groupe (une série par groupe)
    nbGr = nG
    If nbGr < 1 Then nbGr = 1
    ReDim effectif(1 To nbGr)
    ReDim debut(1 To nbGr)
    ReDim pos(1 To nbGr)
    For i = 1 To nInd
        If nG > 0 Then gr = idxG(i) Else gr = 1
        effectif(gr) = effectif(gr) + 1
    Next i
    debut(1) = 1
    For gr = 2 To nbGr
        debut(gr) = debut(gr - 1) + effectif(gr - 1)
    Next gr
    ReDim ordre(1 To nInd)
    For gr = 1 To nbGr
        pos(gr) = debut(gr)
    Next gr
    For i = 1 To nInd
        If nG > 0 Then gr = idxG(i) Else gr = 1
        ordre(pos(gr)) = i
        pos(gr) = pos(gr) + 1
    Next i

    Set z = Zone(nInd, 2)
    ReDim t(1 To nInd, 1 To 2)
    For k = 1 To nInd
        t(k, 1) = fx(ordre(k))
        t(k, 2) = fy(ordre(k))
        If Abs(fx(ordre(k))) > mx Then mx = Abs(fx(ordre(k)))
        If Abs(fy(ordre(k))) > my Then my = Abs(fy(ordre(k)))
    Next k
    z.Value = t

    For gr = 1 To nbGr
        If effectif(gr) > 0 Then
            If nG > 0 Then
                Set s = SerieXY(ch, z.Cells(debut(gr), 1).Resize(effectif(gr), 1), z.Cells(debut(gr), 2).Resize(effectif(gr), 1), nomsG(gr), xlXYScatter)
                coul = CouleurSerie(gr)
                If gr = 8 And nomsG(gr) = "Autres" Then coul = RGB(140, 140, 135)
            Else
                Set s = SerieXY(ch, z.Cells(1, 1).Resize(nInd, 1), z.Cells(1, 2).Resize(nInd, 1), "Individus", xlXYScatter)
                coul = CouleurSerie(1)
            End If
            If nInd > 2000 Then
                Marqueurs s, coul, 4, xlMarkerStyleCircle
            Else
                Marqueurs s, coul, 7, xlMarkerStyleCircle
            End If
            ' Étiquettes des individus choisis
            For k = 1 To effectif(gr)
                i = ordre(debut(gr) + k - 1)
                If montrer(i) Then
                    s.Points(k).HasDataLabel = True
                    With s.Points(k).DataLabel
                        .Text = lbl(i)
                        .Position = xlLabelPositionAbove
                        .Font.Size = 8
                        .Font.Color = RGB(60, 60, 58)
                    End With
                    nbEtiq = nbEtiq + 1
                End If
            Next k
        End If
    Next gr

    ' Barycentres des groupes
    If nG > 0 Then
        Set z = Zone(nG, 2)
        ReDim t(1 To nG, 1 To 2)
        For gr = 1 To nG
            t(gr, 1) = bx(gr)
            t(gr, 2) = by(gr)
        Next gr
        z.Value = t
        Set s = SerieXY(ch, z.Columns(1), z.Columns(2), "Barycentres des groupes", xlXYScatter)
        Marqueurs s, RGB(30, 30, 30), 10, xlMarkerStyleDiamond
        For gr = 1 To nG
            If effectif(gr) > 0 Then
                s.Points(gr).HasDataLabel = True
                With s.Points(gr).DataLabel
                    .Text = nomsG(gr)
                    .Position = xlLabelPositionBelow
                    .Font.Size = 9
                    .Font.Bold = True
                    .Font.Color = RGB(30, 30, 30)
                End With
            End If
        Next gr
        ch.HasLegend = True
        ch.Legend.Position = xlLegendPositionBottom
        ch.Legend.Font.Size = 9
    Else
        ch.HasLegend = False
    End If

    Titre ch, "Carte des individus"
    ' Même échelle sur les deux axes, centrée sur l'origine
    If mx <= 0 Then mx = 1
    If my <= 0 Then my = 1
    mx = BorneRonde(mx * 1.12)
    my = BorneRonde(my * 1.12)
    With ch.Axes(xlCategory)
        .MinimumScale = -mx
        .MaximumScale = mx
    End With
    With ch.Axes(xlValue)
        .MinimumScale = -my
        .MaximumScale = my
    End With
    AxesFactoriels ch, titreX, titreY
End Sub

Private Function BorneRonde(ByVal x As Double) As Double
    Dim e As Double, m As Double
    e = 10 ^ Int(Log(x) / Log(10#))
    m = x / e
    If m <= 1 Then
        m = 1
    ElseIf m <= 1.5 Then
        m = 1.5
    ElseIf m <= 2 Then
        m = 2
    ElseIf m <= 2.5 Then
        m = 2.5
    ElseIf m <= 3 Then
        m = 3
    ElseIf m <= 4 Then
        m = 4
    ElseIf m <= 5 Then
        m = 5
    ElseIf m <= 6 Then
        m = 6
    ElseIf m <= 8 Then
        m = 8
    Else
        m = 10
    End If
    BorneRonde = m * e
End Function

' =============================================================================
'  Diagramme en barres trié (contributions) avec seuil de contribution moyenne
' =============================================================================
Public Sub GraphContributions(ws As Worksheet, ByVal g As Double, ByVal h As Double, ByVal l As Double, ByVal ht As Double, _
                              noms() As String, vals() As Double, ByVal n As Long, ByVal nMax As Long, _
                              ByVal seuil As Double, ByVal titreGraph As String, ByVal nomGraph As String)
    Dim ch As Chart, z As Range, i As Long, s As Series, t() As Variant, ordre() As Long, m As Long

    If n > 2000 Then
        ordre = OrdreDecroissantRapide(vals, n)
    Else
        ordre = OrdreDecroissant(vals, n)
    End If
    m = n
    If m > nMax Then m = nMax
    Set z = Zone(m, 3)
    ReDim t(1 To m, 1 To 3)
    For i = 1 To m
        t(i, 1) = TexteCel(noms(ordre(i)))
        t(i, 2) = vals(ordre(i))
        t(i, 3) = seuil
    Next i
    z.Value = t

    Set ch = NouveauGraphique(ws, g, h, l, ht, nomGraph, xlColumnClustered)
    Set s = ch.SeriesCollection.NewSeries
    s.ChartType = xlColumnClustered
    s.Name = "=""Contribution (%)"""
    s.XValues = z.Columns(1)
    s.Values = z.Columns(2)
    s.Format.Fill.ForeColor.RGB = CouleurSerie(1)
    s.Format.Line.Visible = msoFalse
    For i = 1 To m
        If t(i, 2) < seuil Then s.Points(i).Format.Fill.ForeColor.RGB = RGB(183, 211, 246)
    Next i
    ch.ChartGroups(1).GapWidth = 50

    Set s = ch.SeriesCollection.NewSeries
    s.ChartType = xlLine
    s.Name = "=""Contribution moyenne (" & Format$(seuil, "0.0") & " %)"""
    s.XValues = z.Columns(1)
    s.Values = z.Columns(3)
    s.MarkerStyle = xlMarkerStyleNone
    With s.Format.Line
        .ForeColor.RGB = CouleurSerie(8)
        .Weight = 1.5
        .DashStyle = msoLineDash
    End With

    Titre ch, titreGraph
    ch.HasLegend = True
    ch.Legend.Position = xlLegendPositionBottom
    ch.Legend.Font.Size = 8
    ch.Axes(xlValue).MinimumScale = 0
    ch.Axes(xlValue).TickLabels.NumberFormat = "0"
    ch.Axes(xlValue).HasMajorGridlines = True
    ch.Axes(xlValue).MajorGridlines.Format.Line.ForeColor.RGB = CouleurGrille()
    ch.Axes(xlValue).Format.Line.Visible = msoFalse
    TitreAxe ch.Axes(xlValue), "%"
    ch.Axes(xlCategory).TickLabels.Font.Size = 8
End Sub
