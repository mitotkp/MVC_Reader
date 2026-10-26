//! Envoltura mínima sobre quick-xml: eventos simplificados con nombres locales
//! en minúsculas y entidades HTML resueltas (el XHTML de los EPUB usa &nbsp; etc.).

use quick_xml::Reader;
use quick_xml::events::Event;

pub enum XEv<'a> {
    Start(&'a str, &'a [(String, String)]),
    End(&'a str),
    Text(&'a str),
}

fn local(name: &str) -> String {
    let l = name.rsplit(':').next().unwrap_or(name);
    l.to_ascii_lowercase()
}

pub fn entity(name: &str) -> Option<String> {
    if let Some(num) = name.strip_prefix('#') {
        let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            num.parse().ok()?
        };
        return char::from_u32(code).map(|c| c.to_string());
    }
    let s = match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{00A0}",
        "shy" => "",
        "mdash" => "—",
        "ndash" => "–",
        "hellip" => "…",
        "laquo" => "«",
        "raquo" => "»",
        "ldquo" => "“",
        "rdquo" => "”",
        "lsquo" => "‘",
        "rsquo" => "’",
        "bdquo" => "„",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "deg" => "°",
        "middot" => "·",
        "bull" => "•",
        "iexcl" => "¡",
        "iquest" => "¿",
        "aacute" => "á",
        "eacute" => "é",
        "iacute" => "í",
        "oacute" => "ó",
        "uacute" => "ú",
        "Aacute" => "Á",
        "Eacute" => "É",
        "Iacute" => "Í",
        "Oacute" => "Ó",
        "Uacute" => "Ú",
        "ntilde" => "ñ",
        "Ntilde" => "Ñ",
        "uuml" => "ü",
        "Uuml" => "Ü",
        "ccedil" => "ç",
        "times" => "×",
        "euro" => "€",
        "thinsp" | "ensp" | "emsp" => " ",
        "zwnj" | "zwj" => "",
        _ => return None,
    };
    Some(s.to_string())
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find(';') {
            Some(j) if j <= 10 => match entity(&after[..j]) {
                Some(rep) => {
                    out.push_str(&rep);
                    rest = &after[j + 1..];
                }
                None => {
                    out.push('&');
                    rest = after;
                }
            },
            _ => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Recorre el documento llamando a `f` con cada evento. Tolera documentos mal formados
/// (etiquetas sin cerrar, etc.) y se detiene en silencio ante un error grave.
pub fn walk(src: &str, mut f: impl FnMut(XEv)) {
    let src = src.trim_start_matches('\u{FEFF}');
    let mut reader = Reader::from_str(src);
    {
        let c = reader.config_mut();
        c.check_end_names = false;
        c.allow_unmatched_ends = true;
        c.allow_dangling_amp = true;
        c.trim_text_start = false;
        c.trim_text_end = false;
    }
    let attrs_of = |e: &quick_xml::events::BytesStart| -> Vec<(String, String)> {
        e.attributes()
            .with_checks(false)
            .flatten()
            .map(|a| (local(a.key.as_ref()), decode_entities(&a.value)))
            .collect()
    };
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = local(e.name().as_ref());
                let attrs = attrs_of(&e);
                f(XEv::Start(&name, &attrs));
            }
            Ok(Event::Empty(e)) => {
                let name = local(e.name().as_ref());
                let attrs = attrs_of(&e);
                f(XEv::Start(&name, &attrs));
                f(XEv::End(&name));
            }
            Ok(Event::End(e)) => {
                let name = local(e.name().as_ref());
                f(XEv::End(&name));
            }
            Ok(Event::Text(t)) => {
                let s = t.into_inner();
                f(XEv::Text(&s));
            }
            Ok(Event::CData(t)) => {
                let s = t.into_inner().into_owned();
                f(XEv::Text(&s));
            }
            Ok(Event::GeneralRef(r)) => {
                let name = r.into_inner().into_owned();
                if let Some(s) = entity(&name) {
                    f(XEv::Text(&s));
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

pub fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}
