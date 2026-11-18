package fr.retraites.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import java.util.Locale
import kotlin.math.roundToInt

/**
 * Conversion des grandeurs exprimées en % du PIB vers l'unité choisie
 * (montants en milliards d'euros constants de 2025, avec le PIB projeté de l'année).
 */
class Echelle(val unite: fr.retraites.app.Unite) {
    val enPib: Boolean get() = unite == fr.retraites.app.Unite.POURCENT_PIB
    fun v(pourcentPib: Double, r: fr.retraites.modele.ResultatAnnee): Double =
        if (enPib) pourcentPib else pourcentPib / 100.0 * r.pibMdEuros
    /** Format des axes et de la lecture des graphiques. */
    fun axe(decimales: Int = 1): (Double) -> String = if (enPib) { x -> pourcent(x, decimales) } else { x -> nombre(x, 0) }
    /** Valeur avec son unité, pour les tuiles et les textes. */
    fun texte(x: Double, decimales: Int = 1): String = if (enPib) pourcent(x, decimales) + " PIB" else nombre(x, 0) + " Md€"
    val libelle: String get() = if (enPib) "En % du PIB" else "En milliards d'euros constants de 2025"
}

/** Sélecteur d'unité : % du PIB ou montants. */
@OptIn(androidx.compose.material3.ExperimentalMaterial3Api::class)
@Composable
fun SelecteurUnite(unite: fr.retraites.app.Unite, onChange: (fr.retraites.app.Unite) -> Unit) {
    val options = listOf(fr.retraites.app.Unite.POURCENT_PIB to "% du PIB", fr.retraites.app.Unite.MONTANT to "Montants (Md€)")
    androidx.compose.material3.SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp)) {
        options.forEachIndexed { i, (u, libelle) ->
            androidx.compose.material3.SegmentedButton(
                selected = unite == u,
                onClick = { onChange(u) },
                shape = androidx.compose.material3.SegmentedButtonDefaults.itemShape(i, options.size),
            ) { Text(libelle) }
        }
    }
}

/** Couleurs des séries (palette catégorielle validée, déclinée en mode clair et sombre). */
object Couleurs {
    val avec: Color @Composable get() = if (isSystemInDarkTheme()) Color(0xFF3987E5) else Color(0xFF2A78D6)
    val sans: Color @Composable get() = if (isSystemInDarkTheme()) Color(0xFFD95926) else Color(0xFFEB6834)
    val troisieme: Color @Composable get() = if (isSystemInDarkTheme()) Color(0xFF199E70) else Color(0xFF1BAF7A)
}

private val FR = Locale.FRANCE

fun nombre(x: Double, decimales: Int = 1): String = String.format(FR, "%.${decimales}f", x)
fun pourcent(x: Double, decimales: Int = 1): String = String.format(FR, "%.${decimales}f %%", x)
fun signe(x: Double, decimales: Int = 1): String = String.format(FR, "%+.${decimales}f", x)
fun millions(x: Double, decimales: Int = 1): String = String.format(FR, "%.${decimales}f M", x / 1e6)
fun milliers(x: Double): String = String.format(FR, "%,.0f", x).replace(' ', ' ')

@Composable
fun Section(titre: String, sousTitre: String? = null, contenu: @Composable () -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(titre, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            if (sousTitre != null) {
                Text(sousTitre, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            contenu()
        }
    }
}

/** Curseur avec libellé et valeur affichée ; la valeur est arrondie au pas indiqué. */
@Composable
fun Curseur(
    libelle: String,
    valeur: Double,
    min: Double,
    max: Double,
    pas: Double,
    affichage: (Double) -> String,
    onChange: (Double) -> Unit,
) {
    Column {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Text(libelle, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
            Text(affichage(valeur), style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
        }
        val nbPas = ((max - min) / pas).roundToInt()
        Slider(
            value = valeur.toFloat().coerceIn(min.toFloat(), max.toFloat()),
            onValueChange = { v ->
                val arrondi = min + ((v - min) / pas).roundToInt() * pas
                if (kotlin.math.abs(arrondi - valeur) > pas / 1000) onChange(arrondi)
            },
            valueRange = min.toFloat()..max.toFloat(),
            steps = (nbPas - 1).coerceIn(0, 400),
        )
    }
}

/** Rangée de puces de choix. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun <T> Puces(options: List<Pair<String, T>>, estChoisie: (T) -> Boolean, onChoix: (T) -> Unit) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        options.forEach { (libelle, valeur) ->
            FilterChip(selected = estChoisie(valeur), onClick = { onChoix(valeur) }, label = { Text(libelle) })
        }
    }
}

/** Tuile d'indicateur : valeur principale et comparaison. */
@Composable
fun Tuile(titre: String, valeur: String, detail: String?, modifier: Modifier = Modifier) {
    Card(
        modifier = modifier,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
    ) {
        Column(Modifier.padding(12.dp)) {
            Text(titre, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(valeur, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
            if (detail != null) {
                Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}
