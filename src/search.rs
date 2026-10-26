//! Búsqueda de texto insensible a mayúsculas y tildes.
//! La normalización es carácter a carácter (1:1), así los índices de las
//! coincidencias sirven directamente sobre el texto original.

use crate::store::Loc;

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub start: Loc,
    pub end: Loc,
    /// Página donde aparece (0-based), calculada al mostrarla.
    pub page: usize,
    pub snippet: String,
}

pub fn fold_char(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' | 'ā' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'ē' | 'ę' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'ī' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'ø' | 'ō' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'ū' => 'u',
        'ç' => 'c',
        '\u{2019}' | '\u{2018}' | '`' | '´' => '\'',
        '\u{201C}' | '\u{201D}' | '«' | '»' => '"',
        '\u{00A0}' | '\t' | '\r' | '\n' | '\u{2002}' | '\u{2003}' | '\u{2009}' => ' ',
        '\u{2010}' | '\u{2011}' | '\u{2013}' | '\u{2014}' => '-',
        other => other,
    }
}

pub fn fold(s: &str) -> Vec<char> {
    s.chars().map(fold_char).collect()
}

/// Devuelve los rangos de caracteres `[inicio, fin)` donde aparece `needle`.
pub fn find_all(haystack: &[char], needle: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > haystack.len() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if haystack[i..i + needle.len()] == *needle {
            out.push((i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

/// Fragmento de contexto alrededor de una coincidencia.
pub fn snippet(chars: &[char], start: usize, end: usize) -> String {
    let a = start.saturating_sub(40);
    let b = (end + 60).min(chars.len());
    let mut s = String::new();
    if a > 0 {
        s.push('…');
    }
    for &c in &chars[a..b] {
        s.push(if c.is_whitespace() { ' ' } else { c });
    }
    if b < chars.len() {
        s.push('…');
    }
    s
}
