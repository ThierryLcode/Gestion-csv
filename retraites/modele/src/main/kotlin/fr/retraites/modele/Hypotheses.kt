package fr.retraites.modele

/** Première et dernière année projetées. */
const val ANNEE_DEBUT = 2025
const val ANNEE_FIN = 2070

/**
 * Hypothèses démographiques. Les valeurs par défaut sont celles du scénario central des
 * projections de population de l'Insee (2026), retenues par le COR dans son rapport annuel
 * de juin 2026.
 */
data class HypothesesDemographiques(
    /** Indicateur conjoncturel de fécondité visé (enfants par femme). */
    val fecondite: Double = 1.45,
    /** Année à partir de laquelle la fécondité visée est atteinte. */
    val anneeCibleFecondite: Int = 2028,
    /** Solde migratoire annuel (personnes). */
    val soldeMigratoire: Double = 150_000.0,
    /** Espérance de vie à la naissance des hommes en 2070. */
    val esperanceVieHommes2070: Double = 86.7,
    /** Espérance de vie à la naissance des femmes en 2070. */
    val esperanceVieFemmes2070: Double = 89.5,
) {
    companion object {
        /** Indicateur de fécondité observé en 2025 (Insee). */
        const val FECONDITE_2025 = 1.56
        const val ESPERANCE_VIE_HOMMES_2025 = 80.3
        const val ESPERANCE_VIE_FEMMES_2025 = 85.9
    }
}

/** Convention de financement : comment évoluent les ressources du système. */
enum class ConventionRessources(val libelle: String) {
    COR("Trajectoire du scénario de référence COR"),
    TAUX_CONSTANT("Taux de prélèvement constant"),
}

/** Hypothèses économiques (scénario de référence COR 2026 par défaut). */
data class HypothesesEconomiques(
    /** Croissance annuelle de la productivité du travail à long terme (0,007 = 0,7 %). */
    val productivite: Double = 0.007,
    /** Taux de chômage de long terme. */
    val chomage: Double = 0.07,
    val convention: ConventionRessources = ConventionRessources.COR,
)

/**
 * Mesures appliquées au système de retraite. Les valeurs par défaut (toutes nulles)
 * correspondent à la législation prise en compte dans le scénario de référence.
 */
data class Mesures(
    /** Décalage de l'âge moyen effectif de départ, en années (négatif = départ plus tôt). */
    val decalageAge: Double = 0.0,
    /** Année d'entrée en vigueur du décalage. */
    val anneeDebutAge: Int = 2027,
    /** Vitesse de montée en charge, en années d'âge par année civile (0,25 = 3 mois par an). */
    val rythmeAge: Double = 0.25,
    /** Sous-indexation des pensions en cours de service, en points par an (positif = moins que l'inflation). */
    val sousIndexation: Double = 0.0,
    val anneeDebutSousIndexation: Int = 2027,
    /** Nombre d'années de sous-indexation (0 = permanente). */
    val dureeSousIndexation: Int = 0,
    /** Indexation des pensions en cours de service sur les salaires plutôt que sur les prix. */
    val indexationSalaires: Boolean = false,
    /** Hausse des cotisations, en points de masse salariale. */
    val hausseCotisations: Double = 0.0,
    val anneeHausseCotisations: Int = 2027,
    /** Variation du montant des nouvelles pensions (0,05 = +5 %), à partir de l'année indiquée. */
    val variationNouvellesPensions: Double = 0.0,
    val anneeVariationNouvellesPensions: Int = 2027,
) {
    val estVide: Boolean get() = this == Mesures()
}

/**
 * Capitalisation : pilier de comptes individuels et fonds de réserve collectif.
 * Les montants sont en euros constants ; le rendement est réel (au-delà de l'inflation).
 */
data class Capitalisation(
    /** Rendement réel annuel des placements (0,02 = 2 %), commun au pilier et au fonds. */
    val rendementReel: Double = 0.02,

    /** Points de cotisation versés sur des comptes capitalisés. */
    val pointsPilier: Double = 0.0,
    val anneeDebutPilier: Int = 2027,
    /**
     * false : cotisation en complément (en plus des cotisations actuelles) ;
     * true : prise sur les cotisations de répartition, qui perd ces ressources tout de suite
     * et réduit à proportion les droits futurs (coût de transition).
     */
    val substitution: Boolean = false,

    /** Fonds de réserve collectif, qui finance les déficits du système par répartition. */
    val fondsActif: Boolean = false,
    /** Capital du fonds en 2025 (Md€). */
    val fondsInitialMdEuros: Double = 0.0,
    /** Dotation annuelle, en % du PIB, versée de [anneeDebutFonds] à [anneeDecaissementFonds] - 1. */
    val dotationFondsPib: Double = 0.0,
    val anneeDebutFonds: Int = 2027,
    /** Année à partir de laquelle le fonds couvre les déficits (dans la limite de son capital). */
    val anneeDecaissementFonds: Int = 2035,
    /** Les excédents éventuels du système alimentent le fonds. */
    val placerExcedents: Boolean = true,
) {
    val pilierActif: Boolean get() = pointsPilier > 0.0
    val estVide: Boolean get() = !pilierActif && !fondsActif
}

/** Ensemble complet des paramètres d'une projection. */
data class Scenario(
    val demographie: HypothesesDemographiques = HypothesesDemographiques(),
    val economie: HypothesesEconomiques = HypothesesEconomiques(),
    val mesures: Mesures = Mesures(),
    val capitalisation: Capitalisation = Capitalisation(),
) {
    /** Mêmes hypothèses, sans mesure ni capitalisation (législation actuelle). */
    fun sansMesures(): Scenario = copy(mesures = Mesures(), capitalisation = Capitalisation(rendementReel = capitalisation.rendementReel))
}
