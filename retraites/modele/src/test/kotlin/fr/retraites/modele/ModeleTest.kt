package fr.retraites.modele

import kotlin.math.abs
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

class ModeleTest {
    private val reference = ModeleRetraites.projeter(Scenario())

    @Test
    fun pyramideInitialeCaleeSurInsee() {
        val p = PyramideInitiale.pyramide
        assertEquals(68.6e6, p.total(), 1e3)
        assertEquals(0.237, p.effectif(0, 19) / p.total(), 1e-3)
        assertEquals(0.218, p.effectif(65) / p.total(), 1e-3)
        assertEquals(0.106, p.effectif(75) / p.total(), 1e-3)
        // Le creux des naissances de 1915-1919 doit être visible (âges 105-110).
        assertTrue(p.ensemble(80) > p.ensemble(85))
    }

    @Test
    fun esperancesDeVieAtteintes() {
        val d = reference.demographie
        assertEquals(80.3, d.esperanceVieHommes.first(), 0.01)
        assertEquals(89.5, d.esperanceVieFemmes.last(), 0.01)
        // Espérance de vie à 65 ans en 2070 (Insee 2026 : 24,4 ans H, 26,3 ans F).
        assertEquals(24.4, d.esperanceVie65Hommes.last(), 0.5)
        assertEquals(26.3, d.esperanceVie65Femmes.last(), 0.5)
    }

    @Test
    fun soldeMigratoireEtNaissancesCoherents() {
        val sansMigration = ModeleDemographique.projeter(HypothesesDemographiques(soldeMigratoire = 0.0))
        val avec = ModeleDemographique.projeter(HypothesesDemographiques())
        val ecart = avec.pyramide(2026).total() - sansMigration.pyramide(2026).total()
        assertEquals(150_000.0, ecart, 2_000.0)
        assertTrue(avec.naissances[0] in 600_000.0..680_000.0)
    }

    @Test
    fun scenarioDeReferenceProcheDuCor() {
        val r2025 = reference.en(2025)
        assertEquals(14.1, r2025.depensesPib, 1e-9)
        assertEquals(-0.2, r2025.soldePib, 1e-9)
        assertEquals(2.5, r2025.rapportDemographique, 0.05)
        assertEquals(15.3, reference.en(2070).depensesPib, 0.3)
        assertEquals(14.2, reference.en(2045).depensesPib, 0.4)
        assertEquals(-2.4, reference.en(2070).soldePib, 0.3)
        assertEquals(1.6, reference.en(2070).rapportDemographique, 0.25)
    }

    @Test
    fun sensibiliteProductiviteProcheDuCor() {
        val bas = ModeleRetraites.projeter(Scenario(economie = HypothesesEconomiques(productivite = 0.004))).en(2070)
        val haut = ModeleRetraites.projeter(Scenario(economie = HypothesesEconomiques(productivite = 0.010))).en(2070)
        assertEquals(16.1, bas.depensesPib, 0.3)
        assertEquals(14.5, haut.depensesPib, 0.3)
    }

    @Test
    fun varianteFeconditeBasseDegradeLeSoldeDUnPoint() {
        val r = ModeleRetraites.projeter(Scenario(demographie = Catalogue.demographie[1].valeur)).en(2070)
        val ecart = r.soldePib - reference.en(2070).soldePib
        assertTrue(abs(ecart + 1.0) < 0.35, "écart obtenu : $ecart")
    }

    @Test
    fun mesuresAgissentDansLeBonSens() {
        val base = reference.en(2070).soldePib
        fun solde(m: Mesures) = ModeleRetraites.projeter(Scenario(mesures = m)).en(2070).soldePib
        assertTrue(solde(Mesures(decalageAge = 1.0)) > base)
        assertTrue(solde(Mesures(decalageAge = -1.0)) < base)
        assertTrue(solde(Mesures(sousIndexation = 0.01, dureeSousIndexation = 5)) > base)
        assertTrue(solde(Mesures(indexationSalaires = true)) < base)
        assertEquals(base + ModeleRetraites.POINT_COTISATION_PIB, solde(Mesures(hausseCotisations = 1.0)), 1e-9)
        // Une mesure ne change rien avant son entrée en vigueur.
        val avant = ModeleRetraites.projeter(Scenario(mesures = Mesures(decalageAge = 2.0, anneeDebutAge = 2040))).en(2035)
        assertEquals(reference.en(2035).soldePib, avant.soldePib, 1e-12)
    }

    @Test
    fun leviersEquilibrentLeSysteme() {
        val l = Equilibre.leviers(Scenario())
        assertEquals(-l.soldePib / 0.23, l.pointsCotisation, 1e-9)
        val age = assertNotNull(l.decalageAge)
        val equilibre = ModeleRetraites.projeter(Scenario(mesures = Mesures(decalageAge = age))).en(2070)
        assertEquals(0.0, equilibre.soldePib, 1e-3)
        assertTrue(l.variationPensionsPourcent < 0)
    }

    @Test
    fun lectureCsv() {
        val csv = buildString {
            append("Âge;Hommes;Femmes\n")
            for (a in 0..100) append(if (a == 100) "100+" else a.toString()).append(";1 000;1000,5\n")
        }
        val p = PyramideInitiale.lireCsv(csv)
        assertEquals(1000.0, p.hommes[3])
        assertEquals(1000.5, p.femmes[100])
        assertEquals(101 * 2000.5, p.total(), 1e-6)
    }

    @Test
    fun exportCsv() {
        val texte = ExportCsv.projections(reference, reference)
        val lignes = texte.trim().lines()
        assertEquals(ANNEE_FIN - ANNEE_DEBUT + 2, lignes.size)
        assertTrue(lignes[1].startsWith("2025;"))
        assertEquals(lignes[0].split(';').size, lignes[1].split(';').size)
    }
}
