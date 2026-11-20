Attribute VB_Name = "modMatrice"
' =============================================================================
'  ACP Excel - Module : calcul matriciel
'  Diagonalisation d'une matrice symétrique par la méthode de Jacobi cyclique.
' =============================================================================
Option Explicit

' A        : matrice symétrique (1..n, 1..n), non modifiée
' valeurs  : valeurs propres triées par ordre décroissant (1..n)
' vecteurs : vecteurs propres normés en colonnes (1..n, 1..n)
Public Sub JacobiSym(A() As Double, ByVal n As Long, valeurs() As Double, vecteurs() As Double)
    Dim m() As Double, v() As Double
    Dim i As Long, j As Long, k As Long, p As Long, q As Long, balayage As Long
    Dim horsDiag As Double, normeTot As Double, theta As Double, t As Double, c As Double, s As Double
    Dim mkp As Double, mkq As Double, tmp As Double

    ReDim m(1 To n, 1 To n)
    ReDim v(1 To n, 1 To n)
    For i = 1 To n
        For j = 1 To n
            m(i, j) = A(i, j)
            normeTot = normeTot + A(i, j) * A(i, j)
        Next j
        v(i, i) = 1#
    Next i

    For balayage = 1 To 100
        horsDiag = 0#
        For p = 1 To n - 1
            For q = p + 1 To n
                horsDiag = horsDiag + m(p, q) * m(p, q)
            Next q
        Next p
        If horsDiag <= 1E-30 * normeTot Or horsDiag = 0# Then Exit For

        For p = 1 To n - 1
            For q = p + 1 To n
                If m(p, q) <> 0# Then
                    theta = (m(q, q) - m(p, p)) / (2# * m(p, q))
                    If Abs(theta) > 1E+150 Then
                        t = 1# / (2# * theta)
                    ElseIf theta = 0# Then
                        t = 1#
                    Else
                        t = Sgn(theta) / (Abs(theta) + Sqr(theta * theta + 1#))
                    End If
                    c = 1# / Sqr(t * t + 1#)
                    s = t * c
                    For k = 1 To n
                        mkp = m(k, p): mkq = m(k, q)
                        m(k, p) = c * mkp - s * mkq
                        m(k, q) = s * mkp + c * mkq
                    Next k
                    For k = 1 To n
                        mkp = m(p, k): mkq = m(q, k)
                        m(p, k) = c * mkp - s * mkq
                        m(q, k) = s * mkp + c * mkq
                    Next k
                    m(p, q) = 0#
                    m(q, p) = 0#
                    For k = 1 To n
                        mkp = v(k, p): mkq = v(k, q)
                        v(k, p) = c * mkp - s * mkq
                        v(k, q) = s * mkp + c * mkq
                    Next k
                End If
            Next q
        Next p
    Next balayage

    ReDim valeurs(1 To n)
    For i = 1 To n
        valeurs(i) = m(i, i)
    Next i

    ' Tri décroissant (tri par sélection, échange des colonnes de vecteurs)
    For i = 1 To n - 1
        k = i
        For j = i + 1 To n
            If valeurs(j) > valeurs(k) Then k = j
        Next j
        If k <> i Then
            tmp = valeurs(i): valeurs(i) = valeurs(k): valeurs(k) = tmp
            For j = 1 To n
                tmp = v(j, i): v(j, i) = v(j, k): v(j, k) = tmp
            Next j
        End If
    Next i
    vecteurs = v
End Sub

' Corrélation de Pearson entre deux séries (masque : True = valeur présente)
Public Function CorrelationMasquee(x() As Double, y() As Double, present() As Boolean, ByVal n As Long) As Double
    Dim i As Long, k As Long, mx As Double, my As Double, sxx As Double, syy As Double, sxy As Double
    For i = 1 To n
        If present(i) Then
            k = k + 1
            mx = mx + x(i)
            my = my + y(i)
        End If
    Next i
    If k < 3 Then
        CorrelationMasquee = 0
        Exit Function
    End If
    mx = mx / k
    my = my / k
    For i = 1 To n
        If present(i) Then
            sxx = sxx + (x(i) - mx) ^ 2
            syy = syy + (y(i) - my) ^ 2
            sxy = sxy + (x(i) - mx) * (y(i) - my)
        End If
    Next i
    If sxx <= 0 Or syy <= 0 Then
        CorrelationMasquee = 0
    Else
        CorrelationMasquee = sxy / Sqr(sxx * syy)
    End If
End Function

' Indices triés par valeur décroissante (tri par insertion, suffisant ici)
Public Function OrdreDecroissant(vals() As Double, ByVal n As Long) As Long()
    Dim idx() As Long, i As Long, j As Long, cle As Long
    ReDim idx(1 To n)
    For i = 1 To n
        idx(i) = i
    Next i
    For i = 2 To n
        cle = idx(i)
        j = i - 1
        Do While j >= 1
            If vals(idx(j)) >= vals(cle) Then Exit Do
            idx(j + 1) = idx(j)
            j = j - 1
        Loop
        idx(j + 1) = cle
    Next i
    OrdreDecroissant = idx
End Function

' Tri par tas (pour un grand nombre d'individus) : indices par valeur décroissante
Public Function OrdreDecroissantRapide(vals() As Double, ByVal n As Long) As Long()
    Dim idx() As Long, i As Long, fin As Long, tmp As Long
    ReDim idx(1 To n)
    For i = 1 To n
        idx(i) = i
    Next i
    ' tas « min » sur les valeurs -> extraction donne l'ordre décroissant
    For i = n \ 2 To 1 Step -1
        Tamiser idx, vals, i, n
    Next i
    For fin = n To 2 Step -1
        tmp = idx(1): idx(1) = idx(fin): idx(fin) = tmp
        Tamiser idx, vals, 1, fin - 1
    Next fin
    OrdreDecroissantRapide = idx
End Function

Private Sub Tamiser(idx() As Long, vals() As Double, ByVal debut As Long, ByVal fin As Long)
    Dim racine As Long, enfant As Long, tmp As Long
    racine = debut
    Do While 2 * racine <= fin
        enfant = 2 * racine
        If enfant + 1 <= fin Then
            If vals(idx(enfant + 1)) < vals(idx(enfant)) Then enfant = enfant + 1
        End If
        If vals(idx(enfant)) < vals(idx(racine)) Then
            tmp = idx(racine): idx(racine) = idx(enfant): idx(enfant) = tmp
            racine = enfant
        Else
            Exit Do
        End If
    Loop
End Sub
