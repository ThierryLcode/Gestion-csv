package fr.retraites.modele

import kotlin.math.abs
import kotlin.math.min

/**
 * Pyramide des âges au 1er janvier 2025, reconstruite à partir des naissances annuelles
 * observées en France (série Insee, arrondie au millier) :
 * survie selon une table de mortalité paramétrique et apport migratoire cumulé, puis calage
 * sur les grands agrégats publiés par l'Insee (population totale et part des 0-19 ans,
 * des 65 ans ou plus et des 75 ans ou plus).
 *
 * C'est une approximation : elle reproduit les grandes masses et les creux et pics de
 * génération (guerres, baby-boom), pas chaque âge détaillé. L'application permet d'importer
 * la pyramide exacte de l'Insee au format CSV.
 */
object PyramideInitiale {

    /** Agrégats Insee au 1er janvier 2025 (France, estimations provisoires arrondies). */
    const val POPULATION_TOTALE = 68_600_000.0
    const val PART_MOINS_20 = 0.237
    const val PART_65_PLUS = 0.218
    const val PART_75_PLUS = 0.106
    const val PART_FEMMES = 0.516

    /** Naissances annuelles (milliers) de 1914 à 2024 ; France métropolitaine jusqu'en 1999, France entière ensuite. */
    private val naissancesMilliers = intArrayOf(
        594, 387, 314, 343, 400, 412, // 1914-1919
        834, 813, 760, 762, 753, 770, 768, 743, 749, 730, // 1920-1929
        750, 734, 722, 683, 677, 640, 630, 616, 612, 613, // 1930-1939
        560, 520, 575, 613, 627, 643, 840, 867, 867, 869, // 1940-1949
        858, 823, 822, 804, 811, 806, 807, 816, 812, 829, // 1950-1959
        816, 834, 830, 868, 874, 862, 860, 837, 836, 840, // 1960-1969
        848, 878, 875, 855, 799, 745, 720, 745, 737, 757, // 1970-1979
        800, 805, 797, 749, 760, 768, 778, 768, 771, 765, // 1980-1989
        762, 759, 744, 712, 711, 730, 734, 727, 738, 745, // 1990-1999
        775, 804, 793, 793, 800, 807, 830, 819, 828, 824, // 2000-2009
        832, 823, 822, 811, 812, 799, 783, 770, 759, 753, // 2010-2019
        735, 742, 726, 678, 663, // 2020-2024
    )
    private const val PREMIERE_ANNEE = 1914

    private fun naissances(annee: Int): Double {
        val i = annee - PREMIERE_ANNEE
        return if (i < 0) 600_000.0 else naissancesMilliers[i] * 1000.0
    }

    /**
     * Reconstitution non calée. [coefMortalite] multiplie les forces de mortalité de 2025,
     * [pente] majore la mortalité passée aux grands âges, [migration] est le surcroît
     * relatif d'effectif apporté par les migrations aux âges adultes.
     */
    private fun reconstituer(coefMortalite: Double, pente: Double, migration: Double): Pyramide {
        val base = DoubleArray(2) { s ->
            Mortalite.coefficientPour(
                if (s == 0) Sexe.HOMMES else Sexe.FEMMES,
                if (s == 0) HypothesesDemographiques.ESPERANCE_VIE_HOMMES_2025 else HypothesesDemographiques.ESPERANCE_VIE_FEMMES_2025,
            )
        }
        val sexes = listOf(Sexe.HOMMES, Sexe.FEMMES)
        val res = sexes.mapIndexed { s, sexe ->
            val q = Mortalite.quotients(sexe, base[s] * coefMortalite)
            val partSexe = if (sexe == Sexe.HOMMES) Fecondite.PART_GARCONS else 1 - Fecondite.PART_GARCONS
            DoubleArray(NB_AGES) { age ->
                val annee = 2024 - age
                var survie = 1.0
                for (a in 0 until age) {
                    val majoration = 1.0 + pente * maxOf(0, a - 60) / 30.0
                    survie *= (1.0 - (q[a] * majoration).coerceAtMost(1.0))
                }
                val apport = 1.0 + migration * min(age, 35) / 35.0
                naissances(annee) * partSexe * survie * apport
            }
        }
        return Pyramide(res[0], res[1])
    }

    private fun ecarts(p: Pyramide): DoubleArray {
        val t = p.total()
        return doubleArrayOf(
            p.effectif(0, 19) / t - PART_MOINS_20,
            p.effectif(65) / t - PART_65_PLUS,
            p.effectif(75) / t - PART_75_PLUS,
        )
    }

    /** Pyramide calée (calculée une seule fois). */
    val pyramide: Pyramide by lazy { caler() }

    private fun caler(): Pyramide {
        // Méthode de Newton sur (coefMortalite, pente, migration).
        val x = doubleArrayOf(1.2, 0.3, 0.10)
        repeat(30) {
            val f = ecarts(reconstituer(x[0], x[1], x[2]))
            if (f.all { abs(it) < 1e-6 }) return@repeat
            val j = Array(3) { DoubleArray(3) }
            for (k in 0..2) {
                val h = 1e-4
                val y = x.copyOf()
                y[k] += h
                val g = ecarts(reconstituer(y[0], y[1], y[2]))
                for (i in 0..2) j[i][k] = (g[i] - f[i]) / h
            }
            val dx = resoudre3(j, f.map { -it }.toDoubleArray())
            for (k in 0..2) x[k] += dx[k]
            x[0] = x[0].coerceIn(0.3, 5.0)
            x[1] = x[1].coerceIn(-0.8, 3.0)
            x[2] = x[2].coerceIn(-0.3, 1.0)
        }
        val brute = reconstituer(x[0], x[1], x[2])
        // Calage sur la population totale et la répartition par sexe.
        val fH = POPULATION_TOTALE * (1 - PART_FEMMES) / brute.hommes.sum()
        val fF = POPULATION_TOTALE * PART_FEMMES / brute.femmes.sum()
        return Pyramide(
            DoubleArray(NB_AGES) { brute.hommes[it] * fH },
            DoubleArray(NB_AGES) { brute.femmes[it] * fF },
        )
    }

    private fun resoudre3(m: Array<DoubleArray>, b: DoubleArray): DoubleArray {
        fun det(a: Array<DoubleArray>) =
            a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) -
                a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) +
                a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
        val d = det(m)
        if (abs(d) < 1e-18) return DoubleArray(3)
        return DoubleArray(3) { c ->
            val mc = Array(3) { i -> DoubleArray(3) { k -> if (k == c) b[i] else m[i][k] } }
            det(mc) / d
        }
    }

    /**
     * Lit une pyramide au format CSV : une ligne par âge avec trois colonnes
     * « âge ; hommes ; femmes » (séparateur « ; », « , » ou tabulation, ligne d'en-tête facultative).
     * L'âge peut être écrit « 100+ » ; les âges au-delà de 110 sont regroupés à 110.
     */
    fun lireCsv(texte: String): Pyramide {
        val hommes = DoubleArray(NB_AGES)
        val femmes = DoubleArray(NB_AGES)
        var lignes = 0
        for (brute in texte.lineSequence()) {
            val ligne = brute.trim().removePrefix("﻿")
            if (ligne.isEmpty()) continue
            val sep = when {
                ';' in ligne -> ';'
                '\t' in ligne -> '\t'
                else -> ','
            }
            val champs = ligne.split(sep).map { it.trim().trim('"') }
            if (champs.size < 3) continue
            val age = champs[0].filter { it.isDigit() }.toIntOrNull() ?: continue
            fun nombre(s: String) = s.replace(" ", "").replace(" ", "").replace(',', '.').toDoubleOrNull()
            val h = nombre(champs[1]) ?: continue
            val f = nombre(champs[2]) ?: continue
            val i = age.coerceAtMost(AGE_MAX)
            hommes[i] += h
            femmes[i] += f
            lignes++
        }
        require(lignes >= 50) { "Le fichier doit contenir au moins 50 lignes « âge ; hommes ; femmes » (trouvé : $lignes)." }
        return Pyramide(hommes, femmes)
    }
}
