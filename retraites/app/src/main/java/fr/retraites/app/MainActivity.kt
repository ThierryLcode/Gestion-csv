package fr.retraites.app

import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import fr.retraites.app.ui.EcranDemographie
import fr.retraites.app.ui.EcranHypotheses
import fr.retraites.app.ui.EcranMesures
import fr.retraites.app.ui.EcranMethode
import fr.retraites.app.ui.EcranResultats
import fr.retraites.modele.ExportCsv

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            Theme { Application() }
        }
    }
}

@Composable
private fun Theme(contenu: @Composable () -> Unit) {
    val sombre = isSystemInDarkTheme()
    val contexte = LocalContext.current
    val couleurs = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> if (sombre) dynamicDarkColorScheme(contexte) else dynamicLightColorScheme(contexte)
        sombre -> darkColorScheme()
        else -> lightColorScheme()
    }
    MaterialTheme(colorScheme = couleurs, content = contenu)
}

private val ONGLETS = listOf("Résultats", "Mesures", "Hypothèses", "Démographie", "Méthode")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Application(vm: RetraitesViewModel = viewModel()) {
    val etat by vm.etat.collectAsStateWithLifecycle()
    var onglet by rememberSaveable { mutableIntStateOf(0) }
    val contexte = LocalContext.current
    val snackbar = remember { SnackbarHostState() }

    LaunchedEffect(etat.message) {
        etat.message?.let {
            snackbar.showSnackbar(it)
            vm.messageLu()
        }
    }

    val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri != null) {
            try {
                val texte = contexte.contentResolver.openInputStream(uri)!!.bufferedReader(Charsets.UTF_8).use { it.readText() }
                vm.importerPyramide(nomFichier(contexte, uri) ?: "fichier CSV", texte)
            } catch (e: Exception) {
                vm.signaler("Lecture impossible : ${e.message}")
            }
        }
    }
    val exporter = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("text/csv")) { uri: Uri? ->
        val r = etat.resultats
        if (uri != null && r != null) {
            try {
                contexte.contentResolver.openOutputStream(uri)!!.use { sortie ->
                    // BOM UTF-8 pour une ouverture correcte dans un tableur.
                    sortie.write(byteArrayOf(0xEF.toByte(), 0xBB.toByte(), 0xBF.toByte()))
                    sortie.write(ExportCsv.projections(r.avec, r.sans).toByteArray(Charsets.UTF_8))
                }
                vm.signaler("Résultats exportés")
            } catch (e: Exception) {
                vm.signaler("Export impossible : ${e.message}")
            }
        }
    }

    Scaffold(
        topBar = {
            Column {
                TopAppBar(title = { Text("Projection des retraites") })
                PrimaryScrollableTabRow(selectedTabIndex = onglet, edgePadding = 8.dp) {
                    ONGLETS.forEachIndexed { i, titre ->
                        Tab(selected = onglet == i, onClick = { onglet = i }, text = { Text(titre) })
                    }
                }
                if (etat.calculEnCours) LinearProgressIndicator(Modifier.fillMaxWidth())
            }
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { marges ->
        Box(Modifier.fillMaxSize().padding(marges)) {
            when (onglet) {
                0 -> EcranResultats(etat) { exporter.launch("projection-retraites.csv") }
                1 -> EcranMesures(etat, vm::changerMesures)
                2 -> EcranHypotheses(
                    etat, vm::changerDemographie, vm::changerEconomie,
                    onImporter = { importer.launch(arrayOf("text/*", "application/csv", "application/vnd.ms-excel", "*/*")) },
                    onPyramideParDefaut = vm::pyramideParDefaut,
                )
                3 -> EcranDemographie(etat)
                else -> EcranMethode()
            }
        }
    }
}

private fun nomFichier(contexte: android.content.Context, uri: Uri): String? =
    contexte.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
        if (c.moveToFirst()) c.getString(0) else null
    }
