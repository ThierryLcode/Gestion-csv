package fr.retraites.modele

import kotlin.math.abs
import kotlin.math.exp
import kotlin.math.pow
import kotlin.math.sign

/** Résultats d'une année de projection. */
data class ResultatAnnee(
    val annee: Int,
    val population: Double,
    val moins20: Double,
    val de20a64: Double,
    val plus65: Double,
    val naissances: Double,
    val fecondite: Double,
    val esperanceVieHommes: Double,
    val esperanceVieFemmes: Double,
    /** Rapport démographique : personnes de 20-64 ans pour une de 65 ans ou plus. */
    val rapportDemographique: Double,
    val ageMoyenDepart: Double,
    val cotisants: Double,
    val retraites: Double,
    /** Nombre de cotisants pour un retraité. */
    val rapportCotisantsRetraites: Double,
    /** Pension moyenne rapportée au revenu d'activité moyen. */
    val pensionRelative: Double,
    val depensesPib: Double,
    val ressourcesPib: Double,
    val soldePib: Double,
    /** Solde en milliards d'euros constants de 2025. */
    val soldeMdEuros: Double,
    val pibMdEuros: Double,
    /** Cotisations versées au pilier par capitalisation (% du PIB). */
    val cotisationsCapitalisationPib: Double,
    /** Encours du pilier par capitalisation : comptes et provisions des rentes (% du PIB). */
    val actifsCapitalisationPib: Double,
    /** Rentes versées par le pilier par capitalisation (% du PIB). */
    val rentesCapitalisationPib: Double,
    /** Pension moyenne y compris rentes capitalisées, rapportée au revenu d'activité moyen. */
    val pensionRelativeTotale: Double,
    /** Capital du fonds de réserve en fin d'année (% du PIB). */
    val fondsReservePib: Double,
    val fondsReserveMdEuros: Double,
    /** Solde du système après intervention du fonds de réserve (% du PIB). */
    val soldeApresFondsPib: Double,
)

class Projection(val scenario: Scenario, val annees: List<ResultatAnnee>, val demographie: ProjectionDemographique) {
    fun en(annee: Int): ResultatAnnee = annees[annee - ANNEE_DEBUT]

    /** Première année où le fonds de réserve, sollicité, est épuisé ; null sinon. */
    val anneeEpuisementFonds: Int?
        get() {
            val c = scenario.capitalisation
            if (!c.fondsActif) return null
            return annees.firstOrNull { it.annee >= c.anneeDecaissementFonds && it.soldePib < 0 && it.fondsReserveMdEuros <= 1e-6 }?.annee
        }

    /** Solde cumulé (Md€ constants 2025) sur toute la période. */
    val soldeCumuleMdEuros: Double get() = annees.sumOf { it.soldeMdEuros }

    /** Valeurs de référence publiées par le COR (rapport annuel de juin 2026, scénario de référence). */
    companion object {
        val REFERENCE_COR_DEPENSES = listOf(2025 to 14.1, 2045 to 14.2, 2070 to 15.3)
        val REFERENCE_COR_RESSOURCES = listOf(2025 to 13.9, 2045 to 13.3, 2070 to 12.9)
        val REFERENCE_COR_SOLDE = listOf(2025 to -0.2, 2030 to -0.2, 2045 to -0.9, 2070 to -2.4)
    }
}

/**
 * Modèle du système de retraite, construit sur l'équation comptable utilisée par le COR :
 *
 * dépenses / PIB = (retraités / actifs occupés) × (pension moyenne / revenu d'activité moyen)
 *                  × part des revenus d'activité dans le PIB.
 *
 * Les dépenses sont exprimées par rapport à leur niveau de 2025 (14,1 % du PIB), la part des
 * revenus d'activité dans le PIB étant supposée constante.
 */
object ModeleRetraites {
    const val DEPENSES_PIB_2025 = 14.1
    const val PIB_2025_MD_EUROS = 2_990.0
    /** Ressources apportées par un point de cotisation, en points de PIB. */
    const val POINT_COTISATION_PIB = 0.23
    /** Taux de cotisation retraite total (points) servant à proportionner la perte de droits en cas de substitution. */
    const val TAUX_COTISATION_TOTAL = 28.0
    /** Durée de carrière servant à la montée en charge de la perte de droits. */
    const val DUREE_CARRIERE = 42.0
    /** Gain sur la pension à la liquidation par année de départ plus tardive. */
    const val GAIN_PENSION_PAR_ANNEE = 0.02
    /** Élasticité de la pension à la liquidation (relative au salaire) aux gains de productivité. */
    const val ELASTICITE_PRODUCTIVITE = 0.15
    /** Croissance réelle passée des salaires utilisée pour reconstituer le stock de pensions. */
    private const val CROISSANCE_PASSEE = 0.009
    private const val PENSION_RELATIVE_2025 = 0.50
    private const val CHOMAGE_2025 = 0.075
    private const val DISPERSION_DEPART = 1.1

    /** Âge moyen effectif de départ de la législation de référence (réforme 2023 suspendue jusqu'en 2028 puis reprise). */
    private val AGE_DEPART_REFERENCE = listOf(2025 to 62.9, 2028 to 63.1, 2035 to 64.2, 2045 to 64.4, 2070 to 64.6)

    /** Ressources en % du PIB selon la convention COR (scénario de référence 2026). */
    private val RESSOURCES_COR = listOf(2025 to 13.9, 2030 to 13.9, 2045 to 13.3, 2070 to 12.9)

    /** Taux d'activité par âge des personnes non retraitées. */
    private fun activite(age: Int): Double = when {
        age < 15 -> 0.0
        age < 20 -> 0.17
        age < 25 -> 0.63
        age < 30 -> 0.88
        age < 50 -> 0.92
        age < 55 -> 0.89
        age < 60 -> 0.84
        age < 75 -> 0.72
        else -> 0.0
    }

    fun decalageAgeEn(m: Mesures, annee: Int): Double {
        if (m.decalageAge == 0.0 || annee < m.anneeDebutAge) return 0.0
        if (m.rythmeAge <= 0.0) return m.decalageAge
        val atteint = (annee - m.anneeDebutAge + 1) * m.rythmeAge
        return sign(m.decalageAge) * minOf(abs(m.decalageAge), atteint)
    }

    fun ageMoyenDepartEn(m: Mesures, annee: Int): Double = interpoler(AGE_DEPART_REFERENCE, annee) + decalageAgeEn(m, annee)

    /** Part des personnes d'âge [age] (révolu au 1er janvier) déjà retraitées. */
    private fun partRetraites(age: Int, ageMoyen: Double): Double {
        if (age < 50) return 0.0
        return 1.0 / (1.0 + exp(-((age + 0.5) - ageMoyen) / (DISPERSION_DEPART / 1.7)))
    }

    private fun chomageEn(e: HypothesesEconomiques, annee: Int) = interpoler(listOf(2025 to CHOMAGE_2025, 2032 to e.chomage), annee)

    /** Revalorisation réelle des pensions en cours de service l'année [annee]. */
    private fun revalorisation(s: Scenario, annee: Int): Double {
        val m = s.mesures
        var r = if (m.indexationSalaires && annee >= m.anneeDebutSousIndexation) s.economie.productivite else 0.0
        val fin = if (m.dureeSousIndexation <= 0) Int.MAX_VALUE else m.anneeDebutSousIndexation + m.dureeSousIndexation - 1
        if (annee in m.anneeDebutSousIndexation..fin) r -= m.sousIndexation
        return r
    }

    fun projeter(
        s: Scenario,
        initiale: Pyramide = PyramideInitiale.pyramide,
        demographie: ProjectionDemographique? = null,
    ): Projection {
        val demo = demographie ?: ModeleDemographique.projeter(s.demographie, initiale)
        val m = s.mesures
        val g = s.economie.productivite
        val nb = ANNEE_FIN - ANNEE_DEBUT + 1

        var salaire = 1.0 // indice de salaire réel, base 1 en 2025
        var nbRetraitesPrec = DoubleArray(NB_AGES)
        var pensionsPrec = DoubleArray(NB_AGES) // pension moyenne par âge, en unités de salaire 2025
        var popPrec: Pyramide? = null
        var reference: Triple<Double, Double, Double>? = null // (cotisants, masse des pensions, échelle)
        val res = ArrayList<ResultatAnnee>(nb)
        val cap = s.capitalisation
        val rend = cap.rendementReel
        var partRetraitesPrec = DoubleArray(NB_AGES)
        // Pilier par capitalisation, par âge : comptes en cours de constitution et rentes versées (Md€).
        var comptes = DoubleArray(NB_AGES)
        var rentes = DoubleArray(NB_AGES)
        var fonds = cap.fondsInitialMdEuros

        for (i in 0 until nb) {
            val annee = ANNEE_DEBUT + i
            if (i > 0) salaire *= (1.0 + g)
            val pop = demo.pyramides[i]
            val age = ageMoyenDepartEn(m, annee)
            val u = chomageEn(s.economie, annee)

            // Pension à la liquidation (relative au salaire de 2025).
            var nouvelle = salaire.pow(1.0 - ELASTICITE_PRODUCTIVITE) * (1.0 + GAIN_PENSION_PAR_ANNEE * decalageAgeEn(m, annee))
            if (annee >= m.anneeVariationNouvellesPensions) nouvelle *= (1.0 + m.variationNouvellesPensions)
            // Substitution : les droits de répartition acquis après la bascule diminuent à proportion.
            if (cap.pilierActif && cap.substitution && annee > cap.anneeDebutPilier) {
                val anciennete = ((annee - cap.anneeDebutPilier) / DUREE_CARRIERE).coerceAtMost(1.0)
                nouvelle *= (1.0 - cap.pointsPilier / TAUX_COTISATION_TOTAL * anciennete)
            }

            val nbRetraites = DoubleArray(NB_AGES)
            val partsRetraites = DoubleArray(NB_AGES)
            val emplois = DoubleArray(NB_AGES)
            val pensions = DoubleArray(NB_AGES)
            var cotisants = 0.0
            var retraites = 0.0
            var masse = 0.0
            val reval = revalorisation(s, annee)
            for (a in 0..AGE_MAX) {
                val effectif = pop.ensemble(a)
                val r = partRetraites(a, age)
                nbRetraites[a] = effectif * r
                partsRetraites[a] = r
                emplois[a] = effectif * (1.0 - r) * activite(a) * (1.0 - u)
                cotisants += emplois[a]
                if (nbRetraites[a] <= 0.0) continue
                if (i == 0) {
                    // Stock initial : les pensions des générations anciennes ont moins suivi les salaires.
                    pensions[a] = (1.0 + CROISSANCE_PASSEE).pow(-maxOf(0.0, a - 63.0) * ELASTICITE_PRODUCTIVITE)
                } else {
                    val prec = popPrec!!
                    val survivants = if (a == 0) 0.0 else {
                        val taux = if (prec.ensemble(a - 1) > 0) effectif / prec.ensemble(a - 1) else 0.0
                        minOf(nbRetraites[a], nbRetraitesPrec[a - 1] * taux.coerceAtMost(1.05))
                    }
                    val nouveaux = (nbRetraites[a] - survivants).coerceAtLeast(0.0)
                    val anciens = if (a == 0) 0.0 else pensionsPrec[a - 1] * (1.0 + reval)
                    pensions[a] = (survivants * anciens + nouveaux * nouvelle) / (survivants + nouveaux)
                }
                retraites += nbRetraites[a]
                masse += nbRetraites[a] * pensions[a]
            }

            if (reference == null) {
                val pensionMoyenne = masse / retraites
                reference = Triple(cotisants, masse, PENSION_RELATIVE_2025 / pensionMoyenne)
            }
            val (cot0, masse0, echelle) = reference
            val pensionRelative = masse / retraites / salaire * echelle
            val depenses = DEPENSES_PIB_2025 * (masse / salaire / masse0) * (cot0 / cotisants)
            var ressources = when (s.economie.convention) {
                ConventionRessources.COR -> interpoler(RESSOURCES_COR, annee)
                ConventionRessources.TAUX_CONSTANT -> RESSOURCES_COR.first().second
            }
            if (annee >= m.anneeHausseCotisations) ressources += m.hausseCotisations * POINT_COTISATION_PIB
            val pilierEnCours = cap.pilierActif && annee >= cap.anneeDebutPilier
            if (pilierEnCours && cap.substitution) ressources -= cap.pointsPilier * POINT_COTISATION_PIB
            val solde = ressources - depenses
            val pib = PIB_2025_MD_EUROS * salaire * cotisants / cot0

            // ---- Pilier par capitalisation
            var cotisationsCap = 0.0
            var totalRentes = 0.0
            var actifs = 0.0
            if (cap.pilierActif) {
                val qH = demo.quotientsHommes[i]
                val qF = demo.quotientsFemmes[i]
                fun quotient(a: Int): Double {
                    val h = pop.hommes[a]; val f = pop.femmes[a]
                    return if (h + f > 0) (h * qH[a] + f * qF[a]) / (h + f) else qF[a]
                }
                // Valeur actuelle d'une rente de 1 € par an versée à partir de l'âge a (début d'année).
                val facteur = DoubleArray(NB_AGES)
                facteur[AGE_MAX] = 1.0
                for (a in AGE_MAX - 1 downTo 0) facteur[a] = 1.0 + (1.0 - quotient(a)) * facteur[a + 1] / (1.0 + rend)

                val nouveauxComptes = DoubleArray(NB_AGES)
                val nouvellesRentes = DoubleArray(NB_AGES)
                if (i > 0) {
                    val prec = popPrec!!
                    for (a in 1..AGE_MAX) {
                        // Les comptes des personnes décédées restent mutualisés au sein de leur génération.
                        nouveauxComptes[a] = comptes[a - 1] * (1.0 + rend)
                        val qPrec = if (prec.ensemble(a - 1) > 0) {
                            (prec.hommes[a - 1] * demo.quotientsHommes[i - 1][a - 1] + prec.femmes[a - 1] * demo.quotientsFemmes[i - 1][a - 1]) / prec.ensemble(a - 1)
                        } else 1.0
                        nouvellesRentes[a] = rentes[a - 1] * (1.0 - qPrec)
                    }
                    nouvellesRentes[AGE_MAX] += rentes[AGE_MAX] * 0.5
                }
                if (pilierEnCours) {
                    cotisationsCap = cap.pointsPilier * POINT_COTISATION_PIB / 100.0 * pib
                    for (a in 0..AGE_MAX) nouveauxComptes[a] += cotisationsCap * emplois[a] / cotisants
                }
                // Conversion en rente des comptes des personnes qui partent à la retraite dans l'année.
                for (a in 50..AGE_MAX) {
                    if (nouveauxComptes[a] <= 0.0) continue
                    val avant = if (i == 0) partsRetraites[a - 1] else partRetraitesPrec[a - 1]
                    val part = if (a >= 75 || avant >= 1.0 - 1e-9) 1.0
                    else ((partsRetraites[a] - avant) / (1.0 - avant)).coerceIn(0.0, 1.0)
                    val converti = nouveauxComptes[a] * part
                    nouveauxComptes[a] -= converti
                    nouvellesRentes[a] += converti / facteur[a]
                }
                comptes = nouveauxComptes
                rentes = nouvellesRentes
                for (a in 0..AGE_MAX) {
                    totalRentes += rentes[a]
                    actifs += comptes[a] + rentes[a] * (facteur[a] - 1.0)
                }
            }
            val rentesPib = totalRentes / pib * 100.0

            // ---- Fonds de réserve
            var soldeApres = solde
            if (cap.fondsActif) {
                if (i > 0) fonds *= (1.0 + rend)
                val soldeMd = solde / 100.0 * pib
                if (annee >= cap.anneeDebutFonds && annee < cap.anneeDecaissementFonds) fonds += cap.dotationFondsPib / 100.0 * pib
                if (cap.placerExcedents && soldeMd > 0 && annee >= cap.anneeDebutFonds) {
                    fonds += soldeMd
                    soldeApres = 0.0
                } else if (soldeMd < 0 && annee >= cap.anneeDecaissementFonds) {
                    val prelevement = minOf(fonds, -soldeMd)
                    fonds -= prelevement
                    soldeApres = solde + prelevement / pib * 100.0
                }
            }

            res += ResultatAnnee(
                annee = annee,
                population = pop.total(),
                moins20 = pop.effectif(0, 19),
                de20a64 = pop.effectif(20, 64),
                plus65 = pop.effectif(65),
                naissances = demo.naissances[i],
                fecondite = demo.fecondite[i],
                esperanceVieHommes = demo.esperanceVieHommes[i],
                esperanceVieFemmes = demo.esperanceVieFemmes[i],
                rapportDemographique = pop.effectif(20, 64) / pop.effectif(65),
                ageMoyenDepart = age,
                cotisants = cotisants,
                retraites = retraites,
                rapportCotisantsRetraites = cotisants / retraites,
                pensionRelative = pensionRelative,
                depensesPib = depenses,
                ressourcesPib = ressources,
                soldePib = solde,
                soldeMdEuros = solde / 100.0 * pib,
                pibMdEuros = pib,
                cotisationsCapitalisationPib = cotisationsCap / pib * 100.0,
                actifsCapitalisationPib = actifs / pib * 100.0,
                rentesCapitalisationPib = rentesPib,
                pensionRelativeTotale = pensionRelative * (1.0 + rentesPib / depenses),
                fondsReservePib = if (cap.fondsActif) fonds / pib * 100.0 else 0.0,
                fondsReserveMdEuros = if (cap.fondsActif) fonds else 0.0,
                soldeApresFondsPib = soldeApres,
            )
            partRetraitesPrec = partsRetraites
            nbRetraitesPrec = nbRetraites
            pensionsPrec = pensions
            popPrec = pop
        }
        return Projection(s, res, demo)
    }
}
