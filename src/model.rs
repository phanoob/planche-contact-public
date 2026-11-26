//! Modèle de données et mise en page automatique de la planche.

use image::RgbaImage;
use std::sync::Arc;

pub const A4_W: f32 = 210.0;
pub const A4_H: f32 = 297.0;
const EPS: f32 = 1e-3;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Shape {
    Rect,
    Ellipse,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub margin: f32,
    pub gap: f32,
    pub pages: usize,
    pub shape: Shape,
    pub show_contour: bool,
    /// Mode magnet / sublimation : chaque image déborde de `bleed` mm de chaque côté.
    pub magnet: bool,
    pub bleed: f32,
    /// Impression en miroir (transfert par sublimation).
    pub mirror: bool,
    /// Traits de coupe : seulement des repères aux coins, à l'extérieur des images.
    pub corners: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            margin: 8.0,
            gap: 3.0,
            pages: 1,
            shape: Shape::Rect,
            show_contour: true,
            magnet: true,
            bleed: 1.5,
            mirror: false,
            corners: false,
        }
    }
}

/// Format d'impression (magnet, mug…) : l'image est recadrée à ces proportions.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Format {
    pub name: String,
    pub w: f32,
    pub h: f32,
}

impl Format {
    pub fn defaults() -> Vec<Format> {
        vec![
            Format { name: "Magnet".into(), w: 75.0, h: 55.0 },
            Format { name: "Mug".into(), w: 200.0, h: 75.0 },
        ]
    }
    pub fn label(&self) -> String {
        format!("{} {:.0} × {:.0}", self.name, self.w, self.h)
    }
    /// Largeur × hauteur dans l'orientation de l'image (côté long dans le sens du côté long).
    pub fn oriented(&self, landscape: bool) -> (f32, f32) {
        let (a, b) = (self.w.max(self.h), self.w.min(self.h));
        if landscape { (a, b) } else { (b, a) }
    }
}

/// Résolution en dessous de laquelle l'image risque d'être floue.
pub const LOW_DPI: f32 = 200.0;

#[derive(Clone)]
pub struct Design {
    pub name: String,
    pub path: std::path::PathBuf,
    /// Page du PDF (0 pour une image).
    pub page: u32,
    /// Image d'origine (jamais modifiée).
    pub base: Arc<RgbaImage>,
    /// Image dans l'orientation courante (pivotée ou non).
    pub eff: Arc<RgbaImage>,
    pub rotated: bool,
    pub tex: eframe::egui::TextureHandle,
    /// Largeur en mm dans l'orientation courante ; la hauteur en découle.
    pub width_mm: f32,
    pub count: u32,
    /// Format fixe (largeur / hauteur) : l'image est recadrée. `None` = proportions de l'image.
    pub frame: Option<f32>,
    /// Position du recadrage (0..1, 0.5 = centré).
    pub pan: [f32; 2],
}

impl Design {
    pub fn img_aspect(&self) -> f32 {
        self.eff.width() as f32 / self.eff.height() as f32
    }
    pub fn aspect(&self) -> f32 {
        self.frame.unwrap_or_else(|| self.img_aspect())
    }
    /// Partie de l'image gardée par le recadrage (coordonnées 0..1).
    pub fn crop_uv(&self) -> [f32; 4] {
        let Some(f) = self.frame else { return [0.0, 0.0, 1.0, 1.0] };
        let a = self.img_aspect();
        if a > f {
            let w = f / a;
            let u = self.pan[0].clamp(0.0, 1.0) * (1.0 - w);
            [u, 0.0, u + w, 1.0]
        } else {
            let h = a / f;
            let v = self.pan[1].clamp(0.0, 1.0) * (1.0 - h);
            [0.0, v, 1.0, v + h]
        }
    }
    /// Image recadrée (pour l'impression et le PDF).
    pub fn cropped(&self) -> RgbaImage {
        let [u0, v0, u1, v1] = self.crop_uv();
        let (w, h) = (self.eff.width() as f32, self.eff.height() as f32);
        let x = (u0 * w).round() as u32;
        let y = (v0 * h).round() as u32;
        let cw = (((u1 - u0) * w).round() as u32).clamp(1, self.eff.width() - x.min(self.eff.width() - 1));
        let ch = (((v1 - v0) * h).round() as u32).clamp(1, self.eff.height() - y.min(self.eff.height() - 1));
        image::imageops::crop_imm(self.eff.as_ref(), x, y, cw, ch).to_image()
    }
    /// Résolution d'impression (points par pouce) à la taille choisie.
    pub fn dpi(&self) -> f32 {
        let [u0, _, u1, _] = self.crop_uv();
        self.eff.width() as f32 * (u1 - u0) / (self.width_mm / 25.4)
    }
    /// Applique un format : recadrage centré, taille du format dans l'orientation de l'image.
    pub fn set_format(&mut self, f: Option<&Format>) {
        match f {
            Some(f) => {
                let (w, h) = f.oriented(self.img_aspect() >= 1.0);
                self.frame = Some(w / h);
                self.pan = [0.5, 0.5];
                self.width_mm = w;
            }
            None => {
                // Retour aux proportions de l'image, en gardant le côté le plus long.
                let long = self.width_mm.max(self.height_mm());
                self.frame = None;
                let a = self.img_aspect();
                self.width_mm = if a >= 1.0 { long } else { long * a };
            }
        }
    }
    pub fn height_mm(&self) -> f32 {
        self.width_mm / self.aspect()
    }
    pub fn set_height_mm(&mut self, h: f32) {
        self.width_mm = h * self.aspect();
    }
    pub fn landscape(&self) -> bool {
        self.aspect() >= 1.0
    }
}

#[derive(Clone)]
pub struct Placed {
    pub design: usize,
    pub page: usize,
    /// Case imprimée (débord compris), en mm.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Taille de l'image dessinée, centrée sur la case (plus grande que la case
    /// en mode magnet : l'image la couvre et le surplus est rogné).
    pub iw: f32,
    pub ih: f32,
}

impl Placed {
    /// Partie visible de l'image (coordonnées 0..1) quand elle déborde de la case.
    pub fn uv(&self) -> [f32; 4] {
        let u = ((self.iw - self.w) / 2.0 / self.iw).max(0.0);
        let v = ((self.ih - self.h) / 2.0 / self.ih).max(0.0);
        [u, v, 1.0 - u, 1.0 - v]
    }
}

/// Retourne la page horizontalement (impression miroir).
pub fn mirrored(l: &Layout) -> Layout {
    let mut l = l.clone();
    for p in &mut l.items {
        p.x = A4_W - p.x - p.w;
    }
    l
}

#[derive(Clone, Default)]
pub struct Layout {
    pub items: Vec<Placed>,
    pub pages_needed: usize,
    /// Designs trop grands pour la zone imprimable d'une page.
    pub oversize: Vec<usize>,
}

impl Layout {
    pub fn fits(&self, pages: usize) -> bool {
        self.oversize.is_empty() && self.pages_needed <= pages
    }
}

/// Place les éléments en rangées, dans l'ordre des designs, page après page.
pub fn compute(designs: &[Design], s: &Settings, scales: &[f32]) -> Layout {
    let sizes: Vec<(f32, f32, u32)> =
        designs.iter().enumerate().map(|(i, d)| (d.width_mm * scales[i], d.height_mm() * scales[i], d.count)).collect();
    compute_sizes(&sizes, s)
}

/// Mise en page à partir des tailles (largeur, hauteur en mm, exemplaires) de chaque design.
pub fn compute_sizes(sizes: &[(f32, f32, u32)], s: &Settings) -> Layout {
    let aw = A4_W - 2.0 * s.margin;
    let ah = A4_H - 2.0 * s.margin;
    let b = if s.magnet { s.bleed.max(0.0) } else { 0.0 };
    let (mut x, mut y, mut row_h, mut page) = (0.0f32, 0.0f32, 0.0f32, 0usize);
    let mut items = Vec::new();
    let mut oversize = Vec::new();
    for (i, &(iw0, ih0, count)) in sizes.iter().enumerate() {
        let (w, h) = (iw0 + 2.0 * b, ih0 + 2.0 * b);
        let k = (w / iw0).max(h / ih0);
        let (iw, ih) = (iw0 * k, ih0 * k);
        if count > 0 && (w > aw + EPS || h > ah + EPS) {
            oversize.push(i);
        }
        for _ in 0..count {
            if x > 0.0 && x + w > aw + EPS {
                x = 0.0;
                y += row_h + s.gap;
                row_h = 0.0;
            }
            if y > 0.0 && y + h > ah + EPS {
                page += 1;
                x = 0.0;
                y = 0.0;
                row_h = 0.0;
            }
            items.push(Placed {
                design: i,
                page,
                x: s.margin + x,
                y: s.margin + y,
                w,
                h,
                iw,
                ih,
            });
            x += w + s.gap;
            row_h = row_h.max(h);
        }
    }
    let pages_needed = if items.is_empty() { 1 } else { page + 1 };
    Layout {
        items,
        pages_needed,
        oversize,
    }
}

/// Place occupée : nombre de pages, puis hauteur utilisée sur la dernière (plus petit = mieux).
fn score(l: &Layout) -> (usize, usize, i64) {
    let last = l.pages_needed.saturating_sub(1);
    let used = l.items.iter().filter(|p| p.page == last).map(|p| p.y + p.h).fold(0.0f32, f32::max);
    (l.oversize.len(), l.pages_needed, (used * 100.0) as i64)
}

/// Cherche un ordre des designs et le sens de chacun (pivoté ou non) qui prennent
/// le moins de place. Renvoie (ordre, à pivoter) ; `None` si rien ne s'améliore.
pub fn optimize(designs: &[Design], s: &Settings) -> Option<(Vec<usize>, Vec<bool>)> {
    let n = designs.len();
    if n == 0 {
        return None;
    }
    let base: Vec<(f32, f32, u32)> = designs.iter().map(|d| (d.width_mm, d.height_mm(), d.count)).collect();
    let eval = |order: &[usize], rot: &[bool]| {
        let sizes: Vec<(f32, f32, u32)> = order
            .iter()
            .map(|&i| {
                let (w, h, c) = base[i];
                if rot[i] { (h, w, c) } else { (w, h, c) }
            })
            .collect();
        score(&compute_sizes(&sizes, s))
    };
    let ident: Vec<usize> = (0..n).collect();
    let start = eval(&ident, &vec![false; n]);
    // Ordres essayés : actuel, puis du plus haut / plus large / plus grand au plus petit.
    let mut orders = vec![ident.clone()];
    for key in 0..3 {
        let mut o = ident.clone();
        o.sort_by(|&a, &b| {
            let f = |i: usize| match key {
                0 => base[i].0.min(base[i].1),
                1 => base[i].0.max(base[i].1),
                _ => base[i].0 * base[i].1,
            };
            f(b).partial_cmp(&f(a)).unwrap_or(std::cmp::Ordering::Equal)
        });
        orders.push(o);
    }
    let mut best: Option<((usize, usize, i64), Vec<usize>, Vec<bool>)> = None;
    for order in orders {
        // Deux départs : sens actuel, ou tout couché (côté long à l'horizontale).
        for lay in [false, true] {
            let mut rot: Vec<bool> = (0..n).map(|i| lay && base[i].1 > base[i].0).collect();
            let mut cur = eval(&order, &rot);
            // Améliorations une par une : pivoter un design si ça gagne de la place.
            for _ in 0..3 {
                let mut changed = false;
                for i in 0..n {
                    rot[i] = !rot[i];
                    let sc = eval(&order, &rot);
                    if sc < cur {
                        cur = sc;
                        changed = true;
                    } else {
                        rot[i] = !rot[i];
                    }
                }
                if !changed {
                    break;
                }
            }
            if best.as_ref().is_none_or(|b| cur < b.0) {
                best = Some((cur, order.clone(), rot));
            }
        }
    }
    let (sc, order, rot) = best?;
    (sc < start).then_some((order, rot))
}

/// Repères de coupe aux coins d'une case : 8 petits segments (en mm) tournés vers l'extérieur,
/// assez courts pour ne pas toucher les cases voisines.
pub fn corner_marks(p: &Placed, gap: f32) -> Vec<[(f32, f32); 2]> {
    let len = (gap * 0.45).clamp(0.8, 3.0);
    let (x0, y0, x1, y1) = (p.x, p.y, p.x + p.w, p.y + p.h);
    let mut v = Vec::with_capacity(8);
    for (cx, cy, sx, sy) in [(x0, y0, -1.0, -1.0), (x1, y0, 1.0, -1.0), (x0, y1, -1.0, 1.0), (x1, y1, 1.0, 1.0)] {
        // Prolongement du bord horizontal et du bord vertical, vers l'extérieur.
        v.push([(cx, cy), (cx + sx * len, cy)]);
        v.push([(cx, cy), (cx, cy + sy * len)]);
    }
    v
}

/// Plus grand facteur (≤ 1) à appliquer aux designs concernés pour que tout tienne
/// dans `pages` pages. `only` limite la réduction à un seul design.
pub fn find_scale(designs: &[Design], s: &Settings, pages: usize, only: Option<usize>) -> f32 {
    let scales_for = |f: f32| -> Vec<f32> {
        (0..designs.len())
            .map(|i| if only.is_none() || only == Some(i) { f } else { 1.0 })
            .collect()
    };
    let ok = |f: f32| compute(designs, s, &scales_for(f)).fits(pages);
    if ok(1.0) {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.02f32, 1.0f32);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if ok(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    // Marge de sécurité contre les arrondis, et valeur ronde au dixième de mm près.
    lo * 0.999
}

#[cfg(test)]
mod tests {
    use super::*;

    fn magnet(count: u32) -> Design {
        let img = Arc::new(RgbaImage::new(750, 550));
        let ctx = eframe::egui::Context::default();
        let tex = ctx.load_texture("m", eframe::egui::ColorImage::filled([1, 1], eframe::egui::Color32::WHITE), Default::default());
        Design {
            name: "m".into(),
            path: "m.png".into(),
            page: 0,
            base: img.clone(),
            eff: img,
            rotated: false,
            tex,
            width_mm: 75.0,
            count,
            frame: Some(75.0 / 55.0),
            pan: [0.5, 0.5],
        }
    }

    #[test]
    fn optimize_turns_magnets_to_fit_one_page() {
        // 9 magnets 75 × 55 (+ débord) : 8 par page couchés, 9 debout.
        let s = Settings::default();
        let d = vec![magnet(9)];
        assert_eq!(compute(&d, &s, &[1.0]).pages_needed, 2);
        let (order, rot) = optimize(&d, &s).expect("doit trouver mieux");
        assert_eq!(order, vec![0]);
        assert_eq!(rot, vec![true]);
        // Déjà au mieux : rien à changer.
        let sizes = [(55.0, 75.0, 9)];
        assert_eq!(compute_sizes(&sizes, &s).pages_needed, 1);
    }

    #[test]
    fn corner_marks_stay_outside() {
        let p = Placed { design: 0, page: 0, x: 10.0, y: 20.0, w: 30.0, h: 40.0, iw: 30.0, ih: 40.0 };
        let m = corner_marks(&p, 3.0);
        assert_eq!(m.len(), 8);
        for [a, b] in m {
            // Chaque repère part d'un coin et va vers l'extérieur de la case.
            let inside = |(x, y): (f32, f32)| x > 10.0 + 1e-3 && x < 40.0 - 1e-3 && y > 20.0 + 1e-3 && y < 60.0 - 1e-3;
            assert!(!inside(a) && !inside(b));
            let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            assert!((len - 1.35).abs() < 1e-3);
        }
    }

    #[test]
    fn crop_follows_frame_and_pan() {
        // Image 300 × 200 : moitié gauche rouge, moitié droite bleue.
        let img = RgbaImage::from_fn(300, 200, |x, _| if x < 150 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) });
        let img = Arc::new(img);
        let ctx = eframe::egui::Context::default();
        let tex = ctx.load_texture("t", eframe::egui::ColorImage::filled([1, 1], eframe::egui::Color32::WHITE), Default::default());
        let mut d = Design {
            name: "t".into(),
            path: "t.png".into(),
            page: 0,
            base: img.clone(),
            eff: img,
            rotated: false,
            tex,
            width_mm: 75.0,
            count: 1,
            frame: None,
            pan: [0.5, 0.5],
        };
        assert_eq!(d.cropped().dimensions(), (300, 200));
        // Format carré, cadré à gauche : 200 × 200, rouge à gauche, bleu à droite.
        d.frame = Some(1.0);
        d.pan = [0.0, 0.5];
        let c = d.cropped();
        assert_eq!(c.dimensions(), (200, 200));
        assert_eq!(c.get_pixel(0, 0)[0], 255);
        assert_eq!(c.get_pixel(199, 0)[2], 255);
        // Cadré à droite : colonnes 100..300, donc 50 px rouges puis du bleu.
        d.pan = [1.0, 0.5];
        let c = d.cropped();
        assert_eq!(c.get_pixel(49, 0)[0], 255);
        assert_eq!(c.get_pixel(50, 0)[2], 255);
        // Format magnet appliqué : 75 × 55 en paysage, dpi calculé sur la partie gardée.
        let f = Format { name: "Magnet".into(), w: 55.0, h: 75.0 };
        d.set_format(Some(&f));
        assert!((d.width_mm - 75.0).abs() < 1e-3 && (d.height_mm() - 55.0).abs() < 1e-3);
        assert!((d.dpi() - 200.0 * 75.0 / 55.0 / (75.0 / 25.4)).abs() < 1.0);
    }
}
