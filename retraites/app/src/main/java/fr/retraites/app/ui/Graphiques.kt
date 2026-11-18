package fr.retraites.app.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import fr.retraites.modele.AGE_MAX
import fr.retraites.modele.Pyramide
import kotlin.math.abs
import kotlin.math.ceil
import kotlin.math.floor
import kotlin.math.log10
import kotlin.math.pow
import kotlin.math.roundToInt

/** Une série tracée : une valeur par année. */
data class Serie(
    val nom: String,
    val couleur: Color,
    val annees: List<Int>,
    val valeurs: List<Double>,
    val pointillee: Boolean = false,
)

/** Points de référence (ex. valeurs publiées par le COR), affichés en losanges. */
data class Reperes(val nom: String, val points: List<Pair<Int, Double>>)

private fun pasRond(etendue: Double, nbPas: Int): Double {
    val brut = etendue / nbPas
    val p = 10.0.pow(floor(log10(brut)))
    val m = brut / p
    return p * when {
        m <= 1.0 -> 1.0
        m <= 2.0 -> 2.0
        m <= 2.5 -> 2.5
        m <= 5.0 -> 5.0
        else -> 10.0
    }
}

@Composable
fun Legende(elements: List<Pair<String, Color>>, reperes: String? = null) {
    @OptIn(ExperimentalLayoutApi::class)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        elements.forEach { (nom, couleur) ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(width = 14.dp, height = 3.dp).background(couleur))
                Spacer(Modifier.width(4.dp))
                Text(nom, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        if (reperes != null) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(8.dp).clip(CircleShape).border(2.dp, MaterialTheme.colorScheme.onSurface, CircleShape))
                Spacer(Modifier.width(4.dp))
                Text(reperes, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

/**
 * Graphique en courbes avec axe des années, grille discrète, ligne de zéro facultative et
 * lecture des valeurs au toucher (glisser le doigt le long du graphique).
 */
@Composable
fun GraphiqueCourbes(
    series: List<Serie>,
    format: (Double) -> String,
    modifier: Modifier = Modifier,
    reperes: Reperes? = null,
    ligneZero: Boolean = false,
    hauteur: Int = 200,
) {
    if (series.isEmpty() || series.first().annees.isEmpty()) return
    val mesureur = rememberTextMeasurer()
    val couleurTexte = MaterialTheme.colorScheme.onSurfaceVariant
    val couleurGrille = MaterialTheme.colorScheme.outlineVariant
    val couleurRepere = MaterialTheme.colorScheme.onSurface
    val fond = MaterialTheme.colorScheme.surface
    val annees = series.first().annees
    var selection by remember(annees.first(), annees.last()) { mutableStateOf<Int?>(null) }

    val toutes = series.flatMap { it.valeurs } + (reperes?.points?.map { it.second } ?: emptyList()) +
        (if (ligneZero) listOf(0.0) else emptyList())
    var mini = toutes.min()
    var maxi = toutes.max()
    if (abs(maxi - mini) < 1e-9) { mini -= 1; maxi += 1 }
    val pas = pasRond(maxi - mini, 4)
    val bas = floor(mini / pas) * pas
    val haut = ceil(maxi / pas) * pas

    Column(modifier) {
        Legende(series.map { it.nom to it.couleur }, reperes?.nom)
        Spacer(Modifier.height(4.dp))
        // Lecture de l'année sélectionnée (ou de la dernière année).
        val annee = selection ?: annees.last()
        val i = annees.indexOf(annee).coerceAtLeast(0)
        Text(
            buildString {
                append(annee).append(" : ")
                append(series.joinToString("  ·  ") { "${it.nom} ${format(it.valeurs[i])}" })
                reperes?.points?.firstOrNull { it.first == annee }?.let { append("  ·  ${reperes.nom} ${format(it.second)}") }
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Spacer(Modifier.height(4.dp))
        Canvas(
            Modifier
                .fillMaxWidth()
                .height(hauteur.dp)
                .pointerInput(annees) {
                    detectTapGestures { o -> selection = anneeA(o.x, 44.dp.toPx(), size.width - 52.dp.toPx(), annees) }
                }
                .pointerInput(annees) {
                    detectDragGestures { change, _ -> selection = anneeA(change.position.x, 44.dp.toPx(), size.width - 52.dp.toPx(), annees) }
                },
        ) {
            val styleAxe = TextStyle(fontSize = 10.sp, color = couleurTexte)
            val margeG = 44.dp.toPx()
            val margeB = 18.dp.toPx()
            val l = size.width - margeG - 8.dp.toPx()
            val h = size.height - margeB - 6.dp.toPx()
            val y0 = 6.dp.toPx()
            fun x(a: Int) = margeG + l * (a - annees.first()) / (annees.last() - annees.first()).toFloat()
            fun y(v: Double) = y0 + h * (1 - ((v - bas) / (haut - bas))).toFloat()

            // Grille horizontale et graduations.
            var v = bas
            while (v <= haut + pas / 2) {
                val yy = y(v)
                val estZero = ligneZero && abs(v) < pas / 1000
                drawLine(
                    if (estZero) couleurTexte else couleurGrille,
                    Offset(margeG, yy), Offset(margeG + l, yy),
                    strokeWidth = if (estZero) 1.5.dp.toPx() else 0.5.dp.toPx(),
                )
                val t = mesureur.measure(format(v), styleAxe)
                drawText(t, topLeft = Offset(margeG - t.size.width - 4.dp.toPx(), yy - t.size.height / 2))
                v += pas
            }
            // Années.
            for (a in annees) {
                if (a % 10 != 0 && a != annees.first()) continue
                if (a != annees.first() && a - annees.first() < 4) continue
                val t = mesureur.measure(a.toString(), styleAxe)
                drawText(t, topLeft = Offset((x(a) - t.size.width / 2).coerceIn(margeG - t.size.width / 2, size.width - t.size.width), y0 + h + 3.dp.toPx()))
            }
            // Curseur.
            selection?.let { s ->
                drawLine(couleurTexte, Offset(x(s), y0), Offset(x(s), y0 + h), strokeWidth = 1.dp.toPx())
            }
            // Courbes.
            for (s in series) {
                val chemin = Path()
                s.valeurs.forEachIndexed { k, val_ ->
                    val px = x(s.annees[k]); val py = y(val_)
                    if (k == 0) chemin.moveTo(px, py) else chemin.lineTo(px, py)
                }
                drawPath(
                    chemin, s.couleur,
                    style = Stroke(
                        width = 2.dp.toPx(), cap = StrokeCap.Round,
                        pathEffect = if (s.pointillee) PathEffect.dashPathEffect(floatArrayOf(8f, 6f)) else null,
                    ),
                )
                selection?.let { a ->
                    val k = s.annees.indexOf(a)
                    if (k >= 0) {
                        drawCircle(fond, 5.dp.toPx(), Offset(x(a), y(s.valeurs[k])))
                        drawCircle(s.couleur, 4.dp.toPx(), Offset(x(a), y(s.valeurs[k])))
                    }
                }
            }
            // Repères publiés.
            reperes?.points?.forEach { (a, val_) ->
                if (a in annees.first()..annees.last()) losange(Offset(x(a), y(val_)), 5.dp.toPx(), couleurRepere, fond)
            }
        }
    }
}

private fun anneeA(px: Float, margeG: Float, l: Float, annees: List<Int>): Int {
    val t = ((px - margeG) / l).coerceIn(0f, 1f)
    return (annees.first() + t * (annees.last() - annees.first())).roundToInt()
}

private fun DrawScope.losange(c: Offset, r: Float, couleur: Color, fond: Color) {
    val p = Path().apply {
        moveTo(c.x, c.y - r); lineTo(c.x + r, c.y); lineTo(c.x, c.y + r); lineTo(c.x - r, c.y); close()
    }
    drawPath(p, fond)
    drawPath(p, couleur, style = Stroke(width = 2.dp.toPx()))
}

/**
 * Pyramide des âges : barres pleines pour l'année [a], contour pour l'année [b].
 * Âges regroupés par 1 an jusqu'à 99 ans, puis 100 ans ou plus.
 */
@Composable
fun GraphiquePyramide(
    a: Pyramide, anneeA: Int,
    b: Pyramide, anneeB: Int,
    couleurHommes: Color, couleurFemmes: Color,
    modifier: Modifier = Modifier,
) {
    val mesureur = rememberTextMeasurer()
    val couleurTexte = MaterialTheme.colorScheme.onSurfaceVariant
    val couleurContour = MaterialTheme.colorScheme.onSurface
    val ages = 0..100
    fun valeur(p: Pyramide, h: Boolean, age: Int): Double {
        val t = if (h) p.hommes else p.femmes
        return if (age < 100) t[age] else (100..AGE_MAX).sumOf { t[it] }
    }
    val maxi = ages.maxOf { maxOf(valeur(a, true, it), valeur(a, false, it), valeur(b, true, it), valeur(b, false, it)) }
        .coerceAtMost(ages.filter { it < 100 }.maxOf { maxOf(valeur(a, true, it), valeur(a, false, it), valeur(b, true, it), valeur(b, false, it)) } * 1.05)

    Column(modifier) {
        Legende(listOf("Hommes $anneeA" to couleurHommes, "Femmes $anneeA" to couleurFemmes, "Contour : $anneeB" to couleurContour))
        Spacer(Modifier.height(4.dp))
        Canvas(Modifier.fillMaxWidth().height(380.dp)) {
            val style = TextStyle(fontSize = 10.sp, color = couleurTexte)
            val milieu = size.width / 2
            val axe = 16.dp.toPx()
            val bas = size.height - 16.dp.toPx()
            val hBarre = bas / ages.count()
            val demi = milieu - axe - 4.dp.toPx()
            fun largeur(v: Double) = (v / maxi).toFloat().coerceAtMost(1f) * demi
            for (age in ages) {
                val y = bas - (age + 1) * hBarre
                val lh = largeur(valeur(a, true, age))
                val lf = largeur(valeur(a, false, age))
                drawRect(couleurHommes, Offset(milieu - axe - lh, y), Size(lh, hBarre * 0.85f))
                drawRect(couleurFemmes, Offset(milieu + axe, y), Size(lf, hBarre * 0.85f))
            }
            // Contour de la seconde année.
            val gauche = Path()
            val droite = Path()
            for (age in ages) {
                val y1 = bas - age * hBarre
                val y2 = bas - (age + 1) * hBarre
                val xh = milieu - axe - largeur(valeur(b, true, age))
                val xf = milieu + axe + largeur(valeur(b, false, age))
                if (age == 0) { gauche.moveTo(xh, y1); droite.moveTo(xf, y1) } else { gauche.lineTo(xh, y1); droite.lineTo(xf, y1) }
                gauche.lineTo(xh, y2); droite.lineTo(xf, y2)
            }
            drawPath(gauche, couleurContour, style = Stroke(1.5.dp.toPx()))
            drawPath(droite, couleurContour, style = Stroke(1.5.dp.toPx()))
            for (age in listOf(0, 20, 40, 60, 80, 100)) {
                val t = mesureur.measure(if (age == 100) "100+" else age.toString(), style)
                drawText(t, topLeft = Offset(milieu - t.size.width / 2, bas - (age + 0.5f) * hBarre - t.size.height / 2))
            }
            val echelle = pasRond(maxi / 1000.0, 2)
            var v = echelle
            while (v * 1000 <= maxi) {
                val t = mesureur.measure("${v.roundToInt()} k", style)
                val w = largeur(v * 1000)
                drawText(t, topLeft = Offset(milieu - axe - w - t.size.width / 2, bas + 2.dp.toPx()))
                drawText(t, topLeft = Offset(milieu + axe + w - t.size.width / 2, bas + 2.dp.toPx()))
                v += echelle
            }
        }
    }
}
