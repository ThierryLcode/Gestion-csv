package fr.retraites.modele

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class VariantesTest {
    private val v = Variantes.demographie.associateBy { it.libelle }
    private val e = Variantes.economie.associateBy { it.libelle }
    private val tout = Variantes.demographie

    private fun actives(h: HypothesesDemographiques) = tout.filter { it.estActiveParmi(h, tout) }.map { it.libelle }.toSet()

    @Test
    fun variantesCompatiblesSeCumulent() {
        var h = HypothesesDemographiques()
        h = v.getValue("Fécondité basse (1,2)").basculer(h, tout)
        h = v.getValue("Solde migratoire haut (+230 000)").basculer(h, tout)
        h = v.getValue("Espérance de vie stable").basculer(h, tout)
        assertEquals(setOf("Fécondité basse (1,2)", "Solde migratoire haut (+230 000)", "Espérance de vie stable"), actives(h))
        assertEquals(1.2, h.fecondite)
        assertEquals(230_000.0, h.soldeMigratoire)
        assertEquals(80.3, h.esperanceVieHommes2070)
    }

    @Test
    fun varianteIncompatibleDeselectionneLAutre() {
        var h = v.getValue("Fécondité basse (1,2)").basculer(HypothesesDemographiques(), tout)
        h = v.getValue("Solde migratoire bas (+70 000)").basculer(h, tout)
        h = v.getValue("Fécondité haute (1,7)").basculer(h, tout)
        assertEquals(setOf("Fécondité haute (1,7)", "Solde migratoire bas (+70 000)"), actives(h))
    }

    @Test
    fun anciennesHypothesesRemplacentLesVariantesConcernees() {
        var h = v.getValue("Fécondité basse (1,2)").basculer(HypothesesDemographiques(), tout)
        h = v.getValue("Espérance de vie plus forte (+2 ans)").basculer(h, tout)
        h = v.getValue("Anciennes hypothèses Insee 2021").basculer(h, tout)
        assertEquals(setOf("Anciennes hypothèses Insee 2021"), actives(h))
        // Choisir ensuite une variante de fécondité désactive les anciennes hypothèses
        // et remet migration et espérance de vie à la référence.
        h = v.getValue("Fécondité haute (1,7)").basculer(h, tout)
        assertEquals(setOf("Fécondité haute (1,7)"), actives(h))
        assertEquals(HypothesesDemographiques().soldeMigratoire, h.soldeMigratoire)
        assertEquals(HypothesesDemographiques().esperanceVieFemmes2070, h.esperanceVieFemmes2070)
    }

    @Test
    fun soldeBasApresInsee2021NeGardeQueLaMigration() {
        var h = v.getValue("Anciennes hypothèses Insee 2021").basculer(HypothesesDemographiques(), tout)
        h = v.getValue("Solde migratoire bas (+70 000)").basculer(h, tout)
        assertEquals(setOf("Solde migratoire bas (+70 000)"), actives(h))
        assertEquals(HypothesesDemographiques(soldeMigratoire = 70_000.0), h)
    }

    @Test
    fun deselectionRevientALaReference() {
        val variante = v.getValue("Solde migratoire bas (+70 000)")
        val h = variante.basculer(variante.basculer(HypothesesDemographiques(), tout), tout)
        assertEquals(HypothesesDemographiques(), h)
        assertFalse(variante.estActive(h))
    }

    @Test
    fun variantesEconomiquesCumulables() {
        val toutes = Variantes.economie
        var h = HypothesesEconomiques(convention = ConventionRessources.TAUX_CONSTANT)
        h = e.getValue("Productivité 0,4 %").basculer(h, toutes)
        h = e.getValue("Chômage 10 %").basculer(h, toutes)
        h = e.getValue("Productivité 1,0 %").basculer(h, toutes)
        assertEquals(0.010, h.productivite)
        assertEquals(0.10, h.chomage)
        assertEquals(ConventionRessources.TAUX_CONSTANT, h.convention)
        assertTrue(toutes.filter { it.estActive(h) }.map { it.libelle }.toSet() == setOf("Productivité 1,0 %", "Chômage 10 %"))
    }
}
