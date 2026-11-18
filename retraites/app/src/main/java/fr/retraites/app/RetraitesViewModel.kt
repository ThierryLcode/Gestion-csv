package fr.retraites.app

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import fr.retraites.modele.Equilibre
import fr.retraites.modele.HypothesesDemographiques
import fr.retraites.modele.HypothesesEconomiques
import fr.retraites.modele.LeviersEquilibre
import fr.retraites.modele.Mesures
import fr.retraites.modele.ModeleDemographique
import fr.retraites.modele.ModeleRetraites
import fr.retraites.modele.Projection
import fr.retraites.modele.Pyramide
import fr.retraites.modele.PyramideInitiale
import fr.retraites.modele.Scenario
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Résultats calculés pour le scénario courant. */
data class Resultats(
    /** Projection avec les mesures choisies. */
    val avec: Projection,
    /** Même hypothèses, sans mesure (législation actuelle). */
    val sans: Projection,
    val leviers: LeviersEquilibre,
)

data class EtatUi(
    val scenario: Scenario = Scenario(),
    /** Nom de la pyramide importée, ou null pour la pyramide intégrée. */
    val pyramideImportee: String? = null,
    val resultats: Resultats? = null,
    val calculEnCours: Boolean = true,
    val message: String? = null,
)

class RetraitesViewModel : ViewModel() {
    private val _etat = MutableStateFlow(EtatUi())
    val etat: StateFlow<EtatUi> = _etat.asStateFlow()

    private var pyramide: Pyramide? = null
    private var calcul: Job? = null

    init {
        recalculer(immediat = true)
    }

    fun changerDemographie(d: HypothesesDemographiques) = modifier { it.copy(demographie = d) }
    fun changerEconomie(e: HypothesesEconomiques) = modifier { it.copy(economie = e) }
    fun changerMesures(m: Mesures) = modifier { it.copy(mesures = m) }

    private fun modifier(f: (Scenario) -> Scenario) {
        _etat.update { it.copy(scenario = f(it.scenario)) }
        recalculer()
    }

    fun importerPyramide(nom: String, texte: String) {
        try {
            pyramide = PyramideInitiale.lireCsv(texte)
            _etat.update { it.copy(pyramideImportee = nom, message = "Pyramide « $nom » importée") }
            recalculer(immediat = true)
        } catch (e: Exception) {
            _etat.update { it.copy(message = "Import impossible : ${e.message}") }
        }
    }

    fun pyramideParDefaut() {
        pyramide = null
        _etat.update { it.copy(pyramideImportee = null, message = "Pyramide intégrée rétablie") }
        recalculer(immediat = true)
    }

    fun messageLu() = _etat.update { it.copy(message = null) }

    fun signaler(message: String) = _etat.update { it.copy(message = message) }

    /** Relance le calcul ; les modifications rapprochées (curseurs) sont regroupées. */
    private fun recalculer(immediat: Boolean = false) {
        calcul?.cancel()
        _etat.update { it.copy(calculEnCours = true) }
        calcul = viewModelScope.launch {
            if (!immediat) delay(120)
            val scenario = _etat.value.scenario
            val initiale = pyramide ?: PyramideInitiale.pyramide
            val resultats = withContext(Dispatchers.Default) {
                val demo = ModeleDemographique.projeter(scenario.demographie, initiale)
                Resultats(
                    avec = ModeleRetraites.projeter(scenario, initiale, demo),
                    sans = ModeleRetraites.projeter(scenario.copy(mesures = Mesures()), initiale, demo),
                    leviers = Equilibre.leviers(scenario, initiale = initiale, demographie = demo),
                )
            }
            _etat.update { if (it.scenario == scenario) it.copy(resultats = resultats, calculEnCours = false) else it }
        }
    }
}
