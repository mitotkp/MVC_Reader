//! Lectura de EPUB (2 y 3): contenedor, OPF, spine, índice (nav o NCX) y
//! conversión del XHTML de cada capítulo a bloques.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::{LoadError, TocEntry};
use super::flow::{BOLD, BlockKind, Builder, FlowDoc, ITALIC, MONO};
use super::xml::{XEv, attr, walk};

type Zip = zip::ZipArchive<File>;

fn read_entry(zip: &mut Zip, name: &str) -> Option<Vec<u8>> {
    let candidates = [name.to_string(), percent_decode(name)];
    for n in candidates.iter() {
        if let Ok(mut f) = zip.by_name(n) {
            let mut buf = Vec::with_capacity(f.size() as usize);
            if f.read_to_end(&mut buf).is_ok() {
                return Some(buf);
            }
        }
    }
    None
}

fn read_text(zip: &mut Zip, name: &str) -> Option<String> {
    read_entry(zip, name).map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Resuelve `href` relativo al archivo `base` dentro del zip.
pub fn resolve(base: &str, href: &str) -> String {
    let href = percent_decode(href);
    let mut parts: Vec<&str> = match base.rfind('/') {
        Some(i) => base[..i].split('/').collect(),
        None => Vec::new(),
    };
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn split_frag(href: &str) -> (&str, Option<&str>) {
    match href.find('#') {
        Some(i) => (&href[..i], Some(&href[i + 1..])),
        None => (href, None),
    }
}

struct Html<'a> {
    b: &'a mut Builder,
    anchors: &'a mut HashMap<String, usize>,
    zip: &'a mut Zip,
    file: String,
    skip: usize,
    styles: Vec<(String, u8)>,
    kinds: Vec<(String, BlockKind)>,
    lists: Vec<(bool, usize)>,
    pre: usize,
    bullet: Option<String>,
    /// En títulos, un <span> seguido de texto pegado suele ir en otra línea por CSS
    /// (p. ej. `<h2><span>1</span>La víspera…</h2>`).
    span_break: bool,
}

fn is_block(tag: &str) -> bool {
    matches!(
        tag,
        "p" | "div" | "section" | "article" | "header" | "footer" | "aside" | "nav" | "main" | "figure"
            | "figcaption" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote" | "pre" | "dd"
            | "dt" | "tr" | "table" | "ul" | "ol" | "dl" | "body" | "center" | "address" | "caption"
    )
}

impl Html<'_> {
    fn style(&self) -> u8 {
        self.styles.iter().fold(0, |a, (_, s)| a | s)
    }

    fn parent_kind(&self) -> BlockKind {
        match self.kinds.last() {
            Some((_, k @ (BlockKind::Quote | BlockKind::ListItem | BlockKind::Pre))) => *k,
            _ => BlockKind::Para,
        }
    }

    fn start(&mut self, tag: &str, attrs: &[(String, String)]) {
        if let Some(id) = attr(attrs, "id") {
            let idx = self.b.next_index();
            self.anchors.insert(format!("{}#{}", self.file, id), idx);
        }
        if self.skip > 0 || matches!(tag, "head" | "script" | "style" | "title") {
            self.skip += 1;
            return;
        }
        if is_block(tag) {
            let kind = match tag {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => BlockKind::Heading(tag.as_bytes()[1] - b'0'),
                "li" => BlockKind::ListItem,
                "blockquote" => BlockKind::Quote,
                "pre" => BlockKind::Pre,
                _ => self.parent_kind(),
            };
            match tag {
                "ul" => self.lists.push((false, 0)),
                "ol" => self.lists.push((true, 0)),
                "li" => {
                    let bullet = match self.lists.last_mut() {
                        Some((true, n)) => {
                            *n += 1;
                            format!("{n}. ")
                        }
                        _ => "• ".to_string(),
                    };
                    self.bullet = Some(bullet);
                }
                "pre" => self.pre += 1,
                _ => {}
            }
            self.b.start(kind);
            self.kinds.push((tag.to_string(), kind));
            return;
        }
        match tag {
            "b" | "strong" => self.styles.push((tag.to_string(), BOLD)),
            "i" | "em" | "cite" | "var" | "dfn" => self.styles.push((tag.to_string(), ITALIC)),
            "code" | "tt" | "kbd" | "samp" => self.styles.push((tag.to_string(), MONO)),
            "br" => self.b.line_break(),
            "hr" => self.b.rule(),
            "td" | "th" => {}
            "img" | "image" => {
                let src = attr(attrs, "src").or_else(|| attr(attrs, "href"));
                if let Some(src) = src {
                    let path = resolve(&self.file, split_frag(src).0);
                    let lower = path.to_ascii_lowercase();
                    if !lower.ends_with(".svg") {
                        if let Some(bytes) = read_entry(self.zip, &path) {
                            self.b.image(bytes);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: &str) {
        if self.skip > 0 {
            self.skip -= 1;
            return;
        }
        if is_block(tag) {
            if let Some(pos) = self.kinds.iter().rposition(|(t, _)| t == tag) {
                self.kinds.truncate(pos);
            }
            match tag {
                "ul" | "ol" => {
                    self.lists.pop();
                }
                "pre" => self.pre = self.pre.saturating_sub(1),
                "li" => self.bullet = None,
                _ => {}
            }
            let parent = self.kinds.last().map_or(BlockKind::Para, |(_, k)| *k);
            self.b.start(parent);
            return;
        }
        match tag {
            "td" | "th" => self.b.text("\u{A0}\u{A0}\u{A0}", 0, true),
            "span" if matches!(self.b.current_kind(), BlockKind::Heading(_)) => self.span_break = true,
            _ => {
                if let Some(pos) = self.styles.iter().rposition(|(t, _)| t == tag) {
                    self.styles.remove(pos);
                }
            }
        }
    }

    fn text(&mut self, s: &str) {
        if self.skip > 0 {
            return;
        }
        if self.span_break && !s.is_empty() {
            self.span_break = false;
            if !s.starts_with(char::is_whitespace) {
                self.b.line_break();
            }
        }
        if self.bullet.is_some() && !s.trim().is_empty() {
            let bullet = self.bullet.take().unwrap();
            self.b.text(&bullet, 0, true);
        }
        let style = self.style();
        self.b.text(s, style, self.pre > 0);
    }
}

fn parse_toc_nav(src: &str, nav_path: &str) -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let mut in_toc = false;
    let mut nav_depth = 0usize;
    let mut ol_depth = 0usize;
    let mut cur: Option<(String, String)> = None;
    walk(src, |ev| match ev {
        XEv::Start(tag, attrs) => match tag {
            "nav" => {
                nav_depth += 1;
                if attr(attrs, "type").is_some_and(|t| t.contains("toc")) {
                    in_toc = true;
                    ol_depth = 0;
                }
            }
            "ol" | "ul" if in_toc => ol_depth += 1,
            "a" if in_toc => {
                if let Some(h) = attr(attrs, "href") {
                    cur = Some((resolve(nav_path, h.split('#').next().unwrap_or(""))
                        + &h.find('#').map(|i| h[i..].to_string()).unwrap_or_default(), String::new()));
                }
            }
            _ => {}
        },
        XEv::End(tag) => match tag {
            "nav" => {
                nav_depth = nav_depth.saturating_sub(1);
                in_toc = false;
            }
            "ol" | "ul" if in_toc => ol_depth = ol_depth.saturating_sub(1),
            "a" => {
                if let Some((href, title)) = cur.take() {
                    let t = title.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        out.push((t, ol_depth.saturating_sub(1), href));
                    }
                }
            }
            _ => {}
        },
        XEv::Text(t) => {
            if let Some((_, title)) = cur.as_mut() {
                title.push_str(t);
            }
        }
    });
    out
}

fn parse_toc_ncx(src: &str, ncx_path: &str) -> Vec<(String, usize, String)> {
    let mut out: Vec<(String, usize, String)> = Vec::new();
    let mut depth = 0usize;
    let mut in_label = false;
    let mut label = String::new();
    walk(src, |ev| match ev {
        XEv::Start(tag, attrs) => match tag {
            "navpoint" => {
                depth += 1;
                label.clear();
            }
            "navlabel" => in_label = true,
            "content" => {
                if let Some(src) = attr(attrs, "src") {
                    let (file, frag) = split_frag(src);
                    let mut href = resolve(ncx_path, file);
                    if let Some(f) = frag {
                        href.push('#');
                        href.push_str(f);
                    }
                    let t = label.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        out.push((t, depth.saturating_sub(1), href));
                    }
                }
            }
            _ => {}
        },
        XEv::End(tag) => match tag {
            "navpoint" => depth = depth.saturating_sub(1),
            "navlabel" => in_label = false,
            _ => {}
        },
        XEv::Text(t) => {
            if in_label {
                label.push_str(t);
            }
        }
    });
    out
}

pub fn load(path: &Path) -> Result<FlowDoc, LoadError> {
    let file = File::open(path).map_err(|e| LoadError::Open(e.to_string()))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| LoadError::Damaged(e.to_string()))?;

    let container = read_text(&mut zip, "META-INF/container.xml").ok_or(LoadError::NoContainer)?;
    let mut opf_path = String::new();
    walk(&container, |ev| {
        if let XEv::Start("rootfile", attrs) = ev {
            if opf_path.is_empty() {
                opf_path = attr(attrs, "full-path").unwrap_or("").to_string();
            }
        }
    });
    let opf = read_text(&mut zip, &opf_path).ok_or(LoadError::NoOpf)?;

    let mut title = String::new();
    let mut in_title = false;
    let mut manifest: HashMap<String, (String, String, String)> = HashMap::new();
    let mut spine: Vec<String> = Vec::new();
    let mut ncx_id = String::new();
    walk(&opf, |ev| match ev {
        XEv::Start("title", _) if title.is_empty() => in_title = true,
        XEv::End("title") => in_title = false,
        XEv::Text(t) if in_title => title.push_str(t),
        XEv::Start("item", attrs) => {
            let id = attr(attrs, "id").unwrap_or("").to_string();
            let href = attr(attrs, "href").unwrap_or("").to_string();
            let mt = attr(attrs, "media-type").unwrap_or("").to_string();
            let props = attr(attrs, "properties").unwrap_or("").to_string();
            manifest.insert(id, (resolve(&opf_path, &href), mt, props));
        }
        XEv::Start("spine", attrs) => ncx_id = attr(attrs, "toc").unwrap_or("").to_string(),
        XEv::Start("itemref", attrs) => {
            if let Some(id) = attr(attrs, "idref") {
                spine.push(id.to_string());
            }
        }
        _ => {}
    });
    let title = title.trim().to_string();
    let title = if title.is_empty() {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        title
    };

    let mut builder = Builder::new();
    let mut anchors: HashMap<String, usize> = HashMap::new();
    for id in &spine {
        let Some((file, mt, _)) = manifest.get(id).cloned() else { continue };
        if !(mt.contains("html") || mt.contains("xml")) {
            continue;
        }
        let Some(src) = read_text(&mut zip, &file) else { continue };
        anchors.insert(file.clone(), builder.next_index());
        let mut h = Html {
            b: &mut builder,
            anchors: &mut anchors,
            zip: &mut zip,
            file: file.clone(),
            skip: 0,
            styles: Vec::new(),
            kinds: Vec::new(),
            lists: Vec::new(),
            pre: 0,
            bullet: None,
            span_break: false,
        };
        walk(&src, |ev| match ev {
            XEv::Start(tag, attrs) => h.start(tag, attrs),
            XEv::End(tag) => h.end(tag),
            XEv::Text(t) => h.text(t),
        });
        builder.start(BlockKind::Para);
    }

    // Índice: primero el nav de EPUB 3, si no hay, el NCX de EPUB 2.
    let mut raw_toc = Vec::new();
    if let Some((nav, _, _)) = manifest.values().find(|(_, _, p)| p.split_whitespace().any(|x| x == "nav")).cloned() {
        if let Some(src) = read_text(&mut zip, &nav) {
            raw_toc = parse_toc_nav(&src, &nav);
        }
    }
    if raw_toc.is_empty() {
        let ncx = manifest
            .get(&ncx_id)
            .or_else(|| manifest.values().find(|(_, mt, _)| mt == "application/x-dtbncx+xml"))
            .map(|(p, _, _)| p.clone());
        if let Some(ncx) = ncx {
            if let Some(src) = read_text(&mut zip, &ncx) {
                raw_toc = parse_toc_ncx(&src, &ncx);
            }
        }
    }
    let total = builder.next_index();
    let toc = raw_toc
        .into_iter()
        .filter_map(|(t, depth, href)| {
            let unit = anchors
                .get(&href)
                .or_else(|| anchors.get(split_frag(&href).0))
                .copied()?;
            Some(TocEntry { title: t, depth, unit: unit.min(total.saturating_sub(1)) })
        })
        .collect();

    builder.flush();
    if builder.blocks.is_empty() {
        return Err(LoadError::NoText);
    }
    Ok(FlowDoc::new(title, builder, toc))
}
