//! Documento de texto fluido (EPUB y DOCX): bloques con estilos simples que se
//! paginan según el tamaño de la ventana y las preferencias de lectura.
//!
//! Para ahorrar memoria no se guardan las galleys de egui de todo el libro:
//! al paginar solo se guardan las alturas de las líneas, y al dibujar se vuelve
//! a componer únicamente lo que está en pantalla (egui lo cachea entre cuadros).

use eframe::egui::{
    self, Align, Color32, ColorImage, FontFamily, FontId, Galley, Painter, Pos2, Rect,
    TextureHandle, TextureOptions, Vec2, pos2, text::ByteIndex, text::LayoutJob, text::LayoutSection,
    text::TextFormat, vec2,
};
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use super::TocEntry;
use crate::store::{FontKind, Loc};

pub const BOLD: u8 = 1;
pub const ITALIC: u8 = 2;
pub const MONO: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Para,
    Heading(u8),
    ListItem,
    Quote,
    Pre,
    Image,
    Rule,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub range: Range<usize>,
    pub style: u8,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
    pub runs: Vec<Run>,
    pub image: Option<usize>,
}

impl Block {
    fn new(kind: BlockKind) -> Self {
        Self { kind, text: String::new(), runs: Vec::new(), image: None }
    }
}

/// Construye la lista de bloques a partir de los eventos de los parsers.
pub struct Builder {
    pub blocks: Vec<Block>,
    pub images: Vec<Vec<u8>>,
    cur: Block,
    last_space: bool,
}

impl Builder {
    pub fn new() -> Self {
        Self { blocks: Vec::new(), images: Vec::new(), cur: Block::new(BlockKind::Para), last_space: true }
    }

    /// Índice que tendrá el próximo bloque con contenido.
    pub fn next_index(&self) -> usize {
        self.blocks.len()
    }

    pub fn start(&mut self, kind: BlockKind) {
        self.flush();
        self.cur.kind = kind;
    }

    pub fn current_kind(&self) -> BlockKind {
        self.cur.kind
    }

    pub fn flush(&mut self) {
        while self.cur.text.ends_with(' ') || self.cur.text.ends_with('\n') {
            self.cur.text.pop();
        }
        let len = self.cur.text.len();
        for r in &mut self.cur.runs {
            r.range.end = r.range.end.min(len);
        }
        self.cur.runs.retain(|r| r.range.start < r.range.end);
        let kind = self.cur.kind;
        if !self.cur.text.trim().is_empty() {
            let b = std::mem::replace(&mut self.cur, Block::new(kind));
            self.blocks.push(b);
        } else {
            self.cur = Block::new(kind);
        }
        self.last_space = true;
    }

    /// Agrega texto. Si `preserve` es falso se colapsan los espacios como en HTML.
    pub fn text(&mut self, s: &str, style: u8, preserve: bool) {
        let start = self.cur.text.len();
        for c in s.chars() {
            match c {
                '\u{00AD}' | '\u{200B}' | '\u{FEFF}' => {}
                '\t' if preserve => self.cur.text.push_str("    "),
                '\n' | '\r' if preserve => {
                    if c == '\n' {
                        self.cur.text.push('\n');
                    }
                }
                c if c.is_whitespace() && c != '\u{00A0}' => {
                    if !self.last_space {
                        self.cur.text.push(' ');
                        self.last_space = true;
                    }
                }
                c => {
                    self.cur.text.push(c);
                    self.last_space = false;
                }
            }
        }
        let end = self.cur.text.len();
        if end > start && style != 0 {
            match self.cur.runs.last_mut() {
                Some(r) if r.style == style && r.range.end == start => r.range.end = end,
                _ => self.cur.runs.push(Run { range: start..end, style }),
            }
        }
    }

    /// Salto de línea explícito (<br/>, w:br).
    pub fn line_break(&mut self) {
        while self.cur.text.ends_with(' ') {
            self.cur.text.pop();
        }
        if !self.cur.text.is_empty() {
            self.cur.text.push('\n');
        }
        self.last_space = true;
    }

    pub fn image(&mut self, bytes: Vec<u8>) {
        let kind = self.cur.kind;
        self.flush();
        self.images.push(bytes);
        self.blocks.push(Block {
            kind: BlockKind::Image,
            text: String::new(),
            runs: Vec::new(),
            image: Some(self.images.len() - 1),
        });
        self.cur.kind = kind;
    }

    pub fn rule(&mut self) {
        let kind = self.cur.kind;
        self.flush();
        self.blocks.push(Block::new(BlockKind::Rule));
        self.cur.kind = kind;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutParams {
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub line_spacing: f32,
    pub font: FontKind,
    pub justify: bool,
}

#[derive(Clone, Debug)]
struct RowInfo {
    y0: f32,
    y1: f32,
    ch0: usize,
}

#[derive(Clone, Debug, Default)]
struct BlockLayout {
    rows: Vec<RowInfo>,
    height: f32,
    space_before: f32,
    indent: f32,
    /// (página, y0, y1) de cada trozo del bloque.
    segs: Vec<(usize, f32, f32)>,
}

#[derive(Clone, Debug)]
pub struct Seg {
    pub block: usize,
    pub y0: f32,
    pub y1: f32,
    pub top: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub segs: Vec<Seg>,
}

/// Galley dibujada en pantalla, usada para seleccionar texto con el ratón.
pub struct DrawnText {
    pub block: usize,
    pub galley: Arc<Galley>,
    pub origin: Pos2,
    pub clip: Rect,
}

pub struct FlowDoc {
    pub title: String,
    pub blocks: Vec<Block>,
    pub toc: Vec<TocEntry>,
    images: Vec<Vec<u8>>,
    image_dims: HashMap<usize, (u32, u32)>,
    textures: HashMap<usize, TextureHandle>,
    texture_order: Vec<usize>,
    layout: Vec<BlockLayout>,
    pub pages: Vec<Page>,
    pub params: Option<LayoutParams>,
    /// Aumenta cada vez que se vuelve a paginar.
    pub generation: u64,
    /// Texto normalizado por bloque para la búsqueda (se calcula una vez).
    folded: Option<Vec<Vec<char>>>,
}

pub fn family(font: FontKind, bold: bool) -> FontFamily {
    let name = match (font, bold) {
        (FontKind::Serif, false) => "serif",
        (FontKind::Serif, true) => "serif-bold",
        (FontKind::Sans, false) => "sans",
        (FontKind::Sans, true) => "sans-bold",
    };
    FontFamily::Name(name.into())
}

fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.7,
        2 => 1.45,
        3 => 1.25,
        4 => 1.12,
        _ => 1.04,
    }
}

impl FlowDoc {
    pub fn new(title: String, builder: Builder, mut toc: Vec<TocEntry>) -> Self {
        let mut b = builder;
        b.flush();
        if toc.is_empty() {
            // Sin índice propio: usar los títulos del documento.
            for (i, bl) in b.blocks.iter().enumerate() {
                if let BlockKind::Heading(level) = bl.kind {
                    if level <= 3 {
                        let t: String = bl.text.replace('\n', " ").chars().take(90).collect();
                        toc.push(TocEntry { title: t, depth: (level - 1) as usize, unit: i });
                    }
                }
            }
        }
        Self {
            title,
            blocks: b.blocks,
            toc,
            images: b.images,
            image_dims: HashMap::new(),
            textures: HashMap::new(),
            texture_order: Vec::new(),
            layout: Vec::new(),
            pages: Vec::new(),
            params: None,
            generation: 0,
            folded: None,
        }
    }

    pub fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }

    fn block_indent(&self, kind: BlockKind, p: &LayoutParams) -> f32 {
        match kind {
            BlockKind::Quote | BlockKind::Pre => p.font_size * 1.5,
            BlockKind::ListItem => p.font_size * 0.8,
            _ => 0.0,
        }
    }

    pub fn job(&self, block: usize, p: &LayoutParams) -> LayoutJob {
        let b = &self.blocks[block];
        let indent = self.block_indent(b.kind, p);
        let (size, base_bold, base_italic, mono) = match b.kind {
            BlockKind::Heading(l) => (p.font_size * heading_scale(l), true, false, false),
            BlockKind::Quote => (p.font_size * 0.96, false, true, false),
            BlockKind::Pre => (p.font_size * 0.82, false, false, true),
            _ => (p.font_size, false, false, false),
        };
        let line_h = size * if b.kind == BlockKind::Pre { 1.3 } else { p.line_spacing };
        let fmt = |style: u8| {
            let bold = base_bold || style & BOLD != 0;
            let fam = if mono || style & MONO != 0 { FontFamily::Monospace } else { family(p.font, bold) };
            let mut f = TextFormat::simple(FontId::new(size, fam), Color32::PLACEHOLDER);
            f.italics = base_italic || style & ITALIC != 0;
            f.line_height = Some(line_h);
            f.valign = Align::BOTTOM;
            f
        };
        let mut job = LayoutJob::default();
        job.text = b.text.clone();
        job.wrap.max_width = (p.width - indent).max(50.0);
        job.break_on_newline = true;
        job.justify = p.justify && matches!(b.kind, BlockKind::Para | BlockKind::Quote | BlockKind::ListItem);
        let mut pos = 0;
        for r in &b.runs {
            if r.range.start > pos {
                job.sections.push(LayoutSection { leading_space: 0.0, byte_range: ByteIndex(pos)..ByteIndex(r.range.start), format: fmt(0) });
            }
            job.sections.push(LayoutSection { leading_space: 0.0, byte_range: ByteIndex(r.range.start)..ByteIndex(r.range.end), format: fmt(r.style) });
            pos = r.range.end;
        }
        if pos < b.text.len() || job.sections.is_empty() {
            job.sections.push(LayoutSection { leading_space: 0.0, byte_range: ByteIndex(pos)..ByteIndex(b.text.len()), format: fmt(0) });
        }
        job
    }

    fn image_dims(&mut self, idx: usize) -> (u32, u32) {
        if let Some(d) = self.image_dims.get(&idx) {
            return *d;
        }
        let d = image::ImageReader::new(std::io::Cursor::new(&self.images[idx]))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok())
            .unwrap_or((0, 0));
        self.image_dims.insert(idx, d);
        d
    }

    fn image_size(&mut self, idx: usize, p: &LayoutParams) -> Vec2 {
        let (w, h) = self.image_dims(idx);
        if w == 0 || h == 0 {
            return Vec2::ZERO;
        }
        let (w, h) = (w as f32, h as f32);
        let scale = (p.width / w).min(p.height * 0.92 / h).min(1.5);
        vec2(w * scale, h * scale)
    }

    /// Vuelve a paginar todo el documento con los parámetros dados.
    pub fn relayout(&mut self, ctx: &egui::Context, p: LayoutParams) {
        let mut layout = Vec::with_capacity(self.blocks.len());
        for i in 0..self.blocks.len() {
            let kind = self.blocks[i].kind;
            let space_before = match kind {
                BlockKind::Heading(_) => p.font_size * 1.3,
                BlockKind::ListItem => p.font_size * 0.25,
                BlockKind::Image | BlockKind::Rule => p.font_size * 0.8,
                _ => p.font_size * 0.55,
            };
            let mut bl = BlockLayout { space_before, indent: self.block_indent(kind, &p), ..Default::default() };
            match kind {
                BlockKind::Image => {
                    let idx = self.blocks[i].image.unwrap_or(0);
                    bl.height = self.image_size(idx, &p).y;
                }
                BlockKind::Rule => bl.height = p.font_size * 1.2,
                _ => {
                    let galley = ctx.fonts_mut(|f| f.layout_job(self.job(i, &p)));
                    let mut ch = 0;
                    for row in &galley.rows {
                        let r = row.rect();
                        bl.rows.push(RowInfo { y0: r.min.y, y1: r.max.y, ch0: ch });
                        ch += row.char_count_including_newline().0;
                    }
                    bl.height = galley.rect.height();
                }
            }
            layout.push(bl);
        }

        let page_h = p.height.max(p.font_size * 3.0);
        let mut pages: Vec<Page> = Vec::new();
        let mut cur = Page::default();
        let mut y = 0.0f32;
        // Alto mínimo con que empieza cada bloque (para no dejar un título solo al pie):
        // dos líneas, porque un párrafo no deja una sola línea suelta al final de la página.
        let lead: Vec<f32> = layout
            .iter()
            .map(|bl| match bl.rows.len() {
                0 => bl.space_before + bl.height,
                1 => bl.space_before + bl.rows[0].y1 - bl.rows[0].y0,
                _ => bl.space_before + bl.rows[1].y1 - bl.rows[0].y0,
            })
            .collect();
        for (bi, bl) in layout.iter_mut().enumerate() {
            if y > 0.0 && matches!(self.blocks[bi].kind, BlockKind::Heading(_)) && bl.height < page_h * 0.5 {
                let next = lead.get(bi + 1).copied().unwrap_or(0.0);
                if y + bl.space_before + bl.height + next > page_h {
                    pages.push(std::mem::take(&mut cur));
                    y = 0.0;
                }
            }
            if bl.rows.is_empty() {
                let h = bl.height;
                let mut sb = if y == 0.0 { 0.0 } else { bl.space_before };
                if y > 0.0 && y + sb + h > page_h {
                    pages.push(std::mem::take(&mut cur));
                    y = 0.0;
                    sb = 0.0;
                }
                cur.segs.push(Seg { block: bi, y0: 0.0, y1: h, top: y + sb });
                bl.segs.push((pages.len(), 0.0, h));
                y += sb + h;
                continue;
            }
            let mut r = 0;
            let mut first = true;
            while r < bl.rows.len() {
                let sb = if y == 0.0 || !first { 0.0 } else { bl.space_before };
                let start_y = bl.rows[r].y0;
                let mut j = r;
                while j < bl.rows.len() && y + sb + (bl.rows[j].y1 - start_y) <= page_h {
                    j += 1;
                }
                // Evitar dejar una sola línea huérfana al final de la página.
                if j > r && j < bl.rows.len() && j - r == 1 && r == 0 && y > 0.0 && bl.rows.len() > 1 {
                    j = r;
                }
                if j == r {
                    if y == 0.0 {
                        j = r + 1;
                    } else {
                        pages.push(std::mem::take(&mut cur));
                        y = 0.0;
                        continue;
                    }
                }
                let end_y = bl.rows[j - 1].y1;
                cur.segs.push(Seg { block: bi, y0: start_y, y1: end_y, top: y + sb });
                bl.segs.push((pages.len(), start_y, end_y));
                y += sb + (end_y - start_y);
                r = j;
                first = false;
                if r < bl.rows.len() {
                    pages.push(std::mem::take(&mut cur));
                    y = 0.0;
                }
            }
        }
        if !cur.segs.is_empty() || pages.is_empty() {
            pages.push(cur);
        }
        self.layout = layout;
        self.pages = pages;
        self.params = Some(p);
        self.generation += 1;
    }

    /// Página donde está una posición.
    pub fn page_of(&self, loc: Loc) -> usize {
        let Some(bl) = self.layout.get(loc.unit) else {
            // Bloque vacío o fuera de rango: buscar el siguiente con página.
            return self.pages.len().saturating_sub(1);
        };
        if bl.segs.is_empty() {
            return 0;
        }
        let y = bl
            .rows
            .iter()
            .rev()
            .find(|r| r.ch0 <= loc.ch)
            .map_or(0.0, |r| r.y0);
        bl.segs
            .iter()
            .rev()
            .find(|(_, y0, _)| *y0 <= y + 0.5)
            .map_or(bl.segs[0].0, |s| s.0)
    }

    /// Posición del inicio de una página (se guarda como "última página leída").
    pub fn page_start(&self, page: usize) -> Loc {
        let Some(pg) = self.pages.get(page) else { return Loc::default() };
        let Some(seg) = pg.segs.first() else { return Loc::default() };
        let ch = self.layout[seg.block]
            .rows
            .iter()
            .find(|r| r.y0 >= seg.y0 - 0.5)
            .map_or(0, |r| r.ch0);
        Loc::new(seg.block, ch)
    }

    /// Rango de bloques visibles en una página.
    pub fn page_blocks(&self, page: usize) -> Range<usize> {
        match self.pages.get(page) {
            Some(p) if !p.segs.is_empty() => p.segs[0].block..p.segs[p.segs.len() - 1].block + 1,
            _ => 0..0,
        }
    }

    pub fn page_text(&self, page: usize) -> String {
        let r = self.page_blocks(page);
        let mut s = String::new();
        for i in r {
            s.push_str(&self.blocks[i].text);
            s.push(' ');
        }
        s
    }

    fn texture(&mut self, ctx: &egui::Context, idx: usize) -> Option<TextureHandle> {
        if let Some(t) = self.textures.get(&idx) {
            return Some(t.clone());
        }
        let img = image::load_from_memory(self.images.get(idx)?).ok()?;
        let img = if img.width() > 1600 || img.height() > 1600 {
            img.resize(1600, 1600, image::imageops::FilterType::Triangle)
        } else {
            img
        };
        let rgba = img.to_rgba8();
        let ci = ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
        let tex = ctx.load_texture(format!("flow-img-{idx}"), ci, TextureOptions::LINEAR);
        self.textures.insert(idx, tex.clone());
        self.texture_order.push(idx);
        if self.texture_order.len() > 12 {
            let old = self.texture_order.remove(0);
            self.textures.remove(&old);
        }
        Some(tex)
    }

    /// Dibuja una página. `origin` es la esquina superior izquierda de la columna de texto.
    pub fn paint_page(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        page: usize,
        origin: Pos2,
        rule_color: Color32,
    ) -> Vec<DrawnText> {
        let mut drawn = Vec::new();
        let Some(p) = self.params else { return drawn };
        let Some(pg) = self.pages.get(page).cloned() else { return drawn };
        for seg in &pg.segs {
            let kind = self.blocks[seg.block].kind;
            let top = origin.y + seg.top;
            match kind {
                BlockKind::Image => {
                    let idx = self.blocks[seg.block].image.unwrap_or(0);
                    let size = self.image_size(idx, &p);
                    if let Some(tex) = self.texture(ctx, idx) {
                        let x = origin.x + (p.width - size.x) / 2.0;
                        let rect = Rect::from_min_size(pos2(x, top), size);
                        painter.image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                }
                BlockKind::Rule => {
                    let y = top + p.font_size * 0.6;
                    let cx = origin.x + p.width / 2.0;
                    painter.line_segment([pos2(cx - p.width * 0.15, y), pos2(cx + p.width * 0.15, y)], (1.0, rule_color));
                }
                _ => {
                    let indent = self.layout[seg.block].indent;
                    let galley = painter.layout_job(self.job(seg.block, &p));
                    let g_origin = pos2(origin.x + indent, top - seg.y0);
                    let clip = Rect::from_min_max(
                        pos2(origin.x - 4.0, top - 1.0),
                        pos2(origin.x + p.width + 40.0, top + (seg.y1 - seg.y0) + 1.0),
                    );
                    if kind == BlockKind::Quote {
                        painter.line_segment(
                            [pos2(origin.x + 4.0, top + 2.0), pos2(origin.x + 4.0, top + seg.y1 - seg.y0 - 2.0)],
                            (2.0, rule_color),
                        );
                    }
                    drawn.push(DrawnText { block: seg.block, galley, origin: g_origin, clip });
                }
            }
        }
        // El texto se pinta después (con `paint_text`) para que los subrayados queden detrás.
        drawn
    }

    pub fn paint_text(painter: &Painter, drawn: &[DrawnText], text_color: Color32) {
        for d in drawn {
            painter.with_clip_rect(d.clip.intersect(painter.clip_rect())).galley(d.origin, d.galley.clone(), text_color);
        }
    }

    /// Texto normalizado de cada bloque, para buscar.
    pub fn folded(&mut self) -> &Vec<Vec<char>> {
        if self.folded.is_none() {
            self.folded = Some(self.blocks.iter().map(|b| crate::search::fold(&b.text)).collect());
        }
        self.folded.as_ref().unwrap()
    }
}

/// Rectángulos (en coordenadas de pantalla) que cubren los caracteres `[s, e)` de una galley.
pub fn range_rects(galley: &Galley, origin: Pos2, s: usize, e: usize) -> Vec<Rect> {
    let mut out = Vec::new();
    let mut idx = 0;
    for row in &galley.rows {
        let n = row.glyphs.len();
        let (a, b) = (s.max(idx), e.min(idx + n));
        if a < b {
            let g0 = &row.glyphs[a - idx];
            let g1 = &row.glyphs[b - 1 - idx];
            let x0 = row.pos.x + g0.pos.x;
            let x1 = row.pos.x + g1.pos.x + g1.advance_width;
            out.push(Rect::from_min_max(
                origin + vec2(x0, row.pos.y),
                origin + vec2(x1, row.pos.y + row.size.y),
            ));
        }
        idx += n + row.ends_with_newline as usize;
        if idx >= e {
            break;
        }
    }
    out
}
