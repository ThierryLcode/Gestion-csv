//! Utilitaires sur les valeurs texte : nombres au format français, comparaison
//! sans accents, tri naturel et dates.

use std::cmp::Ordering;

/// Convertit un texte en nombre en acceptant les formats français (« 1 234,5 »)
/// et anglo-saxons (« 1234.5 »).
pub fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let mut cleaned = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            ' ' | '\u{a0}' | '\u{202f}' | '_' => {}
            ',' => cleaned.push('.'),
            c => cleaned.push(c),
        }
    }
    // Refuse les formats ambigus du type « 1.234.567 » ou « 1,234.5 ».
    if cleaned.matches('.').count() > 1 {
        return None;
    }
    if !cleaned.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'+' | b'e' | b'E')) {
        return None;
    }
    cleaned.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Formate un nombre sans notation scientifique inutile.
pub fn format_number(n: f64, decimal: char) -> String {
    let s = if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") };
    if decimal == ',' { s.replace('.', ",") } else { s }
}

fn strip_accent(c: char, out: &mut String) {
    let base = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'œ' => return out.push_str("oe"),
        'æ' => return out.push_str("ae"),
        'ß' => return out.push_str("ss"),
        c => c,
    };
    out.push(base);
}

/// Minuscules sans accents : clé de comparaison « à la française ».
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        strip_accent(c, &mut out);
    }
    out
}

/// Comparaison naturelle : « fichier2 » < « fichier10 ».
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.char_indices().peekable(), b.char_indices().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some((sa, ca)), Some((sb, cb))) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let mut ea = sa;
                    while let Some(&(i, c)) = ai.peek() {
                        if !c.is_ascii_digit() { break; }
                        ea = i + 1;
                        ai.next();
                    }
                    let mut eb = sb;
                    while let Some(&(i, c)) = bi.peek() {
                        if !c.is_ascii_digit() { break; }
                        eb = i + 1;
                        bi.next();
                    }
                    let na = a[sa..ea].trim_start_matches('0');
                    let nb = b[sb..eb].trim_start_matches('0');
                    let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else {
                    if ca != cb {
                        return ca.cmp(&cb);
                    }
                    ai.next();
                    bi.next();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateTime {
    pub y: i32,
    pub m: u32,
    pub d: u32,
    pub h: u32,
    pub mi: u32,
    pub s: u32,
}

#[derive(Clone, Debug, PartialEq)]
enum Part {
    Lit(char),
    Year4,
    Year2,
    Month2,
    Month,
    Day2,
    Day,
    Hour,
    Minute,
    Second,
}

/// Format de date décrit avec les jetons AAAA, AA, MM, M, JJ, J, HH, mm, ss
/// (les équivalents anglais YYYY, YY, DD, D sont aussi acceptés).
#[derive(Clone, Debug)]
pub struct DateFormat {
    parts: Vec<Part>,
}

impl DateFormat {
    pub fn parse(spec: &str) -> Result<DateFormat, String> {
        const TOKENS: [(&str, Part); 13] = [
            ("AAAA", Part::Year4),
            ("YYYY", Part::Year4),
            ("AA", Part::Year2),
            ("YY", Part::Year2),
            ("MM", Part::Month2),
            ("JJ", Part::Day2),
            ("DD", Part::Day2),
            ("HH", Part::Hour),
            ("mm", Part::Minute),
            ("ss", Part::Second),
            ("M", Part::Month),
            ("J", Part::Day),
            ("D", Part::Day),
        ];
        let mut parts = Vec::new();
        let mut rest = spec;
        'outer: while !rest.is_empty() {
            for (tok, part) in &TOKENS {
                if let Some(r) = rest.strip_prefix(tok) {
                    parts.push(part.clone());
                    rest = r;
                    continue 'outer;
                }
            }
            let c = rest.chars().next().unwrap();
            parts.push(Part::Lit(c));
            rest = &rest[c.len_utf8()..];
        }
        if !parts.iter().any(|p| !matches!(p, Part::Lit(_))) {
            return Err(format!("Format de date sans jeton (AAAA, MM, JJ…) : « {spec} »"));
        }
        Ok(DateFormat { parts })
    }

    pub fn read(&self, s: &str) -> Option<DateTime> {
        let s = s.trim().as_bytes();
        let mut pos = 0;
        let mut dt = DateTime { y: 1970, m: 1, d: 1, h: 0, mi: 0, s: 0 };
        let num = |pos: &mut usize, min: usize, max: usize| -> Option<u32> {
            let start = *pos;
            while *pos < s.len() && *pos - start < max && s[*pos].is_ascii_digit() {
                *pos += 1;
            }
            if *pos - start < min {
                return None;
            }
            std::str::from_utf8(&s[start..*pos]).ok()?.parse().ok()
        };
        for p in &self.parts {
            match p {
                Part::Lit(c) => {
                    let mut buf = [0u8; 4];
                    let lit = c.encode_utf8(&mut buf).as_bytes();
                    if !s[pos..].starts_with(lit) {
                        return None;
                    }
                    pos += lit.len();
                }
                Part::Year4 => dt.y = num(&mut pos, 4, 4)? as i32,
                Part::Year2 => {
                    let y = num(&mut pos, 2, 2)? as i32;
                    dt.y = if y < 70 { 2000 + y } else { 1900 + y };
                }
                Part::Month2 => dt.m = num(&mut pos, 2, 2)?,
                Part::Month => dt.m = num(&mut pos, 1, 2)?,
                Part::Day2 => dt.d = num(&mut pos, 2, 2)?,
                Part::Day => dt.d = num(&mut pos, 1, 2)?,
                Part::Hour => dt.h = num(&mut pos, 1, 2)?,
                Part::Minute => dt.mi = num(&mut pos, 2, 2)?,
                Part::Second => dt.s = num(&mut pos, 2, 2)?,
            }
        }
        (pos == s.len() && valid(&dt)).then_some(dt)
    }

    pub fn write(&self, dt: &DateTime) -> String {
        let mut out = String::new();
        for p in &self.parts {
            match p {
                Part::Lit(c) => out.push(*c),
                Part::Year4 => out.push_str(&format!("{:04}", dt.y)),
                Part::Year2 => out.push_str(&format!("{:02}", dt.y.rem_euclid(100))),
                Part::Month2 => out.push_str(&format!("{:02}", dt.m)),
                Part::Month => out.push_str(&dt.m.to_string()),
                Part::Day2 => out.push_str(&format!("{:02}", dt.d)),
                Part::Day => out.push_str(&dt.d.to_string()),
                Part::Hour => out.push_str(&format!("{:02}", dt.h)),
                Part::Minute => out.push_str(&format!("{:02}", dt.mi)),
                Part::Second => out.push_str(&format!("{:02}", dt.s)),
            }
        }
        out
    }
}

fn valid(dt: &DateTime) -> bool {
    let leap = (dt.y % 4 == 0 && dt.y % 100 != 0) || dt.y % 400 == 0;
    let days = match dt.m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    dt.d >= 1 && dt.d <= days && dt.h < 24 && dt.mi < 60 && dt.s < 60
}

/// Formats essayés par la détection automatique, du plus au moins fréquent.
pub const AUTO_FORMATS: [&str; 13] = [
    "AAAA-MM-JJ",
    "JJ/MM/AAAA",
    "AAAA-MM-JJ HH:mm:ss",
    "AAAA-MM-JJTHH:mm:ss",
    "AAAA-MM-JJ HH:mm",
    "JJ/MM/AAAA HH:mm:ss",
    "JJ/MM/AAAA HH:mm",
    "J/M/AAAA",
    "JJ-MM-AAAA",
    "JJ.MM.AAAA",
    "AAAA/MM/JJ",
    "JJ/MM/AA",
    "J/M/AA",
];

thread_local! {
    static AUTO: Vec<DateFormat> = AUTO_FORMATS.iter().map(|f| DateFormat::parse(f).unwrap()).collect();
}

/// Reconnaît une date dans l'un des formats courants (jour avant mois).
pub fn parse_date_auto(s: &str) -> Option<DateTime> {
    let t = s.trim();
    if t.len() < 6 || !t.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    AUTO.with(|formats| formats.iter().find_map(|f| f.read(t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres() {
        assert_eq!(parse_number("1 234,5"), Some(1234.5));
        assert_eq!(parse_number("-12.5"), Some(-12.5));
        assert_eq!(parse_number("1.234.567"), None);
        assert_eq!(parse_number("12 €"), None);
        assert_eq!(parse_number(""), None);
        assert_eq!(format_number(1234.5, ','), "1234,5");
        assert_eq!(format_number(3.0, '.'), "3");
    }

    #[test]
    fn comparaisons() {
        assert_eq!(fold("Élodie ŒUVRE"), "elodie oeuvre");
        assert_eq!(natural_cmp("fichier2", "fichier10"), Ordering::Less);
        assert_eq!(natural_cmp("a010", "a10"), Ordering::Equal);
        assert_eq!(natural_cmp("b1", "a2"), Ordering::Greater);
    }

    #[test]
    fn dates() {
        let fr = DateFormat::parse("JJ/MM/AAAA").unwrap();
        let iso = DateFormat::parse("AAAA-MM-JJ").unwrap();
        let d = fr.read("29/02/2024").unwrap();
        assert_eq!(iso.write(&d), "2024-02-29");
        assert!(fr.read("29/02/2023").is_none());
        assert!(fr.read("2024-02-01").is_none());
        let d = parse_date_auto("5/3/24").unwrap();
        assert_eq!((d.y, d.m, d.d), (2024, 3, 5));
        let d = parse_date_auto("2024-03-05T10:20:30").unwrap();
        assert_eq!(DateFormat::parse("JJ/MM/AAAA HH:mm").unwrap().write(&d), "05/03/2024 10:20");
        assert!(parse_date_auto("12345").is_none());
    }
}
