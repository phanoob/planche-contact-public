#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icc;
mod model;
mod pdf;
mod prefs;
mod print;
mod render;

use eframe::egui::{
    self, epaint::EllipseShape, Align2, Color32, CornerRadius, FontFamily, FontId, Margin, Pos2, Rect,
    RichText, Sense, Shape as EShape, Stroke, StrokeKind, Vec2,
};
use image::RgbaImage;
use model::{compute, find_scale, mirrored, Design, Format, Layout, Settings, Shape, A4_H, A4_W, LOW_DPI};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

// ------------------------------------------------------------ Palette

const ACCENT: Color32 = Color32::from_rgb(234, 118, 60);
const BG: Color32 = Color32::from_rgb(19, 20, 23);
const DOT: Color32 = Color32::from_rgb(33, 35, 40);
const PANEL: Color32 = Color32::from_rgb(26, 27, 31);
const CARD: Color32 = Color32::from_rgb(34, 36, 41);
const FIELD: Color32 = Color32::from_rgb(44, 47, 53);
const BORDER: Color32 = Color32::from_rgb(48, 51, 58);
const TEXT: Color32 = Color32::from_rgb(234, 231, 226);
const MUTED: Color32 = Color32::from_rgb(150, 153, 162);
const WARN: Color32 = Color32::from_rgb(243, 186, 72);
const OK: Color32 = Color32::from_rgb(112, 204, 146);
const CUT: Color32 = Color32::from_rgb(25, 25, 25);

fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 880.0])
            .with_min_inner_size([1040.0, 640.0])
            .with_title("Planche Contact")
            .with_drag_and_drop(true)
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Planche Contact",
        options,
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            setup_style(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}

fn window_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).map(|i| i.to_rgba8());
    match img {
        Ok(i) => egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() },
        Err(_) => egui::IconData::default(),
    }
}

/// Segoe UI (police de Windows) si elle est là, sinon la police d'egui.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = Path::new("C:\\Windows\\Fonts");
    let mut prop = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut semi = prop.clone();
    if let Ok(b) = std::fs::read(dir.join("segoeui.ttf")) {
        fonts.font_data.insert("segoe".into(), Arc::new(egui::FontData::from_owned(b)));
        prop.insert(0, "segoe".into());
        semi.insert(0, "segoe".into());
    }
    if let Ok(b) = std::fs::read(dir.join("seguisb.ttf")) {
        fonts.font_data.insert("segoe_sb".into(), Arc::new(egui::FontData::from_owned(b)));
        semi.insert(0, "segoe_sb".into());
    }
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(semibold(), semi);
    ctx.set_fonts(fonts);
}

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = CARD;
    v.window_stroke = Stroke::new(1.0, BORDER);
    v.extreme_bg_color = Color32::from_rgb(22, 23, 26);
    v.faint_bg_color = CARD;
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = ACCENT;
    v.slider_trailing_fill = true;
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(10);
    let r = CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.bg_fill = FIELD;
    v.widgets.inactive.weak_bg_fill = FIELD;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.bg_fill = Color32::from_rgb(54, 57, 64);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(54, 57, 64);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(72, 76, 85));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.active.bg_fill = Color32::from_rgb(62, 66, 74);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(62, 66, 74);
    v.widgets.open.weak_bg_fill = Color32::from_rgb(54, 57, 64);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 8.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.interact_size.y = 28.0;
        s.spacing.slider_width = 140.0;
        s.spacing.combo_height = 320.0;
        use egui::TextStyle::*;
        s.text_styles.insert(Body, FontId::proportional(14.0));
        s.text_styles.insert(Button, FontId::proportional(14.0));
        s.text_styles.insert(Small, FontId::proportional(12.0));
        s.text_styles.insert(Heading, FontId::new(17.0, semibold()));
    });
}

// ------------------------------------------------------------ Petits composants

fn primary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(Color32::WHITE).family(semibold()))
        .fill(ACCENT)
        .min_size(Vec2::new(0.0, 32.0))
}

fn secondary(text: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.to_owned()).color(TEXT))
        .fill(FIELD)
        .min_size(Vec2::new(0.0, 32.0))
}

/// Compteur « − n + » en forme de pilule.
fn stepper(ui: &mut egui::Ui, v: &mut u32, suffix: &str) {
    egui::Frame::new()
        .fill(FIELD)
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::symmetric(4, 2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                let b = |t: &str| egui::Button::new(RichText::new(t).size(15.0).color(TEXT)).frame(false).min_size(Vec2::new(22.0, 22.0));
                if ui.add(b("−")).clicked() && *v > 0 {
                    *v -= 1;
                }
                ui.add(
                    egui::DragValue::new(v)
                        .range(0..=999)
                        .suffix(suffix)
                        .speed(0.1),
                );
                if ui.add(b("+")).clicked() && *v < 999 {
                    *v += 1;
                }
            });
        });
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED).small());
}

fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.add_sized([86.0, 22.0], egui::Label::new(RichText::new(text).color(MUTED)));
}

/// Section repliable du panneau latéral, présentée en carte.
fn section(ui: &mut egui::Ui, id: &str, title: &str, open: bool, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(CARD)
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::CollapsingHeader::new(RichText::new(title).font(FontId::new(15.0, semibold())).color(TEXT))
                .id_salt(id)
                .default_open(open)
                .show(ui, |ui| {
                    ui.add_space(2.0);
                    add(ui);
                });
        });
    ui.add_space(8.0);
}

fn dashed_rect(painter: &egui::Painter, r: Rect, stroke: Stroke, dash: f32, gap: f32) {
    for (a, b) in [
        (r.left_top(), r.right_top()),
        (r.right_top(), r.right_bottom()),
        (r.right_bottom(), r.left_bottom()),
        (r.left_bottom(), r.left_top()),
    ] {
        painter.extend(EShape::dashed_line(&[a, b], stroke, dash, gap));
    }
}

/// Logo : une planche de quatre vignettes.
fn logo(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 7.0, ACCENT);
    let inner = r.shrink(6.0);
    let s = (inner.width() - 3.0) / 2.0;
    for (i, j) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
        let min = inner.min + Vec2::new(i * (s + 3.0), j * (s + 3.0));
        p.rect_filled(Rect::from_min_size(min, Vec2::splat(s)), 1.5, Color32::from_rgb(255, 238, 225));
    }
}

fn make_tex(ctx: &egui::Context, name: &str, img: &RgbaImage, color: &icc::Color) -> egui::TextureHandle {
    let small;
    let src = if img.width().max(img.height()) > 1024 {
        let k = 1024.0 / img.width().max(img.height()) as f32;
        small = image::imageops::resize(
            img,
            ((img.width() as f32 * k) as u32).max(1),
            ((img.height() as f32 * k) as u32).max(1),
            image::imageops::FilterType::Triangle,
        );
        &small
    } else {
        img
    };
    let shown = color.for_screen(src);
    let ci = egui::ColorImage::from_rgba_unmultiplied([shown.width() as usize, shown.height() as usize], shown.as_raw());
    ctx.load_texture(name, ci, egui::TextureOptions::LINEAR)
}

// ------------------------------------------------------------ Application

struct App {
    designs: Vec<Design>,
    selected: Option<usize>,
    settings: Settings,
    zoom: f32,
    import_mm: f32,
    printers: Arc<Mutex<(Vec<String>, String)>>,
    printer: String,
    status: (String, bool),
    tx: Sender<(String, bool)>,
    rx: Receiver<(String, bool)>,
    counter: u32,
    /// Images à charger à la première image (ligne de commande ou dernière planche).
    restore: Vec<prefs::SavedDesign>,
    pending_remove: Option<usize>,
    pending_clear: bool,
    color: icc::Color,
    color_sig: String,
    profiles: Vec<PathBuf>,
    saved_json: String,
    /// Alerte « vérifier les profils » après un changement d'imprimante.
    printer_alert: bool,
    /// Formats de recadrage (magnet, mug…) et celui des nouvelles images.
    formats: Vec<Format>,
    import_format: Option<String>,
    new_format: (String, f32, f32),
    /// Annuler / rétablir : états de la planche.
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    stable: Option<Snap>,
    stable_sig: String,
    /// -1 = annuler, +1 = rétablir (appliqué au début de l'image suivante).
    pending_undo: i8,
    /// Planches enregistrées sous un nom.
    planches: Vec<String>,
    planche_name: String,
    pending_open: Option<prefs::SavedPlanche>,
    confirm_delete: Option<String>,
    /// « Remplir la planche » avec ce design (appliqué après l'affichage).
    pending_fill: Option<usize>,
    /// « Optimiser la place » (appliqué après l'affichage).
    pending_optimize: bool,
}

/// État de la planche pour annuler / rétablir.
struct Snap {
    designs: Vec<Design>,
    settings: Settings,
    color_sig: String,
}

impl App {
    fn new() -> Self {
        let printers = Arc::new(Mutex::new((Vec::new(), String::new())));
        let p2 = printers.clone();
        std::thread::spawn(move || {
            let r = print::list_printers();
            *p2.lock().unwrap() = r;
        });
        let (tx, rx) = channel();
        let p = prefs::load();

        // Ligne de commande (chemin ou chemin*nombre) : remplace la dernière planche.
        let args: Vec<prefs::SavedDesign> = std::env::args()
            .skip(1)
            .map(|a| {
                let (path, n) = match a.rsplit_once('*') {
                    Some((x, y)) => (x.to_string(), y.parse().unwrap_or(1)),
                    None => (a, 1),
                };
                prefs::SavedDesign { path: path.into(), page: u32::MAX, count: n, width_mm: 0.0, rotated: false, frame: None, pan: [0.5, 0.5] }
            })
            .collect();

        let mut color = icc::Color {
            intent: p.intent,
            softproof: p.softproof,
            bpc: p.bpc,
            gain: p.gain,
            brightness: p.brightness,
            contrast: p.contrast,
            ..Default::default()
        };
        color.src = p.src_icc.as_deref().and_then(|f| icc::Profile::load(f).ok());
        color.out = p.out_icc.as_deref().and_then(|f| icc::Profile::load(f).ok());

        let has_restore = args.is_empty() && !p.designs.is_empty();
        Self {
            designs: Vec::new(),
            selected: None,
            settings: Settings {
                margin: p.margin,
                gap: p.gap,
                pages: p.pages.max(1),
                shape: p.shape,
                show_contour: p.show_contour,
                magnet: p.magnet,
                bleed: p.bleed,
                mirror: p.mirror,
                corners: p.corners,
            },
            zoom: p.zoom.clamp(0.4, 2.5),
            import_mm: p.import_mm,
            printers,
            printer: p.printer.clone(),
            status: (
                if has_restore {
                    "Dernière planche rouverte. Réglages restaurés.".into()
                } else {
                    "Glissez vos images sur la fenêtre pour commencer.".into()
                },
                false,
            ),
            tx,
            rx,
            counter: 0,
            restore: if args.is_empty() { p.designs.clone() } else { args },
            pending_remove: None,
            pending_clear: false,
            color,
            color_sig: String::new(),
            profiles: icc::installed_profiles(),
            saved_json: String::new(),
            printer_alert: false,
            formats: if p.formats.is_empty() { Format::defaults() } else { p.formats.clone() },
            import_format: p.import_format.clone(),
            new_format: (String::new(), 75.0, 55.0),
            undo: Vec::new(),
            redo: Vec::new(),
            stable: None,
            stable_sig: String::new(),
            pending_undo: 0,
            planches: prefs::list_planches(),
            planche_name: String::new(),
            pending_open: None,
            confirm_delete: None,
            pending_fill: None,
            pending_optimize: false,
        }
    }

    fn saved_designs(&self) -> Vec<prefs::SavedDesign> {
        self.designs
            .iter()
            .map(|d| prefs::SavedDesign {
                path: d.path.clone(),
                page: d.page,
                count: d.count,
                width_mm: d.width_mm,
                rotated: d.rotated,
                frame: d.frame,
                pan: d.pan,
            })
            .collect()
    }

    // ---------------------------------------------------- Annuler / rétablir

    fn snap(&self) -> Snap {
        Snap { designs: self.designs.clone(), settings: self.settings.clone(), color_sig: self.color_sig.clone() }
    }

    /// Résumé de ce qui peut être annulé (designs et mise en page).
    fn state_sig(&self) -> String {
        let mut s = format!("{:?}", self.settings);
        for d in &self.designs {
            s += &format!(
                "|{:p} {} {:.3} {} {:?} {:.4},{:.4}",
                Arc::as_ptr(&d.base),
                d.rotated,
                d.width_mm,
                d.count,
                d.frame,
                d.pan[0],
                d.pan[1]
            );
        }
        s
    }

    fn apply_snap(&mut self, ctx: &egui::Context, s: Snap) {
        self.designs = s.designs;
        self.settings = s.settings;
        if s.color_sig != self.color_sig {
            for d in self.designs.iter_mut() {
                self.counter += 1;
                d.tex = make_tex(ctx, &format!("u{}", self.counter), &d.eff, &self.color);
            }
        }
        self.selected = self.selected.filter(|&i| i < self.designs.len());
        self.stable = Some(self.snap());
        self.stable_sig = self.state_sig();
    }

    fn undo_redo(&mut self, ctx: &egui::Context, dir: i8) {
        let (from, to) = if dir < 0 { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        let Some(s) = from.pop() else { return };
        if let Some(cur) = self.stable.take() {
            to.push(cur);
        }
        self.apply_snap(ctx, s);
        self.say(if dir < 0 { "Annulé (Ctrl+Y pour rétablir)." } else { "Rétabli." }, false);
    }

    /// Enregistre l'état précédent dès qu'une modification est terminée (souris relâchée).
    fn track_undo(&mut self, ctx: &egui::Context) {
        let sig = self.state_sig();
        if self.stable.is_none() {
            self.stable = Some(self.snap());
            self.stable_sig = sig;
            return;
        }
        if sig != self.stable_sig && !ctx.input(|i| i.pointer.any_down()) && !ctx.text_edit_focused() {
            if let Some(prev) = self.stable.take() {
                self.undo.push(prev);
                if self.undo.len() > 100 {
                    self.undo.remove(0);
                }
            }
            self.redo.clear();
            self.stable = Some(self.snap());
            self.stable_sig = sig;
        }
    }

    /// Réordonne les designs et pivote ceux qui gagnent de la place.
    fn optimize(&mut self, ctx: &egui::Context) {
        let before = self.layout().pages_needed;
        let Some((order, rot)) = model::optimize(&self.designs, &self.settings) else {
            self.say("La mise en page est déjà la plus compacte trouvée.", false);
            return;
        };
        for (i, &r) in rot.iter().enumerate() {
            if r {
                self.rotate(ctx, i);
            }
        }
        let old = std::mem::take(&mut self.designs);
        let mut slots: Vec<Option<Design>> = old.into_iter().map(Some).collect();
        self.designs = order.iter().filter_map(|&i| slots[i].take()).collect();
        self.selected = self.selected.and_then(|s| order.iter().position(|&i| i == s));
        let after = self.layout().pages_needed;
        let turned = rot.iter().filter(|&&r| r).count();
        let msg = if after < before {
            format!("Place optimisée : {after} page(s) au lieu de {before} ({turned} design(s) pivoté(s)).")
        } else {
            format!("Place optimisée : plus de place libre en bas de la dernière page ({turned} design(s) pivoté(s)).")
        };
        self.say(msg, false);
    }

    /// Exemplaires par format (Magnet, Mug…, « autre » pour les images libres), pour les pages imprimées.
    fn recap(&self, layout: &Layout) -> Vec<(String, u32)> {
        let mut v: Vec<(String, u32)> = Vec::new();
        for p in layout.items.iter().filter(|p| p.page < self.settings.pages) {
            let name = format_name(&self.formats, &self.designs[p.design]).unwrap_or_else(|| "autre".into());
            match v.iter_mut().find(|(n, _)| *n == name) {
                Some(e) => e.1 += 1,
                None => v.push((name, 1)),
            }
        }
        v
    }

    /// Ajoute une mire de test couleur (gris, couleurs pures, teintes chair, dégradés).
    fn add_test_chart(&mut self, ctx: &egui::Context) {
        let path = prefs::dir().join("mire-couleur.png");
        let _ = std::fs::create_dir_all(prefs::dir());
        if let Err(e) = test_chart().save(&path) {
            self.say(format!("Impossible de créer la mire : {e}"), true);
            return;
        }
        if self.add_file(ctx, &path, 0) == 1 {
            let d = self.designs.last_mut().unwrap();
            d.name = "Mire couleur".into();
            d.set_format(None);
            d.width_mm = 180.0;
            d.count = 1;
            self.selected = Some(self.designs.len() - 1);
            self.say("Mire ajoutée : imprime-la, puis compare avec l'écran pour régler profils et teinte.", false);
        }
    }


    /// Ajoute des exemplaires de ce design tant qu'il reste de la place sur les pages.
    fn fill(&mut self, i: usize) {
        if i >= self.designs.len() {
            return;
        }
        let pages = self.settings.pages;
        if !self.layout().fits(pages) {
            self.say("La planche déborde déjà : rien à remplir.", true);
            return;
        }
        let start = self.designs[i].count;
        while self.designs[i].count < start + 500 {
            self.designs[i].count += 1;
            if !self.layout().fits(pages) {
                self.designs[i].count -= 1;
                break;
            }
        }
        let added = self.designs[i].count - start;
        let name = self.designs[i].name.clone();
        if added == 0 {
            self.say(format!("Plus de place libre pour « {name} » sur {pages} page(s)."), false);
        } else {
            self.say(format!("« {name} » : {added} exemplaire(s) ajouté(s), {} au total.", self.designs[i].count), false);
        }
    }


    fn say(&mut self, msg: impl Into<String>, warn: bool) {
        self.status = (msg.into(), warn);
    }

    fn to_prefs(&self) -> prefs::Prefs {
        prefs::Prefs {
            margin: self.settings.margin,
            gap: self.settings.gap,
            pages: self.settings.pages,
            shape: self.settings.shape,
            show_contour: self.settings.show_contour,
            magnet: self.settings.magnet,
            bleed: self.settings.bleed,
            mirror: self.settings.mirror,
            corners: self.settings.corners,
            import_mm: self.import_mm,
            zoom: self.zoom,
            printer: self.printer.clone(),
            src_icc: self.color.src.as_ref().map(|p| p.path.clone()),
            out_icc: self.color.out.as_ref().map(|p| p.path.clone()),
            intent: self.color.intent,
            softproof: self.color.softproof,
            bpc: self.color.bpc,
            gain: self.color.gain,
            brightness: self.color.brightness,
            contrast: self.color.contrast,
            designs: self.saved_designs(),
            formats: self.formats.clone(),
            import_format: self.import_format.clone(),
        }
    }

    /// Ajoute une image, ou toutes les pages d'un PDF (`page` = u32::MAX),
    /// ou une seule page d'un PDF (rechargement de la dernière planche).
    /// Renvoie le nombre de designs ajoutés.
    fn add_file(&mut self, ctx: &egui::Context, path: &Path, page: u32) -> usize {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Image".into());
        if !pdf::is_pdf(path) {
            return match image::open(path) {
                Ok(img) => {
                    self.push_design(ctx, stem, path, 0, img.to_rgba8());
                    1
                }
                Err(e) => {
                    self.say(format!("Impossible d'ouvrir {} : {e}", path.display()), true);
                    0
                }
            };
        }
        let pages: Vec<u32> = if page != u32::MAX {
            vec![page]
        } else {
            match pdf::page_count(path) {
                Ok(n) => (0..n).collect(),
                Err(e) => {
                    self.say(format!("Impossible d'ouvrir le PDF {} : {e}", path.display()), true);
                    return 0;
                }
            }
        };
        let multi = pages.len() > 1 || page != u32::MAX && pdf::page_count(path).is_ok_and(|n| n > 1);
        let mut added = 0;
        for p in pages {
            match pdf::render(path, p) {
                Ok(img) => {
                    let name = if multi { format!("{stem} · p.{}", p + 1) } else { stem.clone() };
                    self.push_design(ctx, name, path, p, img);
                    added += 1;
                }
                Err(e) => self.say(format!("Page {} de {} illisible : {e}", p + 1, path.display()), true),
            }
        }
        if added > 1 {
            self.say(format!("« {stem} » : {added} pages ajoutées, une par design."), false);
        }
        added
    }

    fn add_image(&mut self, ctx: &egui::Context, path: &Path) -> bool {
        self.add_file(ctx, path, u32::MAX) > 0
    }

    fn push_design(&mut self, ctx: &egui::Context, name: String, path: &Path, page: u32, img: RgbaImage) {
        let base = Arc::new(img);
        self.counter += 1;
        let aspect = base.width() as f32 / base.height() as f32;
        let width_mm = if aspect >= 1.0 { self.import_mm } else { self.import_mm * aspect };
        let tex = make_tex(ctx, &format!("d{}", self.counter), &base, &self.color);
        self.designs.push(Design {
            name: name.clone(),
            path: path.to_path_buf(),
            page,
            eff: base.clone(),
            base,
            rotated: false,
            tex,
            width_mm,
            count: 1,
            frame: None,
            pan: [0.5, 0.5],
        });
        let f = self.import_format.as_ref().and_then(|n| self.formats.iter().find(|f| &f.name == n)).cloned();
        if let Some(f) = f {
            self.designs.last_mut().unwrap().set_format(Some(&f));
        }
        self.selected = Some(self.designs.len() - 1);
        self.say(format!("« {name} » ajouté. Clic droit sur l'image pour choisir le nombre d'exemplaires."), false);
    }

    fn pick_images(&mut self, ctx: &egui::Context) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title("Ajouter des images")
            .add_filter("Images et PDF", &["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff", "pdf"])
            .pick_files()
        {
            for f in files {
                self.add_image(ctx, &f);
            }
        }
    }

    fn rotate(&mut self, ctx: &egui::Context, i: usize) {
        let d = &mut self.designs[i];
        let h = d.height_mm();
        d.rotated = !d.rotated;
        // Le recadrage tourne avec l'image (rotation d'un quart de tour horaire).
        d.frame = d.frame.map(|f| 1.0 / f);
        d.pan = [1.0 - d.pan[1], d.pan[0]];
        d.eff = if d.rotated { Arc::new(image::imageops::rotate90(d.base.as_ref())) } else { d.base.clone() };
        d.width_mm = h;
        self.counter += 1;
        d.tex = make_tex(ctx, &format!("r{}", self.counter), &d.eff, &self.color);
    }

    /// Images converties pour la sortie + mise en page (retournées si miroir).
    fn output(&mut self, layout: &Layout) -> Option<(Vec<Arc<RgbaImage>>, Layout)> {
        let mut v = Vec::new();
        for d in &self.designs {
            let cropped;
            let src = if d.frame.is_some() {
                cropped = d.cropped();
                &cropped
            } else {
                d.eff.as_ref()
            };
            match self.color.for_output(src) {
                Ok(mut i) => {
                    if self.settings.mirror {
                        image::imageops::flip_horizontal_in_place(&mut i);
                    }
                    v.push(Arc::new(i));
                }
                Err(e) => {
                    self.status = (format!("Conversion ICC impossible : {e}. Choisis un profil RVB."), true);
                    return None;
                }
            }
        }
        let l = if self.settings.mirror { mirrored(layout) } else { layout.clone() };
        Some((v, l))
    }

    fn layout(&self) -> Layout {
        compute(&self.designs, &self.settings, &vec![1.0; self.designs.len()])
    }

    fn export_pdf(&mut self, layout: &Layout) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Enregistrer la planche en PDF")
            .set_file_name("planche-contact.pdf")
            .add_filter("PDF", &["pdf"])
            .save_file()
        else {
            return;
        };
        let Some((imgs, l)) = self.output(layout) else { return };
        let icc = self.color.out.as_ref().map(|p| p.bytes.clone());
        let r = render::export_pdf(&imgs, &l, &self.settings, self.settings.pages, &path, icc.as_ref().map(|b| b.as_slice()));
        match r {
            Ok(()) => {
                self.say(format!("PDF enregistré : {}", path.display()), false);
                let _ = open_file(&path);
            }
            Err(e) => self.say(format!("Échec de l'export PDF : {e}"), true),
        }
    }

    fn print(&mut self, ctx: &egui::Context, layout: &Layout) {
        let printer = self.printer.clone();
        let Some((imgs, layout)) = self.output(layout) else { return };
        let settings = self.settings.clone();
        let tx = self.tx.clone();
        let rctx = ctx.clone();
        self.say("Préparation de l'impression… la fenêtre d'impression Windows va s'ouvrir.", false);
        std::thread::spawn(move || {
            let dir: PathBuf = std::env::temp_dir().join("planche-contact");
            let _ = std::fs::create_dir_all(&dir);
            let mut files = Vec::new();
            for pg in 0..settings.pages {
                let img = render::render_page(&imgs, &layout, &settings, pg, 300.0);
                let f = dir.join(format!("page{}.png", pg + 1));
                if let Err(e) = img.save(&f) {
                    let _ = tx.send((format!("Échec du rendu : {e}"), true));
                    return;
                }
                files.push(f);
            }
            let msg = match print::print_files(&printer, &files, &dir) {
                Ok(()) => ("Impression envoyée.".to_string(), false),
                Err(e) if e == "annulée" => ("Impression annulée.".to_string(), false),
                Err(e) => (format!("Échec de l'impression : {e}"), true),
            };
            let _ = tx.send(msg);
            rctx.request_repaint();
        });
    }

    fn total(&self) -> u32 {
        self.designs.iter().map(|d| d.count).sum()
    }

    fn bleed(&self) -> f32 {
        if self.settings.magnet { self.settings.bleed } else { 0.0 }
    }
}

/// Nom du format du design s'il correspond à un format de la liste.
fn format_name(formats: &[Format], d: &Design) -> Option<String> {
    d.frame?;
    let (w, h) = (d.width_mm, d.height_mm());
    formats
        .iter()
        .find(|f| {
            let (fw, fh) = f.oriented(w >= h);
            (fw - w).abs() < 0.2 && (fh - h).abs() < 0.2
        })
        .map(|f| f.name.clone())
}

#[allow(dead_code)]
/// Mire de test couleur (sRGB) : gris, teintes, couleurs pures et chair, dégradés.
fn test_chart() -> RgbaImage {
    let (cols, cell, pad) = (12u32, 200u32, 12u32);
    let w = cols * cell;
    let rows_h = [300u32, 300, 300, 110, 110, 110, 110];
    let h: u32 = rows_h.iter().sum();
    let mut img = RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
    let hue = |t: f32| -> [u8; 3] {
        let f = |n: f32| {
            let k = (n + t * 6.0) % 6.0;
            ((1.0 - (k.min(4.0 - k).clamp(0.0, 1.0))) * 255.0) as u8
        };
        [f(5.0), f(3.0), f(1.0)]
    };
    let solid: [[u8; 3]; 12] = [
        [0, 255, 255], [255, 0, 255], [255, 255, 0], [255, 0, 0], [0, 255, 0], [0, 0, 255],
        [0, 0, 0], [255, 255, 255], [255, 224, 196], [234, 189, 157], [198, 134, 103], [141, 85, 54],
    ];
    let mut y0 = 0;
    for (row, &rh) in rows_h.iter().enumerate() {
        for x in 0..w {
            let col = (x / cell) as usize;
            let t = x as f32 / (w - 1) as f32;
            let in_patch = row < 3 && (x % cell < pad / 2 || x % cell >= cell - pad / 2);
            for y in y0..y0 + rh {
                let edge = if row < 3 { y < y0 + pad / 2 || y >= y0 + rh - pad / 2 } else { y >= y0 + rh - 4 };
                if in_patch || edge {
                    continue;
                }
                let c = match row {
                    0 => {
                        let g = (col as f32 / 11.0 * 255.0).round() as u8;
                        [g, g, g]
                    }
                    1 => hue(col as f32 / 12.0),
                    2 => solid[col],
                    3 => [(t * 255.0) as u8, 0, 0],
                    4 => [0, (t * 255.0) as u8, 0],
                    5 => [0, 0, (t * 255.0) as u8],
                    _ => {
                        let g = (t * 255.0) as u8;
                        [g, g, g]
                    }
                };
                img.put_pixel(x, y, image::Rgba([c[0], c[1], c[2], 255]));
            }
        }
        y0 += rh;
    }
    img
}

/// Nom du format du design, « Libre » ou « Personnalisé ».
fn format_label(formats: &[Format], d: &Design) -> String {
    if d.frame.is_none() {
        return "Libre (proportions de l'image)".into();
    }
    let (w, h) = (d.width_mm, d.height_mm());
    formats
        .iter()
        .find(|f| {
            let (fw, fh) = f.oriented(w >= h);
            (fw - w).abs() < 0.2 && (fh - h).abs() < 0.2
        })
        .map_or(format!("Personnalisé {:.0} × {:.0}", w, h), |f| f.label())
}

/// Menu des formats pour un design : Libre ou un format de la liste.
fn format_menu(ui: &mut egui::Ui, formats: &[Format], d: &mut Design, id: impl egui::AsIdSalt) {
    egui::ComboBox::from_id_salt(id).width(200.0).selected_text(format_label(formats, d)).show_ui(ui, |ui| {
        if ui.selectable_label(d.frame.is_none(), "Libre (proportions de l'image)").clicked() {
            d.set_format(None);
        }
        for f in formats {
            if ui.selectable_label(false, f.label()).clicked() {
                d.set_format(Some(f));
            }
        }
    });
}

fn open_file(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd").args(["/C", "start", ""]).arg(path).spawn().map(|_| ())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn().map(|_| ())
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();

        // Ouverture d'une planche enregistrée : remplace la planche actuelle.
        if let Some(pl) = self.pending_open.take() {
            self.designs.clear();
            self.selected = None;
            self.settings = pl.settings();
            self.restore = pl.designs;
        }

        // Dernière planche, planche enregistrée ou images passées en ligne de commande.
        let restore = std::mem::take(&mut self.restore);
        let mut missing = 0;
        for s in restore {
            let n = self.add_file(ctx, &s.path, s.page);
            if n == 0 {
                missing += 1;
                continue;
            }
            for i in self.designs.len() - n..self.designs.len() {
                if s.rotated {
                    self.rotate(ctx, i);
                }
                let d = &mut self.designs[i];
                d.count = s.count;
                if s.width_mm > 0.0 {
                    d.frame = s.frame;
                    d.pan = s.pan;
                    d.width_mm = s.width_mm;
                }
            }
            self.selected = None;
        }
        if missing > 0 {
            self.say(format!("{missing} image(s) de la planche sont introuvables."), true);
        }

        // Raccourcis clavier (sauf pendant la saisie dans un champ). Seulement à la première
        // passe : egui peut refaire une passe avec les mêmes touches, qui seraient comptées deux fois.
        if !ctx.text_edit_focused() && ctx.current_pass_index() == 0 {
            use egui::{Key, KeyboardShortcut, Modifiers};
            let redo = ctx.input_mut(|i| {
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z))
                    || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y))
            });
            let undo = ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z)));
            if redo {
                self.pending_undo = 1;
            } else if undo {
                self.pending_undo = -1;
            }
            // Suppr : un exemplaire de moins, ou retire le design s'il n'en reste qu'un.
            if ctx.input(|i| i.key_pressed(Key::Delete)) {
                if let Some(i) = self.selected.filter(|&i| i < self.designs.len()) {
                    let d = &mut self.designs[i];
                    if d.count > 1 {
                        d.count -= 1;
                        let msg = format!("« {} » : {} exemplaire(s).", d.name, d.count);
                        self.say(msg, false);
                    } else {
                        let msg = format!("« {} » retiré (Ctrl+Z pour annuler).", d.name);
                        self.designs.remove(i);
                        self.selected = None;
                        self.say(msg, false);
                    }
                }
            }
        }
        match std::mem::take(&mut self.pending_undo) {
            0 => {}
            d => self.undo_redo(ctx, d),
        }

        while let Ok(m) = self.rx.try_recv() {
            self.status = m;
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            self.add_image(ctx, &p);
        }
        // Imprimante : celle enregistrée si elle existe encore, sinon celle par défaut.
        {
            let g = self.printers.lock().unwrap();
            if g.0.is_empty() {
                ctx.request_repaint_after(std::time::Duration::from_millis(300));
            } else if !g.0.contains(&self.printer) {
                self.printer = if g.1.is_empty() { g.0[0].clone() } else { g.1.clone() };
            }
        }

        let sig = self.color.signature();
        if sig != self.color_sig {
            self.color_sig = sig;
            for d in self.designs.iter_mut() {
                self.counter += 1;
                d.tex = make_tex(ctx, &format!("c{}", self.counter), &d.eff, &self.color);
            }
        }
        if let Some(i) = self.pending_remove.take() {
            if i < self.designs.len() {
                self.designs.remove(i);
            }
            self.selected = None;
        }
        if std::mem::take(&mut self.pending_clear) {
            self.designs.clear();
            self.selected = None;
            self.settings.pages = 1;
            self.say("Planche vidée.", false);
        }
        let layout = self.layout();

        self.top_bar(ui, &layout);
        self.status_bar(ui, &layout);
        self.side_panel(ui);
        self.canvas(ui, &layout);
        self.printer_alert_window(ctx);
        if let Some(i) = self.pending_fill.take() {
            self.fill(i);
        }
        if std::mem::take(&mut self.pending_optimize) {
            self.optimize(ctx);
        }
        self.track_undo(ctx);

        if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let r = ctx.content_rect();
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::NULL.with("drop")));
            p.rect_filled(r, 0.0, Color32::from_black_alpha(180));
            let zone = r.shrink(40.0);
            dashed_rect(&p, zone, Stroke::new(2.0, ACCENT), 10.0, 8.0);
            p.text(zone.center(), Align2::CENTER_CENTER, "Déposez vos images", FontId::new(30.0, semibold()), Color32::WHITE);
        }

        // Enregistrement automatique dès qu'un réglage change.
        if let Ok(json) = serde_json::to_string_pretty(&self.to_prefs()) {
            if json != self.saved_json {
                prefs::save(&json);
                self.saved_json = json;
            }
        }
    }
}

impl App {
    /// Fenêtre d'alerte après un changement d'imprimante : le profil de sortie
    /// est propre à une imprimante et à son papier.
    fn printer_alert_window(&mut self, ctx: &egui::Context) {
        if !self.printer_alert {
            return;
        }
        let out = self.color.out.as_ref().map_or("sRGB (par défaut)".to_string(), |p| p.name.clone());
        let src = self.color.src.as_ref().map_or("sRGB (par défaut)".to_string(), |p| p.name.clone());
        let mut close = false;
        let modal = egui::Modal::new(egui::Id::NULL.with("printer_alert"))
            .frame(
                egui::Frame::new()
                    .fill(CARD)
                    .stroke(Stroke::new(1.0, Color32::from_rgb(110, 84, 36)))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::same(20)),
            )
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.label(RichText::new("⚠  Changement d'imprimante").font(FontId::new(18.0, semibold())).color(WARN));
                ui.add_space(6.0);
                ui.label(RichText::new("Pensez à vérifier les profils colorimétriques.").color(TEXT).size(15.0));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("Imprimante : {}", self.printer)).color(MUTED));
                ui.label(RichText::new(format!("Profil de sortie actuel : {out}")).color(TEXT));
                ui.label(RichText::new(format!("Profil des images : {src}")).color(MUTED));
                ui.add_space(4.0);
                hint(ui, "Un profil de sublimation (ex. Asub125) ne convient qu'à l'imprimante et aux encres pour lesquelles il a été fait. Pour une imprimante classique, choisis plutôt sRGB.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if self.color.out.is_some() && ui.add(secondary("Passer en sRGB")).clicked() {
                        self.color.out = None;
                        self.color.softproof = false;
                        close = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(primary("C'est bon")).clicked() {
                            close = true;
                        }
                    });
                });
            });
        if close || modal.should_close() {
            self.printer_alert = false;
            if self.color.out.is_none() {
                self.say(format!("Imprimante : {}. Profil de sortie : sRGB.", self.printer), false);
            } else {
                self.say(format!("Imprimante : {}. Profil de sortie : {out}.", self.printer), false);
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, layout: &Layout) {
        let ctx = &ui.ctx().clone();
        egui::Panel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(16, 10))
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    logo(ui);
                    ui.add_space(2.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(RichText::new("Planche Contact").font(FontId::new(17.0, semibold())).color(TEXT));
                        ui.label(RichText::new("Magnets · sublimation · A4").color(MUTED).small());
                    });
                    ui.add_space(18.0);
                    if ui.add(primary("Ajouter des images")).clicked() {
                        self.pick_images(ctx);
                    }
                    ui.add_space(6.0);
                    if ui.add_enabled(!self.undo.is_empty(), secondary("Annuler")).on_hover_text("Ctrl+Z").clicked() {
                        self.pending_undo = -1;
                        ctx.request_repaint();
                    }
                    if ui.add_enabled(!self.redo.is_empty(), secondary("Rétablir")).on_hover_text("Ctrl+Y").clicked() {
                        self.pending_undo = 1;
                        ctx.request_repaint();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let can = !self.designs.is_empty();
                        let can_print = can && !self.printer.is_empty();
                        if ui.add_enabled(can_print, primary("Imprimer…")).clicked() {
                            self.print(ctx, layout);
                        }
                        if ui.add_enabled(can, secondary("Exporter en PDF")).clicked() {
                            self.export_pdf(layout);
                        }
                        let list = self.printers.lock().unwrap().0.clone();
                        egui::ComboBox::from_id_salt("printer")
                            .width(220.0)
                            .selected_text(if list.is_empty() { "Recherche des imprimantes…".to_string() } else { self.printer.clone() })
                            .show_ui(ui, |ui| {
                                for p in &list {
                                    if ui.selectable_value(&mut self.printer, p.clone(), p).changed() {
                                        self.printer_alert = true;
                                    }
                                }
                            });
                        ui.add_space(14.0);
                        // Zoom compact.
                        egui::Frame::new().fill(FIELD).corner_radius(CornerRadius::same(8)).inner_margin(Margin::symmetric(4, 2)).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let b = |t: &str| egui::Button::new(RichText::new(t).size(15.0)).frame(false).min_size(Vec2::new(24.0, 24.0));
                            if ui.add(b("+")).on_hover_text("Zoom avant (Ctrl + molette)").clicked() {
                                self.zoom = (self.zoom + 0.1).min(2.5);
                            }
                            if ui
                                .add(egui::Button::new(RichText::new(format!("{:.0} %", self.zoom * 100.0)).color(TEXT)).frame(false).min_size(Vec2::new(52.0, 24.0)))
                                .on_hover_text("Ajuster à la largeur")
                                .clicked()
                            {
                                self.zoom = 1.0;
                            }
                            if ui.add(b("−")).on_hover_text("Zoom arrière").clicked() {
                                self.zoom = (self.zoom - 0.1).max(0.4);
                            }
                        });
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, layout: &Layout) {
        let pages = self.settings.pages;
        let fits = layout.fits(pages);
        let total = self.total();
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(16, 6)).stroke(Stroke::new(1.0, BORDER)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let c = if self.status.1 { WARN } else { MUTED };
                    ui.label(RichText::new(&self.status.0).color(c));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let recap = self.recap(layout);
                        let parts: Vec<String> = recap.iter().map(|(n, c)| format!("{c} {n}")).collect();
                        let what = if recap.len() > 1 || recap.iter().any(|(n, _)| n != "autre") {
                            parts.join(" · ")
                        } else {
                            format!("{total} exemplaire(s)")
                        };
                        let (txt, col) = if fits {
                            (format!("{what} · {pages} page(s)"), OK)
                        } else {
                            (format!("{total} exemplaire(s) · ne tient pas"), WARN)
                        };
                        let detail: Vec<String> = self
                            .designs
                            .iter()
                            .filter(|d| d.count > 0)
                            .map(|d| format!("{} × {} ({:.0} × {:.0} mm)", d.count, d.name, d.width_mm, d.height_mm()))
                            .collect();
                        egui::Frame::new()
                            .fill(col.gamma_multiply(0.16))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(10, 2))
                            .show(ui, |ui| {
                                let r = ui.label(RichText::new(txt).color(col).small());
                                if !detail.is_empty() {
                                    r.on_hover_text(detail.join("\n"));
                                }
                            });
                        if self.settings.mirror {
                            ui.label(RichText::new("Miroir à l'impression").color(ACCENT).small());
                        }
                    });
                });
            });
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        let ctx = &ui.ctx().clone();
        egui::Panel::left("side")
            .exact_size(340.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(14, 14)).stroke(Stroke::new(1.0, BORDER)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Designs").heading().color(TEXT));
                        ui.label(RichText::new(format!("{}", self.designs.len())).color(MUTED));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !self.designs.is_empty()
                                && ui
                                    .add(egui::Button::new(RichText::new("Vider").color(MUTED).small()).frame(false))
                                    .on_hover_text("Retirer toutes les images de la planche")
                                    .clicked()
                            {
                                // Vidé au début de l'image suivante : la mise en page de celle-ci référence encore les designs.
                                self.pending_clear = true;
                                ctx.request_repaint();
                            }
                        });
                    });
                    ui.add_space(2.0);
                    if self.designs.is_empty() {
                        egui::Frame::new()
                            .fill(CARD)
                            .corner_radius(CornerRadius::same(12))
                            .inner_margin(Margin::same(16))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new("Aucune image").color(TEXT));
                                hint(ui, "Glissez vos designs sur la fenêtre ou cliquez sur « Ajouter des images ».");
                            });
                        ui.add_space(8.0);
                    }
                    self.design_cards(ui, ctx);
                    ui.add_space(6.0);

                    let b = self.bleed();
                    section(ui, "s_magnet", "Magnet & sublimation", true, |ui| {
                        ui.checkbox(&mut self.settings.magnet, "Ajouter un débord autour de chaque image");
                        ui.add_enabled_ui(self.settings.magnet, |ui| {
                            ui.horizontal(|ui| {
                                field_label(ui, "Débord");
                                ui.add(egui::DragValue::new(&mut self.settings.bleed).range(0.0..=6.0).speed(0.05).fixed_decimals(1).suffix(" mm"));
                                ui.label(RichText::new("par côté").color(MUTED));
                            });
                        });
                        if self.settings.magnet {
                            let ex = self.selected.or(if self.designs.is_empty() { None } else { Some(0) });
                            let txt = match ex.and_then(|i| self.designs.get(i)) {
                                Some(d) => format!(
                                    "Magnet {:.0} × {:.0} mm → imprimé {:.0} × {:.0} mm. L'image est agrandie pour couvrir le débord.",
                                    d.width_mm, d.height_mm(), d.width_mm + 2.0 * b, d.height_mm() + 2.0 * b
                                ),
                                None => format!("Ex. magnet 75 × 55 mm → imprimé {:.0} × {:.0} mm.", 75.0 + 2.0 * b, 55.0 + 2.0 * b),
                            };
                            hint(ui, &txt);
                        }
                        ui.checkbox(&mut self.settings.mirror, "Imprimer en miroir (transfert)");
                    });

                    section(ui, "s_planche", "Planche", true, |ui| {
                        ui.horizontal(|ui| {
                            field_label(ui, "Pages");
                            let mut n = self.settings.pages as u32;
                            stepper(ui, &mut n, "");
                            self.settings.pages = n.clamp(1, 20) as usize;
                        });
                        ui.horizontal(|ui| {
                            field_label(ui, "Découpe");
                            ui.selectable_value(&mut self.settings.shape, Shape::Rect, "Carré");
                            ui.selectable_value(&mut self.settings.shape, Shape::Ellipse, "Rond");
                        });
                        ui.horizontal(|ui| {
                            field_label(ui, "");
                            ui.checkbox(&mut self.settings.show_contour, "Traits de coupe");
                        });
                        ui.horizontal(|ui| {
                            field_label(ui, "");
                            ui.add_enabled_ui(self.settings.show_contour, |ui| {
                                ui.checkbox(&mut self.settings.corners, "Coins seulement")
                                    .on_hover_text("Petits repères aux coins, à l'extérieur des images : rien n'est tracé sur le design.");
                            });
                        });
                        ui.horizontal(|ui| {
                            field_label(ui, "Marge");
                            ui.add(egui::DragValue::new(&mut self.settings.margin).range(0.0..=30.0).speed(0.1).suffix(" mm"));
                            ui.label(RichText::new("Espace").color(MUTED));
                            ui.add(egui::DragValue::new(&mut self.settings.gap).range(0.0..=20.0).speed(0.1).suffix(" mm"));
                        });
                        ui.horizontal(|ui| {
                            field_label(ui, "Taille import");
                            ui.add(egui::DragValue::new(&mut self.import_mm).range(10.0..=200.0).speed(0.5).suffix(" mm"));
                        });
                        hint(ui, "Largeur donnée aux nouvelles images (côté le plus long).");
                        if ui
                            .add_enabled(!self.designs.is_empty(), secondary("Optimiser la place"))
                            .on_hover_text("Change l'ordre et le sens (portrait / paysage) des designs pour utiliser moins de papier")
                            .clicked()
                        {
                            self.pending_optimize = true;
                        }
                    });

                    section(ui, "s_formats", "Formats et recadrage", true, |ui| {
                        ui.horizontal(|ui| {
                            field_label(ui, "Nouvelles images");
                            let cur = self.import_format.clone().unwrap_or_else(|| "Libre".into());
                            egui::ComboBox::from_id_salt("import_fmt").width(150.0).selected_text(cur).show_ui(ui, |ui| {
                                if ui.selectable_label(self.import_format.is_none(), "Libre").clicked() {
                                    self.import_format = None;
                                }
                                for f in &self.formats {
                                    if ui.selectable_label(self.import_format.as_ref() == Some(&f.name), f.label()).clicked() {
                                        self.import_format = Some(f.name.clone());
                                    }
                                }
                            });
                        });
                        hint(ui, "Avec un format, chaque image ajoutée est recadrée au centre, dans son sens (portrait ou paysage).");
                        let mut del = None;
                        for (k, f) in self.formats.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(f.label()).color(TEXT));
                                if ui.add(egui::Button::new(RichText::new("✕").color(MUTED)).frame(false)).on_hover_text("Supprimer ce format").clicked() {
                                    del = Some(k);
                                }
                            });
                        }
                        if let Some(k) = del {
                            let f = self.formats.remove(k);
                            if self.import_format.as_ref() == Some(&f.name) {
                                self.import_format = None;
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.new_format.0).hint_text("Nom (ex. Mug)").desired_width(92.0));
                            ui.add(egui::DragValue::new(&mut self.new_format.1).range(5.0..=400.0).speed(0.5).suffix(" mm"));
                            ui.label(RichText::new("×").color(MUTED));
                            ui.add(egui::DragValue::new(&mut self.new_format.2).range(5.0..=400.0).speed(0.5).suffix(" mm"));
                        });
                        let name = self.new_format.0.trim().to_string();
                        if ui.add_enabled(!name.is_empty(), secondary("Ajouter ce format")).clicked() {
                            let (w, h) = (self.new_format.1, self.new_format.2);
                            match self.formats.iter_mut().find(|f| f.name == name) {
                                Some(f) => {
                                    f.w = w;
                                    f.h = h;
                                }
                                None => self.formats.push(Format { name: name.clone(), w, h }),
                            }
                            self.new_format.0.clear();
                            self.say(format!("Format « {name} » {w:.0} × {h:.0} mm enregistré."), false);
                        }
                    });

                    section(ui, "s_planches", "Planches enregistrées", true, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.planche_name).hint_text("Nom de la planche").desired_width(170.0));
                            let can = !self.planche_name.trim().is_empty() && !self.designs.is_empty();
                            if ui.add_enabled(can, primary("Enregistrer")).clicked() {
                                let pl = prefs::SavedPlanche {
                                    margin: self.settings.margin,
                                    gap: self.settings.gap,
                                    pages: self.settings.pages,
                                    shape: self.settings.shape,
                                    show_contour: self.settings.show_contour,
                                    magnet: self.settings.magnet,
                                    bleed: self.settings.bleed,
                                    mirror: self.settings.mirror,
                                    corners: self.settings.corners,
                                    designs: self.saved_designs(),
                                };
                                match prefs::save_planche(&self.planche_name, &pl) {
                                    Ok(n) => {
                                        self.say(format!("Planche « {n} » enregistrée."), false);
                                        self.planche_name.clear();
                                        self.planches = prefs::list_planches();
                                    }
                                    Err(e) => self.say(format!("Impossible d'enregistrer : {e}"), true),
                                }
                            }
                        });
                        if self.planches.is_empty() {
                            hint(ui, "Donne un nom à la planche actuelle pour la retrouver plus tard. Un nom existant est remplacé.");
                        }
                        let mut open = None;
                        let mut delete = None;
                        for n in &self.planches {
                            ui.horizontal_wrapped(|ui| {
                                if ui.small_button("Ouvrir").on_hover_text("Remplace la planche actuelle (Ctrl+Z pour revenir)").clicked() {
                                    open = Some(n.clone());
                                }
                                ui.label(RichText::new(n).color(TEXT));
                                if self.confirm_delete.as_ref() == Some(n) {
                                    if ui.small_button(RichText::new("Supprimer ?").color(Color32::from_rgb(240, 130, 120))).clicked() {
                                        delete = Some(n.clone());
                                    }
                                    if ui.small_button("Non").clicked() {
                                        self.confirm_delete = None;
                                    }
                                } else if ui.add(egui::Button::new(RichText::new("✕").color(MUTED)).frame(false)).on_hover_text("Supprimer cette planche").clicked() {
                                    self.confirm_delete = Some(n.clone());
                                }
                            });
                        }
                        if let Some(n) = open {
                            match prefs::load_planche(&n) {
                                Ok(pl) => {
                                    self.pending_open = Some(pl);
                                    self.planche_name = n.clone();
                                    self.say(format!("Planche « {n} » ouverte (Ctrl+Z pour revenir à la précédente)."), false);
                                    ctx.request_repaint();
                                }
                                Err(e) => self.say(format!("Impossible d'ouvrir « {n} » : {e}"), true),
                            }
                        }
                        if let Some(n) = delete {
                            match prefs::delete_planche(&n) {
                                Ok(()) => self.say(format!("Planche « {n} » supprimée."), false),
                                Err(e) => self.say(format!("Impossible de supprimer « {n} » : {e}"), true),
                            }
                            self.confirm_delete = None;
                            self.planches = prefs::list_planches();
                        }
                    });

                    section(ui, "s_teinte", "Teinte et lumière", false, |ui| {
                        for (i, name) in ["Rouge", "Vert", "Bleu"].iter().enumerate() {
                            ui.horizontal(|ui| {
                                field_label(ui, name);
                                ui.add(egui::Slider::new(&mut self.color.gain[i], -40..=40).suffix(" %"));
                            });
                        }
                        ui.horizontal(|ui| {
                            field_label(ui, "Luminosité");
                            ui.add(egui::Slider::new(&mut self.color.brightness, -50..=50).suffix(" %"));
                        });
                        ui.horizontal(|ui| {
                            field_label(ui, "Contraste");
                            ui.add(egui::Slider::new(&mut self.color.contrast, -50..=50).suffix(" %"));
                        });
                        let touched = self.color.gain != [0; 3] || self.color.brightness != 0 || self.color.contrast != 0;
                        ui.horizontal(|ui| {
                            hint(ui, "Négatif = moins de cette couleur.");
                            if touched && ui.small_button("Réinitialiser").clicked() {
                                self.color.gain = [0; 3];
                                self.color.brightness = 0;
                                self.color.contrast = 0;
                            }
                        });
                    });

                    section(ui, "s_icc", "Couleurs (profil ICC)", false, |ui| {
                        profile_picker(ui, "icc_src", "Profil des images", &mut self.color.src, &self.profiles, &mut self.status);
                        profile_picker(ui, "icc_out", "Profil de sortie (imprimante + papier)", &mut self.color.out, &self.profiles, &mut self.status);
                        ui.horizontal(|ui| {
                            field_label(ui, "Intention");
                            egui::ComboBox::from_id_salt("intent")
                                .width(180.0)
                                .selected_text(self.color.intent.label())
                                .show_ui(ui, |ui| {
                                    for it in icc::Intent::ALL {
                                        ui.selectable_value(&mut self.color.intent, it, it.label());
                                    }
                                });
                        });
                        ui.checkbox(&mut self.color.bpc, "Compensation du point noir (comme Photoshop)");
                        ui.add_enabled(self.color.out.is_some(), egui::Checkbox::new(&mut self.color.softproof, "Aperçu fidèle à l'écran"));
                        if self.color.out.is_some() {
                            hint(ui, "Couleurs converties vers ce profil : désactive la gestion des couleurs du pilote dans « Propriétés ». Le PDF embarque le profil.");
                        }
                        if ui
                            .button("Ajouter une mire de test couleur")
                            .on_hover_text("Gris, couleurs pures, teintes chair et dégradés : à imprimer pour vérifier les réglages")
                            .clicked()
                        {
                            self.add_test_chart(ctx);
                        }
                    });
                    hint(ui, "Réglages et dernière planche enregistrés automatiquement.");
                });
            });
    }

    fn design_cards(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let b = self.bleed();
        let mut remove = None;
        let mut rotate = None;
        let mut sel = self.selected;
        for i in 0..self.designs.len() {
            let selected = self.selected == Some(i);
            let d = &mut self.designs[i];
            let resp = ui
                .scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
                    egui::Frame::new()
                        .fill(if selected { Color32::from_rgb(44, 38, 35) } else { CARD })
                        .stroke(if selected { Stroke::new(1.5, ACCENT) } else { Stroke::new(1.0, CARD) })
                        .corner_radius(CornerRadius::same(12))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                // Vignette.
                                let (r, _) = ui.allocate_exact_size(Vec2::splat(60.0), Sense::hover());
                                ui.painter().rect_filled(r, 8.0, Color32::from_rgb(24, 25, 29));
                                let a = d.aspect();
                                let s = if a >= 1.0 { Vec2::new(56.0, 56.0 / a) } else { Vec2::new(56.0 * a, 56.0) };
                                ui.painter().image(
                                    d.tex.id(),
                                    Rect::from_center_size(r.center(), s),
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 3.0;
                                    ui.add(egui::Label::new(RichText::new(&d.name).family(semibold()).color(TEXT)).truncate());
                                    let size = if b > 0.0 {
                                        format!(
                                            "{:.0} × {:.0} mm · impr. {:.0} × {:.0}",
                                            d.width_mm,
                                            d.height_mm(),
                                            d.width_mm + 2.0 * b,
                                            d.height_mm() + 2.0 * b
                                        )
                                    } else {
                                        format!("{:.0} × {:.0} mm", d.width_mm, d.height_mm())
                                    };
                                    ui.label(RichText::new(size).color(MUTED).small());
                                    let dpi = d.dpi();
                                    if dpi < LOW_DPI {
                                        ui.label(RichText::new(format!("⚠ {dpi:.0} dpi : risque de flou")).color(WARN).small())
                                            .on_hover_text("Image trop petite pour cette taille d'impression. Il faut au moins 200 dpi, idéalement 300.");
                                    }
                                    ui.add_space(2.0);
                                    stepper(ui, &mut d.count, " ex.");
                                });
                            });
                            if selected {
                                ui.add_space(4.0);
                                ui.separator();
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("Format").color(MUTED));
                                    format_menu(ui, &self.formats, d, ("fmt", i));
                                });
                                ui.horizontal(|ui| {
                                    let mut w = d.width_mm;
                                    let mut h = d.height_mm();
                                    ui.label(RichText::new("L").color(MUTED));
                                    if ui.add(egui::DragValue::new(&mut w).range(5.0..=400.0).suffix(" mm").speed(0.2).fixed_decimals(1)).changed() {
                                        if d.frame.is_some() {
                                            // Format fixe : la hauteur ne bouge pas, le recadrage s'adapte.
                                            d.frame = Some(w / h);
                                        }
                                        d.width_mm = w;
                                    }
                                    ui.label(RichText::new("H").color(MUTED));
                                    if ui.add(egui::DragValue::new(&mut h).range(5.0..=400.0).suffix(" mm").speed(0.2).fixed_decimals(1)).changed() {
                                        if d.frame.is_some() {
                                            d.frame = Some(d.width_mm / h);
                                        } else {
                                            d.set_height_mm(h);
                                        }
                                    }
                                });
                                hint(
                                    ui,
                                    if d.frame.is_some() {
                                        "Image recadrée : fais-la glisser sur la planche pour choisir la partie gardée."
                                    } else {
                                        "Proportions de l'image gardées."
                                    },
                                );
                                ui.horizontal_wrapped(|ui| {
                                    let label = if d.landscape() { "Passer en portrait" } else { "Passer en paysage" };
                                    if ui.button(label).clicked() {
                                        rotate = Some(i);
                                    }
                                    if ui.button("Remplir la planche").on_hover_text("Ajoute des exemplaires de ce design dans toute la place libre").clicked() {
                                        self.pending_fill = Some(i);
                                    }
                                    if ui.button(RichText::new("Retirer").color(Color32::from_rgb(240, 130, 120))).clicked() {
                                        remove = Some(i);
                                    }
                                });
                            }
                        });
                })
                .response;
            if resp.clicked() {
                sel = Some(i);
            }
            ui.add_space(6.0);
        }
        self.selected = sel;
        if let Some(i) = rotate {
            self.rotate(ctx, i);
        }
        if let Some(i) = remove {
            // Retiré au début de l'image suivante : la mise en page de celle-ci le référence encore.
            self.pending_remove = Some(i);
            ctx.request_repaint();
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui, layout: &Layout) {
        let ctx = &ui.ctx().clone();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(0)))
            .show(ui, |ui| {
                // Fond pointillé façon tapis de découpe.
                let area = ui.max_rect();
                {
                    let p = ui.painter();
                    let step = 22.0;
                    let mut y = area.top() + step / 2.0;
                    while y < area.bottom() {
                        let mut x = area.left() + step / 2.0;
                        while x < area.right() {
                            p.circle_filled(Pos2::new(x, y), 1.1, DOT);
                            x += step;
                        }
                        y += step;
                    }
                }
                // Ctrl + molette = zoom.
                if ui.rect_contains_pointer(area) {
                    let z = ctx.input(|i| i.zoom_delta());
                    if z != 1.0 {
                        self.zoom = (self.zoom * z).clamp(0.4, 2.5);
                    }
                }
                self.banners(ui, layout);
                let mut rotate_req = None;
                let mut delete_req = None;
                let mut select_req = None;
                let mut fill_req = None;
                let b = self.bleed();
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(18.0);
                    let avail = ui.available_width();
                    let pw = ((avail - 80.0).max(240.0) * self.zoom).min(2600.0);
                    let pxmm = pw / A4_W;
                    let ph = A4_H * pxmm;
                    for pg in 0..self.settings.pages {
                        let row_w = avail.max(pw + 60.0);
                        let (full, _) = ui.allocate_exact_size(Vec2::new(row_w, ph + 48.0), Sense::hover());
                        let page = Rect::from_min_size(Pos2::new(full.center().x - pw / 2.0, full.top() + 30.0), Vec2::new(pw, ph));
                        let painter = ui.painter();
                        // Étiquette de page.
                        let n = layout.items.iter().filter(|p| p.page == pg).count();
                        painter.text(
                            Pos2::new(page.left() + 2.0, page.top() - 8.0),
                            Align2::LEFT_BOTTOM,
                            format!("Page {} / {}", pg + 1, self.settings.pages),
                            FontId::new(13.0, semibold()),
                            TEXT,
                        );
                        painter.text(
                            Pos2::new(page.right() - 2.0, page.top() - 8.0),
                            Align2::RIGHT_BOTTOM,
                            format!("A4 · 210 × 297 mm · {n} élément(s)"),
                            FontId::proportional(12.0),
                            MUTED,
                        );
                        // Ombre douce + feuille.
                        for (i, a) in [(14.0, 18u8), (8.0, 30), (3.0, 50)] {
                            painter.rect_filled(page.expand(i * 0.5).translate(Vec2::new(0.0, i * 0.4)), 4.0 + i, Color32::from_black_alpha(a));
                        }
                        painter.rect_filled(page, 2.0, Color32::WHITE);
                        dashed_rect(painter, page.shrink(self.settings.margin * pxmm), Stroke::new(1.0, Color32::from_rgb(214, 218, 226)), 5.0, 4.0);
                        if self.designs.is_empty() {
                            let z = Rect::from_center_size(page.center(), Vec2::new(pw * 0.6, pw * 0.32));
                            painter.rect_filled(z, 14.0, Color32::from_rgb(250, 246, 242));
                            dashed_rect(painter, z, Stroke::new(1.5, ACCENT.gamma_multiply(0.7)), 8.0, 6.0);
                            painter.text(z.center() - Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, "Glissez vos designs ici", FontId::new(20.0, semibold()), Color32::from_rgb(70, 60, 55));
                            painter.text(
                                z.center() + Vec2::new(0.0, 16.0),
                                Align2::CENTER_CENTER,
                                "ou cliquez sur « Ajouter des images »",
                                FontId::proportional(14.0),
                                Color32::from_rgb(140, 130, 125),
                            );
                        }

                        for (idx, p) in layout.items.iter().enumerate().filter(|(_, p)| p.page == pg) {
                            let r = Rect::from_min_size(page.min + Vec2::new(p.x * pxmm, p.y * pxmm), Vec2::new(p.w * pxmm, p.h * pxmm));
                            let sel = self.selected == Some(p.design);
                            let croppable = sel && self.designs[p.design].frame.is_some();
                            let sense = if croppable { Sense::click_and_drag() } else { Sense::click() };
                            let resp = ui.interact(r, ui.scope_id().with(("item", idx)), sense);
                            let painter = ui.painter();
                            let d = &mut self.designs[p.design];
                            // Recadrage du design puis rognage du débord.
                            let c = d.crop_uv();
                            let pu = p.uv();
                            let uv = [
                                c[0] + pu[0] * (c[2] - c[0]),
                                c[1] + pu[1] * (c[3] - c[1]),
                                c[0] + pu[2] * (c[2] - c[0]),
                                c[1] + pu[3] * (c[3] - c[1]),
                            ];
                            painter.image(d.tex.id(), r, Rect::from_min_max(Pos2::new(uv[0], uv[1]), Pos2::new(uv[2], uv[3])), Color32::WHITE);
                            if croppable {
                                if resp.hovered() || resp.dragged() {
                                    ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
                                }
                                if resp.dragged() {
                                    // Pixels écran par unité d'image (0..1), puis part déplaçable de l'image.
                                    let dl = resp.drag_delta();
                                    let ppu = (r.width() / (uv[2] - uv[0]), r.height() / (uv[3] - uv[1]));
                                    let free = (1.0 - (c[2] - c[0]), 1.0 - (c[3] - c[1]));
                                    if free.0 > 1e-4 {
                                        d.pan[0] = (d.pan[0] - dl.x / ppu.0 / free.0).clamp(0.0, 1.0);
                                    }
                                    if free.1 > 1e-4 {
                                        d.pan[1] = (d.pan[1] - dl.y / ppu.1 / free.1).clamp(0.0, 1.0);
                                    }
                                }
                            }
                            // Résolution trop faible : pastille d'alerte (écran seulement).
                            let dpi = d.dpi();
                            if dpi < LOW_DPI {
                                let t = format!("{dpi:.0} dpi · flou");
                                let g = painter.layout_no_wrap(t, FontId::proportional(11.0), Color32::from_rgb(40, 30, 10));
                                let br = Rect::from_min_size(r.min + Vec2::new(4.0, 4.0), g.size() + Vec2::new(10.0, 4.0));
                                painter.rect_filled(br, 6.0, WARN);
                                painter.galley(br.min + Vec2::new(5.0, 2.0), g, Color32::from_rgb(40, 30, 10));
                            }
                            // Format final du magnet (écran seulement, jamais imprimé).
                            if b > 0.0 {
                                dashed_rect(painter, r.shrink(b * pxmm), Stroke::new(1.0, Color32::from_white_alpha(170)), 4.0, 3.0);
                            }
                            if self.settings.show_contour && self.settings.corners {
                                let st = Stroke::new(1.2, CUT);
                                for [(ax, ay), (bx, by)] in model::corner_marks(p, self.settings.gap) {
                                    let m = |x: f32, y: f32| r.min + Vec2::new((x - p.x) * pxmm, (y - p.y) * pxmm);
                                    painter.line_segment([m(ax, ay), m(bx, by)], st);
                                }
                            } else if self.settings.show_contour {
                                let st = Stroke::new(1.2, CUT);
                                match self.settings.shape {
                                    Shape::Rect => {
                                        painter.rect_stroke(r, 0.0, st, StrokeKind::Middle);
                                    }
                                    Shape::Ellipse => {
                                        painter.add(EShape::Ellipse(EllipseShape::stroke(r.center(), r.size() / 2.0, st)));
                                    }
                                }
                            }
                            if sel {
                                painter.rect_stroke(r.expand(3.0), 4.0, Stroke::new(2.0, ACCENT), StrokeKind::Middle);
                            } else if resp.hovered() {
                                painter.rect_stroke(r.expand(2.0), 4.0, Stroke::new(1.5, ACCENT.gamma_multiply(0.6)), StrokeKind::Middle);
                            }
                            if resp.clicked() || resp.secondary_clicked() {
                                select_req = Some(p.design);
                            }
                            let resp = resp.on_hover_text(if croppable {
                                "Glisser : choisir la partie gardée · Clic droit : exemplaires, format, taille"
                            } else {
                                "Clic droit : exemplaires, format, taille · Suppr : un exemplaire de moins"
                            });
                            egui::Popup::context_menu(&resp)
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                .show(|ui| {
                                    ui.set_min_width(250.0);
                                    ui.label(RichText::new(&d.name).family(semibold()).color(TEXT));
                                    ui.add_space(2.0);
                                    ui.horizontal(|ui| {
                                        field_label(ui, "Exemplaires");
                                        stepper(ui, &mut d.count, "");
                                    });
                                    ui.horizontal(|ui| {
                                        field_label(ui, "Largeur");
                                        ui.add(egui::DragValue::new(&mut d.width_mm).range(5.0..=400.0).suffix(" mm").speed(0.2).fixed_decimals(1));
                                    });
                                    ui.horizontal(|ui| {
                                        field_label(ui, "Format");
                                        format_menu(ui, &self.formats, d, ("cfmt", idx));
                                    });
                                    ui.separator();
                                    if ui.button("Remplir la planche avec ce design").clicked() {
                                        fill_req = Some(p.design);
                                        ui.close();
                                    }
                                    let label = if d.landscape() { "Passer en portrait" } else { "Passer en paysage" };
                                    if ui.button(label).clicked() {
                                        rotate_req = Some(p.design);
                                        ui.close();
                                    }
                                    if ui.button(RichText::new("Retirer ce design").color(Color32::from_rgb(240, 130, 120))).clicked() {
                                        delete_req = Some(p.design);
                                        ui.close();
                                    }
                                });

                            // Poignée de redimensionnement (coin bas-droit) du design sélectionné.
                            if sel {
                                let hp = r.right_bottom();
                                let hr = Rect::from_center_size(hp, Vec2::splat(18.0));
                                let hresp = ui.interact(hr, ui.scope_id().with(("handle", idx)), Sense::drag());
                                let painter = ui.painter();
                                painter.circle_filled(hp, 7.0, ACCENT);
                                painter.circle_stroke(hp, 7.0, Stroke::new(2.0, Color32::WHITE));
                                if hresp.hovered() || hresp.dragged() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
                                }
                                if hresp.dragged() {
                                    let dx = hresp.drag_delta().x / pxmm;
                                    let d = &mut self.designs[p.design];
                                    d.width_mm = (d.width_mm + dx).clamp(5.0, 400.0);
                                }
                            }
                        }
                    }
                    ui.add_space(24.0);
                });
                if let Some(i) = select_req {
                    self.selected = Some(i);
                }
                if let Some(i) = rotate_req {
                    self.rotate(ctx, i);
                }
                if let Some(i) = delete_req {
                    self.pending_remove = Some(i);
                    ctx.request_repaint();
                }
                if fill_req.is_some() {
                    self.pending_fill = fill_req;
                }
            });
    }

    /// Alerte en haut du canvas : image trop grande, ou plus de place.
    fn banners(&mut self, ui: &mut egui::Ui, layout: &Layout) {
        let pages = self.settings.pages;
        if layout.fits(pages) {
            return;
        }
        let b = self.bleed();
        let aw = A4_W - 2.0 * self.settings.margin;
        let ah = A4_H - 2.0 * self.settings.margin;
        let hidden = layout.items.iter().filter(|p| p.page >= pages).count();
        let scale = find_scale(&self.designs, &self.settings, pages, None);
        let mut act: Option<u8> = None;
        let mut fit_one: Option<usize> = None;
        egui::Frame::new()
            .outer_margin(Margin { left: 16, right: 16, top: 12, bottom: 0 })
            .fill(Color32::from_rgb(52, 42, 24))
            .stroke(Stroke::new(1.0, Color32::from_rgb(110, 84, 36)))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                for &i in &layout.oversize {
                    let d = &self.designs[i];
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "« {} » ({:.0} × {:.0} mm) dépasse la zone utile d'une page ({:.0} × {:.0} mm).",
                                d.name,
                                d.width_mm + 2.0 * b,
                                d.height_mm() + 2.0 * b,
                                aw,
                                ah
                            ))
                            .color(WARN),
                        );
                        if ui.button("Ajuster à la page").clicked() {
                            fit_one = Some(i);
                        }
                    });
                }
                if hidden > 0 {
                    ui.label(RichText::new(format!("{hidden} élément(s) ne tiennent pas sur {pages} page(s).")).color(WARN).family(semibold()));
                }
                ui.horizontal_wrapped(|ui| {
                    if layout.oversize.is_empty() && ui.add(primary("Optimiser la place")).on_hover_text("Changer l'ordre et le sens des designs").clicked() {
                        act = Some(2);
                    }
                    if layout.oversize.is_empty() && ui.add(secondary(&format!("Ajouter une page ({})", pages + 1))).clicked() {
                        act = Some(0);
                    }
                    if ui
                        .add(secondary(&format!("Réduire de {:.0} % pour tout faire tenir sur {pages} page(s)", (1.0 - scale) * 100.0)))
                        .clicked()
                    {
                        act = Some(1);
                    }
                });
            });
        if let Some(i) = fit_one {
            let d = &mut self.designs[i];
            let k = ((aw - 2.0 * b) / d.width_mm).min((ah - 2.0 * b) / d.height_mm()) * 0.999;
            d.width_mm *= k;
        }
        match act {
            Some(0) => self.settings.pages += 1,
            Some(2) => self.pending_optimize = true,
            Some(1) => {
                for d in &mut self.designs {
                    d.width_mm *= scale;
                }
            }
            _ => {}
        }
    }
}

/// Liste déroulante de profils : « sRGB », profils installés dans Windows, ou parcourir un fichier.
fn profile_picker(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    cur: &mut Option<icc::Profile>,
    list: &[PathBuf],
    status: &mut (String, bool),
) {
    ui.label(RichText::new(label).color(MUTED));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt(id)
            .width(196.0)
            .selected_text(cur.as_ref().map_or("sRGB (par défaut)".to_string(), |p| p.name.clone()))
            .show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_none(), "sRGB (par défaut)").clicked() {
                    *cur = None;
                }
                for p in list {
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    let sel = cur.as_ref().is_some_and(|c| c.name == name);
                    if ui.selectable_label(sel, &name).clicked() {
                        match icc::Profile::load(p) {
                            Ok(pr) => *cur = Some(pr),
                            Err(e) => *status = (format!("Profil « {name} » : {e}"), true),
                        }
                    }
                }
            });
        if ui.button("Parcourir…").clicked() {
            if let Some(f) = rfd::FileDialog::new()
                .set_title("Choisir un profil ICC")
                .add_filter("Profils ICC", &["icc", "icm"])
                .pick_file()
            {
                match icc::Profile::load(&f) {
                    Ok(pr) => *cur = Some(pr),
                    Err(e) => *status = (format!("Profil illisible : {e}"), true),
                }
            }
        }
    });
}
