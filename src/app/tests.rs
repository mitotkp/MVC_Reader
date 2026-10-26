//! Pruebas de la interfaz sin ventana: se maneja la app real con eventos sintéticos
//! de ratón y teclado. Los documentos se indican con LECTOR_EPUB / LECTOR_PDF / LECTOR_DOCX.

use super::*;
use eframe::App as _;
use eframe::egui::{PointerButton, RawInput, pos2, vec2};

struct Harness {
    ctx: egui::Context,
    app: App,
    frame: eframe::Frame,
    time: f64,
    size: Vec2,
}

fn data_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mvcreader-ui-test-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

impl Harness {
    fn new(file: PathBuf) -> Self {
        let ctx = egui::Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let app = App::new(&cc, store::load_settings(), Some(file));
        let mut h = Self { ctx, app, frame: eframe::Frame::_new_kittest(), time: 0.0, size: vec2(1280.0, 860.0) };
        h.frames(4);
        h
    }

    fn step(&mut self, events: Vec<Event>) {
        self.time += 0.05;
        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let (app, frame) = (&mut self.app, &mut self.frame);
        let _ = self.ctx.run_ui(raw, |ui| app.ui(ui, frame));
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.step(Vec::new());
        }
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        let ev = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        self.step(vec![ev(true)]);
        self.step(vec![ev(false)]);
        self.frames(1);
    }

    fn text(&mut self, s: &str) {
        self.step(vec![Event::Text(s.to_string())]);
        self.frames(1);
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
    }

    fn click(&mut self, pos: Pos2) {
        self.step(vec![Event::PointerMoved(pos)]);
        self.step(vec![Self::button(pos, true)]);
        self.step(vec![Self::button(pos, false)]);
        self.frames(1);
    }

    fn drag(&mut self, a: Pos2, b: Pos2) {
        self.step(vec![Event::PointerMoved(a)]);
        self.step(vec![Self::button(a, true)]);
        for i in 1..=8 {
            let p = a + (b - a) * (i as f32 / 8.0);
            self.step(vec![Event::PointerMoved(p)]);
        }
        self.step(vec![Self::button(b, false)]);
        self.frames(1);
    }

    fn od(&mut self) -> &mut OpenDoc {
        self.app.doc.as_mut().expect("documento abierto")
    }
}

fn select_and_highlight(h: &mut Harness) -> String {
    let r = h.od().page_rect;
    assert!(r.is_positive(), "la página no se dibujó");
    let a = pos2(r.left() + r.width() * 0.15, r.top() + r.height() * 0.2);
    let b = pos2(r.left() + r.width() * 0.8, r.top() + r.height() * 0.3);
    h.drag(a, b);
    let od = h.od();
    assert!(od.selection.is_some(), "arrastrar debe seleccionar texto");
    assert!(matches!(od.popup, Some(Popup::Selection { .. })), "debe aparecer el menú de subrayado");
    let id = od.add_highlight(1).expect("subrayado creado");
    let hl = od.data.highlights.iter().find(|x| x.id == id).unwrap().clone();
    println!("subrayado: {:?} .. {:?} = {}", hl.start, hl.end, short(&hl.text, 120));
    assert!(!hl.text.trim().is_empty());
    od.popup = None;

    // Clic sobre el subrayado: abre su menú y se puede escribir una nota. Se prueban varios
    // puntos del recorrido porque alguno puede caer en el espacio entre párrafos.
    let candidates = [0.5, 0.3, 0.7, 0.15, 0.85].map(|t| a + (b - a) * t);
    for p in candidates.into_iter().chain([pos2(a.x + 40.0, a.y), pos2(b.x - 40.0, b.y)]) {
        h.click(p);
        if matches!(h.od().popup, Some(Popup::Highlight { .. })) {
            break;
        }
    }
    assert!(
        matches!(h.od().popup, Some(Popup::Highlight { id: x, .. }) if x == id),
        "clic sobre el subrayado debe abrir su menú"
    );
    h.frames(2);
    h.text("nota de prueba");
    let note = h.od().data.highlights.iter().find(|x| x.id == id).unwrap().note.clone();
    assert_eq!(note, "nota de prueba");
    h.key(Key::Escape, Modifiers::NONE);
    assert!(h.od().popup.is_none());
    hl.text
}

fn search(h: &mut Harness, q: &str) -> usize {
    h.key(Key::F, Modifiers::COMMAND);
    assert!(h.app.search.open, "Ctrl+F abre el buscador");
    h.frames(1);
    h.text(q);
    assert_eq!(h.app.search.query, q, "el texto se escribe en el buscador");
    h.key(Key::Enter, Modifiers::NONE);
    for _ in 0..3000 {
        if h.app.search.pdf_next.is_none() {
            break;
        }
        h.frames(1);
    }
    let n = h.app.search.hits.len();
    let page = h.od().page + 1;
    println!("búsqueda {q:?}: {n} resultados, actual {:?}, página {page}", h.app.search.current);
    n
}

/// Una palabra larga de la página actual, para buscarla (las pruebas sirven con cualquier documento).
fn word_on_page(h: &mut Harness) -> String {
    let page = h.od().page;
    let text = h.od().page_text_preview(page);
    text.split(|c: char| !c.is_alphabetic())
        .find(|w| w.chars().count() >= 7)
        .expect("la página tiene alguna palabra larga")
        .to_string()
}

#[test]
fn ui_epub() {
    let Some(file) = std::env::var_os("LECTOR_EPUB").map(PathBuf::from) else { return };
    let dir = data_dir("epub");
    unsafe { std::env::set_var("MVCREADER_DATA_DIR", &dir) };
    let mut h = Harness::new(file.clone());
    let n = h.od().doc.page_count();
    println!("páginas: {n}");
    assert!(h.od().layout_ready() && n >= 3);

    // Navegación con teclado.
    let steps = 4.min(n - 1);
    for _ in 0..steps {
        h.key(Key::ArrowRight, Modifiers::NONE);
    }
    assert_eq!(h.od().page, steps);
    h.key(Key::ArrowLeft, Modifiers::NONE);
    assert_eq!(h.od().page, steps - 1);
    let target = (n / 2).clamp(1, n - 1);
    h.od().go(target);
    h.frames(2);

    let text = select_and_highlight(&mut h);
    assert!(text.chars().count() > 20);

    // Marcador con Ctrl+D.
    h.key(Key::D, Modifiers::COMMAND);
    assert_eq!(h.od().data.bookmarks.len(), 1);
    let page = h.od().page;

    // Nota en la página.
    let loc = h.od().doc.page_start(page);
    h.od().data.notes.push(Note { id: 1, loc: Some(loc), text: "nota de página".into(), created: 0 });
    assert_eq!(h.od().notes_on_page(page), 2, "nota de página + nota del subrayado");

    // Cambiar el tamaño de letra repagina y conserva la posición.
    let before = h.od().doc.page_start(page);
    h.key(Key::Plus, Modifiers::COMMAND);
    std::thread::sleep(Duration::from_millis(200));
    h.frames(3);
    let n2 = h.od().doc.page_count();
    let cur = h.od().page;
    let after = h.od().doc.page_start(cur);
    println!("letra más grande: {n} -> {n2} páginas; posición {before:?} -> {after:?}");
    assert!(n2 >= n);
    assert!(after.unit.abs_diff(before.unit) <= 3, "la posición se conserva al repaginar");
    // El marcador sigue en la página correcta tras repaginar.
    let p = h.od().page;
    assert_eq!(h.od().bookmark_on_page(p), Some(0));
    h.key(Key::Num0, Modifiers::COMMAND);
    std::thread::sleep(Duration::from_millis(200));
    h.frames(3);

    let w = word_on_page(&mut h);
    assert!(search(&mut h, &w) > 0);

    // Guardar y reabrir: vuelve a la misma página con todo.
    let page = h.od().page;
    h.app.save_now();
    drop(h);
    let mut h = Harness::new(file);
    println!("reabierto en la página {}", h.od().page + 1);
    assert_eq!(h.od().page, page);
    let d = &h.od().data;
    assert_eq!((d.highlights.len(), d.bookmarks.len(), d.notes.len()), (1, 1, 1));
    assert_eq!(d.highlights[0].note, "nota de prueba");
}

#[test]
fn ui_pdf() {
    let Some(file) = std::env::var_os("LECTOR_PDF").map(PathBuf::from) else { return };
    let dir = data_dir("pdf");
    unsafe { std::env::set_var("MVCREADER_DATA_DIR", &dir) };
    let mut h = Harness::new(file.clone());
    assert!(h.app.error.is_none(), "error: {:?}", h.app.error);
    let n = h.od().doc.page_count();
    println!("páginas: {n}");
    h.od().go((n / 2).max(1).min(n - 1));
    h.frames(3);
    select_and_highlight(&mut h);
    h.key(Key::D, Modifiers::COMMAND);
    let w = word_on_page(&mut h);
    assert!(search(&mut h, &w) > 0);
    h.app.save_now();
    let page = h.od().page;
    drop(h);
    let mut h = Harness::new(file);
    assert_eq!(h.od().page, page);
    assert_eq!(h.od().data.highlights.len(), 1);
}

#[test]
fn ui_docx() {
    let Some(file) = std::env::var_os("LECTOR_DOCX").map(PathBuf::from) else { return };
    let dir = data_dir("docx");
    unsafe { std::env::set_var("MVCREADER_DATA_DIR", &dir) };
    let mut h = Harness::new(file);
    println!("páginas: {}", h.od().doc.page_count());
    select_and_highlight(&mut h);
    let w = word_on_page(&mut h);
    assert!(search(&mut h, &w) > 0);
}
