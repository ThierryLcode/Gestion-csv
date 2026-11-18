package fr.retraites.modele

import kotlin.math.pow
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

class CapitalisationTest {
    private val reference = ModeleRetraites.projeter(Scenario())
    private fun projeter(c: Capitalisation, m: Mesures = Mesures()) = ModeleRetraites.projeter(Scenario(mesures = m, capitalisation = c))

    @Test
    fun sansCapitalisationRienNeChange() {
        for (r in reference.annees) {
            assertEquals(0.0, r.actifsCapitalisationPib)
            assertEquals(0.0, r.rentesCapitalisationPib)
            assertEquals(0.0, r.fondsReservePib)
            assertEquals(r.soldePib, r.soldeApresFondsPib)
            assertEquals(r.pensionRelative, r.pensionRelativeTotale)
        }
        // Le rendement seul n'a aucun effet.
        val p = projeter(Capitalisation(rendementReel = 0.05))
        assertEquals(reference.en(2070).soldePib, p.en(2070).soldePib, 1e-12)
    }

    @Test
    fun pilierEnComplementNeChangePasLaRepartition() {
        val p = projeter(Capitalisation(pointsPilier = 2.0))
        for (a in listOf(2026, 2040, 2070)) assertEquals(reference.en(a).soldePib, p.en(a).soldePib, 1e-12)
        assertEquals(0.0, p.en(2026).cotisationsCapitalisationPib)
        assertEquals(2 * ModeleRetraites.POINT_COTISATION_PIB, p.en(2030).cotisationsCapitalisationPib, 1e-9)
        val r2070 = p.en(2070)
        assertTrue(r2070.actifsCapitalisationPib > 10.0, "actifs : ${r2070.actifsCapitalisationPib}")
        assertTrue(r2070.rentesCapitalisationPib > 0.1, "rentes : ${r2070.rentesCapitalisationPib}")
        assertTrue(r2070.pensionRelativeTotale > r2070.pensionRelative)
        // Un meilleur rendement donne plus de rentes.
        val haut = projeter(Capitalisation(pointsPilier = 2.0, rendementReel = 0.04)).en(2070)
        assertTrue(haut.rentesCapitalisationPib > r2070.rentesCapitalisationPib)
        // Les premières rentes apparaissent progressivement.
        assertTrue(p.en(2030).rentesCapitalisationPib < p.en(2050).rentesCapitalisationPib)
    }

    @Test
    fun substitutionCreeUnCoutDeTransition() {
        val p = projeter(Capitalisation(pointsPilier = 2.0, substitution = true))
        val perte = 2 * ModeleRetraites.POINT_COTISATION_PIB
        // Au début, la répartition perd les cotisations sans réduire encore les pensions.
        assertEquals(reference.en(2028).soldePib - perte, p.en(2028).soldePib, 0.02)
        // Plus tard, la baisse des droits allège les dépenses de répartition.
        assertTrue(p.en(2070).depensesPib < reference.en(2070).depensesPib)
        assertTrue(p.en(2070).soldePib - reference.en(2070).soldePib > p.en(2028).soldePib - reference.en(2028).soldePib)
    }

    @Test
    fun fondsSansDecaissementCapitaliseSimplement() {
        val p = projeter(Capitalisation(fondsActif = true, fondsInitialMdEuros = 100.0, anneeDecaissementFonds = 2100, placerExcedents = false))
        assertEquals(100.0 * 1.02.pow(ANNEE_FIN - ANNEE_DEBUT), p.en(2070).fondsReserveMdEuros, 1e-6)
    }

    @Test
    fun fondsCouvreLesDeficitsPuisSEpuise() {
        val p = projeter(Capitalisation(fondsActif = true, fondsInitialMdEuros = 100.0, anneeDecaissementFonds = 2030))
        for (r in p.annees) {
            assertTrue(r.fondsReserveMdEuros >= -1e-9)
            assertTrue(r.soldeApresFondsPib >= r.soldePib - 1e-12)
        }
        // 2030 : le fonds couvre entièrement le déficit.
        assertEquals(0.0, p.en(2030).soldeApresFondsPib, 1e-9)
        val epuisement = assertNotNull(p.anneeEpuisementFonds)
        assertTrue(epuisement in 2031..2070)
        assertEquals(p.en(2070).soldePib, p.en(2070).soldeApresFondsPib, 1e-12)
        assertNull(reference.anneeEpuisementFonds)
    }

    @Test
    fun dotationsAlimententLeFonds() {
        val c = Capitalisation(fondsActif = true, dotationFondsPib = 0.2, anneeDebutFonds = 2027, anneeDecaissementFonds = 2040)
        val p = projeter(c)
        assertEquals(0.0, p.en(2026).fondsReserveMdEuros)
        val f2039 = p.en(2039).fondsReservePib
        assertTrue(f2039 > 13 * 0.2 && f2039 < 13 * 0.2 * 1.3, "fonds 2039 : $f2039")
        assertTrue(p.en(2040).fondsReserveMdEuros < p.en(2039).fondsReserveMdEuros * 1.02)
    }

    @Test
    fun scenarioSansMesuresRetireLaCapitalisation() {
        val s = Scenario(mesures = Mesures(hausseCotisations = 1.0), capitalisation = Capitalisation(pointsPilier = 1.0, rendementReel = 0.03))
        val sans = s.sansMesures()
        assertTrue(sans.mesures.estVide && sans.capitalisation.estVide)
        assertEquals(0.03, sans.capitalisation.rendementReel)
    }
}
