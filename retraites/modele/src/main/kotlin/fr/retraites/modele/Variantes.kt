package fr.retraites.modele

/** Dimensions sur lesquelles portent les variantes d'hypothèses. */
enum class Dimension { FECONDITE, MIGRATION, ESPERANCE_VIE, PRODUCTIVITE, CHOMAGE }

/**
 * Variante d'hypothèses cumulable : elle ne modifie que ses [dimensions], en reprenant les
 * valeurs de [cible]. Deux variantes portant sur une même dimension sont incompatibles :
 * activer l'une désactive l'autre (dont les dimensions reviennent d'abord à la référence).
 */
class Variante<T>(
    val libelle: String,
    val detail: String,
    val dimensions: Set<Dimension>,
    val cible: T,
    private val reference: T,
    private val lire: (T, Dimension) -> Any,
    private val copier: (T, Dimension, T) -> T,
) {
    /** La variante est active si toutes ses dimensions ont les valeurs de la variante. */
    fun estActive(h: T): Boolean = dimensions.all { lire(h, it) == lire(cible, it) }

    /**
     * Active parmi [toutes] : ses valeurs sont en place et aucune variante plus large
     * (portant sur toutes ses dimensions et d'autres) n'est active.
     * Ex. : les anciennes hypothèses Insee 2021 incluent un solde de +70 000 ; seule la
     * variante Insee 2021 est alors considérée comme active.
     */
    fun estActiveParmi(h: T, toutes: List<Variante<T>>): Boolean =
        estActive(h) && toutes.none { it !== this && it.dimensions.containsAll(dimensions) && it.dimensions.size > dimensions.size && it.estActive(h) }

    fun estCompatibleAvec(autre: Variante<T>): Boolean = (dimensions intersect autre.dimensions).isEmpty()

    /** Remet les dimensions de la variante aux valeurs de référence. */
    fun retirer(h: T): T = dimensions.fold(h) { r, d -> copier(r, d, reference) }

    /** Active la variante en désactivant d'abord les variantes actives incompatibles. */
    fun activer(h: T, toutes: List<Variante<T>>): T {
        var r = h
        for (v in toutes) if (v !== this && !estCompatibleAvec(v) && v.estActive(h)) r = v.retirer(r)
        return dimensions.fold(r) { acc, d -> copier(acc, d, cible) }
    }

    /** Bascule : active si inactive, retire sinon. */
    fun basculer(h: T, toutes: List<Variante<T>>): T = if (estActiveParmi(h, toutes)) retirer(h) else activer(h, toutes)
}

object Variantes {
    private val refDemo = HypothesesDemographiques()
    private val refEco = HypothesesEconomiques()

    private fun lireDemo(h: HypothesesDemographiques, d: Dimension): Any = when (d) {
        Dimension.FECONDITE -> h.fecondite to h.anneeCibleFecondite
        Dimension.MIGRATION -> h.soldeMigratoire
        Dimension.ESPERANCE_VIE -> h.esperanceVieHommes2070 to h.esperanceVieFemmes2070
        else -> Unit
    }

    private fun copierDemo(h: HypothesesDemographiques, d: Dimension, s: HypothesesDemographiques) = when (d) {
        Dimension.FECONDITE -> h.copy(fecondite = s.fecondite, anneeCibleFecondite = s.anneeCibleFecondite)
        Dimension.MIGRATION -> h.copy(soldeMigratoire = s.soldeMigratoire)
        Dimension.ESPERANCE_VIE -> h.copy(esperanceVieHommes2070 = s.esperanceVieHommes2070, esperanceVieFemmes2070 = s.esperanceVieFemmes2070)
        else -> h
    }

    private fun lireEco(h: HypothesesEconomiques, d: Dimension): Any = when (d) {
        Dimension.PRODUCTIVITE -> h.productivite
        Dimension.CHOMAGE -> h.chomage
        else -> Unit
    }

    private fun copierEco(h: HypothesesEconomiques, d: Dimension, s: HypothesesEconomiques) = when (d) {
        Dimension.PRODUCTIVITE -> h.copy(productivite = s.productivite)
        Dimension.CHOMAGE -> h.copy(chomage = s.chomage)
        else -> h
    }

    private fun demo(libelle: String, detail: String, dims: Set<Dimension>, cible: HypothesesDemographiques) =
        Variante(libelle, detail, dims, cible, refDemo, ::lireDemo, ::copierDemo)

    private fun eco(libelle: String, detail: String, dims: Set<Dimension>, cible: HypothesesEconomiques) =
        Variante(libelle, detail, dims, cible, refEco, ::lireEco, ::copierEco)

    val demographie: List<Variante<HypothesesDemographiques>> = listOf(
        demo("Fécondité basse (1,2)", "Variante du COR 2026 : 1,2 enfant par femme en 2030", setOf(Dimension.FECONDITE), HypothesesDemographiques(fecondite = 1.2, anneeCibleFecondite = 2030)),
        demo("Fécondité haute (1,7)", "Variante du COR 2026 : 1,7 enfant par femme en 2030", setOf(Dimension.FECONDITE), HypothesesDemographiques(fecondite = 1.7, anneeCibleFecondite = 2030)),
        demo("Solde migratoire bas (+70 000)", "Variante basse de l'Insee 2026", setOf(Dimension.MIGRATION), HypothesesDemographiques(soldeMigratoire = 70_000.0)),
        demo("Solde migratoire haut (+230 000)", "Variante haute de l'Insee 2026", setOf(Dimension.MIGRATION), HypothesesDemographiques(soldeMigratoire = 230_000.0)),
        demo("Espérance de vie stable", "Variante de travail Insee : mortalité figée au niveau de 2025", setOf(Dimension.ESPERANCE_VIE), HypothesesDemographiques(esperanceVieHommes2070 = 80.3, esperanceVieFemmes2070 = 85.9)),
        demo("Espérance de vie plus forte (+2 ans)", "Illustratif : +2 ans en 2070 par rapport au scénario central", setOf(Dimension.ESPERANCE_VIE), HypothesesDemographiques(esperanceVieHommes2070 = 88.7, esperanceVieFemmes2070 = 91.5)),
        demo(
            "Anciennes hypothèses Insee 2021",
            "Fécondité 1,8 ; solde migratoire +70 000 ; espérance de vie 2070 : 87,5 ans (H) et 90,0 ans (F) — rapports COR jusqu'en 2025. Remplace les variantes de fécondité, de migration et d'espérance de vie.",
            setOf(Dimension.FECONDITE, Dimension.MIGRATION, Dimension.ESPERANCE_VIE),
            HypothesesDemographiques(fecondite = 1.8, soldeMigratoire = 70_000.0, esperanceVieHommes2070 = 87.5, esperanceVieFemmes2070 = 90.0),
        ),
    )

    val economie: List<Variante<HypothesesEconomiques>> = listOf(
        eco("Productivité 0,4 %", "Variante basse du COR 2026", setOf(Dimension.PRODUCTIVITE), HypothesesEconomiques(productivite = 0.004)),
        eco("Productivité 1,0 %", "Variante haute du COR 2026", setOf(Dimension.PRODUCTIVITE), HypothesesEconomiques(productivite = 0.010)),
        eco("Chômage 4,5 %", "Plein emploi (variante illustrative)", setOf(Dimension.CHOMAGE), HypothesesEconomiques(chomage = 0.045)),
        eco("Chômage 10 %", "Variante illustrative", setOf(Dimension.CHOMAGE), HypothesesEconomiques(chomage = 0.10)),
    )
}
