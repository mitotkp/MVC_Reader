//! Paneles de la interfaz: barra superior, buscador, barra de estado, panel lateral,
//! pantalla de inicio, ajustes, ayuda, errores y avisos.

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Id, Key, Layout, Margin, Order,
    Rect, Response, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, UiBuilder, pos2, vec2,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{App, Tab, fmt_date, short};
use crate::doc::Document;
use crate::i18n::{Lang, fill};
use crate::store::{self, FontKind, Loc, Note, PdfFit, Settings, ThemeKind};
use crate::theme::{self, Palette, kind_color, solid};

/// Botón de herramienta: icono sin marco que se resalta al pasar el ratón o si está activo.
fn tool(ui: &mut Ui, icon: &str, tip: &str, selected: bool, enabled: bool) -> bool {
    let b = egui::Button::selectable(selected, RichText::new(icon).size(16.0)).min_size(vec2(32.0, 30.0));
    ui.add_enabled(enabled, b).on_hover_text(tip).clicked()
}

/// Grupo de controles sobre un fondo redondeado suave.
fn group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::NONE
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(3, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(add).inner
        })
        .inner
}

fn card<R>(ui: &mut Ui, pal: &Palette, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    Frame::NONE
        .fill(pal.card)
        .stroke(Stroke::new(1.0, pal.card_stroke))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
}

/// Etiqueta de color con el tipo de archivo (PDF, EPUB, DOCX).
fn kind_badge(ui: &mut Ui, kind: &str) {
    let c = kind_color(kind);
    Frame::NONE
        .fill(c.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(5))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(kind).size(11.0).color(c).strong());
        });
}

/// Barra de progreso delgada con el color de acento.
fn progress_bar(ui: &mut Ui, frac: f32, width: f32, accent: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, ui.visuals().faint_bg_color.gamma_multiply(1.6));
    p.rect_filled(rect, 3.0, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6));
    let mut fill = rect;
    fill.set_width((rect.width() * frac.clamp(0.0, 1.0)).max(6.0));
    p.rect_filled(fill, 3.0, accent);
    resp
}

fn section_title(ui: &mut Ui, text: &str, pal: &Palette) {
    ui.add_space(6.0);
    ui.label(RichText::new(text.to_uppercase()).size(11.0).color(pal.weak).strong());
    ui.add_space(2.0);
}

fn empty_state(ui: &mut Ui, icon: &str, text: &str, pal: &Palette) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(icon).size(34.0).color(pal.weak.gamma_multiply(0.7)));
        ui.add_space(4.0);
        ui.label(RichText::new(text).color(pal.weak));
    });
}

impl App {
    pub(super) fn top_bar(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let mut open_dialog = false;
        let mut close = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if self.doc.is_some() {
                let icon = if self.settings.sidebar_open { "◧" } else { "☰" };
                if tool(ui, icon, tr.sidebar_tip, self.settings.sidebar_open, true) {
                    self.settings.sidebar_open = !self.settings.sidebar_open;
                    self.settings_dirty = true;
                }
            } else if let Some(logo) = &self.logo {
                ui.add(egui::Image::new((logo.id(), vec2(24.0, 24.0))));
                ui.label(RichText::new("MVCreader").strong().size(15.0));
            }
            let open_btn = egui::Button::new(RichText::new(format!("📂  {}", tr.open)).color(Color32::WHITE))
                .fill(pal.accent)
                .min_size(vec2(0.0, 30.0));
            if ui.add(open_btn).on_hover_text(tr.open_tip).clicked() {
                open_dialog = true;
            }
            if self.doc.is_some() && tool(ui, "🏠", tr.home_tip, false, true) {
                close = true;
            }

            if let Some(od) = &mut self.doc {
                let n = od.doc.page_count();
                let ready = od.layout_ready();
                ui.add_space(6.0);
                group(ui, |ui| {
                    if tool(ui, "⏮", tr.first_page, false, od.page > 0) {
                        od.go(0);
                    }
                    if tool(ui, "◀", tr.prev_page, false, od.page > 0) {
                        od.go(od.page.saturating_sub(1));
                    }
                    if !self.page_input_focus {
                        self.page_input = if ready { (od.page + 1).to_string() } else { "…".into() };
                    }
                    let resp = ui.add(
                        TextEdit::singleline(&mut self.page_input)
                            .desired_width(44.0)
                            .align(Align2::CENTER_CENTER)
                            .margin(Margin::symmetric(4, 4)),
                    );
                    if self.page_input_focus && !resp.has_focus() && !resp.lost_focus() {
                        resp.request_focus();
                    }
                    if resp.gained_focus() || resp.has_focus() {
                        self.page_input_focus = true;
                    }
                    if resp.lost_focus() {
                        self.page_input_focus = false;
                        if ui.input(|i| i.key_pressed(Key::Enter)) {
                            if let Ok(p) = self.page_input.trim().parse::<usize>() {
                                od.go(p.max(1) - 1);
                            }
                        }
                    }
                    resp.on_hover_text(tr.page_input_tip);
                    ui.label(RichText::new(format!(" / {} ", if ready { n.to_string() } else { "…".into() })).color(pal.weak));
                    if tool(ui, "▶", tr.next_page, false, od.page + 1 < n) {
                        od.go(od.page + 1);
                    }
                    if tool(ui, "⏭", tr.last_page, false, od.page + 1 < n) {
                        od.go(usize::MAX);
                    }
                });

                group(ui, |ui| {
                    if od.doc.is_pdf() {
                        if tool(ui, "−", tr.zoom_out, false, true) {
                            od.pdf_fit = PdfFit::Free;
                            od.pdf_zoom = (od.pdf_zoom / 1.15).max(0.25);
                        }
                        ui.label(RichText::new(format!("{:.0} %", od.pdf_zoom * 100.0)).monospace().size(12.5));
                        if tool(ui, "+", tr.zoom_in, false, true) {
                            od.pdf_fit = PdfFit::Free;
                            od.pdf_zoom = (od.pdf_zoom * 1.15).min(6.0);
                        }
                        let w = egui::Button::selectable(od.pdf_fit == PdfFit::Width, format!("↔ {}", tr.fit_width)).min_size(vec2(0.0, 30.0));
                        if ui.add(w).on_hover_text(tr.fit_width_tip).clicked() {
                            od.pdf_fit = PdfFit::Width;
                        }
                        let p = egui::Button::selectable(od.pdf_fit == PdfFit::Page, format!("⬚ {}", tr.fit_page)).min_size(vec2(0.0, 30.0));
                        if ui.add(p).on_hover_text(tr.fit_page_tip).clicked() {
                            od.pdf_fit = PdfFit::Page;
                        }
                    } else {
                        if tool(ui, "A−", tr.font_smaller, false, true) {
                            self.settings.font_size = (self.settings.font_size - 1.0).max(11.0);
                            self.settings_dirty = true;
                        }
                        ui.label(RichText::new(format!("{:.0}", self.settings.font_size)).monospace().size(12.5));
                        if tool(ui, "A+", tr.font_bigger, false, true) {
                            self.settings.font_size = (self.settings.font_size + 1.0).min(48.0);
                            self.settings_dirty = true;
                        }
                    }
                });
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if tool(ui, "?", tr.help_tip, self.show_help, true) {
                    self.show_help = !self.show_help;
                }
                let (icon, next) = match self.settings.theme {
                    ThemeKind::Light => ("☀", ThemeKind::Sepia),
                    ThemeKind::Sepia => ("📜", ThemeKind::Dark),
                    ThemeKind::Dark => ("🌙", ThemeKind::Light),
                };
                if tool(ui, icon, tr.theme_tip, false, true) {
                    self.settings.theme = next;
                    self.settings_dirty = true;
                }
                if tool(ui, "⚙", tr.settings_tip, self.show_settings, true) {
                    self.show_settings = !self.show_settings;
                }
                if self.doc.is_some() {
                    if tool(ui, "🔍", tr.search_tip, self.search.open, true) {
                        self.search.open = !self.search.open;
                        self.search.focus = self.search.open;
                    }
                    let marked = self.doc.as_ref().is_some_and(|od| od.bookmark_on_page(od.page).is_some());
                    let tip = if marked { tr.unbookmark_tip } else { tr.bookmark_tip };
                    let txt = RichText::new("🔖").size(16.0).color(if marked { Color32::from_rgb(220, 60, 60) } else { ui.visuals().text_color() });
                    if ui.add(egui::Button::selectable(false, txt).min_size(vec2(32.0, 30.0))).on_hover_text(tip).clicked() {
                        self.toggle_bookmark();
                    }
                    ui.add_space(6.0);
                    let title = self.doc.as_ref().map(|od| od.data.title.clone()).unwrap_or_default();
                    ui.add(egui::Label::new(RichText::new(title).strong()).truncate());
                }
            });
        });
        if open_dialog {
            self.open_dialog();
        }
        if close {
            self.close_doc();
        }
    }

    pub(super) fn search_bar(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let mut run = false;
        let mut step: Option<bool> = None;
        ui.horizontal(|ui| {
            let resp = ui.add(
                TextEdit::singleline(&mut self.search.query)
                    .hint_text(format!("🔍  {}", tr.search_hint))
                    .desired_width(300.0)
                    .margin(Margin::symmetric(8, 5)),
            );
            if self.search.focus {
                resp.request_focus();
                self.search.focus = false;
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                let back = ui.input(|i| i.modifiers.shift);
                if self.search.query.trim() != self.search.searched {
                    run = true;
                } else {
                    step = Some(!back);
                }
                resp.request_focus();
            }
            if ui.add(egui::Button::new(tr.search_btn).min_size(vec2(0.0, 28.0))).clicked() {
                run = true;
            }
            group(ui, |ui| {
                if tool(ui, "▲", tr.prev_hit, false, !self.search.hits.is_empty()) {
                    step = Some(false);
                }
                if tool(ui, "▼", tr.next_hit, false, !self.search.hits.is_empty()) {
                    step = Some(true);
                }
            });
            let n = self.search.hits.len();
            if !self.search.searched.is_empty() {
                if n == 0 && self.search.pdf_next.is_none() {
                    ui.label(RichText::new(tr.no_results).color(ui.visuals().error_fg_color));
                } else {
                    let cur = self.search.current.map_or(0, |c| c + 1);
                    let total = format!("{n}{}", if n >= 5000 { "+" } else { "" });
                    ui.label(RichText::new(fill(tr.n_of_m, &[&cur, &total])).strong());
                }
            }
            if let (Some(p), Some(od)) = (self.search.pdf_next, &self.doc) {
                ui.spinner();
                ui.label(RichText::new(fill(tr.searching, &[&(p + 1), &od.doc.page_count()])).color(pal.weak));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if tool(ui, "✕", tr.close_esc, false, true) {
                    self.search.open = false;
                }
                ui.label(RichText::new(tr.search_note).size(11.5).color(pal.weak));
            });
        });
        if run {
            self.run_search();
            if !self.search.hits.is_empty() || self.search.pdf_next.is_some() {
                self.tab = Tab::Search;
            }
        } else if let Some(fwd) = step {
            self.step_hit(fwd);
        }
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let Some(od) = &self.doc else { return };
        let n = od.doc.page_count();
        let ready = od.layout_ready();
        let mut open_notes = false;
        let mut fullscreen = false;
        ui.horizontal(|ui| {
            if ready {
                ui.label(RichText::new(fill(tr.page_x_of_y, &[&(od.page + 1), &n])).size(12.5));
                progress_bar(ui, od.progress(), 150.0, pal.accent);
                ui.label(RichText::new(format!("{:.0} %", od.progress() * 100.0)).size(12.5).color(pal.weak));
            } else {
                ui.spinner();
                ui.label(tr.paginating);
            }
            if let Some(ch) = od.chapter_for_page(od.page) {
                ui.add_space(8.0);
                ui.add(egui::Label::new(RichText::new(short(&ch, 80)).size(12.5).color(pal.weak)).truncate());
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new("⛶").size(13.0)).frame(false)).on_hover_text(tr.fullscreen_tip).clicked() {
                    fullscreen = true;
                }
                kind_badge(ui, od.kind);
                let notes = od.notes_on_page(od.page);
                if notes > 0 && ui.link(RichText::new(format!("📝 {}", fill(tr.notes_on_page, &[&notes]))).size(12.5)).clicked() {
                    open_notes = true;
                }
            });
        });
        if open_notes {
            self.tab = Tab::Notes;
            self.settings.sidebar_open = true;
        }
        if fullscreen {
            self.toggle_fullscreen(ui.ctx());
        }
    }

    // ---------------------------------------------------------------- panel lateral

    pub(super) fn sidebar(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let Some(od) = self.doc.as_mut() else { return };

        // Cabecera: título, formato y progreso.
        ui.add(egui::Label::new(RichText::new(&od.data.title).size(15.0).family(FontFamily::Name("ui-semibold".into()))).wrap());
        ui.horizontal(|ui| {
            kind_badge(ui, od.kind);
            if od.layout_ready() {
                ui.label(RichText::new(fill(tr.page_x_of_y, &[&(od.page + 1), &od.doc.page_count()])).size(12.0).color(pal.weak));
            }
        });
        progress_bar(ui, od.progress(), ui.available_width(), pal.accent);
        ui.add_space(8.0);

        // Pestañas como control segmentado.
        let mut tabs = vec![
            (Tab::Toc, "📑", tr.tab_toc, None),
            (Tab::Bookmarks, "🔖", tr.tab_bookmarks, Some(od.data.bookmarks.len())),
            (Tab::Highlights, "🖍", tr.tab_highlights, Some(od.data.highlights.len())),
            (Tab::Notes, "📝", tr.tab_notes, Some(od.data.notes.len())),
        ];
        if !self.search.searched.is_empty() {
            tabs.push((Tab::Search, "🔍", tr.tab_search, Some(self.search.hits.len())));
        }
        if self.tab == Tab::Search && self.search.searched.is_empty() {
            self.tab = Tab::Toc;
        }
        Frame::NONE
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(CornerRadius::same(9))
            .inner_margin(Margin::same(3))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let w = (ui.available_width() - 2.0 * (tabs.len() as f32 - 1.0)) / tabs.len() as f32;
                ui.horizontal(|ui| {
                    for (tab, icon, name, count) in &tabs {
                        let text = match count {
                            Some(c) if *c > 0 => format!("{icon} {c}"),
                            _ => icon.to_string(),
                        };
                        let selected = self.tab == *tab;
                        let mut rt = RichText::new(text).size(14.0);
                        if selected {
                            rt = rt.color(Color32::WHITE).strong();
                        }
                        let b = egui::Button::selectable(selected, rt).min_size(vec2(w, 30.0));
                        if ui.add(b).on_hover_text(*name).clicked() {
                            self.tab = *tab;
                        }
                    }
                });
            });
        let current_name = tabs.iter().find(|t| t.0 == self.tab).map_or("", |t| t.2);
        ui.add_space(6.0);
        ui.label(RichText::new(current_name).size(16.0).family(FontFamily::Name("ui-semibold".into())));
        ui.add_space(2.0);

        let mut goto: Option<usize> = None;
        let mut goto_loc: Option<Loc> = None;
        let mut export = false;
        let mut toggle = false;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match self.tab {
            Tab::Toc => {
                let toc = od.doc.toc();
                if toc.is_empty() {
                    empty_state(ui, "📑", tr.no_toc, &pal);
                }
                // Mientras se pagina, las páginas del índice todavía no son válidas.
                let cur = if od.layout_ready() { od.doc.toc_current(od.page) } else { None };
                for (i, e) in toc.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_space(e.depth.min(6) as f32 * 14.0);
                        let selected = cur == Some(i);
                        let mut txt = RichText::new(&e.title);
                        if e.depth == 0 {
                            txt = txt.family(FontFamily::Name("ui-semibold".into()));
                        }
                        if selected {
                            txt = txt.color(Color32::WHITE);
                        }
                        let r = ui.add(egui::Button::selectable(selected, txt).wrap());
                        if r.clicked() {
                            goto_loc = Some(Loc::new(e.unit, 0));
                        }
                        // Mantener visible el capítulo actual cuando cambia la página.
                        if selected && self.toc_scrolled != Some(od.page) {
                            r.scroll_to_me(Some(Align::Center));
                            self.toc_scrolled = Some(od.page);
                        }
                    });
                }
            }
            Tab::Bookmarks => {
                if ui.button(format!("🔖  {}", tr.toggle_bookmark)).clicked() {
                    toggle = true;
                }
                ui.add_space(6.0);
                if od.data.bookmarks.is_empty() {
                    empty_state(ui, "🔖", tr.no_bookmarks, &pal);
                }
                let mut remove = None;
                for (i, b) in od.data.bookmarks.iter_mut().enumerate() {
                    let page = od.doc.page_of(b.loc);
                    card(ui, &pal, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("🔖").color(Color32::from_rgb(220, 60, 60)));
                            if ui.link(RichText::new(fill(tr.page_short, &[&(page + 1)])).strong()).clicked() {
                                goto = Some(page);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(egui::Button::new("✕").small().frame(false)).on_hover_text(tr.remove_bookmark).clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                        if ui.add(TextEdit::singleline(&mut b.label).desired_width(f32::INFINITY)).changed() {
                            od.dirty = true;
                        }
                    });
                    ui.add_space(4.0);
                }
                if let Some(i) = remove {
                    od.data.bookmarks.remove(i);
                    od.dirty = true;
                }
            }
            Tab::Highlights => {
                ui.label(RichText::new(tr.highlights_hint).color(pal.weak));
                if !od.data.highlights.is_empty() && ui.button(format!("⬇  {}", tr.export)).clicked() {
                    export = true;
                }
                ui.add_space(6.0);
                if od.data.highlights.is_empty() {
                    empty_state(ui, "🖍", tr.no_highlights, &pal);
                }
                let mut remove = None;
                for h in od.data.highlights.iter_mut() {
                    let page = od.doc.page_of(h.start);
                    let r = card(ui, &pal, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            if ui.link(RichText::new(fill(tr.page_short, &[&(page + 1)])).strong()).clicked() {
                                goto = Some(page);
                            }
                            ui.label(RichText::new(fmt_date(h.created)).size(11.0).color(pal.weak));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(egui::Button::new("✕").small().frame(false)).on_hover_text(tr.remove_highlight).clicked() {
                                    remove = Some(h.id);
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.add_space(4.0);
                            ui.add(egui::Label::new(RichText::new(short(&h.text, 280)).italics()).wrap());
                        });
                        let r = ui.add(
                            TextEdit::multiline(&mut h.note)
                                .hint_text(tr.add_note_hint)
                                .desired_rows(1)
                                .desired_width(f32::INFINITY),
                        );
                        if r.changed() {
                            od.dirty = true;
                        }
                    });
                    // Franja de color a la izquierda de la tarjeta.
                    let strip = Rect::from_min_size(r.response.rect.min, vec2(5.0, r.response.rect.height()));
                    ui.painter().rect_filled(strip, CornerRadius { nw: 9, sw: 9, ne: 0, se: 0 }, solid(h.color));
                    ui.add_space(4.0);
                }
                if let Some(id) = remove {
                    od.data.highlights.retain(|h| h.id != id);
                    od.dirty = true;
                }
            }
            Tab::Notes => {
                let page = od.page;
                ui.horizontal_wrapped(|ui| {
                    if ui.button(format!("➕ {}", tr.note_on_page)).clicked() && od.layout_ready() {
                        let loc = od.doc.page_start(page);
                        od.data.notes.push(Note { id: store::new_id(), loc: Some(loc), text: String::new(), created: store::now_secs() });
                        od.dirty = true;
                    }
                    if ui.button(format!("➕ {}", tr.general_note)).clicked() {
                        od.data.notes.push(Note { id: store::new_id(), loc: None, text: String::new(), created: store::now_secs() });
                        od.dirty = true;
                    }
                });
                if !(od.data.notes.is_empty() && od.data.highlights.is_empty()) && ui.button(format!("⬇  {}", tr.export)).clicked() {
                    export = true;
                }
                let doc = &od.doc;
                let mut remove = None;
                let mut changed = false;
                let mut section = |ui: &mut Ui, title: &str, filter: &dyn Fn(&Note) -> bool, notes: &mut Vec<Note>, empty: &str| {
                    section_title(ui, title, &pal);
                    let mut any = false;
                    for n in notes.iter_mut().filter(|n| filter(n)) {
                        any = true;
                        card(ui, &pal, |ui| {
                            ui.horizontal(|ui| {
                                if let Some(l) = n.loc {
                                    let p = doc.page_of(l);
                                    if ui.link(RichText::new(fill(tr.page_short, &[&(p + 1)])).strong()).clicked() {
                                        goto = Some(p);
                                    }
                                } else {
                                    ui.label(RichText::new(tr.general).strong());
                                }
                                ui.label(RichText::new(fmt_date(n.created)).size(11.0).color(pal.weak));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.add(egui::Button::new("✕").small().frame(false)).on_hover_text(tr.delete_note).clicked() {
                                        remove = Some(n.id);
                                    }
                                });
                            });
                            let r = ui.add(
                                TextEdit::multiline(&mut n.text)
                                    .hint_text(tr.write_note)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY),
                            );
                            if r.changed() {
                                changed = true;
                            }
                        });
                        ui.add_space(4.0);
                    }
                    if !any && !empty.is_empty() {
                        ui.label(RichText::new(empty).color(pal.weak));
                    }
                };
                let on_page = |n: &Note| n.loc.is_some_and(|l| doc.page_of(l) == page);
                section(ui, &fill(tr.this_page, &[&(page + 1)]), &on_page, &mut od.data.notes, tr.no_page_notes);
                section(ui, tr.general_notes, &|n: &Note| n.loc.is_none(), &mut od.data.notes, tr.no_general_notes);
                let others = |n: &Note| n.loc.is_some_and(|l| doc.page_of(l) != page);
                if od.data.notes.iter().any(others) {
                    section(ui, tr.other_pages, &others, &mut od.data.notes, "");
                }
                if changed {
                    od.dirty = true;
                }
                if let Some(id) = remove {
                    od.data.notes.retain(|n| n.id != id);
                    od.dirty = true;
                }
            }
            Tab::Search => {
                if self.search.hits.is_empty() && self.search.pdf_next.is_none() {
                    empty_state(ui, "🔍", tr.no_results, &pal);
                }
                for (i, h) in self.search.hits.iter().enumerate().take(1000) {
                    let selected = self.search.current == Some(i);
                    let job = {
                        let mut j = egui::text::LayoutJob::default();
                        j.append(
                            &format!("{}  ", fill(tr.page_short, &[&(h.page + 1)])),
                            0.0,
                            egui::TextFormat::simple(FontId::new(12.5, FontFamily::Name("ui-semibold".into())), pal.accent),
                        );
                        j.append(&h.snippet, 0.0, egui::TextFormat::simple(FontId::proportional(12.5), ui.visuals().text_color()));
                        j
                    };
                    if ui.add(egui::Button::selectable(selected, job).wrap()).clicked() {
                        self.search.current = Some(i);
                        goto = Some(h.page);
                    }
                }
                if self.search.hits.len() > 1000 {
                    ui.label(RichText::new(tr.first_results).color(pal.weak));
                }
            }
        });
        if let Some(p) = goto {
            od.go(p);
        }
        if let Some(l) = goto_loc {
            od.go_loc(l);
        }
        if toggle {
            self.toggle_bookmark();
        }
        if export {
            self.export_notes();
        }
    }

    // ---------------------------------------------------------------- inicio

    pub(super) fn home(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let full = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full, 0.0, pal.desk);
        let mut open: Option<PathBuf> = None;
        let mut remove: Option<usize> = None;
        let mut clear = false;
        let mut dialog = false;
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(44.0);
                if let Some(logo) = &self.logo {
                    ui.add(egui::Image::new((logo.id(), vec2(104.0, 104.0))));
                }
                ui.add_space(6.0);
                ui.label(RichText::new("MVCreader").size(34.0).family(FontFamily::Name("ui-semibold".into())));
                ui.label(RichText::new(tr.tagline).size(15.0).color(pal.weak));
                ui.add_space(22.0);
                let b = egui::Button::new(RichText::new(format!("📂   {}", tr.open_file)).size(17.0).color(Color32::WHITE))
                    .fill(pal.accent)
                    .corner_radius(CornerRadius::same(10))
                    .min_size(vec2(240.0, 46.0));
                if ui.add(b).on_hover_text(tr.open_tip).clicked() {
                    dialog = true;
                }
                ui.add_space(4.0);
                ui.label(RichText::new(tr.drop_hint).color(pal.weak));
                ui.add_space(30.0);
            });

            let max_w = ui.available_width().min(1040.0);
            let margin = ((ui.available_width() - max_w) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(margin + 16.0);
                ui.vertical(|ui| {
                    let w = max_w - 32.0;
                    ui.set_width(w);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr.recent).size(18.0).family(FontFamily::Name("ui-semibold".into())));
                        if !self.settings.recent.is_empty() {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui.add(egui::Button::new(RichText::new(tr.clear_recent).color(pal.weak)).frame(false)).clicked() {
                                    clear = true;
                                }
                            });
                        }
                    });
                    ui.add_space(6.0);
                    if self.settings.recent.is_empty() {
                        ui.label(RichText::new(tr.no_recent).color(pal.weak));
                        return;
                    }
                    let gap = 12.0;
                    let cols = ((w + gap) / (300.0 + gap)).floor().max(1.0) as usize;
                    let card_w = (w - gap * (cols as f32 - 1.0)) / cols as f32;
                    for (row_i, row) in self.settings.recent.chunks(cols).enumerate() {
                        // Fila completa reservada de una vez; cada tarjeta se ubica dentro con su separación.
                        let (row_rect, _) = ui.allocate_exact_size(vec2(w, 104.0), Sense::hover());
                        for (j, r) in row.iter().enumerate() {
                            let i = row_i * cols + j;
                            let exists = Path::new(&r.path).exists();
                            let rect = Rect::from_min_size(row_rect.min + vec2(j as f32 * (card_w + gap), 0.0), vec2(card_w, 104.0));
                            let resp = ui.interact(rect, ui.scope_id().with(("recent", i)), Sense::click());
                            let hovered = resp.hovered() && exists;
                            let stroke = if hovered { Stroke::new(1.5, pal.accent) } else { Stroke::new(1.0, pal.card_stroke) };
                            ui.painter().rect(rect, 12.0, pal.card, stroke, egui::StrokeKind::Inside);
                            if resp.clicked() && exists {
                                open = Some(PathBuf::from(&r.path));
                            }
                            if exists {
                                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                            }
                            let inner = rect.shrink2(vec2(14.0, 12.0));
                            let mut cu = ui.new_child(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)));
                            cu.horizontal(|ui| {
                                kind_badge(ui, &r.kind);
                                let mut title = RichText::new(&r.title).size(14.5).family(FontFamily::Name("ui-semibold".into()));
                                if !exists {
                                    title = title.color(pal.weak);
                                }
                                ui.add(egui::Label::new(title).truncate().selectable(false));
                            });
                            let path = if exists { r.path.clone() } else { format!("{} ({})", r.path, tr.not_found) };
                            cu.add(egui::Label::new(RichText::new(path).size(11.5).color(pal.weak)).truncate().selectable(false));
                            cu.add_space(6.0);
                            cu.horizontal(|ui| {
                                progress_bar(ui, r.progress, (inner.width() - 120.0).max(40.0), pal.accent);
                                ui.label(
                                    RichText::new(format!("{:.0} % · {}", r.progress * 100.0, fmt_date(r.last_opened)))
                                        .size(11.5)
                                        .color(pal.weak),
                                );
                            });
                            if ui.rect_contains_pointer(rect) {
                                let x = Rect::from_center_size(pos2(rect.right() - 16.0, rect.top() + 16.0), vec2(20.0, 20.0));
                                let mut xu = ui.new_child(UiBuilder::new().max_rect(x));
                                if xu.add(egui::Button::new("✕").small().frame(false)).on_hover_text(tr.remove_recent_tip).clicked() {
                                    remove = Some(i);
                                }
                            }
                        }
                        ui.add_space(gap);
                    }
                });
            });
            ui.add_space(20.0);
        });
        if dialog {
            self.open_dialog();
        }
        if clear {
            self.settings.recent.clear();
            self.settings_dirty = true;
        }
        if let Some(i) = remove {
            self.settings.recent.remove(i);
            self.settings_dirty = true;
        }
        if open.is_some() {
            self.pending_open = open;
        }
    }

    // ---------------------------------------------------------------- ajustes

    pub(super) fn settings_panel(&mut self, ui: &mut Ui) {
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let before = self.settings.clone();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("⚙  {}", tr.settings_title)).size(18.0).family(FontFamily::Name("ui-semibold".into())));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if tool(ui, "✕", tr.close_esc, false, true) {
                    self.show_settings = false;
                }
            });
        });
        ui.add_space(4.0);
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let s = &mut self.settings;
            ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(120.0);

            section_title(ui, tr.sec_appearance, &pal);
            ui.label(tr.theme);
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 16.0) / 3.0;
                for (kind, name) in [(ThemeKind::Light, tr.light), (ThemeKind::Sepia, tr.sepia), (ThemeKind::Dark, tr.dark)] {
                    let p = theme::palette(kind);
                    let (rect, resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::click());
                    let selected = s.theme == kind;
                    ui.painter().rect_filled(rect, 10.0, p.desk);
                    let paper = Rect::from_min_max(rect.min + vec2(10.0, 8.0), pos2(rect.max.x - 10.0, rect.max.y - 22.0));
                    ui.painter().rect_filled(paper, 4.0, p.paper);
                    ui.painter().text(paper.center(), Align2::CENTER_CENTER, "Aa", FontId::new(15.0, FontFamily::Name("serif".into())), p.text);
                    ui.painter().text(pos2(rect.center().x, rect.bottom() - 11.0), Align2::CENTER_CENTER, name, FontId::proportional(11.5), p.text);
                    let stroke = if selected { Stroke::new(2.0, pal.accent) } else if resp.hovered() { Stroke::new(1.0, pal.accent) } else { Stroke::new(1.0, pal.card_stroke) };
                    ui.painter().rect_stroke(rect, 10.0, stroke, egui::StrokeKind::Inside);
                    if resp.clicked() {
                        s.theme = kind;
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(tr.language);
            ui.horizontal(|ui| {
                let cur = s.language.unwrap_or(Lang::Es);
                if ui.selectable_label(cur == Lang::Es, "Español").clicked() {
                    s.language = Some(Lang::Es);
                }
                if ui.selectable_label(cur == Lang::En, "English").clicked() {
                    s.language = Some(Lang::En);
                }
            });
            ui.add_space(6.0);
            ui.label(tr.ui_scale);
            ui.add(egui::Slider::new(&mut s.ui_scale, 0.8..=1.6).step_by(0.05).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)));

            section_title(ui, tr.sec_text, &pal);
            ui.label(tr.font_size);
            ui.add(egui::Slider::new(&mut s.font_size, 11.0..=48.0).step_by(1.0));
            ui.label(tr.font);
            ui.horizontal(|ui| {
                let serif = RichText::new("Georgia").family(FontFamily::Name("serif".into())).size(15.0);
                let sans = RichText::new("Segoe UI").family(FontFamily::Name("sans".into())).size(15.0);
                if ui.add(egui::Button::selectable(s.font == FontKind::Serif, serif).min_size(vec2(100.0, 30.0))).clicked() {
                    s.font = FontKind::Serif;
                }
                if ui.add(egui::Button::selectable(s.font == FontKind::Sans, sans).min_size(vec2(100.0, 30.0))).clicked() {
                    s.font = FontKind::Sans;
                }
            });
            ui.label(tr.line_spacing);
            ui.add(egui::Slider::new(&mut s.line_spacing, 1.0..=2.4).step_by(0.05));
            ui.label(tr.text_width);
            ui.add(egui::Slider::new(&mut s.text_width, 380.0..=1600.0).step_by(10.0).suffix(" px"));
            ui.checkbox(&mut s.justify, tr.justify);

            // Vista previa con la letra elegida.
            ui.add_space(4.0);
            let fam = crate::doc::flow::family(s.font, false);
            let size = s.font_size.min(26.0);
            let mut job = egui::text::LayoutJob::default();
            let mut fmt = egui::TextFormat::simple(FontId::new(size, fam), pal.text);
            fmt.line_height = Some(size * s.line_spacing);
            job.append(tr.preview, 0.0, fmt);
            job.justify = s.justify;
            Frame::NONE
                .fill(pal.paper)
                .stroke(Stroke::new(1.0, pal.card_stroke))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    job.wrap.max_width = ui.available_width();
                    ui.label(job);
                });

            section_title(ui, "PDF", &pal);
            ui.checkbox(&mut s.pdf_invert_dark, tr.invert);

            ui.add_space(14.0);
            if ui.button(format!("↺  {}", tr.reset)).clicked() {
                let recent = std::mem::take(&mut s.recent);
                *s = Settings {
                    recent,
                    window_size: s.window_size,
                    sidebar_open: s.sidebar_open,
                    language: s.language,
                    ..Settings::default()
                };
            }
        });
        let s = &self.settings;
        let changed = s.theme != before.theme
            || s.font_size != before.font_size
            || s.font != before.font
            || s.line_spacing != before.line_spacing
            || s.text_width != before.text_width
            || s.justify != before.justify
            || s.pdf_invert_dark != before.pdf_invert_dark
            || s.ui_scale != before.ui_scale
            || s.language != before.language;
        if changed {
            self.settings_dirty = true;
            if s.pdf_invert_dark != before.pdf_invert_dark {
                if let Some(Document::Pdf(p)) = self.doc.as_mut().map(|d| &mut d.doc) {
                    p.clear_textures();
                }
            }
        }
    }

    // ---------------------------------------------------------------- ventanas y avisos

    pub(super) fn help_window(&mut self, ctx: &egui::Context) {
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let mut open = self.show_help;
        egui::Window::new(format!("⌨  {}", tr.shortcuts_title))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                egui::Grid::new("atajos").num_columns(2).spacing([22.0, 7.0]).show(ui, |ui| {
                    for (k, d) in tr.shortcuts {
                        ui.label(
                            RichText::new(format!(" {k} "))
                                .monospace()
                                .size(12.5)
                                .background_color(ui.visuals().faint_bg_color)
                                .color(pal.accent),
                        );
                        ui.label(*d);
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                ui.label(RichText::new(tr.help_footer).color(pal.weak));
            });
        self.show_help = open;
    }

    pub(super) fn error_modal(&mut self, ctx: &egui::Context) {
        let Some(err) = self.error.clone() else { return };
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let modal = egui::Modal::new(Id::unique("error")).show(ctx, |ui| {
            ui.set_max_width(440.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚠").size(30.0).color(ui.visuals().warn_fg_color));
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr.error_title).size(17.0).family(FontFamily::Name("ui-semibold".into())));
                    ui.add_space(4.0);
                    ui.label(err);
                });
            });
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add(egui::Button::new(RichText::new(tr.ok).color(Color32::WHITE)).fill(pal.accent).min_size(vec2(90.0, 30.0)))
                    .clicked()
            })
            .inner
        });
        if modal.inner || modal.should_close() {
            self.error = None;
        }
    }

    /// Aviso grande al arrastrar un archivo sobre la ventana.
    pub(super) fn drop_overlay(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.raw.hovered_files.is_empty()) {
            return;
        }
        let tr = self.tr();
        let pal = theme::palette(self.settings.theme);
        let rect = ctx.content_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::unique("drop")));
        painter.rect_filled(rect, 0.0, pal.desk.gamma_multiply(0.85));
        painter.rect_stroke(rect.shrink(18.0), 18.0, Stroke::new(3.0, pal.accent), egui::StrokeKind::Inside);
        painter.text(rect.center() - vec2(0.0, 24.0), Align2::CENTER_CENTER, "📂", FontId::proportional(54.0), pal.accent);
        painter.text(rect.center() + vec2(0.0, 30.0), Align2::CENTER_CENTER, tr.drop_overlay, FontId::proportional(22.0), pal.text);
    }

    pub(super) fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = &self.toast else { return };
        let age = at.elapsed().as_secs_f32();
        if age > 2.4 {
            self.toast = None;
            return;
        }
        let alpha = if age > 2.0 { (2.4 - age) / 0.4 } else { 1.0 };
        let msg = msg.clone();
        egui::Area::new(Id::unique("toast"))
            .order(Order::Tooltip)
            .anchor(Align2::CENTER_BOTTOM, [0.0, -48.0])
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(alpha);
                Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(30, 30, 36, 235))
                    .corner_radius(CornerRadius::same(18))
                    .inner_margin(Margin::symmetric(16, 8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(msg).color(Color32::WHITE));
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
