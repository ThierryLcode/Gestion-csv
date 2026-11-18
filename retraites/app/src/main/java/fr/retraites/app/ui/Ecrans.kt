package fr.retraites.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import fr.retraites.app.EtatUi
import fr.retraites.app.Resultats
import fr.retraites.modele.ANNEE_DEBUT
import fr.retraites.modele.ANNEE_FIN
import fr.retraites.modele.Catalogue
import fr.retraites.modele.ConventionRessources
import fr.retraites.modele.HypothesesDemographiques
import fr.retraites.modele.HypothesesEconomiques
import fr.retraites.modele.Mesures
import fr.retraites.modele.ModeleRetraites
import fr.retraites.modele.Projection
import fr.retraites.modele.ResultatAnnee

@Composable
private fun Defilement(contenu: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
        contenu()
        Spacer(Modifier.height(24.dp))
    }
}

// ---------------------------------------------------------------- Hypothèses

@Composable
fun EcranHypotheses(
    etat: EtatUi,
    onDemographie: (HypothesesDemographiques) -> Unit,
    onEconomie: (HypothesesEconomiques) -> Unit,
    onImporter: () -> Unit,
    onPyramideParDefaut: () -> Unit,
) = Defilement {
    val d = etat.scenario.demographie
    val e = etat.scenario.economie

    Section("Démographie", "Hypothèses Insee 2026 retenues par le COR (rapport de juin 2026) et leurs variantes. Touchez une variante, puis ajustez si besoin.") {
        Puces(Catalogue.demographie.map { it.libelle to it.valeur }, { it == d }, onDemographie)
        Catalogue.demographie.firstOrNull { it.valeur == d }?.let {
            Text(it.detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Curseur("Fécondité (enfants par femme)", d.fecondite, 1.0, 2.2, 0.05, { nombre(it, 2) }) { onDemographie(d.copy(fecondite = it)) }
        Curseur("Atteinte en", d.anneeCibleFecondite.toDouble(), 2026.0, 2050.0, 1.0, { it.toInt().toString() }) {
            onDemographie(d.copy(anneeCibleFecondite = it.toInt()))
        }
        Curseur("Solde migratoire annuel", d.soldeMigratoire, -50_000.0, 350_000.0, 10_000.0, { signe(it / 1000, 0) + " 000" }) {
            onDemographie(d.copy(soldeMigratoire = it))
        }
        Curseur("Espérance de vie des hommes en 2070", d.esperanceVieHommes2070, 78.0, 93.0, 0.1, { nombre(it) + " ans" }) {
            onDemographie(d.copy(esperanceVieHommes2070 = it))
        }
        Curseur("Espérance de vie des femmes en 2070", d.esperanceVieFemmes2070, 84.0, 96.0, 0.1, { nombre(it) + " ans" }) {
            onDemographie(d.copy(esperanceVieFemmes2070 = it))
        }
        Text(
            "Niveaux de départ (2025) : fécondité 1,56 ; espérance de vie 80,3 ans (H) et 85,9 ans (F).",
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    Section("Économie") {
        Puces(Catalogue.economie.map { it.libelle to it.valeur }, { it == e }, onEconomie)
        Curseur("Gains de productivité du travail", e.productivite * 100, 0.0, 2.0, 0.1, { pourcent(it) + " / an" }) {
            onEconomie(e.copy(productivite = it / 100))
        }
        Curseur("Taux de chômage de long terme (atteint en 2032)", e.chomage * 100, 3.0, 12.0, 0.5, { pourcent(it) }) {
            onEconomie(e.copy(chomage = it / 100))
        }
        Text("Ressources du système", style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
        ConventionRessources.entries.forEach { c ->
            Row(
                Modifier.fillMaxWidth().selectable(selected = e.convention == c, onClick = { onEconomie(e.copy(convention = c)) }),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RadioButton(selected = e.convention == c, onClick = { onEconomie(e.copy(convention = c)) })
                Text(c.libelle, style = MaterialTheme.typography.bodyMedium)
            }
        }
        Text(
            "Trajectoire COR : les ressources passent de 13,9 % du PIB en 2025 à 12,9 % en 2070. Taux constant : 13,9 % sur toute la période.",
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    Section(
        "Pyramide des âges de départ",
        etat.pyramideImportee?.let { "Pyramide importée : $it" }
            ?: "Pyramide intégrée : reconstruite à partir des naissances depuis 1914 et calée sur les agrégats Insee au 1er janvier 2025.",
    ) {
        Text(
            "Vous pouvez importer la pyramide exacte de l'Insee : fichier CSV avec une ligne par âge « âge ; hommes ; femmes ».",
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = onImporter) { Text("Importer un CSV") }
            if (etat.pyramideImportee != null) OutlinedButton(onClick = onPyramideParDefaut) { Text("Pyramide intégrée") }
        }
    }
}

// ---------------------------------------------------------------- Mesures

@Composable
fun EcranMesures(etat: EtatUi, onMesures: (Mesures) -> Unit) = Defilement {
    val m = etat.scenario.mesures

    Section("Mesures types", "Point de départ : touchez une mesure, puis combinez-la avec les réglages ci-dessous.") {
        Puces(Catalogue.mesures.map { it.libelle to it.valeur }, { it == m }, onMesures)
        Catalogue.mesures.firstOrNull { it.valeur == m }?.let {
            Text(it.detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }

    Section("Âge de départ", "Décalage de l'âge moyen effectif de départ par rapport à la législation actuelle (≈ 64,6 ans à terme).") {
        Curseur("Décalage", m.decalageAge, -3.0, 4.0, 0.1, { signe(it) + " an" }) { onMesures(m.copy(decalageAge = it)) }
        Curseur("À partir de", m.anneeDebutAge.toDouble(), 2026.0, 2045.0, 1.0, { it.toInt().toString() }) {
            onMesures(m.copy(anneeDebutAge = it.toInt()))
        }
        Text("Montée en charge", style = MaterialTheme.typography.bodyMedium)
        Puces(listOf("Immédiate" to 0.0, "3 mois par an" to 0.25, "6 mois par an" to 0.5), { it == m.rythmeAge }) {
            onMesures(m.copy(rythmeAge = it))
        }
    }

    Section("Revalorisation des pensions", "Par défaut, les pensions en cours de service suivent l'inflation.") {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("Indexer sur les salaires", Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
            Switch(checked = m.indexationSalaires, onCheckedChange = { onMesures(m.copy(indexationSalaires = it)) })
        }
        Curseur("Sous-indexation (inflation moins…)", m.sousIndexation * 100, 0.0, 3.0, 0.1, { nombre(it) + " pt" }) {
            onMesures(m.copy(sousIndexation = it / 100))
        }
        Curseur("À partir de", m.anneeDebutSousIndexation.toDouble(), 2026.0, 2045.0, 1.0, { it.toInt().toString() }) {
            onMesures(m.copy(anneeDebutSousIndexation = it.toInt()))
        }
        Curseur("Durée", m.dureeSousIndexation.toDouble(), 0.0, 20.0, 1.0, { if (it < 1) "permanente" else "${it.toInt()} an(s)" }) {
            onMesures(m.copy(dureeSousIndexation = it.toInt()))
        }
    }

    Section("Cotisations") {
        Curseur("Hausse des cotisations", m.hausseCotisations, 0.0, 5.0, 0.1, { "+" + nombre(it) + " pt" }) {
            onMesures(m.copy(hausseCotisations = it))
        }
        Curseur("À partir de", m.anneeHausseCotisations.toDouble(), 2026.0, 2045.0, 1.0, { it.toInt().toString() }) {
            onMesures(m.copy(anneeHausseCotisations = it.toInt()))
        }
        Text(
            "1 point de cotisation ≈ ${nombre(ModeleRetraites.POINT_COTISATION_PIB, 2)} point de PIB de ressources.",
            style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    Section("Calcul des nouvelles pensions", "Modification du montant des pensions liquidées (règles de calcul, taux de remplacement).") {
        Curseur("Variation", m.variationNouvellesPensions * 100, -20.0, 10.0, 1.0, { signe(it, 0) + " %" }) {
            onMesures(m.copy(variationNouvellesPensions = it / 100))
        }
        Curseur("À partir de", m.anneeVariationNouvellesPensions.toDouble(), 2026.0, 2045.0, 1.0, { it.toInt().toString() }) {
            onMesures(m.copy(anneeVariationNouvellesPensions = it.toInt()))
        }
    }

    if (!m.estVide) {
        Row(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.Center) {
            OutlinedButton(onClick = { onMesures(Mesures()) }) { Text("Revenir à la législation actuelle") }
        }
    }
}

// ---------------------------------------------------------------- Résultats

private fun serie(nom: String, p: Projection, couleur: androidx.compose.ui.graphics.Color, pointillee: Boolean = false, f: (ResultatAnnee) -> Double) =
    Serie(nom, couleur, p.annees.map { it.annee }, p.annees.map(f), pointillee)

@Composable
fun EcranResultats(etat: EtatUi, onExporter: () -> Unit) = Defilement {
    val r = etat.resultats ?: return@Defilement
    val avecMesures = !etat.scenario.mesures.estVide
    val nomAvec = if (avecMesures) "Avec mesures" else "Projection"
    val cAvec = Couleurs.avec
    val cSans = Couleurs.sans
    val cTroisieme = Couleurs.troisieme
    fun seriesDe(f: (ResultatAnnee) -> Double): List<Serie> = buildList {
        add(serie(nomAvec, r.avec, cAvec, f = f))
        if (avecMesures) add(serie("Sans mesures", r.sans, cSans, pointillee = true, f = f))
    }

    Section("Solde du système", "En % du PIB ; négatif = besoin de financement.") {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            for (a in listOf(2030, 2045, 2070)) {
                val v = r.avec.en(a)
                Tuile(
                    a.toString(),
                    pourcent(v.soldePib),
                    if (avecMesures) "sans : ${pourcent(r.sans.en(a).soldePib)}" else "${nombre(v.soldeMdEuros, 0)} Md€",
                    Modifier.weight(1f),
                )
            }
        }
        GraphiqueCourbes(
            seriesDe { it.soldePib }, { pourcent(it) },
            reperes = Reperes("COR 2026", Projection.REFERENCE_COR_SOLDE), ligneZero = true,
        )
    }

    Section("Pour équilibrer en ${r.leviers.annee}", "Effort supplémentaire sur un seul levier, en plus des mesures choisies.") {
        val l = r.leviers
        if (l.soldePib >= 0) {
            Text("Le système est à l'équilibre ou excédentaire en ${l.annee} (${pourcent(l.soldePib)} du PIB).", style = MaterialTheme.typography.bodyMedium)
        } else {
            Leviers("Cotisations", "+${nombre(l.pointsCotisation)} points")
            Leviers("Pensions de tous les retraités", "${signe(l.variationPensionsPourcent)} %")
            Leviers("Âge moyen de départ", l.decalageAge?.let { "${signe(it)} an (≈ ${nombre(r.avec.en(l.annee).ageMoyenDepart + it)} ans)" } ?: "hors de portée")
        }
    }

    Section("Dépenses et ressources", "En % du PIB.") {
        val series = seriesDe { it.depensesPib }.map { it.copy(nom = "Dépenses " + it.nom.lowercase()) } +
            serie("Ressources", r.avec, cTroisieme) { it.ressourcesPib }
        GraphiqueCourbes(series, { pourcent(it) }, reperes = Reperes("Dépenses COR 2026", Projection.REFERENCE_COR_DEPENSES))
    }

    Section("Cotisants par retraité") {
        GraphiqueCourbes(seriesDe { it.rapportCotisantsRetraites }, { nombre(it, 2) })
    }

    Section("Pension moyenne relative", "Pension moyenne rapportée au revenu d'activité moyen (base 50 % en 2025).") {
        GraphiqueCourbes(seriesDe { it.pensionRelative * 100 }, { pourcent(it) })
    }

    Section("Âge moyen de départ") {
        GraphiqueCourbes(seriesDe { it.ageMoyenDepart }, { nombre(it) })
    }

    Section("Solde en milliards d'euros", "Euros constants de 2025.") {
        GraphiqueCourbes(seriesDe { it.soldeMdEuros }, { nombre(it, 0) }, ligneZero = true)
        Text(
            "Solde cumulé ${ANNEE_DEBUT}-${ANNEE_FIN} : ${nombre(r.avec.soldeCumuleMdEuros, 0)} Md€" +
                if (avecMesures) " (sans mesures : ${nombre(r.sans.soldeCumuleMdEuros, 0)} Md€)" else "",
            style = MaterialTheme.typography.bodySmall,
        )
    }

    Row(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.Center) {
        Button(onClick = onExporter) { Text("Exporter les résultats (CSV)") }
    }
}

@Composable
private fun Leviers(libelle: String, valeur: String) {
    Row(Modifier.fillMaxWidth()) {
        Text(libelle, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
        Text(valeur, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.SemiBold)
    }
}

// ---------------------------------------------------------------- Démographie

@Composable
fun EcranDemographie(etat: EtatUi) = Defilement {
    val r: Resultats = etat.resultats ?: return@Defilement
    val p = r.avec
    var annee by remember { mutableIntStateOf(ANNEE_FIN) }

    Section("Pyramide des âges", "Barres : $ANNEE_DEBUT. Contour : année choisie.") {
        Curseur("Année comparée", annee.toDouble(), ANNEE_DEBUT.toDouble(), ANNEE_FIN.toDouble(), 1.0, { it.toInt().toString() }) { annee = it.toInt() }
        GraphiquePyramide(
            p.demographie.pyramide(ANNEE_DEBUT), ANNEE_DEBUT,
            p.demographie.pyramide(annee), annee,
            Couleurs.avec, Couleurs.sans,
        )
        val a = p.en(annee)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Tuile("Population $annee", millions(a.population), "${signe((a.population / p.en(ANNEE_DEBUT).population - 1) * 100)} % / $ANNEE_DEBUT", Modifier.weight(1f))
            Tuile("65 ans ou plus", pourcent(a.plus65 / a.population * 100), "${pourcent(p.en(ANNEE_DEBUT).plus65 / p.en(ANNEE_DEBUT).population * 100)} en $ANNEE_DEBUT", Modifier.weight(1f))
        }
    }

    Section("Personnes de 20-64 ans pour une de 65 ans ou plus") {
        GraphiqueCourbes(listOf(serie("Rapport démographique", p, Couleurs.avec) { it.rapportDemographique }), { nombre(it, 2) })
    }

    Section("Population", "En millions.") {
        GraphiqueCourbes(
            listOf(
                serie("0-19 ans", p, Couleurs.troisieme) { it.moins20 / 1e6 },
                serie("20-64 ans", p, Couleurs.avec) { it.de20a64 / 1e6 },
                serie("65 ans ou plus", p, Couleurs.sans) { it.plus65 / 1e6 },
            ),
            { nombre(it) },
        )
    }

    Section("Naissances", "En milliers par an.") {
        GraphiqueCourbes(listOf(serie("Naissances", p, Couleurs.avec) { it.naissances / 1000 }), { nombre(it, 0) })
    }

    Section("Espérance de vie à la naissance") {
        GraphiqueCourbes(
            listOf(
                serie("Femmes", p, Couleurs.sans) { it.esperanceVieFemmes },
                serie("Hommes", p, Couleurs.avec) { it.esperanceVieHommes },
            ),
            { nombre(it) },
        )
    }
}

// ---------------------------------------------------------------- Méthode

@Composable
fun EcranMethode() = Defilement {
    Section("Ce que fait l'application") {
        Paragraphe(
            "Elle projette, année par année de 2025 à 2070, la population française par sexe et âge détaillé, " +
                "puis l'équilibre financier de l'ensemble du système de retraite, avec et sans les mesures que vous choisissez.",
        )
    }
    Section("Hypothèses de référence") {
        Paragraphe(
            "Démographie : projections de population de l'Insee (2026), scénario central retenu par le Conseil d'orientation " +
                "des retraites (COR) dans son rapport annuel de juin 2026 : fécondité de 1,45 enfant par femme à partir de 2028, " +
                "solde migratoire de +150 000 personnes par an, espérance de vie à la naissance de 86,7 ans pour les hommes et " +
                "89,5 ans pour les femmes en 2070. Variantes : fécondité 1,2 et 1,7 ; solde migratoire +70 000 et +230 000.",
        )
        Paragraphe(
            "Économie : convergence des gains de productivité vers 0,7 % par an (variantes 0,4 % et 1,0 %), chômage de long terme de 7 %. " +
                "Résultats publiés par le COR (repères ◇ sur les graphiques) : dépenses de 14,1 % du PIB en 2025, 14,2 % en 2045 et 15,3 % en 2070 ; " +
                "solde de −0,2 % du PIB en 2025 et 2030, −0,9 % en 2045 et −2,4 % en 2070.",
        )
    }
    Section("Méthode") {
        Paragraphe(
            "1. Démographie par composantes : chaque année, les décès (table de mortalité paramétrique ajustée pour atteindre " +
                "l'espérance de vie visée), les naissances (calendrier de fécondité par âge de la mère) et le solde migratoire " +
                "(concentré entre 18 et 35 ans) font évoluer la pyramide.",
        )
        Paragraphe(
            "2. Retraités et cotisants : la part de retraités à chaque âge dépend de l'âge moyen de départ ; les autres personnes " +
                "sont actives selon des taux d'activité par âge, puis en emploi selon le taux de chômage.",
        )
        Paragraphe(
            "3. Pensions : les nouvelles pensions suivent les salaires (avec une élasticité réduite, les règles de calcul étant " +
                "en partie indexées sur les prix) ; les pensions en cours de service suivent l'inflation, sauf mesure contraire.",
        )
        Paragraphe(
            "4. Équation comptable du COR : dépenses / PIB = (retraités / cotisants) × (pension moyenne / revenu d'activité moyen) " +
                "× part des revenus d'activité dans le PIB. Les dépenses sont exprimées par rapport à 2025. Le modèle a été calé pour " +
                "reproduire les dépenses 2070 du COR et leur sensibilité à la productivité (16,1 % à 0,4 % ; 14,5 % à 1,0 %).",
        )
    }
    Section("Limites") {
        Paragraphe(
            "C'est un modèle simplifié, à visée pédagogique : il ne distingue pas les régimes, les carrières, les pensions de " +
                "réversion ni les minima de pension, et l'effet d'une mesure d'âge sur l'âge effectif de départ est un ordre de " +
                "grandeur. La pyramide intégrée est une reconstitution approchée (importez la pyramide Insee pour plus de précision). " +
                "Les effets du solde migratoire sont probablement surestimés : les migrants sont supposés avoir les mêmes " +
                "carrières et pensions que la moyenne. Les résultats ne remplacent pas les projections officielles du COR.",
        )
    }
}

@Composable
private fun Paragraphe(texte: String) {
    Text(texte, style = MaterialTheme.typography.bodyMedium)
}
