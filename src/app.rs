//! Estado y lógica de MVCreader. Los paneles están en `app/panels.rs` y el
//! dibujo de las páginas en `app/view.rs`.

mod panels;
mod view;

use eframe::egui::{
    self, ColorImage, Event, FontData, FontDefinitions, FontFamily, Frame, Key, Margin, Modifiers, Pos2,
    Rect, TextureHandle, TextureOptions, Vec2, ViewportCommand,
};
use pdfium_render::prelude::Pdfium;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::doc::pdf::{self, PdfDoc};
use crate::doc::{self, Document};
use crate::i18n::{Lang, Tr, fill};
use crate::search::{self, SearchHit};
use crate::store::{self, Bookmark, DocData, Highlight, Loc, Note, PdfFit, RecentItem, Settings, ThemeKind};
use crate::theme;

/// Escala de puntos PDF a píxeles lógicos (zoom 100 %).
const PT: f32 = 96.0 / 72.0;

static ICON_PNG: &[u8] = include_bytes!("../assets/icon.png");

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Toc,
    Bookmarks,
    Highlights,
    Notes,
    Search,
}

enum Popup {
    Selection { pos: Pos2, opened: Instant },
    Highlight { id: u64, pos: Pos2, opened: Instant },
}

struct OpenDoc {
    key: String,
    kind: &'static str,
    doc: Document,
    data: DocData,
    dirty: bool,
    page: usize,
    /// Posición pendiente de aplicar cuando termine la paginación.
    restore: Option<Loc>,
    layout_pending: Option<Instant>,
    sel_anchor: Option<Loc>,
    selection: Option<(Loc, Loc)>,
    popup: Option<Popup>,
    pdf_zoom: f32,
    pdf_fit: PdfFit,
    scroll_to: Option<f32>,
    wheel_acc: f32,
    /// Zona de la página en pantalla en el último cuadro (texto en EPUB/DOCX, hoja en PDF).
    page_rect: Rect,
}

#[derive(Default)]
struct SearchState {
    open: bool,
    query: String,
    searched: String,
    hits: Vec<SearchHit>,
    current: Option<usize>,
    /// Próxima página a revisar en un PDF (la búsqueda avanza por cuadros).
    pdf_next: Option<usize>,
    focus: bool,
    /// Ir a la primera coincidencia cuando aparezca.
    jump_pending: bool,
    /// Paginación con la que se calcularon las páginas de los resultados (EPUB/DOCX).
    layout_generation: u64,
}

pub struct App {
    settings: Settings,
    settings_dirty: bool,
    pdfium: Option<&'static Pdfium>,
    doc: Option<OpenDoc>,
    error: Option<String>,
    tab: Tab,
    search: SearchState,
    page_input: String,
    page_input_focus: bool,
    show_settings: bool,
    show_help: bool,
    fullscreen: bool,
    last_save: Instant,
    applied_theme: Option<ThemeKind>,
    applied_scale: f32,
    pending_open: Option<PathBuf>,
    win_size: Option<Vec2>,
    /// Página para la que ya se desplazó el índice hasta el capítulo actual.
    toc_scrolled: Option<usize>,
    /// Aviso breve en la parte inferior ("Texto copiado", etc.).
    toast: Option<(String, Instant)>,
    logo: Option<TextureHandle>,
}

fn load_font(path: &str) -> Option<Vec<u8>> {
    std::fs::read(Path::new("C:\\Windows\\Fonts").join(path)).ok()
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut add = |name: &str, files: &[&str]| -> Option<String> {
        for f in files {
            if let Some(bytes) = load_font(f) {
                fonts.font_data.insert(name.to_string(), Arc::new(FontData::from_owned(bytes)));
                return Some(name.to_string());
            }
        }
        None
    };
    let serif = add("serif", &["georgia.ttf", "times.ttf"]);
    let serif_b = add("serif-bold", &["georgiab.ttf", "timesbd.ttf"]);
    let sans = add("sans", &["segoeui.ttf", "arial.ttf"]);
    let sans_b = add("sans-bold", &["segoeuib.ttf", "arialbd.ttf"]);
    let ui_semibold = add("ui-semibold", &["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf"]);
    let symbols = add("symbols", &["seguisym.ttf"]);
    let fam = |main: &Option<String>| -> Vec<String> {
        let mut v: Vec<String> = main.iter().cloned().collect();
        v.extend(fallback.iter().cloned());
        v.extend(symbols.iter().cloned());
        v
    };
    let families = [
        ("serif", fam(&serif)),
        ("serif-bold", fam(&serif_b.or(serif.clone()))),
        ("sans", fam(&sans)),
        ("sans-bold", fam(&sans_b.or(sans.clone()))),
        ("ui-semibold", fam(&ui_semibold.or(sans.clone()))),
    ];
    for (name, list) in families {
        fonts.families.insert(FontFamily::Name(name.into()), list);
    }
    // La interfaz usa Segoe UI si está disponible (se ve nativa en Windows).
    if let Some(s) = &sans {
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, s.clone());
        }
    }
    if let Some(sym) = symbols {
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&fam) {
                list.push(sym.clone());
            }
        }
    }
    ctx.set_fonts(fonts);
}

fn decode_icon() -> Option<image::RgbaImage> {
    image::load_from_memory(ICON_PNG).ok().map(|i| i.to_rgba8())
}

/// Icono de la ventana (el del .exe va como recurso, ver build.rs).
pub fn icon() -> egui::IconData {
    match decode_icon() {
        Some(img) => egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() },
        None => egui::IconData::default(),
    }
}

fn range_in_block(start: Loc, end: Loc, unit: usize) -> Option<(usize, usize)> {
    if unit < start.unit || unit > end.unit {
        return None;
    }
    let s = if unit == start.unit { start.ch } else { 0 };
    let e = if unit == end.unit { end.ch } else { usize::MAX };
    (s < e).then_some((s, e))
}

fn short(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

fn fmt_date(secs: u64) -> String {
    // Fecha simple (UTC) sin dependencias extra.
    let days = (secs / 86400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{d:02}/{m:02}/{y}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

impl OpenDoc {
    fn layout_ready(&self) -> bool {
        match &self.doc {
            Document::Pdf(_) => true,
            Document::Flow(f) => f.params.is_some(),
        }
    }

    fn go(&mut self, page: usize) {
        let n = self.doc.page_count();
        self.page = page.min(n.saturating_sub(1));
        self.selection = None;
        self.popup = None;
        self.scroll_to = Some(0.0);
        if self.layout_ready() {
            self.data.position = self.doc.page_start(self.page);
            self.data.current_page = self.page;
            self.data.total_pages = n;
            self.dirty = true;
        }
    }

    fn go_loc(&mut self, loc: Loc) {
        if self.layout_ready() {
            let p = self.doc.page_of(loc);
            self.go(p);
        } else {
            self.restore = Some(loc);
        }
    }

    fn progress(&self) -> f32 {
        let n = self.doc.page_count();
        if n <= 1 { 1.0 } else { self.page as f32 / (n - 1) as f32 }
    }

    fn bookmark_on_page(&self, page: usize) -> Option<usize> {
        self.data.bookmarks.iter().position(|b| self.doc.page_of(b.loc) == page)
    }

    fn chapter_for_page(&self, page: usize) -> Option<String> {
        self.doc.toc_current(page).map(|i| self.doc.toc()[i].title.clone())
    }

    fn page_text_preview(&mut self, page: usize) -> String {
        match &mut self.doc {
            Document::Flow(f) => f.page_text(page),
            Document::Pdf(p) => {
                let t = p.text(page);
                t.slice(0, t.chars.len())
            }
        }
    }

    /// Marca o desmarca la página actual. Devuelve `Some(true)` si quedó marcada.
    fn toggle_bookmark(&mut self) -> Option<bool> {
        if !self.layout_ready() {
            return None;
        }
        let added = if let Some(i) = self.bookmark_on_page(self.page) {
            self.data.bookmarks.remove(i);
            false
        } else {
            let label = self
                .chapter_for_page(self.page)
                .unwrap_or_else(|| short(self.page_text_preview(self.page).trim(), 60));
            self.data.bookmarks.push(Bookmark { loc: self.doc.page_start(self.page), label, created: store::now_secs() });
            let doc = &self.doc;
            self.data.bookmarks.sort_by_key(|b| doc.page_of(b.loc));
            true
        };
        self.dirty = true;
        Some(added)
    }

    fn text_between(&mut self, a: Loc, b: Loc) -> String {
        match &mut self.doc {
            Document::Flow(f) => {
                let mut out = String::new();
                for unit in a.unit..=b.unit.min(f.blocks.len().saturating_sub(1)) {
                    if let Some((s, e)) = range_in_block(a, b, unit) {
                        let t: String = f.blocks[unit].text.chars().skip(s).take(e.saturating_sub(s)).collect();
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(&t);
                    }
                }
                out
            }
            Document::Pdf(p) => {
                let t = p.text(a.unit);
                t.slice(a.ch, if b.unit == a.unit { b.ch } else { usize::MAX })
            }
        }
    }

    fn add_highlight(&mut self, color: usize) -> Option<u64> {
        let (a, b) = self.selection.take()?;
        let text = self.text_between(a, b);
        if text.trim().is_empty() {
            return None;
        }
        let id = store::new_id();
        self.data.highlights.push(Highlight { id, start: a, end: b, color, text, note: String::new(), created: store::now_secs() });
        self.data.highlights.sort_by_key(|h| h.start);
        self.dirty = true;
        Some(id)
    }

    fn highlight_at(&self, loc: Loc) -> Option<u64> {
        self.data.highlights.iter().rev().find(|h| h.start <= loc && loc < h.end).map(|h| h.id)
    }

    fn notes_on_page(&self, page: usize) -> usize {
        let notes = self.data.notes.iter().filter(|n| n.loc.is_some_and(|l| self.doc.page_of(l) == page)).count();
        let hl = self
            .data
            .highlights
            .iter()
            .filter(|h| !h.note.trim().is_empty() && self.doc.page_of(h.start) == page)
            .count();
        notes + hl
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, mut settings: Settings, file: Option<PathBuf>) -> Self {
        setup_fonts(&cc.egui_ctx);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let logo = decode_icon().map(|img| {
            let ci = ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            cc.egui_ctx.load_texture("logo", ci, TextureOptions::LINEAR)
        });
        if settings.language.is_none() {
            settings.language = Some(Lang::system());
        }
        Self {
            settings,
            settings_dirty: false,
            pdfium: None,
            doc: None,
            error: None,
            tab: Tab::Toc,
            search: SearchState::default(),
            page_input: String::new(),
            page_input_focus: false,
            show_settings: false,
            show_help: false,
            fullscreen: false,
            last_save: Instant::now(),
            applied_theme: None,
            applied_scale: 0.0,
            pending_open: file,
            win_size: None,
            toc_scrolled: None,
            toast: None,
            logo,
        }
    }

    fn tr(&self) -> &'static Tr {
        self.settings.language.unwrap_or(Lang::Es).tr()
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.settings.theme) {
            theme::apply(ctx, self.settings.theme);
            if let Some(od) = &mut self.doc {
                if let Document::Pdf(p) = &mut od.doc {
                    p.clear_textures();
                }
            }
            self.applied_theme = Some(self.settings.theme);
        }
        if self.applied_scale != self.settings.ui_scale {
            ctx.set_zoom_factor(self.settings.ui_scale);
            self.applied_scale = self.settings.ui_scale;
        }
    }

    fn pdf_invert(&self) -> bool {
        self.settings.theme == ThemeKind::Dark && self.settings.pdf_invert_dark
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(self.fullscreen));
    }

    fn toggle_bookmark(&mut self) {
        let tr = self.tr();
        if let Some(added) = self.doc.as_mut().and_then(|od| od.toggle_bookmark()) {
            self.notify(if added { tr.toast_bookmark_added } else { tr.toast_bookmark_removed });
        }
    }

    // ---------------------------------------------------------------- archivos

    fn open_dialog(&mut self) {
        let tr = self.tr();
        let file = rfd::FileDialog::new()
            .set_title(tr.dialog_title)
            .add_filter(tr.filter_all, &["pdf", "epub", "docx"])
            .add_filter("PDF", &["pdf"])
            .add_filter("EPUB", &["epub"])
            .add_filter("Word (DOCX)", &["docx"])
            .pick_file();
        if let Some(f) = file {
            self.pending_open = Some(f);
        }
    }

    fn open(&mut self, path: PathBuf) {
        self.save_now();
        let tr = self.tr();
        let Some(kind) = doc::kind_of(&path) else {
            self.error = Some(tr.unsupported.into());
            return;
        };
        let key = match store::file_key(&path) {
            Ok(k) => k,
            Err(e) => {
                self.error = Some(fill(tr.cant_read, &[&path.display(), &e]));
                return;
            }
        };
        let loaded = match kind {
            "PDF" => {
                if self.pdfium.is_none() {
                    match pdf::bind_pdfium() {
                        Ok(p) => self.pdfium = Some(p),
                        Err(e) => {
                            self.error = Some(tr.load_error(&e));
                            return;
                        }
                    }
                }
                PdfDoc::load(self.pdfium.unwrap(), &path).map(Document::Pdf)
            }
            "EPUB" => doc::epub::load(&path).map(Document::Flow),
            _ => doc::docx::load(&path).map(Document::Flow),
        };
        let document = match loaded {
            Ok(d) => d,
            Err(e) => {
                self.error = Some(tr.load_error(&e));
                return;
            }
        };
        let mut data = store::load_doc_data(&key).unwrap_or_default();
        data.path = path.display().to_string();
        data.title = document.title().to_string();
        let is_pdf = document.is_pdf();
        let page = if is_pdf { data.position.unit.min(document.page_count() - 1) } else { 0 };
        let od = OpenDoc {
            key: key.clone(),
            kind,
            restore: (!is_pdf).then_some(data.position),
            pdf_zoom: data.pdf_zoom.unwrap_or(1.0),
            pdf_fit: data.pdf_fit.unwrap_or(PdfFit::Width),
            doc: document,
            data,
            dirty: false,
            page,
            layout_pending: None,
            sel_anchor: None,
            selection: None,
            popup: None,
            scroll_to: Some(0.0),
            wheel_acc: 0.0,
            page_rect: Rect::NOTHING,
        };
        self.settings.recent.retain(|r| r.key != key && r.path != od.data.path);
        self.settings.recent.insert(
            0,
            RecentItem {
                key,
                path: od.data.path.clone(),
                title: od.data.title.clone(),
                kind: kind.to_string(),
                progress: od.progress(),
                last_opened: store::now_secs(),
            },
        );
        self.settings.recent.truncate(30);
        self.settings_dirty = true;
        self.search = SearchState::default();
        self.toc_scrolled = None;
        if od.doc.toc().is_empty() && self.tab == Tab::Toc {
            self.tab = Tab::Notes;
        }
        self.doc = Some(od);
    }

    fn close_doc(&mut self) {
        self.save_now();
        self.doc = None;
        self.search = SearchState::default();
    }

    fn save_now(&mut self) {
        if let Some(od) = &mut self.doc {
            if od.layout_ready() {
                od.data.total_pages = od.doc.page_count();
                od.data.current_page = od.page;
            }
            od.data.pdf_zoom = Some(od.pdf_zoom);
            od.data.pdf_fit = Some(od.pdf_fit);
            store::save_doc_data(&od.key, &od.data);
            od.dirty = false;
            let progress = od.progress();
            if let Some(r) = self.settings.recent.iter_mut().find(|r| r.key == od.key) {
                r.progress = progress;
                r.title = od.data.title.clone();
            }
            self.settings_dirty = true;
        }
        if let Some(s) = self.win_size {
            self.settings.window_size = Some([s.x, s.y]);
        }
        if self.settings_dirty {
            store::save_settings(&self.settings);
            self.settings_dirty = false;
        }
        self.last_save = Instant::now();
    }

    fn autosave(&mut self) {
        let dirty = self.settings_dirty || self.doc.as_ref().is_some_and(|d| d.dirty);
        if dirty && self.last_save.elapsed() > Duration::from_millis(1500) {
            self.save_now();
        }
    }

    fn export_notes(&mut self) {
        let tr = self.tr();
        let Some(od) = &self.doc else { return };
        let name = format!(
            "{} - {}.md",
            od.data.title.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_"),
            tr.md_suffix
        );
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).add_filter("Markdown", &["md"]).save_file() else {
            return;
        };
        let d = &od.data;
        let mut s = format!("# {}\n\n{}: `{}`\n\n", d.title, tr.md_file, d.path);
        let general: Vec<&Note> = d.notes.iter().filter(|n| n.loc.is_none()).collect();
        if !general.is_empty() {
            s.push_str(&format!("## {}\n\n", tr.general_notes));
            for n in general {
                s.push_str(&format!("- {}\n", n.text.trim().replace('\n', "\n  ")));
            }
            s.push('\n');
        }
        if !d.bookmarks.is_empty() {
            s.push_str(&format!("## {}\n\n", tr.tab_bookmarks));
            for b in &d.bookmarks {
                s.push_str(&format!("- {} — {}\n", fill(tr.page_short, &[&(od.doc.page_of(b.loc) + 1)]), b.label));
            }
            s.push('\n');
        }
        let mut items: Vec<(usize, String)> = Vec::new();
        for h in &d.highlights {
            let mut t = format!("> {}\n>\n> — *{}*\n", h.text.trim().replace('\n', "\n> "), tr.colors[h.color.min(3)]);
            if !h.note.trim().is_empty() {
                t.push_str(&format!("\n**{}:** {}\n", tr.md_note, h.note.trim()));
            }
            items.push((od.doc.page_of(h.start), t));
        }
        for n in d.notes.iter().filter(|n| n.loc.is_some()) {
            items.push((od.doc.page_of(n.loc.unwrap()), format!("📝 {}\n", n.text.trim())));
        }
        items.sort_by_key(|(p, _)| *p);
        if !items.is_empty() {
            s.push_str(&format!("## {}\n", tr.md_by_page));
            let mut last = usize::MAX;
            for (p, t) in items {
                if p != last {
                    s.push_str(&format!("\n### {}\n\n", fill(tr.md_page, &[&(p + 1)])));
                    last = p;
                }
                s.push_str(&t);
                s.push('\n');
            }
        }
        match std::fs::write(&path, s) {
            Ok(()) => {
                let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                self.notify(fill(tr.toast_exported, &[&file]));
            }
            Err(e) => self.error = Some(fill(tr.save_failed, &[&e])),
        }
    }

    // ---------------------------------------------------------------- búsqueda

    fn run_search(&mut self) {
        let q = self.search.query.trim().to_string();
        self.search.searched = q.clone();
        self.search.hits.clear();
        self.search.current = None;
        self.search.pdf_next = None;
        let Some(od) = &mut self.doc else { return };
        if q.is_empty() {
            return;
        }
        let needle = search::fold(&q);
        match &mut od.doc {
            Document::Flow(f) => {
                for (bi, hay) in f.folded().iter().enumerate() {
                    for (s, e) in search::find_all(hay, &needle) {
                        self.search.hits.push(SearchHit {
                            start: Loc::new(bi, s),
                            end: Loc::new(bi, e),
                            page: 0,
                            snippet: search::snippet(hay, s, e),
                        });
                    }
                    if self.search.hits.len() > 5000 {
                        break;
                    }
                }
                self.search.layout_generation = f.generation;
                let doc = &od.doc;
                for h in &mut self.search.hits {
                    h.page = doc.page_of(h.start);
                }
                self.search.jump_pending = true;
            }
            Document::Pdf(_) => {
                self.search.pdf_next = Some(0);
                self.search.jump_pending = true;
            }
        }
        self.jump_to_first_hit();
    }

    /// Avanza la búsqueda en PDF unas páginas por cuadro para no congelar la ventana.
    fn pdf_search_step(&mut self, ctx: &egui::Context) {
        let Some(next) = self.search.pdf_next else { return };
        let Some(od) = &mut self.doc else { return };
        let Document::Pdf(p) = &mut od.doc else { return };
        let needle = search::fold(&self.search.searched);
        let n = p.page_count();
        let started = Instant::now();
        let mut page = next;
        while page < n && started.elapsed() < Duration::from_millis(25) && self.search.hits.len() < 5000 {
            let t = p.text_uncached(page);
            let hay: Vec<char> = t.chars.iter().map(|c| search::fold_char(*c)).collect();
            for (s, e) in search::find_all(&hay, &needle) {
                self.search.hits.push(SearchHit {
                    start: Loc::new(page, s),
                    end: Loc::new(page, e),
                    page,
                    snippet: search::snippet(&t.chars, s, e),
                });
            }
            page += 1;
        }
        self.search.pdf_next = if page < n && self.search.hits.len() < 5000 { Some(page) } else { None };
        if self.search.jump_pending {
            self.jump_to_first_hit();
        }
        ctx.request_repaint();
    }

    fn jump_to_first_hit(&mut self) {
        let Some(od) = &mut self.doc else { return };
        let cur = od.page;
        let still_searching = self.search.pdf_next.is_some();
        let idx = self.search.hits.iter().position(|h| h.page >= cur);
        let idx = match idx {
            Some(i) => Some(i),
            None if still_searching => return,
            None => (!self.search.hits.is_empty()).then_some(0),
        };
        self.search.jump_pending = false;
        if let Some(i) = idx {
            self.search.current = Some(i);
            let p = self.search.hits[i].page;
            od.go(p);
        }
    }

    fn step_hit(&mut self, forward: bool) {
        if self.search.query.trim() != self.search.searched {
            self.run_search();
            return;
        }
        let n = self.search.hits.len();
        if n == 0 {
            return;
        }
        let i = match self.search.current {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None => 0,
        };
        self.search.current = Some(i);
        let p = self.search.hits[i].page;
        if let Some(od) = &mut self.doc {
            if od.page != p {
                od.go(p);
            }
        }
    }

    // ---------------------------------------------------------------- teclado

    fn zoom(&mut self, factor: f32) {
        let Some(od) = &mut self.doc else { return };
        if od.doc.is_pdf() {
            od.pdf_fit = PdfFit::Free;
            od.pdf_zoom = (od.pdf_zoom * factor).clamp(0.25, 6.0);
            od.dirty = true;
        } else {
            self.settings.font_size = (self.settings.font_size * factor).clamp(11.0, 48.0).round();
            self.settings_dirty = true;
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let cmd = Modifiers::COMMAND;
        let none = Modifiers::NONE;
        let (open, find, f3, sf3, esc, bm, zin, zout, z0, f11, f1, close, goto, side) = ctx.input_mut(|i| {
            (
                i.consume_key(cmd, Key::O),
                i.consume_key(cmd, Key::F),
                i.consume_key(none, Key::F3) || (self.search.open && !typing && i.consume_key(none, Key::Enter)),
                i.consume_key(Modifiers::SHIFT, Key::F3),
                i.key_pressed(Key::Escape),
                i.consume_key(cmd, Key::D),
                i.consume_key(cmd, Key::Plus) || i.consume_key(cmd, Key::Equals),
                i.consume_key(cmd, Key::Minus),
                i.consume_key(cmd, Key::Num0),
                i.consume_key(none, Key::F11),
                i.consume_key(none, Key::F1),
                i.consume_key(cmd, Key::W),
                i.consume_key(cmd, Key::G),
                i.consume_key(cmd, Key::B),
            )
        });
        if open {
            self.open_dialog();
        }
        if f11 {
            self.toggle_fullscreen(ctx);
        }
        if f1 {
            self.show_help = !self.show_help;
        }
        if side {
            self.settings.sidebar_open = !self.settings.sidebar_open;
            self.settings_dirty = true;
        }
        if esc && self.doc.is_none() {
            if self.show_help {
                self.show_help = false;
            } else if self.show_settings {
                self.show_settings = false;
            }
        }
        if self.doc.is_none() {
            return;
        }
        if find {
            self.search.open = true;
            self.search.focus = true;
        }
        if f3 {
            self.step_hit(true);
        }
        if sf3 {
            self.step_hit(false);
        }
        if bm {
            self.toggle_bookmark();
        }
        if zin {
            self.zoom(1.1);
        }
        if zout {
            self.zoom(1.0 / 1.1);
        }
        if z0 {
            if let Some(od) = &mut self.doc {
                if od.doc.is_pdf() {
                    od.pdf_fit = PdfFit::Width;
                } else {
                    self.settings.font_size = Settings::default().font_size;
                    self.settings_dirty = true;
                }
            }
        }
        if close {
            self.close_doc();
            return;
        }
        if goto {
            self.page_input_focus = true;
        }
        if esc {
            let od = self.doc.as_mut().unwrap();
            if od.popup.is_some() || od.selection.is_some() {
                od.popup = None;
                od.selection = None;
            } else if self.show_help {
                self.show_help = false;
            } else if self.search.open {
                self.search.open = false;
            } else if self.show_settings {
                self.show_settings = false;
            } else if self.fullscreen {
                self.toggle_fullscreen(ctx);
            }
        }
        let copy = ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::Copy)));
        if copy && !typing {
            let od = self.doc.as_mut().unwrap();
            if let Some((a, b)) = od.selection {
                let t = od.text_between(a, b);
                ctx.copy_text(t);
                let tr = self.tr();
                self.notify(tr.toast_copied);
            }
        }
        if typing {
            return;
        }
        let od = self.doc.as_mut().unwrap();
        let pdf = od.doc.is_pdf();
        let (next, prev, home, end) = ctx.input_mut(|i| {
            let next = i.consume_key(none, Key::ArrowRight)
                || i.consume_key(none, Key::PageDown)
                || i.consume_key(none, Key::Space)
                || (!pdf && i.consume_key(none, Key::ArrowDown));
            let prev = i.consume_key(none, Key::ArrowLeft)
                || i.consume_key(none, Key::PageUp)
                || i.consume_key(Modifiers::SHIFT, Key::Space)
                || (!pdf && i.consume_key(none, Key::ArrowUp));
            (next, prev, i.consume_key(none, Key::Home), i.consume_key(none, Key::End))
        });
        if next {
            od.go(od.page + 1);
        }
        if prev && od.page > 0 {
            od.go(od.page - 1);
        }
        if home {
            od.go(0);
        }
        if end {
            od.go(usize::MAX);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        self.win_size = ctx.input(|i| i.viewport().inner_rect.map(|r| r.size()));

        let dropped = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).next());
        if let Some(p) = dropped {
            self.pending_open = Some(p);
        }
        if let Some(p) = self.pending_open.take() {
            self.open(p);
        }
        self.handle_keys(&ctx);
        self.pdf_search_step(&ctx);

        // Ctrl + rueda en EPUB/DOCX: tamaño de letra.
        if let Some(od) = &self.doc {
            if !od.doc.is_pdf() {
                let z = ctx.input(|i| i.zoom_delta());
                if z != 1.0 {
                    self.settings.font_size = (self.settings.font_size * z).clamp(11.0, 48.0);
                    self.settings_dirty = true;
                }
            }
        }

        let panel = ui.visuals().panel_fill;
        let bar = Frame::NONE.fill(panel).inner_margin(Margin::symmetric(10, 6));
        if !self.fullscreen {
            egui::Panel::top("top").frame(bar).show(ui, |ui| self.top_bar(ui));
            if self.search.open && self.doc.is_some() {
                egui::Panel::top("search").frame(bar).show(ui, |ui| self.search_bar(ui));
            }
        }
        if self.doc.is_some() {
            egui::Panel::bottom("status").frame(Frame::NONE.fill(panel).inner_margin(Margin::symmetric(12, 4))).show(ui, |ui| {
                self.status_bar(ui);
            });
        }
        if self.show_settings && !self.fullscreen {
            egui::Panel::right("settings")
                .resizable(false)
                .exact_size(330.0)
                .frame(Frame::NONE.fill(panel).inner_margin(Margin::same(14)))
                .show(ui, |ui| self.settings_panel(ui));
        }
        if self.doc.is_some() && self.settings.sidebar_open && !self.fullscreen {
            egui::Panel::left("side")
                .resizable(true)
                .default_size(310.0)
                .size_range(230.0..=560.0)
                .frame(Frame::NONE.fill(panel).inner_margin(Margin::symmetric(10, 8)))
                .show(ui, |ui| self.sidebar(ui));
        }
        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            if self.doc.is_some() {
                self.viewer(ui);
            } else {
                self.home(ui);
            }
        });

        if self.show_help {
            self.help_window(&ctx);
        }
        self.error_modal(&ctx);
        self.drop_overlay(&ctx);
        self.toast_ui(&ctx);
        self.autosave();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_now();
    }
}

#[cfg(test)]
mod tests;
