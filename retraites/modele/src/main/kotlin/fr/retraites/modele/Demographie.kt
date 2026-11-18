package fr.retraites.modele

import kotlin.math.exp
import kotlin.math.max
import kotlin.math.pow

/** Âge maximal suivi ; le dernier âge est un groupe ouvert (110 ans et plus). */
const val AGE_MAX = 110
const val NB_AGES = AGE_MAX + 1

enum class Sexe { HOMMES, FEMMES }

/** Population par âge détaillé (0 à 110 ans et plus) pour chaque sexe. */
class Pyramide(val hommes: DoubleArray, val femmes: DoubleArray) {
    init {
        require(hommes.size == NB_AGES && femmes.size == NB_AGES) { "La pyramide doit couvrir les âges 0 à $AGE_MAX" }
    }

    fun total(): Double = hommes.sum() + femmes.sum()

    /** Effectif des âges [debut, fin] inclus, deux sexes. */
    fun effectif(debut: Int, fin: Int = AGE_MAX): Double {
        var s = 0.0
        for (a in debut.coerceAtLeast(0)..fin.coerceAtMost(AGE_MAX)) s += hommes[a] + femmes[a]
        return s
    }

    fun ensemble(a: Int): Double = hommes[a] + femmes[a]

    fun copie() = Pyramide(hommes.copyOf(), femmes.copyOf())
}

/**
 * Mortalité paramétrique : quotients de mortalité par âge obtenus à partir d'un barème de
 * forces de mortalité (mortalité infantile + accidents des jeunes adultes + loi de Gompertz)
 * multiplié par un coefficient ajusté pour atteindre une espérance de vie à la naissance visée.
 */
object Mortalite {
    private fun forceBase(sexe: Sexe, age: Int): Double {
        val x = age + 0.5
        return when (sexe) {
            Sexe.HOMMES -> 0.0040 * exp(-1.6 * x) + 0.00045 * exp(-((x - 22.0) / 9.0).pow(2)) +
                2.2e-5 * exp(0.0955 * x)
            Sexe.FEMMES -> 0.0033 * exp(-1.6 * x) + 0.00015 * exp(-((x - 22.0) / 9.0).pow(2)) +
                5.0e-6 * exp(0.1075 * x)
        }
    }

    /** Quotients de mortalité pour un coefficient multiplicateur donné. */
    fun quotients(sexe: Sexe, coefficient: Double): DoubleArray = DoubleArray(NB_AGES) { a ->
        if (a == AGE_MAX) 1.0 else (1.0 - exp(-coefficient * forceBase(sexe, a))).coerceIn(0.0, 1.0)
    }

    /** Espérance de vie à l'âge [age] d'une table du moment. */
    fun esperanceVie(q: DoubleArray, age: Int = 0): Double {
        var l = 1.0
        var e = 0.0
        for (a in age until AGE_MAX) {
            val suivant = l * (1.0 - q[a])
            e += (l + suivant) / 2.0
            l = suivant
        }
        // Groupe ouvert : durée moyenne restante d'environ 1,5 an.
        return e + l * 1.5
    }

    /** Coefficient multiplicateur donnant l'espérance de vie à la naissance visée. */
    fun coefficientPour(sexe: Sexe, e0: Double): Double {
        var bas = 0.01
        var haut = 20.0
        repeat(80) {
            val milieu = (bas + haut) / 2.0
            // Plus le coefficient est élevé, plus l'espérance de vie est faible.
            if (esperanceVie(quotients(sexe, milieu)) > e0) bas = milieu else haut = milieu
        }
        return (bas + haut) / 2.0
    }

    fun quotientsPour(sexe: Sexe, e0: Double): DoubleArray = quotients(sexe, coefficientPour(sexe, e0))
}

/** Calendrier de fécondité par âge de la mère (15 à 49 ans), de somme 1 ; âge moyen ≈ 31 ans. */
object Fecondite {
    val profil: DoubleArray by lazy {
        val p = DoubleArray(NB_AGES)
        val forme = 9.0
        val echelle = 1.85
        for (a in 15..49) {
            val x = a + 0.5 - 14.5
            p[a] = x.pow(forme - 1) * exp(-x / echelle)
        }
        val s = p.sum()
        for (a in p.indices) p[a] /= s
        p
    }

    /** Proportion de garçons parmi les naissances. */
    const val PART_GARCONS = 0.512
}

/** Répartition par âge du solde migratoire (somme 1), concentrée entre 18 et 35 ans. */
object ProfilMigratoire {
    val profil: DoubleArray by lazy {
        val p = DoubleArray(NB_AGES) { a ->
            if (a >= 75) 0.0
            else exp(-((a - 26.0) / 8.0).pow(2)) + 0.30 * exp(-((a - 6.0) / 6.0).pow(2)) +
                0.10 * exp(-((a - 45.0) / 12.0).pow(2))
        }
        val s = p.sum()
        for (a in p.indices) p[a] /= s
        p
    }
}

/** Interpolation linéaire entre points (année, valeur), constante hors de l'intervalle. */
fun interpoler(points: List<Pair<Int, Double>>, annee: Int): Double {
    if (annee <= points.first().first) return points.first().second
    if (annee >= points.last().first) return points.last().second
    for (i in 0 until points.size - 1) {
        val (a0, v0) = points[i]
        val (a1, v1) = points[i + 1]
        if (annee in a0..a1) return v0 + (v1 - v0) * (annee - a0).toDouble() / (a1 - a0)
    }
    return points.last().second
}

/** Résultat de la projection démographique : une pyramide par année (au 1er janvier). */
class ProjectionDemographique(
    val pyramides: List<Pyramide>,
    val naissances: DoubleArray,
    val fecondite: DoubleArray,
    val esperanceVieHommes: DoubleArray,
    val esperanceVieFemmes: DoubleArray,
    val esperanceVie65Hommes: DoubleArray,
    val esperanceVie65Femmes: DoubleArray,
    /** Quotients de mortalité de chaque année, par âge. */
    val quotientsHommes: List<DoubleArray>,
    val quotientsFemmes: List<DoubleArray>,
) {
    fun pyramide(annee: Int): Pyramide = pyramides[annee - ANNEE_DEBUT]
}

/** Modèle par composantes (naissances, décès, migrations), par sexe et âge détaillé. */
object ModeleDemographique {

    fun feconditeEn(h: HypothesesDemographiques, annee: Int): Double {
        val depart = HypothesesDemographiques.FECONDITE_2025
        if (annee >= h.anneeCibleFecondite) return h.fecondite
        val duree = max(1, h.anneeCibleFecondite - ANNEE_DEBUT)
        return depart + (h.fecondite - depart) * (annee - ANNEE_DEBUT).toDouble() / duree
    }

    fun esperanceVieEn(h: HypothesesDemographiques, sexe: Sexe, annee: Int): Double {
        val (depart, arrivee) = when (sexe) {
            Sexe.HOMMES -> HypothesesDemographiques.ESPERANCE_VIE_HOMMES_2025 to h.esperanceVieHommes2070
            Sexe.FEMMES -> HypothesesDemographiques.ESPERANCE_VIE_FEMMES_2025 to h.esperanceVieFemmes2070
        }
        val t = ((annee - ANNEE_DEBUT).toDouble() / (ANNEE_FIN - ANNEE_DEBUT)).coerceIn(0.0, 1.0)
        // Gains plus rapides au début qu'en fin de période (ralentissement progressif).
        val forme = 1.0 - (1.0 - t).pow(1.3)
        return depart + (arrivee - depart) * forme
    }

    fun projeter(h: HypothesesDemographiques, initiale: Pyramide = PyramideInitiale.pyramide): ProjectionDemographique {
        val nbAnnees = ANNEE_FIN - ANNEE_DEBUT + 1
        val pyramides = ArrayList<Pyramide>(nbAnnees)
        val naissances = DoubleArray(nbAnnees)
        val icf = DoubleArray(nbAnnees)
        val e0h = DoubleArray(nbAnnees)
        val e0f = DoubleArray(nbAnnees)
        val e65h = DoubleArray(nbAnnees)
        val e65f = DoubleArray(nbAnnees)
        val quotientsH = ArrayList<DoubleArray>(nbAnnees)
        val quotientsF = ArrayList<DoubleArray>(nbAnnees)
        var courante = initiale.copie()
        pyramides.add(courante)
        val profilF = Fecondite.profil
        val profilM = ProfilMigratoire.profil

        for (i in 0 until nbAnnees) {
            val annee = ANNEE_DEBUT + i
            val qH = Mortalite.quotientsPour(Sexe.HOMMES, esperanceVieEn(h, Sexe.HOMMES, annee))
            val qF = Mortalite.quotientsPour(Sexe.FEMMES, esperanceVieEn(h, Sexe.FEMMES, annee))
            quotientsH.add(qH)
            quotientsF.add(qF)
            e0h[i] = Mortalite.esperanceVie(qH)
            e0f[i] = Mortalite.esperanceVie(qF)
            e65h[i] = Mortalite.esperanceVie(qH, 65)
            e65f[i] = Mortalite.esperanceVie(qF, 65)
            icf[i] = feconditeEn(h, annee)

            // Naissances de l'année : femmes en moyenne sur l'année (approximation : début d'année).
            var n = 0.0
            for (a in 15..49) n += courante.femmes[a] * profilF[a]
            n *= icf[i]
            naissances[i] = n
            if (i == nbAnnees - 1) break

            val hommes = DoubleArray(NB_AGES)
            val femmes = DoubleArray(NB_AGES)
            val migrantsParSexe = h.soldeMigratoire / 2.0
            hommes[0] = n * Fecondite.PART_GARCONS * (1.0 - 0.9 * qH[0])
            femmes[0] = n * (1 - Fecondite.PART_GARCONS) * (1.0 - 0.9 * qF[0])
            for (a in 0 until AGE_MAX) {
                val cible = a + 1
                // Les migrants de l'année sont exposés à la moitié du risque de décès.
                hommes[cible] += courante.hommes[a] * (1.0 - qH[a]) + migrantsParSexe * profilM[a] * (1.0 - qH[a] / 2)
                femmes[cible] += courante.femmes[a] * (1.0 - qF[a]) + migrantsParSexe * profilM[a] * (1.0 - qF[a] / 2)
            }
            // Groupe ouvert : les survivants de 110 ans et plus y restent.
            hommes[AGE_MAX] += courante.hommes[AGE_MAX] * 0.5
            femmes[AGE_MAX] += courante.femmes[AGE_MAX] * 0.5
            courante = Pyramide(hommes, femmes)
            pyramides.add(courante)
        }
        return ProjectionDemographique(pyramides, naissances, icf, e0h, e0f, e65h, e65f, quotientsH, quotientsF)
    }
}
