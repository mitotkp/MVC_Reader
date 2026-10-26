//! Dibujo de las páginas (PDF y texto), selección con el ratón, subrayados,
//! menú flotante y flechas para pasar de página.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Margin, Order, Pos2, Rect, RichText,
    ScrollArea, Sense, Stroke, TextEdit, Ui, pos2, vec2,
};
use std::time::{Duration, Instant};

use super::{App, OpenDoc, PT, Popup, Tab, range_in_block, short};
use crate::doc::Document;
use crate::doc::flow::{self, DrawnText, FlowDoc, LayoutParams};
use crate::i18n::Tr;
use crate::store::{Loc, PdfFit, Settings, ThemeKind};
use crate::theme::{self, Palette, hl_color, solid};

const SEARCH_FILL: Color32 = Color32::from_rgba_premultiplied(70, 45, 0, 70);

fn current_hit_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 120, 0, 130)
}

fn selection_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(70, 120, 250, 90)
}

impl App {
    pub(super) fn viewer(&mut self, ui: &mut Ui) {
        let pal = theme::palette(self.settings.theme);
        let theme = self.settings.theme;
        let invert = self.pdf_invert();
        let st = self.settings.clone();
        let tr = self.tr();
        let Some(od) = self.doc.as_mut() else { return };
        let hits: Vec<(Loc, Loc, bool)> = self
            .search
            .hits
            .iter()
            .enumerate()
            .filter(|(_, h)| h.page == od.page || !od.doc.is_pdf())
            .map(|(i, h)| (h.start, h.end, self.search.current == Some(i)))
            .collect();
        let open_notes = if od.doc.is_pdf() {
            pdf_view(ui, od, &pal, theme, invert, &hits, tr)
        } else {
            flow_view(ui, od, &st, &pal, theme, &hits, tr)
        };
        if open_notes {
            self.tab = Tab::Notes;
            self.settings.sidebar_open = true;
        }
        let od = self.doc.as_mut().unwrap();
        if let Some(msg) = popup_ui(ui.ctx(), od, tr) {
            self.notify(msg);
        }
        // Al repaginar (otro tamaño de letra o de ventana) cambian las páginas de los resultados.
        let od = self.doc.as_mut().unwrap();
        if let Document::Flow(f) = &od.doc {
            if f.generation != self.search.layout_generation && !self.search.hits.is_empty() {
                for h in &mut self.search.hits {
                    h.page = f.page_of(h.start);
                }
                self.search.layout_generation = f.generation;
            }
        }
    }
}

/// Menú flotante de selección o de un subrayado existente. Devuelve un aviso para mostrar.
fn popup_ui(ctx: &egui::Context, od: &mut OpenDoc, tr: &'static Tr) -> Option<&'static str> {
    let popup = od.popup.as_ref()?;
    let (pos, opened) = match popup {
        Popup::Selection { pos, opened } | Popup::Highlight { pos, opened, .. } => (*pos, *opened),
    };
    let hl_id = match popup {
        Popup::Highlight { id, .. } => Some(*id),
        _ => None,
    };
    let mut close = false;
    let mut toast = None;
    let resp = egui::Area::new(Id::unique("mvc_popup"))
        .order(Order::Foreground)
        .fixed_pos(pos + vec2(0.0, 14.0))
        .constrain(true)
        .show(ctx, |ui| {
            Frame::popup(ui.style()).inner_margin(Margin::same(10)).show(ui, |ui| {
                ui.set_max_width(310.0);
                let swatches = |ui: &mut Ui, current: Option<usize>| -> Option<usize> {
                    let mut picked = None;
                    ui.horizontal(|ui| {
                        for i in 0..4 {
                            let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                            let hovered = resp.hovered();
                            ui.painter().circle_filled(r.center(), if hovered { 12.0 } else { 11.0 }, solid(i));
                            if current == Some(i) {
                                ui.painter().circle_stroke(r.center(), 14.0, Stroke::new(2.0, ui.visuals().text_color()));
                            }
                            if resp.on_hover_text(tr.colors[i]).on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                picked = Some(i);
                            }
                        }
                    });
                    picked
                };
                match hl_id {
                    None => {
                        ui.label(RichText::new(tr.highlight).size(11.5).weak());
                        if let Some(c) = swatches(ui, None) {
                            if od.add_highlight(c).is_some() {
                                toast = Some(tr.toast_highlight);
                            }
                            close = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button(format!("📋 {}", tr.copy)).clicked() {
                                if let Some((a, b)) = od.selection {
                                    let t = od.text_between(a, b);
                                    ui.ctx().copy_text(t);
                                    toast = Some(tr.toast_copied);
                                }
                                close = true;
                            }
                            if ui.button(format!("📝 {}", tr.highlight_and_note)).clicked() {
                                if let Some(id) = od.add_highlight(0) {
                                    od.popup = Some(Popup::Highlight { id, pos, opened: Instant::now() });
                                }
                            }
                        });
                    }
                    Some(id) => {
                        let Some(idx) = od.data.highlights.iter().position(|h| h.id == id) else {
                            close = true;
                            return;
                        };
                        let cur = od.data.highlights[idx].color;
                        if let Some(c) = swatches(ui, Some(cur)) {
                            od.data.highlights[idx].color = c;
                            od.dirty = true;
                        }
                        let r = ui.add(
                            TextEdit::multiline(&mut od.data.highlights[idx].note)
                                .hint_text(tr.note_for_highlight)
                                .desired_rows(3)
                                .desired_width(290.0),
                        );
                        if r.changed() {
                            od.dirty = true;
                        }
                        if opened.elapsed() < Duration::from_millis(60) {
                            r.request_focus();
                        }
                        ui.horizontal(|ui| {
                            if ui.button(format!("📋 {}", tr.copy)).clicked() {
                                ui.ctx().copy_text(od.data.highlights[idx].text.clone());
                                toast = Some(tr.toast_copied);
                            }
                            if ui.button(format!("🗑 {}", tr.remove_highlight)).clicked() {
                                od.data.highlights.remove(idx);
                                od.dirty = true;
                                close = true;
                            }
                            if ui.button(tr.done).clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });
        });
    if resp.response.clicked_elsewhere() && opened.elapsed() > Duration::from_millis(250) {
        close = true;
    }
    if close {
        od.popup = None;
        od.selection = None;
    }
    toast
}

/// Maneja selección con el ratón y clics sobre subrayados (común a PDF y texto).
fn page_pointer(od: &mut OpenDoc, resp: &egui::Response, hit: &dyn Fn(Pos2) -> Option<Loc>) {
    if resp.drag_started() {
        od.popup = None;
        od.sel_anchor = resp.interact_pointer_pos().and_then(hit);
        od.selection = None;
    }
    if resp.dragged() {
        if let (Some(a), Some(b)) = (od.sel_anchor, resp.interact_pointer_pos().and_then(hit)) {
            od.selection = if a == b { None } else { Some((a.min(b), a.max(b))) };
        }
    }
    if resp.drag_stopped() {
        if let (Some(_), Some(pos)) = (od.selection, resp.interact_pointer_pos()) {
            od.popup = Some(Popup::Selection { pos, opened: Instant::now() });
        }
        od.sel_anchor = None;
    }
    if resp.clicked() {
        let pos = resp.interact_pointer_pos();
        let loc = pos.and_then(hit);
        od.selection = None;
        od.popup = match (loc.and_then(|l| od.highlight_at(l)), pos) {
            (Some(id), Some(pos)) => Some(Popup::Highlight { id, pos, opened: Instant::now() }),
            _ => None,
        };
    }
}

/// Cinta de marcador y aviso de notas en la esquina de la página. Devuelve true si se pidió ver las notas.
fn page_badges(ui: &mut Ui, od: &mut OpenDoc, paper: Rect, pal: &Palette, tr: &Tr) -> bool {
    let mut open_notes = false;
    let marked = od.bookmark_on_page(od.page).is_some();
    let rib = Rect::from_min_size(pos2(paper.right() - 46.0, paper.top()), vec2(22.0, 34.0));
    let r = ui.interact(rib, ui.scope_id().with("ribbon"), Sense::click());
    if marked || r.hovered() {
        let c = if marked { Color32::from_rgb(214, 58, 58) } else { pal.rule };
        let pts = vec![rib.left_top(), rib.right_top(), rib.right_bottom(), pos2(rib.center().x, rib.bottom() - 8.0), rib.left_bottom()];
        ui.painter().add(egui::Shape::convex_polygon(pts, c, Stroke::NONE));
    }
    if r.on_hover_text(if marked { tr.unbookmark_tip } else { tr.bookmark_tip })
        .on_hover_cursor(CursorIcon::PointingHand)
        .clicked()
    {
        od.toggle_bookmark();
    }
    let notes = od.notes_on_page(od.page);
    if notes > 0 {
        let rect = Rect::from_min_size(pos2(paper.left() + 14.0, paper.top() + 10.0), vec2(50.0, 24.0));
        let r = ui.interact(rect, ui.scope_id().with("notesbadge"), Sense::click());
        let fill = if r.hovered() { Color32::from_rgba_unmultiplied(255, 200, 0, 120) } else { Color32::from_rgba_unmultiplied(255, 200, 0, 70) };
        ui.painter().rect_filled(rect, 12.0, fill);
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, format!("📝 {notes}"), FontId::proportional(13.0), pal.text);
        if r.on_hover_text(tr.see_notes).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            open_notes = true;
        }
    }
    open_notes
}

/// Flechas redondas a los lados de la página para pasar de página con el ratón.
fn nav_arrows(ui: &mut Ui, od: &mut OpenDoc, paper: Rect, avail: Rect, pal: &Palette, tr: &Tr) {
    let space = (paper.left() - avail.left()).min(avail.right() - paper.right());
    if space < 60.0 {
        return;
    }
    let y = avail.center().y;
    let n = od.doc.page_count();
    let mut go: Option<usize> = None;
    for (right, x) in [(false, paper.left() - space / 2.0), (true, paper.right() + space / 2.0)] {
        let enabled = if right { od.page + 1 < n } else { od.page > 0 };
        if !enabled {
            continue;
        }
        let rect = Rect::from_center_size(pos2(x, y), vec2(40.0, 40.0));
        let r = ui.interact(rect, ui.scope_id().with(("nav", right)), Sense::click());
        let hovered = r.hovered();
        let p = ui.painter();
        p.circle_filled(rect.center(), 20.0, if hovered { pal.accent } else { pal.card.gamma_multiply(0.9) });
        p.circle_stroke(rect.center(), 20.0, Stroke::new(1.0, if hovered { pal.accent } else { pal.card_stroke }));
        let c = if hovered { Color32::WHITE } else { pal.weak };
        p.text(rect.center(), Align2::CENTER_CENTER, if right { "▶" } else { "◀" }, FontId::proportional(15.0), c);
        let tip = if right { tr.next_page } else { tr.prev_page };
        if r.on_hover_text(tip).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            go = Some(if right { od.page + 1 } else { od.page - 1 });
        }
    }
    if let Some(p) = go {
        od.go(p);
    }
}

fn flow_view(
    ui: &mut Ui,
    od: &mut OpenDoc,
    st: &Settings,
    pal: &Palette,
    theme: ThemeKind,
    hits: &[(Loc, Loc, bool)],
    tr: &Tr,
) -> bool {
    let ctx = ui.ctx().clone();
    let avail = ui.available_rect_before_wrap();
    ui.painter().rect_filled(avail, 0.0, pal.desk);
    let margin_x = if avail.width() < 700.0 { 22.0 } else { 56.0 };
    let paper_w = (st.text_width + 2.0 * margin_x).min(avail.width() - 24.0).max(220.0);
    let paper = Rect::from_center_size(avail.center(), vec2(paper_w, (avail.height() - 28.0).max(200.0)));
    let content = Rect::from_min_max(paper.min + vec2(margin_x, 48.0), paper.max - vec2(margin_x, 48.0));
    let params = LayoutParams {
        width: content.width().floor(),
        height: content.height().floor(),
        font_size: st.font_size,
        line_spacing: st.line_spacing,
        font: st.font,
        justify: st.justify,
    };

    let Document::Flow(fd) = &mut od.doc else { return false };
    if fd.params != Some(params) {
        let first = fd.params.is_none();
        let due = od.layout_pending.is_some_and(|t| t.elapsed() > Duration::from_millis(140));
        if first || due {
            let anchor = od.restore.take().unwrap_or(od.data.position);
            fd.relayout(&ctx, params);
            od.page = fd.page_of(anchor).min(fd.page_count() - 1);
            od.layout_pending = None;
            od.data.total_pages = fd.page_count();
            od.data.current_page = od.page;
            if first {
                od.data.position = fd.page_start(od.page);
            }
            od.dirty = true;
        } else {
            if od.layout_pending.is_none() {
                od.layout_pending = Some(Instant::now());
            }
            ctx.request_repaint_after(Duration::from_millis(150));
        }
    }
    let page = od.page.min(fd.page_count() - 1);
    od.page_rect = content;

    let painter = ui.painter_at(avail);
    painter.rect_filled(paper.translate(vec2(0.0, 4.0)).expand(1.0), 10.0, pal.shadow);
    painter.rect_filled(paper, 8.0, pal.paper);
    let drawn: Vec<DrawnText> = fd.paint_page(&ctx, &painter, page, content.min, pal.rule);

    // Fondos: subrayados, coincidencias de búsqueda y selección.
    let paint_range = |start: Loc, end: Loc, color: Color32, stroke: Option<Stroke>| {
        for d in &drawn {
            if let Some((s, e)) = range_in_block(start, end, d.block) {
                for r in flow::range_rects(&d.galley, d.origin, s, e) {
                    let r = r.intersect(d.clip);
                    if r.is_positive() {
                        painter.rect_filled(r, 3.0, color);
                        if let Some(s) = stroke {
                            painter.rect_stroke(r.expand(1.0), 3.0, s, egui::StrokeKind::Outside);
                        }
                    }
                }
            }
        }
    };
    for h in &od.data.highlights {
        paint_range(h.start, h.end, hl_color(h.color, theme), None);
    }
    let blocks = fd.page_blocks(page);
    for (a, b, current) in hits {
        if b.unit < blocks.start || a.unit >= blocks.end {
            continue;
        }
        if *current {
            paint_range(*a, *b, current_hit_fill(), Some(Stroke::new(1.5, Color32::from_rgb(230, 100, 0))));
        } else {
            paint_range(*a, *b, SEARCH_FILL, None);
        }
    }
    if let Some((a, b)) = od.selection {
        paint_range(a, b, selection_fill(), None);
    }
    FlowDoc::paint_text(&painter, &drawn, pal.text);

    // Encabezado (capítulo) y pie (número de página).
    if let Some(h) = od.chapter_for_page(page) {
        painter.text(pos2(paper.center().x, paper.top() + 24.0), Align2::CENTER_CENTER, short(&h, 80), FontId::proportional(12.0), pal.weak);
    }
    let total = od.doc.page_count();
    painter.text(
        pos2(paper.center().x, paper.bottom() - 24.0),
        Align2::CENTER_CENTER,
        format!("{} / {}", page + 1, total),
        FontId::proportional(12.0),
        pal.weak,
    );

    // Interacción: selección de texto, clics y rueda del ratón.
    let resp = ui.interact(content.expand2(vec2(16.0, 10.0)), ui.scope_id().with("flowpage"), Sense::click_and_drag());
    let hit = |pos: Pos2| -> Option<Loc> {
        let dist = |r: &Rect| (r.top() - pos.y).max(pos.y - r.bottom()).max(0.0);
        let d = drawn.iter().min_by(|a, b| dist(&a.clip).total_cmp(&dist(&b.clip)))?;
        let y = pos.y.clamp(d.clip.top() + 1.0, d.clip.bottom() - 1.0);
        let c = d.galley.cursor_from_pos(pos2(pos.x, y) - d.origin);
        Some(Loc::new(d.block, c.index.0))
    };
    if resp.hovered() {
        ctx.set_cursor_icon(CursorIcon::Text);
    }
    page_pointer(od, &resp, &hit);

    let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
    if ui.rect_contains_pointer(avail) {
        od.wheel_acc += scroll;
        if od.wheel_acc < -70.0 {
            od.wheel_acc = 0.0;
            let n = od.doc.page_count();
            if od.page + 1 < n {
                od.go(od.page + 1);
            }
        } else if od.wheel_acc > 70.0 {
            od.wheel_acc = 0.0;
            if od.page > 0 {
                od.go(od.page - 1);
            }
        }
        if scroll == 0.0 {
            od.wheel_acc *= 0.8;
        }
    }
    nav_arrows(ui, od, paper, avail, pal, tr);
    page_badges(ui, od, paper, pal, tr)
}

fn pdf_view(
    ui: &mut Ui,
    od: &mut OpenDoc,
    pal: &Palette,
    theme: ThemeKind,
    invert: bool,
    hits: &[(Loc, Loc, bool)],
    tr: &Tr,
) -> bool {
    let ctx = ui.ctx().clone();
    let avail = ui.available_rect_before_wrap();
    ui.painter().rect_filled(avail, 0.0, pal.desk);
    let page = od.page;
    let Document::Pdf(pd) = &od.doc else { return false };
    let psz = pd.sizes[page.min(pd.sizes.len() - 1)];

    let zoom_in = ctx.input(|i| i.zoom_delta());
    if zoom_in != 1.0 && ui.rect_contains_pointer(avail) {
        od.pdf_fit = PdfFit::Free;
        od.pdf_zoom = (od.pdf_zoom * zoom_in).clamp(0.25, 6.0);
    }
    // Se deja sitio a los lados para las flechas de navegación.
    let side = if avail.width() > 900.0 { 140.0 } else { 40.0 };
    let scale = match od.pdf_fit {
        PdfFit::Width => ((avail.width() - side) / psz.x).max(0.05),
        PdfFit::Page => ((avail.width() - side) / psz.x).min((avail.height() - 28.0) / psz.y).max(0.05),
        PdfFit::Free => od.pdf_zoom * PT,
    };
    if od.pdf_fit != PdfFit::Free {
        od.pdf_zoom = scale / PT;
    }
    let disp = psz * scale;

    let mut sa = ScrollArea::both().id_salt("pdfscroll").auto_shrink(false);
    if let Some(y) = od.scroll_to.take() {
        sa = sa.vertical_scroll_offset(y);
    }
    let mut open_notes = false;
    let mut page_rect = Rect::NOTHING;
    let out = sa.show(ui, |ui| {
        let w = disp.x.max(ui.available_width());
        let (area, _) = ui.allocate_exact_size(vec2(w, disp.y + 28.0), Sense::hover());
        let prect = Rect::from_min_size(pos2(area.center().x - disp.x / 2.0, area.top() + 14.0), disp);
        od.page_rect = prect;
        page_rect = prect;
        let painter = ui.painter();
        painter.rect_filled(prect.translate(vec2(0.0, 4.0)).expand(1.0), 4.0, pal.shadow);
        let ppp = ctx.pixels_per_point();
        let Document::Pdf(pd) = &mut od.doc else { return };
        let tex = pd.texture(&ctx, page, (disp.x * ppp).round() as i32, invert);
        let text = pd.text(page);
        match tex {
            Some(tex) => {
                painter.image(tex.id(), prect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                painter.rect_filled(prect, 2.0, pal.paper);
                painter.text(prect.center(), Align2::CENTER_CENTER, tr.cant_render, FontId::proportional(14.0), pal.weak);
            }
        }
        let to_screen = |r: Rect| Rect::from_min_max(prect.min + r.min.to_vec2() * scale, prect.min + r.max.to_vec2() * scale);
        let fill = |s: usize, e: usize, color: Color32, stroke: Option<Stroke>| {
            for r in text.range_rects(s, e) {
                let r = to_screen(r);
                painter.rect_filled(r, CornerRadius::same(2), color);
                if let Some(s) = stroke {
                    painter.rect_stroke(r.expand(1.0), 2.0, s, egui::StrokeKind::Outside);
                }
            }
        };
        let mul = if invert { 0.8 } else { 1.0 };
        for h in &od.data.highlights {
            if let Some((s, e)) = range_in_block(h.start, h.end, page) {
                fill(s, e, hl_color(h.color, theme).gamma_multiply(mul), None);
            }
        }
        for (a, b, current) in hits {
            if let Some((s, e)) = range_in_block(*a, *b, page) {
                if *current {
                    fill(s, e, current_hit_fill(), Some(Stroke::new(1.5, Color32::from_rgb(230, 100, 0))));
                } else {
                    fill(s, e, SEARCH_FILL, None);
                }
            }
        }
        if let Some((a, b)) = od.selection {
            if let Some((s, e)) = range_in_block(a, b, page) {
                fill(s, e, selection_fill(), None);
            }
        }

        let resp = ui.interact(prect, ui.scope_id().with("pdfpage"), Sense::click_and_drag());
        let has_text = text.rects.iter().any(|r| r.is_positive());
        if resp.hovered() && has_text {
            ctx.set_cursor_icon(CursorIcon::Text);
        }
        let t2 = text.clone();
        let hit = move |pos: Pos2| -> Option<Loc> {
            let p = ((pos - prect.min) / scale).to_pos2();
            t2.hit(p).map(|i| Loc::new(page, i))
        };
        if has_text {
            page_pointer(od, &resp, &hit);
        }
        open_notes = page_badges(ui, od, prect, pal, tr);
    });

    // Rueda del ratón en los bordes: pasar de página.
    if ui.rect_contains_pointer(avail) && zoom_in == 1.0 {
        let dy = ctx.input(|i| i.smooth_scroll_delta.y);
        let max_off = (out.content_size.y - out.inner_rect.height()).max(0.0);
        let off = out.state.clamped_offset().y;
        let at_bottom = off >= max_off - 1.0;
        let at_top = off <= 1.0;
        if (dy < 0.0 && at_bottom) || (dy > 0.0 && at_top) {
            od.wheel_acc += dy;
        } else if dy != 0.0 {
            od.wheel_acc = 0.0;
        }
        let n = od.doc.page_count();
        if od.wheel_acc < -90.0 {
            od.wheel_acc = 0.0;
            if od.page + 1 < n {
                od.go(od.page + 1);
            }
        } else if od.wheel_acc > 90.0 {
            od.wheel_acc = 0.0;
            if od.page > 0 {
                od.go(od.page - 1);
                od.scroll_to = Some(1.0e6);
            }
        }
    }
    if page_rect.is_positive() {
        let paper = Rect::from_x_y_ranges(page_rect.x_range(), avail.y_range());
        nav_arrows(ui, od, paper, avail, pal, tr);
    }
    open_notes
}
