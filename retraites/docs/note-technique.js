// Génère la note technique Word de l'application Projection retraites.
// Usage : node note-technique.js Note-technique-projection-retraites.docx  (module npm « docx » requis)
const fs = require("fs");
const {
  Document, Packer, Paragraph, TextRun, HeadingLevel, AlignmentType, Table, TableRow, TableCell,
  WidthType, ShadingType, BorderStyle, LevelFormat, Footer, Header, PageNumber, PageBreak,
  Math: OMath, MathRun, MathFraction, MathSubScript, MathSuperScript, MathSubSuperScript, MathSum, MathRoundBrackets,
} = require("docx");

const sortie = process.argv[2];
const FONT = "Arial";
const BLEU = "1F4E8C";
const LARGEUR = 9026; // A4, marges 2,5 cm (1417 dxa) : 11906 - 2*1440 ≈ 9026

// ---------- Aides texte
function runs(texte, opts = {}) {
  // Indices : « π_new » s'écrit π suivi de « new » en indice.
  if (/_[A-Za-zé0-9]/.test(texte)) {
    const morceaux = [];
    const re = /_([A-Za-zé0-9]+)/g;
    let i = 0, m;
    while ((m = re.exec(texte))) {
      if (m.index > i) morceaux.push(...runs(texte.slice(i, m.index), opts));
      morceaux.push(new TextRun({ text: m[1], subScript: true, ...opts }));
      i = m.index + m[0].length;
    }
    if (i < texte.length) morceaux.push(...runs(texte.slice(i), opts));
    return morceaux;
  }
  // Gras entre **…**
  const parts = [];
  const re = /(\*\*[^*]+\*\*)/g;
  let i = 0, m;
  while ((m = re.exec(texte))) {
    if (m.index > i) parts.push(new TextRun({ text: texte.slice(i, m.index), ...opts }));
    const t = m[0];
    parts.push(new TextRun({ text: t.slice(2, -2), bold: true, ...opts }));
    i = m.index + t.length;
  }
  if (i < texte.length) parts.push(new TextRun({ text: texte.slice(i), ...opts }));
  return parts;
}
const p = (texte, opts = {}) => new Paragraph({ children: runs(texte), spacing: { after: 120 }, ...opts });
const h1 = (t) => new Paragraph({ heading: HeadingLevel.HEADING_1, children: [new TextRun(t)] });
const h2 = (t) => new Paragraph({ heading: HeadingLevel.HEADING_2, children: [new TextRun(t)] });
const puce = (t) => new Paragraph({ numbering: { reference: "puces", level: 0 }, children: runs(t), spacing: { after: 60 } });
const num = (t, ref = "etapes") => new Paragraph({ numbering: { reference: ref, level: 0 }, children: runs(t), spacing: { after: 60 } });
const note = (t) => new Paragraph({ children: runs(t, { size: 18, color: "555555" }), spacing: { after: 120 } });

// ---------- Aides formules (Office Math)
const r = (t) => new MathRun(t);
const sub = (base, s) => new MathSubScript({ children: [].concat(base).map(x => typeof x === "string" ? r(x) : x), subScript: [].concat(s).map(x => typeof x === "string" ? r(x) : x) });
const sup = (base, s) => new MathSuperScript({ children: [].concat(base).map(x => typeof x === "string" ? r(x) : x), superScript: [].concat(s).map(x => typeof x === "string" ? r(x) : x) });
const subsup = (base, b, h) => new MathSubSuperScript({ children: [].concat(base).map(x => typeof x === "string" ? r(x) : x), subScript: [].concat(b).map(x => typeof x === "string" ? r(x) : x), superScript: [].concat(h).map(x => typeof x === "string" ? r(x) : x) });
const frac = (n, d) => new MathFraction({ numerator: [].concat(n).map(x => typeof x === "string" ? r(x) : x), denominator: [].concat(d).map(x => typeof x === "string" ? r(x) : x) });
const somme = (b, h, corps) => new MathSum({
  children: [].concat(corps).map(x => typeof x === "string" ? r(x) : x),
  subScript: [].concat(b).map(x => typeof x === "string" ? r(x) : x),
  ...(h === "" ? {} : { superScript: [].concat(h).map(x => typeof x === "string" ? r(x) : x) }),
});
const par = (corps) => new MathRoundBrackets({ children: [].concat(corps).map(x => typeof x === "string" ? r(x) : x) });
const eq = (...parts) => new Paragraph({
  alignment: AlignmentType.CENTER, spacing: { before: 80, after: 160 },
  children: [new OMath({ children: parts.map(x => typeof x === "string" ? r(x) : x) })],
});

// ---------- Tableaux
const bord = { style: BorderStyle.SINGLE, size: 4, color: "BFBFBF" };
const bords = { top: bord, bottom: bord, left: bord, right: bord };
function tableau(entetes, lignes, largeurs) {
  const total = largeurs.reduce((a, b) => a + b, 0);
  const cellule = (t, l, entete) => new TableCell({
    borders: bords, width: { size: l, type: WidthType.DXA },
    shading: entete ? { fill: "DCE6F2", type: ShadingType.CLEAR, color: "auto" } : undefined,
    margins: { top: 60, bottom: 60, left: 100, right: 100 },
    children: [new Paragraph({ children: runs(String(t), { bold: entete, size: 18 }) })],
  });
  return new Table({
    width: { size: total, type: WidthType.DXA }, columnWidths: largeurs,
    rows: [
      new TableRow({ tableHeader: true, children: entetes.map((t, i) => cellule(t, largeurs[i], true)) }),
      ...lignes.map(l => new TableRow({ children: l.map((t, i) => cellule(t, largeurs[i], false)) })),
    ],
  });
}
const espace = () => new Paragraph({ children: [], spacing: { after: 120 } });

// ---------- Contenu
const c = [];

// Page de titre
c.push(new Paragraph({ spacing: { before: 2400, after: 240 }, children: [new TextRun({ text: "Projection retraites", size: 56, bold: true, color: BLEU, font: FONT })] }));
c.push(new Paragraph({ spacing: { after: 480 }, children: [new TextRun({ text: "Note technique : méthodologie et formules", size: 36, color: "404040", font: FONT })] }));
c.push(p("Application Android de projection de l'équilibre du système de retraite français, 2025-2070, à partir des hypothèses du Conseil d'orientation des retraites (rapport annuel de juin 2026)."));
c.push(new Paragraph({ spacing: { before: 480, after: 80 }, children: runs("**Développeur :** Thierry LORGEOU") }));
c.push(new Paragraph({ spacing: { after: 80 }, children: runs("**Version de l'application :** 0.4.0") }));
c.push(new Paragraph({ spacing: { after: 80 }, children: runs("**Date :** novembre 2026") }));
c.push(new Paragraph({ children: [new PageBreak()] }));

// 1
c.push(h1("1. Objet et périmètre"));
c.push(p("L'application projette, année par année de 2025 à 2070, la population française par sexe et âge détaillé, puis l'équilibre financier de l'ensemble du système de retraite (tous régimes confondus), avec et sans les mesures choisies par l'utilisateur. Elle permet aussi de simuler un pilier par capitalisation et un fonds de réserve."));
c.push(p("Le modèle est volontairement simplifié et transparent : il ne distingue ni les régimes, ni les carrières individuelles, ni les pensions de réversion ou les minima. Il est calé pour reproduire les principaux résultats publiés par le COR dans son scénario de référence, et sert à mesurer l'effet relatif de mesures et d'hypothèses. Tous les calculs sont faits sur le téléphone, sans connexion réseau."));
c.push(p("Toutes les grandeurs monétaires sont exprimées en euros constants de 2025. Les résultats financiers peuvent être affichés en % du PIB ou en milliards d'euros (section 7.5)."));

// 2
c.push(h1("2. Sources et hypothèses de référence"));
c.push(p("Le scénario de référence reprend les hypothèses du rapport annuel du COR de juin 2026, fondées sur les projections de population de l'Insee publiées en 2026."));
c.push(tableau(["Hypothèse", "Référence", "Variantes proposées"], [
  ["Fécondité (enfants par femme)", "1,45 à partir de 2028 (1,56 en 2025)", "1,2 et 1,7 (atteintes en 2030)"],
  ["Solde migratoire annuel", "+150 000", "+70 000 et +230 000"],
  ["Espérance de vie à la naissance en 2070", "86,7 ans (H), 89,5 ans (F) ; 2025 : 80,3 et 85,9 ans", "Stable au niveau 2025 ; +2 ans (illustratif)"],
  ["Productivité du travail", "0,7 % par an", "0,4 % et 1,0 %"],
  ["Chômage de long terme", "7 % (atteint en 2032 ; 7,5 % en 2025)", "4,5 % et 10 % (illustratifs)"],
  ["Ressources du système", "Trajectoire COR : 13,9 % du PIB en 2025, 12,9 % en 2070", "Taux de prélèvement constant (13,9 %)"],
  ["Anciennes hypothèses Insee 2021", "—", "Fécondité 1,8 ; solde +70 000 ; 87,5 / 90,0 ans"],
], [2600, 3426, 3000]));
c.push(espace());
c.push(p("Résultats publiés par le COR utilisés pour le calage et affichés en repères dans l'application :"));
c.push(tableau(["Indicateur (% du PIB)", "2025", "2030", "2045", "2070"], [
  ["Dépenses de retraite", "14,1", "—", "14,2", "15,3"],
  ["Ressources", "13,9", "—", "13,3", "12,9"],
  ["Solde", "−0,2", "−0,2", "−0,9", "−2,4"],
], [3426, 1400, 1400, 1400, 1400]));
c.push(note("Sources : COR, rapport annuel de juin 2026 et sa synthèse ; Insee, Projections de population 2026 pour la France et Insee Première n° 2108. Les valeurs ont été relevées dans ces publications et dans des synthèses de presse spécialisée."));

// 3
c.push(h1("3. Vue d'ensemble"));
c.push(p("Chaque projection enchaîne les étapes suivantes, pour chaque année t de 2025 à 2070 :"));
c.push(num("Projection démographique par composantes : décès, naissances et migrations font évoluer la pyramide des âges (section 4)."));
c.push(num("Répartition de la population de chaque âge entre retraités, actifs occupés et autres, selon l'âge moyen de départ et les taux d'activité (section 5)."));
c.push(num("Suivi par âge de la pension moyenne des retraités, selon les règles de revalorisation (section 6)."));
c.push(num("Calcul des dépenses, des ressources et du solde par l'équation comptable du COR (section 7)."));
c.push(num("Le cas échéant, pilier par capitalisation et fonds de réserve (section 10)."));
c.push(p("Deux projections sont calculées à chaque modification : l'une avec les mesures et la capitalisation choisies, l'autre « sans mesures » (législation actuelle, mêmes hypothèses démographiques et économiques)."));

// 4
c.push(h1("4. Projection démographique"));
c.push(p("La population est suivie au 1er janvier, par sexe s (hommes H, femmes F) et âge révolu x de 0 à 110 ans, le dernier âge regroupant les 110 ans ou plus. On note P_s(x, t) l'effectif correspondant."));
c.push(h2("4.1 Population initiale (1er janvier 2025)"));
c.push(p("La pyramide intégrée est reconstruite à partir des naissances annuelles observées N(y) depuis 1914 (série Insee, en milliers), puis calée. Pour l'âge a, la génération est née en y = 2024 − a :"));
c.push(eq(sub("P", "s"), par("a"), " = N", par("2024 − a"), " × ", sub("π", "s"), " × ", sub("S", "s"), par("a"), " × ", par(["1 + m × ", frac("min(a, 35)", "35")])));
c.push(p("où π_H = 0,512 et π_F = 0,488 sont les parts de garçons et de filles à la naissance, m l'apport relatif des migrations aux âges adultes, et S_s(a) la survie de la naissance à l'âge a, calculée avec les quotients de mortalité de 2025 multipliés par un coefficient c et majorés aux grands âges par une pente p :"));
c.push(eq(sub("S", "s"), par("a"), " = ", sub("∏", "x<a"), " ", par(["1 − ", sub("q", "s,2025"), par("x"), " × c × ", par(["1 + p × ", frac("max(0, x − 60)", "30")])])));
c.push(p("Les trois paramètres (c, p, m) sont déterminés par la méthode de Newton pour reproduire la part des 0-19 ans (23,7 %), des 65 ans ou plus (21,8 %) et des 75 ans ou plus (10,6 %) au 1er janvier 2025. La pyramide est enfin mise à l'échelle de la population totale (68,6 millions) et de la part des femmes (51,6 %)."));
c.push(p("L'utilisateur peut remplacer cette pyramide approchée par la pyramide exacte de l'Insee, importée au format CSV (âge ; hommes ; femmes)."));

c.push(h2("4.2 Mortalité"));
c.push(p("La force de mortalité à l'âge x (évaluée au milieu de l'âge, ξ = x + 0,5) combine une composante infantile, une bosse accidentelle des jeunes adultes et une loi de Gompertz :"));
c.push(eq(sub("μ", "s"), par("x"), " = ", sub("a", "s"), " ", sup("e", "−1,6ξ"), " + ", sub("h", "s"), " ", sup("e", ["−", sup(par(frac("ξ − 22", "9")), "2")]), " + ", sub("G", "s"), " ", sup("e", [sub("b", "s"), "ξ"])));
c.push(tableau(["Paramètre", "Hommes", "Femmes"], [
  ["a (mortalité infantile)", "0,0040", "0,0033"],
  ["h (bosse des jeunes adultes)", "0,00045", "0,00015"],
  ["G (niveau de Gompertz)", "2,2 × 10⁻⁵", "5,0 × 10⁻⁶"],
  ["b (pente de Gompertz)", "0,0955", "0,1075"],
], [4026, 2500, 2500]));
c.push(espace());
c.push(p("Les quotients de mortalité de l'année t s'obtiennent avec un coefficient multiplicateur k_s(t) :"));
c.push(eq(sub("q", "s"), par("x, t"), " = 1 − ", sup("e", ["−", sub("k", "s"), par("t"), " ", sub("μ", "s"), par("x")]), ",   ", sub("q", "s"), par("110, t"), " = 1"));
c.push(p("L'espérance de vie à la naissance d'une table du moment est calculée par la méthode des trapèzes, avec une durée de vie restante de 1,5 an dans le groupe ouvert :"));
c.push(eq(sub("e", "0"), " = ", somme("x=0", "109", frac([sub("l", "x"), " + ", sub("l", "x+1")], "2")), " + 1,5 ", sub("l", "110"), ",   ", sub("l", "0"), " = 1,   ", sub("l", "x+1"), " = ", sub("l", "x"), par(["1 − ", sub("q", "x")])));
c.push(p("Le coefficient k_s(t) est déterminé chaque année par dichotomie (80 itérations) pour que l'espérance de vie à la naissance atteigne la cible, qui progresse de la valeur de 2025 vers celle de 2070 avec un ralentissement progressif :"));
c.push(eq(sub("e", "0,s"), par("t"), " = ", sub("e", "0,s"), par("2025"), " + ", par([sub("e", "0,s"), par("2070"), " − ", sub("e", "0,s"), par("2025")]), " × ", par(["1 − ", sup(par("1 − τ"), "1,3")]), ",   τ = ", frac("t − 2025", "45")));
c.push(p("Avec les hypothèses de référence, l'espérance de vie à 65 ans obtenue en 2070 est de 24,4 ans pour les hommes et 26,1 ans pour les femmes (Insee 2026 : 24,4 et 26,3 ans)."));

c.push(h2("4.3 Fécondité et naissances"));
c.push(p("Le calendrier de fécondité par âge de la mère, de somme 1, suit une loi gamma (âge moyen d'environ 31 ans), pour x de 15 à 49 ans :"));
c.push(eq("f", par("x"), " ∝ ", sup("u", "8"), " ", sup("e", "−u/1,85"), ",   u = x + 0,5 − 14,5,   ", somme("x=15", "49", ["f", par("x")]), " = 1"));
c.push(p("L'indicateur conjoncturel de fécondité ICF(t) évolue linéairement de 1,56 en 2025 jusqu'à la valeur cible, atteinte l'année choisie, puis reste constant. Les naissances de l'année sont :"));
c.push(eq("B", par("t"), " = ICF", par("t"), " × ", somme("x=15", "49", [sub("P", "F"), par("x, t"), " f", par("x")])));

c.push(h2("4.4 Migrations"));
c.push(p("Le solde migratoire annuel M est réparti à parts égales entre les sexes et selon un profil par âge m(x), de somme 1, concentré entre 18 et 35 ans (nul à partir de 75 ans) :"));
c.push(eq("m", par("x"), " ∝ ", sup("e", ["−", sup(par(frac("x − 26", "8")), "2")]), " + 0,30 ", sup("e", ["−", sup(par(frac("x − 6", "6")), "2")]), " + 0,10 ", sup("e", ["−", sup(par(frac("x − 45", "12")), "2")])));

c.push(h2("4.5 Équation de passage d'une année à l'autre"));
c.push(eq(sub("P", "s"), par("x + 1, t + 1"), " = ", sub("P", "s"), par("x, t"), " ", par(["1 − ", sub("q", "s"), par("x, t")]), " + ", frac("M", "2"), " m", par("x"), " ", par(["1 − ", frac([sub("q", "s"), par("x, t")], "2")])));
c.push(eq(sub("P", "H"), par("0, t + 1"), " = 0,512 B", par("t"), " ", par(["1 − 0,9 ", sub("q", "H"), par("0, t")]), ",   ", sub("P", "F"), par("0, t + 1"), " = 0,488 B", par("t"), " ", par(["1 − 0,9 ", sub("q", "F"), par("0, t")])));
c.push(p("Les migrants de l'année ne sont exposés qu'à la moitié du risque de décès. Dans le groupe ouvert, la moitié des personnes de 110 ans ou plus survivent d'une année à l'autre."));

// 5
c.push(h1("5. Retraités et cotisants"));
c.push(h2("5.1 Âge moyen de départ"));
c.push(p("L'âge moyen effectif de départ de la législation de référence (réforme de 2023 suspendue jusqu'en 2028 puis reprise) est interpolé linéairement entre les points suivants : 62,9 ans (2025), 63,1 (2028), 64,2 (2035), 64,4 (2045), 64,6 (2070). Une mesure d'âge y ajoute un décalage δ(t), d'amplitude Δ, appliqué à partir de l'année t₀ au rythme ρ (années d'âge par année civile ; ρ = 0 pour une application immédiate) :"));
c.push(eq("A", par("t"), " = ", sub("A", "réf"), par("t"), " + δ", par("t"), ",   δ", par("t"), " = signe", par("Δ"), " × min", par(["abs", par("Δ"), ", ", par(["t − ", sub("t", "0"), " + 1"]), " ρ"]), "  pour t ≥ ", sub("t", "0")));
c.push(h2("5.2 Part de retraités par âge"));
c.push(p("La part des personnes d'âge x déjà retraitées suit une courbe logistique centrée sur l'âge moyen de départ (nulle avant 50 ans), avec un paramètre de dispersion σ = 1,1 / 1,7 ≈ 0,65 an :"));
c.push(eq("R", par("x, t"), " = ", frac("1", ["1 + ", sup("e", ["−", par("x + 0,5 − A(t)"), " / σ"])])));
c.push(eq("Retraités : ", sub("N", "ret"), par("t"), " = ", somme("x=0", "110", ["P", par("x, t"), " R", par("x, t")]), ",   où P = ", sub("P", "H"), " + ", sub("P", "F")));
c.push(h2("5.3 Cotisants (actifs occupés)"));
c.push(p("Les personnes non retraitées sont actives selon un taux d'activité par âge α(x), puis en emploi selon le taux de chômage u(t), qui évolue linéairement de 7,5 % en 2025 jusqu'à sa valeur de long terme en 2032 :"));
c.push(eq("E", par("t"), " = ", somme("x=0", "110", ["P", par("x, t"), " ", par("1 − R(x, t)"), " α", par("x"), " ", par("1 − u(t)")])));
c.push(tableau(["Âge", "15-19", "20-24", "25-29", "30-49", "50-54", "55-59", "60-74", "75 +"], [
  ["α(x)", "0,17", "0,63", "0,88", "0,92", "0,89", "0,84", "0,72", "0"],
], [1426, 950, 950, 950, 950, 950, 950, 950, 950]));
c.push(espace());
c.push(p("Le rapport démographique est le nombre de personnes de 20-64 ans pour une personne de 65 ans ou plus ; le rapport cotisants / retraités vaut E(t) / N_ret(t)."));

// 6
c.push(h1("6. Pensions"));
c.push(p("Le salaire réel moyen croît au rythme g des gains de productivité : W(t) = (1 + g)^(t − 2025), avec W(2025) = 1. Les pensions sont exprimées dans la même unité (salaire de 2025)."));
c.push(h2("6.1 Pension à la liquidation"));
c.push(p("La pension des nouveaux retraités suit les salaires avec une élasticité réduite (1 − ε), les règles de calcul étant en partie indexées sur les prix. Elle est modulée par les mesures :"));
c.push(eq(sub("π", "new"), par("t"), " = ", sup("W(t)", "1 − ε"), " × ", par("1 + 0,02 δ(t)"), " × ", par(["1 + v ", sub("𝟙", ["t ≥ ", sub("t", "v")])]), " × ", par(["1 − ", frac("x", "28"), " min", par(["1, ", frac(["t − ", sub("t", "c")], "42")])])));
c.push(puce("ε = 0,15 : élasticité calée sur la sensibilité des dépenses à la productivité publiée par le COR (section 11) ;"));
c.push(puce("0,02 : gain de pension par année de départ plus tardive (décote, surcote, trimestres supplémentaires) ;"));
c.push(puce("v : variation du montant des nouvelles pensions à partir de l'année t_v (mesure « calcul des nouvelles pensions ») ;"));
c.push(puce("le dernier facteur ne s'applique qu'en cas de bascule de x points de cotisation vers la capitalisation, à partir de l'année t_c (section 10.1)."));
c.push(h2("6.2 Pensions en cours de service"));
c.push(p("Pour chaque âge, la pension moyenne est la moyenne pondérée de celle des retraités déjà présents l'année précédente (revalorisée) et de celle des nouveaux retraités :"));
c.push(eq(sub("S", "x,t"), " = min", par([sub("N", "x,t"), ",  ", sub("N", "x−1,t−1"), " × min", par(["1,05 ; ", frac("P(x, t)", "P(x − 1, t − 1)")])]), ",   ", sub("Nouv", "x,t"), " = ", sub("N", "x,t"), " − ", sub("S", "x,t")));
c.push(eq("π", par("x, t"), " = ", frac([sub("S", "x,t"), " π", par("x − 1, t − 1"), " ", par([sub("1 + r", "t")]), " + ", sub("Nouv", "x,t"), " ", sub("π", "new"), par("t")], sub("N", "x,t"))));
c.push(p("où N_x,t = P(x, t) R(x, t). La revalorisation réelle r_t est nulle en cas d'indexation sur les prix ; elle vaut g en cas d'indexation sur les salaires, diminuée de s pendant la période de sous-indexation éventuelle."));
c.push(p("Le stock initial reflète le fait que les pensions des générations anciennes ont moins suivi les salaires : π(x, 2025) = 1,009^(−0,15 × max(0, x − 63))."));
c.push(h2("6.3 Masse et pension moyenne relative"));
c.push(eq("Masse : ", sub("M", "p"), par("t"), " = ", somme("x=0", "110", [sub("N", "x,t"), " π", par("x, t")]), ",   pension relative : ", sub("ρ", "p"), par("t"), " = ", frac([sub("M", "p"), "(t) / ", sub("N", "ret"), "(t)"], "W(t)"), " × κ"));
c.push(p("Le facteur d'échelle κ fixe la pension moyenne relative à 50 % du revenu d'activité moyen en 2025."));

// 7
c.push(h1("7. Équilibre financier"));
c.push(h2("7.1 Équation comptable"));
c.push(p("Le modèle repose sur l'équation comptable utilisée par le COR :"));
c.push(eq(frac("Dépenses", "PIB"), " = ", frac("Retraités", "Cotisants"), " × ", frac("Pension moyenne", "Revenu d'activité moyen"), " × ", frac("Revenus d'activité", "PIB")));
c.push(p("La part des revenus d'activité dans le PIB étant supposée constante, les dépenses sont exprimées par rapport à leur niveau de 2025 (14,1 % du PIB) :"));
c.push(eq("D", par("t"), " = 14,1 × ", frac([sub("M", "p"), "(t) / W(t)"], [sub("M", "p"), "(2025)"]), " × ", frac("E(2025)", "E(t)"), "   (% du PIB)"));
c.push(h2("7.2 Ressources"));
c.push(p("Selon la convention choisie, les ressources (en % du PIB) suivent la trajectoire du scénario de référence du COR (13,9 % en 2025 et 2030, 13,3 % en 2045, 12,9 % en 2070, interpolation linéaire) ou restent constantes à 13,9 %. Les mesures y ajoutent :"));
c.push(eq("Res", par("t"), " = ", sub("Res", "conv"), par("t"), " + 0,23 × ", r("c₊"), par("t"), " − 0,23 × ", sub("x", "subst"), par("t")));
c.push(p("où c₊(t) est la hausse de cotisation (en points), nulle avant son année d'entrée en vigueur, x_subst(t) le nombre de points basculés vers la capitalisation en cas de substitution (nul sinon et avant l'année de bascule), et 0,23 le rendement en points de PIB d'un point de cotisation retenu par le modèle."));
c.push(h2("7.3 Solde"));
c.push(eq("Solde", par("t"), " = Res", par("t"), " − D", par("t"), "   (% du PIB)"));
c.push(h2("7.4 PIB"));
c.push(p("Le PIB en volume évolue comme l'emploi multiplié par la productivité, à partir de 2 990 Md€ en 2025 :"));
c.push(eq("PIB", par("t"), " = 2 990 × W", par("t"), " × ", frac("E(t)", "E(2025)"), "   (Md€ de 2025)"));
c.push(h2("7.5 Affichage en % du PIB ou en montants"));
c.push(p("Un sélecteur permet d'afficher les grandeurs financières (solde, dépenses, ressources, flux et encours de capitalisation, fonds de réserve) soit en % du PIB, soit en milliards d'euros constants de 2025. La conversion utilise le PIB projeté de l'année :"));
c.push(eq("Montant", par("t"), " = ", frac("Valeur en % du PIB (t)", "100"), " × PIB", par("t")));
c.push(p("Les repères publiés par le COR, exprimés en % du PIB, sont convertis de la même manière, avec le PIB projeté par le modèle. Le solde cumulé sur la période est la somme des soldes annuels en Md€ constants."));

// 8
c.push(h1("8. Mesures"));
c.push(tableau(["Levier", "Paramètres", "Effet dans le modèle"], [
  ["Âge de départ", "Décalage Δ (−5 à +4 ans), année t₀, rythme ρ (immédiat, 3 ou 6 mois par an)", "Décale A(t) (5.1) ; modifie R(x, t), donc retraités et cotisants ; +2 % de pension par année de report (6.1)"],
  ["Revalorisation", "Indexation sur les salaires ; sous-indexation s (points), début, durée", "Modifie r_t pour les pensions en cours de service (6.2)"],
  ["Cotisations", "Hausse c₊ (points), année t₊", "+0,23 point de PIB de ressources par point (7.2)"],
  ["Nouvelles pensions", "Variation v (%), année t_v", "Multiplie π_new(t) (6.1)"],
], [1800, 3426, 3800]));
c.push(espace());
c.push(p("Mesures types proposées :"));
c.push(tableau(["Mesure type", "Paramétrage"], [
  ["Départ à 60 ans", "Δ = −2,3 ans à partir de 2027, 6 mois par an (âge légal 60 ans, durée de 43 ans maintenue ; ordre de grandeur)"],
  ["Retour à 62 ans", "Δ = −1,0 an à partir de 2027, 3 mois par an"],
  ["Âge légal à 65 ans", "Δ = +0,8 an à partir de 2033, 3 mois par an"],
  ["Âge légal à 66 ans", "Δ = +1,6 an à partir de 2033, 3 mois par an"],
  ["Année blanche en 2027", "s = 1,5 point en 2027 (inflation supposée de 1,5 %)"],
  ["Sous-indexation de 1 point pendant 5 ans", "s = 1 point de 2027 à 2031"],
  ["Indexation sur les salaires", "r_t = g"],
  ["+1 point de cotisation", "c₊ = 1 à partir de 2027"],
  ["Nouvelles pensions −5 %", "v = −5 % à partir de 2027"],
], [3000, 6026]));

// 9
c.push(h1("9. Leviers d'équilibre"));
c.push(p("Pour l'année 2070, l'application calcule l'effort qui annulerait seul le solde, en plus des mesures choisies :"));
c.push(puce("**Cotisations** : hausse de −Solde / 0,23 points ;"));
c.push(puce("**Pensions** : variation de la pension de tous les retraités de Solde / D × 100 % ;"));
c.push(puce("**Âge** : décalage supplémentaire de l'âge moyen de départ, trouvé par dichotomie entre −4 et +8 ans (40 itérations) ; il est signalé « hors de portée » si la montée en charge n'a pas le temps de l'atteindre."));
c.push(p("Ces leviers portent sur le solde du système par répartition, hors fonds de réserve."));

// 10
c.push(h1("10. Capitalisation"));
c.push(p("Les deux dispositifs utilisent un même rendement réel annuel r (net de frais, au-delà de l'inflation ; 2 % par défaut)."));
c.push(h2("10.1 Pilier par capitalisation"));
c.push(p("À partir de l'année t_c, x points de cotisation alimentent des comptes individuels, répartis entre les âges au prorata de l'emploi E(a, t) :"));
c.push(eq("C", par("t"), " = ", frac("0,23 x", "100"), " × PIB", par("t"), ",   K", par("a, t"), " = K", par("a − 1, t − 1"), " ", par("1 + r"), " + C", par("t"), " ", frac("E(a, t)", "E(t)")));
c.push(p("Les comptes des personnes décédées avant la retraite restent mutualisés dans leur génération. Chaque année, la part des comptes convertie en rente est celle des personnes qui partent à la retraite (tous les comptes restants sont convertis à 75 ans) :"));
c.push(eq("φ", par("a, t"), " = ", frac("R(a, t) − R(a − 1, t − 1)", "1 − R(a − 1, t − 1)")));
c.push(p("Le capital converti finance une rente viagère indexée sur les prix, dont le prix est le facteur d'annuité calculé avec la mortalité de l'année (moyenne des deux sexes pondérée par les effectifs) et le rendement r :"));
c.push(eq("ä", par("a"), " = 1 + ", frac([par("1 − q(a)"), " ä", par("a + 1")], "1 + r"), ",   ä", par("110"), " = 1"));
c.push(eq("Rente", par("a, t"), " = Rente", par("a − 1, t − 1"), " ", par("1 − q(a − 1, t − 1)"), " + ", frac("φ(a, t) K(a, t)", "ä(a)")));
c.push(p("Les actifs du pilier sont les comptes en cours de constitution et les provisions des rentes en cours de service : Actifs(t) = Σ K(a, t) + Σ Rente(a, t) (ä(a) − 1). La pension moyenne relative y compris rentes vaut ρ_p(t) × (1 + Rentes(t) / D(t)), les rentes et les dépenses étant exprimées en % du PIB."));
c.push(p("**En complément**, la cotisation s'ajoute aux cotisations existantes et ne modifie pas le système par répartition. **En substitution**, la répartition perd immédiatement 0,23 x point de PIB de ressources (7.2) et les droits acquis après la bascule diminuent à proportion (facteur 1 − (x / 28) min(1, (t − t_c) / 42) sur les nouvelles pensions, 28 points étant le taux de cotisation retraite total et 42 ans la durée de carrière) : c'est le coût de transition."));
c.push(h2("10.2 Fonds de réserve"));
c.push(p("Le fonds part d'un capital initial F₀ (2025), reçoit une dotation de d % du PIB de l'année t_d à l'année t_D − 1 (financée hors du système), et, si l'option est retenue, les excédents éventuels du système. À partir de l'année t_D, il couvre les déficits dans la limite de son capital :"));
c.push(eq("F", par("t"), " = F", par("t − 1"), " ", par("1 + r"), " + ", frac("d", "100"), " PIB", par("t"), " ", sub("𝟙", [sub("t", "d"), " ≤ t < ", sub("t", "D")]), " + Excédents", par("t"), " − Prélèvement", par("t")));
c.push(eq("Prélèvement", par("t"), " = min", par(["F, −Solde", par("t"), " × ", frac("PIB(t)", "100")]), "  si Solde", par("t"), " < 0 et t ≥ ", sub("t", "D")));
c.push(p("Le solde après fonds est Solde(t) + Prélèvement(t) × 100 / PIB(t) (ramené à 0 lorsque les excédents sont placés). L'année d'épuisement est la première année, après t_D, où le système est en déficit et le fonds vide."));

// 11
c.push(h1("11. Calage et validation"));
c.push(p("Les paramètres non observés (notamment l'élasticité ε et le profil d'âge de départ) ont été choisis pour reproduire les résultats du COR. Des tests automatiques vérifient à chaque compilation que ces résultats restent obtenus :"));
c.push(tableau(["Indicateur", "COR 2026", "Modèle"], [
  ["Dépenses 2070, productivité 0,7 %", "15,3 % du PIB", "15,3 %"],
  ["Dépenses 2070, productivité 0,4 %", "16,1 %", "16,2 %"],
  ["Dépenses 2070, productivité 1,0 %", "14,5 %", "14,5 %"],
  ["Dépenses 2045", "14,2 %", "14,0 %"],
  ["Solde 2070", "−2,4 %", "−2,4 %"],
  ["Effet d'une fécondité de 1,2 sur le solde 2070", "≈ −1 point", "−1,1 point"],
  ["Rapport 20-64 / 65 ans ou plus, 2025", "≈ 2,5", "2,49"],
  ["Espérance de vie à 65 ans en 2070 (H / F)", "24,4 / 26,3 ans", "24,4 / 26,1 ans"],
], [4526, 2250, 2250]));
c.push(espace());
c.push(p("D'autres tests contrôlent la cohérence interne : effet du solde migratoire sur la population, sens des effets de chaque mesure, absence d'effet avant l'entrée en vigueur, équilibre effectivement obtenu avec le levier d'âge calculé, comptabilité du fonds de réserve (capitalisation simple sans décaissement, couverture puis épuisement), coût de transition en cas de substitution, lecture et export des fichiers CSV."));

// 12
c.push(h1("12. Limites"));
c.push(puce("Modèle agrégé : pas de distinction des régimes, des carrières, des pensions de réversion, des minima ni des écarts de salaire entre âges."));
c.push(puce("L'effet d'une mesure d'âge légal sur l'âge effectif de départ est un ordre de grandeur, fixé par l'utilisateur ou par les mesures types."));
c.push(puce("La pyramide intégrée est une reconstitution approchée ; l'import de la pyramide Insee la remplace."));
c.push(puce("L'effet du solde migratoire est probablement surestimé : les migrants sont supposés avoir les carrières et les pensions de la moyenne."));
c.push(puce("Le rapport 20-64 / 65 ans ou plus en 2070 (1,8) est supérieur à celui annoncé par le COR (environ 1,6)."));
c.push(puce("Le rendement d'un point de cotisation (0,23 point de PIB) est une hypothèse prudente : pour l'ensemble des régimes, il pourrait être plus élevé."));
c.push(puce("Capitalisation : rendement certain et constant, sans risque de marché ni frais explicites ; pas d'effet en retour sur la croissance."));
c.push(puce("Les résultats ne remplacent pas les projections officielles du COR."));

// Annexe
c.push(new Paragraph({ heading: HeadingLevel.HEADING_1, pageBreakBefore: true, children: [new TextRun("Annexe : principales notations")] }));
c.push(tableau(["Notation", "Signification"], [
  ["P_s(x, t)", "Population de sexe s et d'âge x au 1er janvier de l'année t"],
  ["q_s(x, t), μ_s(x)", "Quotient et force de mortalité"],
  ["B(t), ICF(t), M", "Naissances, indicateur de fécondité, solde migratoire"],
  ["A(t), R(x, t)", "Âge moyen de départ, part de retraités à l'âge x"],
  ["E(t), N_ret(t)", "Cotisants (actifs occupés), retraités"],
  ["W(t), g", "Indice de salaire réel, gains de productivité"],
  ["π(x, t), π_new(t), M_p(t)", "Pension moyenne à l'âge x, pension à la liquidation, masse des pensions"],
  ["D(t), Res(t)", "Dépenses et ressources, en % du PIB"],
  ["r", "Rendement réel des placements (capitalisation)"],
  ["K(a, t), ä(a), F(t)", "Comptes capitalisés, facteur d'annuité, capital du fonds de réserve"],
], [2800, 6226]));

// ---------- Document
const doc = new Document({
  creator: "Thierry LORGEOU",
  title: "Projection retraites - Note technique",
  description: "Méthodologie et formules de l'application Projection retraites",
  styles: {
    default: { document: { run: { font: FONT, size: 21 } } },
    paragraphStyles: [
      { id: "Heading1", name: "Heading 1", basedOn: "Normal", next: "Normal", quickFormat: true,
        run: { font: FONT, size: 30, bold: true, color: BLEU }, paragraph: { spacing: { before: 360, after: 160 }, keepNext: true, outlineLevel: 0 } },
      { id: "Heading2", name: "Heading 2", basedOn: "Normal", next: "Normal", quickFormat: true,
        run: { font: FONT, size: 24, bold: true, color: "333333" }, paragraph: { spacing: { before: 240, after: 120 }, keepNext: true, outlineLevel: 1 } },
    ],
  },
  numbering: {
    config: [
      { reference: "puces", levels: [{ level: 0, format: LevelFormat.BULLET, text: "•", alignment: AlignmentType.LEFT, style: { paragraph: { indent: { left: 720, hanging: 360 } } } }] },
      { reference: "etapes", levels: [{ level: 0, format: LevelFormat.DECIMAL, text: "%1.", alignment: AlignmentType.LEFT, style: { paragraph: { indent: { left: 720, hanging: 360 } } } }] },
    ],
  },
  sections: [{
    properties: { page: { size: { width: 11906, height: 16838 }, margin: { top: 1440, bottom: 1440, left: 1440, right: 1440 } }, titlePage: true },
    headers: { default: new Header({ children: [new Paragraph({ alignment: AlignmentType.RIGHT, children: [new TextRun({ text: "Projection retraites — Note technique", size: 16, color: "808080" })] })] }) },
    footers: { default: new Footer({ children: [new Paragraph({ alignment: AlignmentType.CENTER, children: [new TextRun({ children: [PageNumber.CURRENT], size: 16, color: "808080" })] })] }) },
    children: c,
  }],
});

Packer.toBuffer(doc).then(b => { fs.writeFileSync(sortie, b); console.log("écrit", sortie); });
