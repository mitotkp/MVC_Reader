//! Lectura de DOCX: párrafos, títulos (por estilo o nivel de esquema), listas,
//! negrita/cursiva, tablas simples e imágenes incrustadas.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::LoadError;
use super::epub::resolve;
use super::flow::{BOLD, BlockKind, Builder, FlowDoc, ITALIC};
use super::xml::{XEv, attr, walk};

fn read_entry(zip: &mut zip::ZipArchive<File>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn on(attrs: &[(String, String)]) -> bool {
    !matches!(attr(attrs, "val"), Some("0" | "false" | "off" | "none"))
}

fn heading_from_name(name: &str) -> Option<u8> {
    let n = name.to_lowercase();
    if n == "title" || n == "título" || n == "titulo" {
        return Some(1);
    }
    for prefix in ["heading", "título", "titulo", "encabezado"] {
        if let Some(rest) = n.strip_prefix(prefix) {
            if let Ok(l) = rest.trim().parse::<u8>() {
                return Some(l.clamp(1, 6));
            }
        }
    }
    None
}

/// styleId -> nivel de título.
fn parse_styles(src: &str) -> HashMap<String, u8> {
    let mut out = HashMap::new();
    let mut cur_id = String::new();
    let mut level: Option<u8> = None;
    walk(src, |ev| match ev {
        XEv::Start("style", attrs) => {
            cur_id = attr(attrs, "styleid").unwrap_or("").to_string();
            level = None;
        }
        XEv::Start("name", attrs) if !cur_id.is_empty() => {
            if let Some(l) = attr(attrs, "val").and_then(heading_from_name) {
                level = Some(l);
            }
        }
        XEv::Start("outlinelvl", attrs) if !cur_id.is_empty() => {
            if let Some(v) = attr(attrs, "val").and_then(|v| v.parse::<u8>().ok()) {
                if v < 9 {
                    level = Some((v + 1).min(6));
                }
            }
        }
        XEv::End("style") => {
            if let Some(l) = level.or_else(|| heading_from_name(&cur_id)) {
                out.insert(cur_id.clone(), l);
            }
            cur_id.clear();
        }
        _ => {}
    });
    out
}

pub fn load(path: &Path) -> Result<FlowDoc, LoadError> {
    let file = File::open(path).map_err(|e| LoadError::Open(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| LoadError::Damaged(e.to_string()))?;
    let doc = read_entry(&mut zip, "word/document.xml").ok_or(LoadError::NotWord)?;
    let doc = String::from_utf8_lossy(&doc).into_owned();

    let styles = read_entry(&mut zip, "word/styles.xml")
        .map(|b| parse_styles(&String::from_utf8_lossy(&b)))
        .unwrap_or_default();

    let mut rels: HashMap<String, String> = HashMap::new();
    if let Some(b) = read_entry(&mut zip, "word/_rels/document.xml.rels") {
        walk(&String::from_utf8_lossy(&b), |ev| {
            if let XEv::Start("relationship", attrs) = ev {
                if let (Some(id), Some(t)) = (attr(attrs, "id"), attr(attrs, "target")) {
                    rels.insert(id.to_string(), resolve("word/document.xml", t));
                }
            }
        });
    }

    let mut title = String::new();
    if let Some(b) = read_entry(&mut zip, "docProps/core.xml") {
        let mut in_t = false;
        walk(&String::from_utf8_lossy(&b), |ev| match ev {
            XEv::Start("title", _) => in_t = true,
            XEv::End("title") => in_t = false,
            XEv::Text(t) if in_t => title.push_str(t),
            _ => {}
        });
    }
    let title = title.trim().to_string();
    let title = if title.is_empty() {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        title
    };

    let mut b = Builder::new();
    let mut in_run = false;
    let mut in_ppr = false;
    let mut in_rpr = false;
    let mut in_t = false;
    let mut skip = 0usize;
    let mut tbl = 0usize;
    let mut style = 0u8;
    let mut bullet: Option<&str> = None;
    let mut images: Vec<String> = Vec::new();

    walk(&doc, |ev| match ev {
        XEv::Start(tag, attrs) => {
            if skip > 0 {
                skip += 1;
                return;
            }
            match tag {
                "del" | "instrtext" | "footnotereference" | "fldchar" | "commentreference" => skip = 1,
                "tbl" => {
                    tbl += 1;
                    b.start(BlockKind::Para);
                }
                "p" if tbl == 0 => {
                    b.start(BlockKind::Para);
                    bullet = None;
                }
                "ppr" => in_ppr = true,
                "pstyle" if in_ppr && tbl == 0 => {
                    if let Some(l) = attr(attrs, "val").and_then(|v| styles.get(v)) {
                        b.start(BlockKind::Heading(*l));
                    }
                }
                "outlinelvl" if in_ppr && tbl == 0 => {
                    if let Some(v) = attr(attrs, "val").and_then(|v| v.parse::<u8>().ok()) {
                        if v < 9 {
                            b.start(BlockKind::Heading((v + 1).min(6)));
                        }
                    }
                }
                "numpr" if in_ppr && tbl == 0 => {
                    if !matches!(b.current_kind(), BlockKind::Heading(_)) {
                        b.start(BlockKind::ListItem);
                        bullet = Some("• ");
                    }
                }
                "r" => {
                    in_run = true;
                    style = 0;
                }
                "rpr" if in_run => in_rpr = true,
                "b" if in_rpr && on(attrs) => style |= BOLD,
                "i" if in_rpr && on(attrs) => style |= ITALIC,
                "t" if in_run => in_t = true,
                "tab" if in_run => b.text("    ", style, true),
                "br" | "cr" if in_run => {
                    if attr(attrs, "type") != Some("page") {
                        b.line_break();
                    }
                }
                "nobreakhyphen" if in_run => b.text("-", style, true),
                "blip" => {
                    if let Some(target) = attr(attrs, "embed").and_then(|id| rels.get(id)) {
                        images.push(target.clone());
                    }
                }
                "imagedata" => {
                    if let Some(target) = attr(attrs, "id").and_then(|id| rels.get(id)) {
                        images.push(target.clone());
                    }
                }
                _ => {}
            }
        }
        XEv::End(tag) => {
            if skip > 0 {
                skip -= 1;
                return;
            }
            match tag {
                "p" => {
                    for img in images.drain(..) {
                        if let Some(bytes) = read_entry(&mut zip, &img) {
                            b.image(bytes);
                        }
                    }
                    if tbl == 0 {
                        b.start(BlockKind::Para);
                    } else {
                        b.text(" ", 0, false);
                    }
                }
                "ppr" => in_ppr = false,
                "rpr" => in_rpr = false,
                "r" => in_run = false,
                "t" => in_t = false,
                "tc" => b.text("\u{A0}\u{A0}\u{A0}", 0, true),
                "tr" => b.start(BlockKind::Para),
                "tbl" => {
                    tbl = tbl.saturating_sub(1);
                    b.start(BlockKind::Para);
                }
                _ => {}
            }
        }
        XEv::Text(t) => {
            if skip == 0 && in_t {
                if let Some(bl) = bullet.take() {
                    b.text(bl, 0, true);
                }
                b.text(t, style, true);
            }
        }
    });
    b.flush();
    if b.blocks.is_empty() {
        return Err(LoadError::NoText);
    }
    Ok(FlowDoc::new(title, b, Vec::new()))
}
