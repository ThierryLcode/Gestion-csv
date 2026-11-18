package fr.retraites.modele

import java.util.Locale

/** Effort à fournir, sur un seul levier à la fois, pour équilibrer le système une année donnée. */
data class LeviersEquilibre(
    val annee: Int,
    val soldePib: Double,
    /** Points de cotisation supplémentaires. */
    val pointsCotisation: Double,
    /** Variation de la pension moyenne de tous les retraités (négatif = baisse), en %. */
    val variationPensionsPourcent: Double,
    /** Années de décalage supplémentaire de l'âge moyen de départ ; null si hors de portée. */
    val decalageAge: Double?,
)

object Equilibre {
    /**
     * Calcule, pour l'année [annee], chacun des trois leviers (cotisations, pensions, âge)
     * qui annulerait seul le solde, en plus des mesures déjà présentes dans le scénario.
     */
    fun leviers(
        scenario: Scenario,
        annee: Int = ANNEE_FIN,
        initiale: Pyramide = PyramideInitiale.pyramide,
        demographie: ProjectionDemographique? = null,
    ): LeviersEquilibre {
        val demo = demographie ?: ModeleDemographique.projeter(scenario.demographie, initiale)
        val r = ModeleRetraites.projeter(scenario, initiale, demo).en(annee)
        val points = -r.soldePib / ModeleRetraites.POINT_COTISATION_PIB
        val pensions = r.soldePib / r.depensesPib * 100.0

        val m = scenario.mesures
        fun soldeAvec(decalage: Double): Double {
            val s = scenario.copy(mesures = m.copy(decalageAge = m.decalageAge + decalage, anneeDebutAge = m.anneeDebutAge))
            return ModeleRetraites.projeter(s, initiale, demo).en(annee).soldePib
        }
        var bas = -4.0
        var haut = 8.0
        val age: Double? = if (soldeAvec(bas) > 0 || soldeAvec(haut) < 0) null else {
            repeat(40) {
                val milieu = (bas + haut) / 2
                if (soldeAvec(milieu) < 0) bas = milieu else haut = milieu
            }
            (bas + haut) / 2
        }
        // La montée en charge doit avoir eu le temps d'atteindre le décalage visé.
        val atteignable = age == null || m.rythmeAge <= 0 ||
            (annee - m.anneeDebutAge + 1) * m.rythmeAge >= kotlin.math.abs(m.decalageAge + age)
        return LeviersEquilibre(annee, r.soldePib, points, pensions, if (atteignable) age else null)
    }
}

/** Un choix prédéfini proposé dans l'application. */
data class Preset<T>(val libelle: String, val detail: String, val valeur: T)

object Catalogue {
    val demographie = listOf(
        Preset("Central Insee 2026 (référence COR)", "Fécondité 1,45 dès 2028 ; solde migratoire +150 000 ; espérance de vie 2070 : 86,7 ans (H) et 89,5 ans (F)", HypothesesDemographiques()),
        Preset("Fécondité basse (1,2)", "Variante du COR 2026 : 1,2 enfant par femme", HypothesesDemographiques(fecondite = 1.2, anneeCibleFecondite = 2030)),
        Preset("Fécondité haute (1,7)", "Variante du COR 2026 : 1,7 enfant par femme", HypothesesDemographiques(fecondite = 1.7, anneeCibleFecondite = 2030)),
        Preset("Solde migratoire bas (+70 000)", "Variante basse de l'Insee 2026", HypothesesDemographiques(soldeMigratoire = 70_000.0)),
        Preset("Solde migratoire haut (+230 000)", "Variante haute de l'Insee 2026", HypothesesDemographiques(soldeMigratoire = 230_000.0)),
        Preset("Espérance de vie stable", "Variante de travail Insee : mortalité figée au niveau de 2025", HypothesesDemographiques(esperanceVieHommes2070 = 80.3, esperanceVieFemmes2070 = 85.9)),
        Preset("Espérance de vie plus forte (+2 ans)", "Illustratif : +2 ans en 2070 par rapport au scénario central", HypothesesDemographiques(esperanceVieHommes2070 = 88.7, esperanceVieFemmes2070 = 91.5)),
        Preset("Anciennes hypothèses Insee 2021", "Fécondité 1,8 ; solde migratoire +70 000 ; espérance de vie 2070 : 87,5 ans (H) et 90,0 ans (F) — rapports COR jusqu'en 2025", HypothesesDemographiques(fecondite = 1.8, soldeMigratoire = 70_000.0, esperanceVieHommes2070 = 87.5, esperanceVieFemmes2070 = 90.0)),
    )

    val economie = listOf(
        Preset("Référence COR (productivité 0,7 %)", "Convergence des gains de productivité vers 0,7 % par an ; chômage 7 %", HypothesesEconomiques()),
        Preset("Productivité 0,4 %", "Variante basse du COR 2026", HypothesesEconomiques(productivite = 0.004)),
        Preset("Productivité 1,0 %", "Variante haute du COR 2026", HypothesesEconomiques(productivite = 0.010)),
        Preset("Chômage 4,5 %", "Plein emploi (variante illustrative)", HypothesesEconomiques(chomage = 0.045)),
        Preset("Chômage 10 %", "Variante illustrative", HypothesesEconomiques(chomage = 0.10)),
    )

    val mesures = listOf(
        Preset("Législation actuelle", "Aucune mesure : réforme 2023 suspendue jusqu'en 2028 puis reprise", Mesures()),
        Preset("Retour à 62 ans", "Abrogation de la réforme de 2023 : âge moyen de départ réduit d'environ 1 an (ordre de grandeur)", Mesures(decalageAge = -1.0, anneeDebutAge = 2027)),
        Preset("Âge légal porté à 65 ans", "+3 mois par génération à partir de 2033 ; effet sur l'âge effectif ≈ 0,8 an", Mesures(decalageAge = 0.8, anneeDebutAge = 2033)),
        Preset("Âge légal porté à 66 ans", "+3 mois par génération à partir de 2033 ; effet sur l'âge effectif ≈ 1,6 an", Mesures(decalageAge = 1.6, anneeDebutAge = 2033)),
        Preset("Année blanche en 2027", "Pensions non revalorisées en 2027 (inflation supposée de 1,5 %)", Mesures(sousIndexation = 0.015, anneeDebutSousIndexation = 2027, dureeSousIndexation = 1)),
        Preset("Sous-indexation de 1 pt pendant 5 ans", "Revalorisation égale à l'inflation moins 1 point de 2027 à 2031", Mesures(sousIndexation = 0.01, anneeDebutSousIndexation = 2027, dureeSousIndexation = 5)),
        Preset("Indexation sur les salaires", "Pensions en cours de service revalorisées comme les salaires", Mesures(indexationSalaires = true)),
        Preset("+1 point de cotisation", "Hausse des cotisations retraite à partir de 2027", Mesures(hausseCotisations = 1.0)),
        Preset("Nouvelles pensions −5 %", "Baisse de 5 % des pensions liquidées à partir de 2027", Mesures(variationNouvellesPensions = -0.05)),
    )
}

object CatalogueCapitalisation {
    val options = listOf(
        Preset("Aucune", "Système entièrement par répartition", Capitalisation()),
        Preset("Pilier complémentaire de 2 points", "2 points de cotisation en plus, placés sur des comptes et convertis en rente au départ", Capitalisation(pointsPilier = 2.0)),
        Preset("Bascule de 2 points vers la capitalisation", "2 points pris sur les cotisations de répartition : moins de ressources tout de suite, moins de droits plus tard", Capitalisation(pointsPilier = 2.0, substitution = true)),
        Preset("Fonds de réserve : 0,2 % du PIB par an", "Dotation de 2027 à 2039, puis couverture des déficits à partir de 2040", Capitalisation(fondsActif = true, dotationFondsPib = 0.2, anneeDecaissementFonds = 2040)),
        Preset("Fonds de réserve de 100 Md€", "Capital de 100 Md€ placé dès 2025, mobilisé à partir de 2035", Capitalisation(fondsActif = true, fondsInitialMdEuros = 100.0)),
    )
}

object ExportCsv {
    private fun f(x: Double, d: Int) = String.format(Locale.FRANCE, "%.${d}f", x)

    /** Exporte une ou deux projections (avec et sans mesures) au format CSV (séparateur « ; »). */
    fun projections(avec: Projection, sans: Projection? = null): String {
        val colonnes = listOf(
            "Année", "Population", "0-19 ans", "20-64 ans", "65 ans ou plus", "Naissances", "Fécondité",
            "Espérance de vie H", "Espérance de vie F", "Rapport 20-64 / 65+", "Âge moyen de départ",
            "Cotisants", "Retraités", "Cotisants par retraité", "Pension relative (%)",
            "Dépenses (% PIB)", "Ressources (% PIB)", "Solde (% PIB)", "Solde (Md€ 2025)",
        )
        val sb = StringBuilder()
        val capitalisation = listOf(
            "Cotisations capitalisées (% PIB)", "Actifs capitalisés (% PIB)", "Rentes capitalisées (% PIB)",
            "Pension relative y c. rentes (%)", "Fonds de réserve (% PIB)", "Fonds de réserve (Md€ 2025)", "Solde après fonds (% PIB)",
        )
        val entete = colonnes + capitalisation +
            if (sans == null) emptyList() else listOf("Dépenses sans mesures (% PIB)", "Solde sans mesures (% PIB)")
        sb.append(entete.joinToString(";")).append('\n')
        for ((i, r) in avec.annees.withIndex()) {
            val v = mutableListOf(
                r.annee.toString(), f(r.population, 0), f(r.moins20, 0), f(r.de20a64, 0), f(r.plus65, 0),
                f(r.naissances, 0), f(r.fecondite, 2), f(r.esperanceVieHommes, 1), f(r.esperanceVieFemmes, 1),
                f(r.rapportDemographique, 2), f(r.ageMoyenDepart, 2), f(r.cotisants, 0), f(r.retraites, 0),
                f(r.rapportCotisantsRetraites, 2), f(r.pensionRelative * 100, 1), f(r.depensesPib, 2),
                f(r.ressourcesPib, 2), f(r.soldePib, 2), f(r.soldeMdEuros, 1),
                f(r.cotisationsCapitalisationPib, 2), f(r.actifsCapitalisationPib, 1), f(r.rentesCapitalisationPib, 2),
                f(r.pensionRelativeTotale * 100, 1), f(r.fondsReservePib, 1), f(r.fondsReserveMdEuros, 1), f(r.soldeApresFondsPib, 2),
            )
            if (sans != null) {
                v += f(sans.annees[i].depensesPib, 2)
                v += f(sans.annees[i].soldePib, 2)
            }
            sb.append(v.joinToString(";")).append('\n')
        }
        return sb.toString()
    }
}
