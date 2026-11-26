//! Rendu hors écran : PDF vectoriel (contours nets) et bitmap 300 dpi (impression directe).

use crate::model::{corner_marks, Layout, Placed, Settings, Shape, A4_H, A4_W};
use image::{imageops::FilterType, Rgb, RgbImage, RgbaImage};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};
use std::sync::Arc;

const PT_PER_MM: f32 = 72.0 / 25.4;

/// Aplatit l'image sur fond blanc (la transparence devient blanche).
fn flatten(img: &RgbaImage) -> RgbImage {
    let mut out = RgbImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        let a = p[3] as u32;
        let m = |c: u8| -> u8 { ((c as u32 * a + 255 * (255 - a)) / 255) as u8 };
        out.put_pixel(x, y, Rgb([m(p[0]), m(p[1]), m(p[2])]));
    }
    out
}

fn px_for(mm: f32, dpi: f32) -> u32 {
    ((mm / 25.4 * dpi).round() as u32).max(1)
}

// ---------------------------------------------------------------- PDF

pub fn export_pdf(
    imgs: &[Arc<RgbaImage>],
    layout: &Layout,
    s: &Settings,
    pages: usize,
    path: &std::path::Path,
    icc: Option<&[u8]>,
) -> Result<(), String> {
    let mut pdf = Pdf::new();
    let catalog = Ref::new(1);
    let tree = Ref::new(2);
    let mut next = 3;
    let mut alloc = || {
        let r = Ref::new(next);
        next += 1;
        r
    };

    let icc_id = icc.map(|b| {
        let id = alloc();
        pdf.icc_profile(id, b).n(3);
        id
    });
    // Une image JPEG par design utilisé, à 300 dpi de la plus grande taille imprimée.
    let mut xobj: Vec<Option<Ref>> = vec![None; imgs.len()];
    for p in &layout.items {
        if p.page >= pages || xobj[p.design].is_some() {
            continue;
        }
        let src = &imgs[p.design];
        let (tw, th) = (px_for(p.iw, 300.0), px_for(p.ih, 300.0));
        let resized = if tw < src.width() {
            image::imageops::resize(src.as_ref(), tw, th, FilterType::Lanczos3)
        } else {
            (**src).clone()
        };
        let rgb = flatten(&resized);
        let mut jpg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 93)
            .encode_image(&rgb)
            .map_err(|e| e.to_string())?;
        let id = alloc();
        let mut x = pdf.image_xobject(id, &jpg);
        x.filter(Filter::DctDecode);
        x.width(rgb.width() as i32);
        x.height(rgb.height() as i32);
        match icc_id {
            Some(id) => x.color_space().icc_based(id),
            None => {
                x.color_space().device_rgb();
            }
        }
        x.bits_per_component(8);
        x.finish();
        xobj[p.design] = Some(id);
    }

    let page_ids: Vec<Ref> = (0..pages).map(|_| alloc()).collect();
    let content_ids: Vec<Ref> = (0..pages).map(|_| alloc()).collect();

    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(page_ids.iter().copied())
        .count(pages as i32);

    for pg in 0..pages {
        let mut page = pdf.page(page_ids[pg]);
        page.media_box(Rect::new(0.0, 0.0, A4_W * PT_PER_MM, A4_H * PT_PER_MM));
        page.parent(tree);
        page.contents(content_ids[pg]);
        {
            let mut res = page.resources();
            let mut xo = res.x_objects();
            for (i, r) in xobj.iter().enumerate() {
                if let Some(r) = r {
                    xo.pair(Name(format!("Im{i}").as_bytes()), *r);
                }
            }
        }
        page.finish();

        let mut c = Content::new();
        for p in layout.items.iter().filter(|p| p.page == pg) {
            let (x, y, w, h) = (
                p.x * PT_PER_MM,
                (A4_H - p.y - p.h) * PT_PER_MM,
                p.w * PT_PER_MM,
                p.h * PT_PER_MM,
            );
            let (iw, ih) = (p.iw * PT_PER_MM, p.ih * PT_PER_MM);
            c.save_state();
            if p.iw > p.w + 1e-3 || p.ih > p.h + 1e-3 {
                // Mode magnet : l'image couvre la case, le surplus est rogné.
                c.rect(x, y, w, h);
                c.clip_nonzero();
                c.end_path();
            }
            c.transform([iw, 0.0, 0.0, ih, x - (iw - w) / 2.0, y - (ih - h) / 2.0]);
            c.x_object(Name(format!("Im{}", p.design).as_bytes()));
            c.restore_state();
        }
        if s.show_contour {
            c.set_line_width(0.3);
            c.set_stroke_rgb(0.0, 0.0, 0.0);
            for p in layout.items.iter().filter(|p| p.page == pg) {
                let (x, y, w, h) = (
                    p.x * PT_PER_MM,
                    (A4_H - p.y - p.h) * PT_PER_MM,
                    p.w * PT_PER_MM,
                    p.h * PT_PER_MM,
                );
                if s.corners {
                    for [(ax, ay), (bx, by)] in corner_marks(p, s.gap) {
                        c.move_to(ax * PT_PER_MM, (A4_H - ay) * PT_PER_MM);
                        c.line_to(bx * PT_PER_MM, (A4_H - by) * PT_PER_MM);
                    }
                    c.stroke();
                    continue;
                }
                match s.shape {
                    Shape::Rect => {
                        c.rect(x, y, w, h);
                    }
                    Shape::Ellipse => {
                        let k = 0.552_284_75;
                        let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
                        c.move_to(cx + rx, cy);
                        c.cubic_to(cx + rx, cy + k * ry, cx + k * rx, cy + ry, cx, cy + ry);
                        c.cubic_to(cx - k * rx, cy + ry, cx - rx, cy + k * ry, cx - rx, cy);
                        c.cubic_to(cx - rx, cy - k * ry, cx - k * rx, cy - ry, cx, cy - ry);
                        c.cubic_to(cx + k * rx, cy - ry, cx + rx, cy - k * ry, cx + rx, cy);
                        c.close_path();
                    }
                }
                c.stroke();
            }
        }
        pdf.stream(content_ids[pg], &c.finish());
    }

    std::fs::write(path, pdf.finish()).map_err(|e| e.to_string())
}

// ------------------------------------------------------------- Bitmap

fn dot(img: &mut RgbImage, cx: i32, cy: i32, r: i32) {
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y) = (cx + dx, cy + dy);
            if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
                img.put_pixel(x as u32, y as u32, Rgb([0, 0, 0]));
            }
        }
    }
}

fn contour(img: &mut RgbImage, p: &Placed, s: &Settings, pxmm: f32) {
    if s.corners {
        for [(ax, ay), (bx, by)] in corner_marks(p, s.gap) {
            let n = (((bx - ax).abs() + (by - ay).abs()) * pxmm) as i32;
            for i in 0..=n {
                let t = i as f32 / n.max(1) as f32;
                dot(img, ((ax + (bx - ax) * t) * pxmm) as i32, ((ay + (by - ay) * t) * pxmm) as i32, 1);
            }
        }
        return;
    }
    let shape = s.shape;
    let (x, y, w, h) = (p.x * pxmm, p.y * pxmm, p.w * pxmm, p.h * pxmm);
    match shape {
        Shape::Rect => {
            let (x1, y1) = ((x + w) as i32, (y + h) as i32);
            for i in 0..=(w as i32) {
                dot(img, x as i32 + i, y as i32, 1);
                dot(img, x as i32 + i, y1, 1);
            }
            for i in 0..=(h as i32) {
                dot(img, x as i32, y as i32 + i, 1);
                dot(img, x1, y as i32 + i, 1);
            }
        }
        Shape::Ellipse => {
            let (cx, cy, rx, ry) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
            let steps = ((rx + ry) * 6.0) as usize + 8;
            for i in 0..steps {
                let a = i as f32 / steps as f32 * std::f32::consts::TAU;
                dot(img, (cx + rx * a.cos()) as i32, (cy + ry * a.sin()) as i32, 1);
            }
        }
    }
}

/// Dessine une page A4 entière à `dpi` points par pouce.
pub fn render_page(
    imgs: &[Arc<RgbaImage>],
    layout: &Layout,
    s: &Settings,
    page: usize,
    dpi: f32,
) -> RgbImage {
    let pxmm = dpi / 25.4;
    let mut out = RgbImage::from_pixel(px_for(A4_W, dpi), px_for(A4_H, dpi), Rgb([255, 255, 255]));
    for p in layout.items.iter().filter(|p| p.page == page) {
        let r = image::imageops::resize(
            imgs[p.design].as_ref(),
            px_for(p.iw, dpi),
            px_for(p.ih, dpi),
            FilterType::CatmullRom,
        );
        let ox = ((p.x - (p.iw - p.w) / 2.0) * pxmm).round() as i64;
        let oy = ((p.y - (p.ih - p.h) / 2.0) * pxmm).round() as i64;
        // Case (débord compris) : rien n'est dessiné en dehors.
        let bx0 = (p.x * pxmm).round() as i64;
        let by0 = (p.y * pxmm).round() as i64;
        let bx1 = ((p.x + p.w) * pxmm).round() as i64;
        let by1 = ((p.y + p.h) * pxmm).round() as i64;
        for (x, y, px) in r.enumerate_pixels() {
            let (tx, ty) = (ox + x as i64, oy + y as i64);
            if tx < bx0.max(0) || ty < by0.max(0) || tx >= bx1.min(out.width() as i64) || ty >= by1.min(out.height() as i64) {
                continue;
            }
            let a = px[3] as u32;
            let m = |c: u8| -> u8 { ((c as u32 * a + 255 * (255 - a)) / 255) as u8 };
            out.put_pixel(tx as u32, ty as u32, Rgb([m(px[0]), m(px[1]), m(px[2])]));
        }
    }
    if s.show_contour {
        for p in layout.items.iter().filter(|p| p.page == page) {
            contour(&mut out, p, s, pxmm);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layout, Placed, Settings, Shape};
    #[test]
    fn export_samples() {
        let dir = std::env::temp_dir().join("pc-test");
        std::fs::create_dir_all(&dir).unwrap();
        let mk = |c: [u8; 4], w, h| Arc::new(RgbaImage::from_pixel(w, h, image::Rgba(c)));
        let imgs = vec![mk([230, 80, 40, 255], 900, 600), mk([30, 140, 60, 255], 600, 800)];
        let items = vec![
            Placed { design: 0, page: 0, x: 8.0, y: 8.0, w: 53.0, h: 36.3, iw: 54.5, ih: 36.3 },
            Placed { design: 1, page: 0, x: 64.0, y: 8.0, w: 43.0, h: 56.3, iw: 43.0, ih: 57.3 },
        ];
        let layout = Layout { items, pages_needed: 1, oversize: vec![] };
        for (shape, corners, n) in [(Shape::Rect, false, "rect"), (Shape::Ellipse, false, "ell"), (Shape::Rect, true, "coins")] {
            let s = Settings { shape, corners, ..Settings::default() };
            export_pdf(&imgs, &layout, &s, 1, &dir.join(format!("t-{n}.pdf")), None).unwrap();
            render_page(&imgs, &layout, &s, 0, 100.0).save(dir.join(format!("t-{n}.png"))).unwrap();
        }
    }
}


