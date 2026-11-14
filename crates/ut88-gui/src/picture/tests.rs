//! Тесты рисунка компьютера: размеры, зоны нажатия, нажатые клавиши, индикаторы, экран,
//! кэш-слой и совпадение зон с прежним `draw.rs` (egui).

use std::collections::HashSet;

use super::*;

/// Цвет экрана в тестах.
const FG: [u8; 4] = [0x66, 0xFF, 0x88, 0xFF];
/// Цвет люминофора.
const GLOW: Color = Color::from_rgb(0x66, 0xFF, 0x88);

/// Держит нажатыми перечисленные клавиши.
struct Down(Vec<Hit>);

impl Pressed for Down {
    fn is_down(&self, h: Hit) -> bool {
        self.0.contains(&h)
    }
}

/// Нарисовать весь рисунок на холсте `w × h` физических пикселей с масштабом `ppp`.
#[allow(clippy::too_many_arguments)]
fn draw(
    w: u32,
    h: u32,
    ppp: f32,
    full: bool,
    segs: [u8; 6],
    screen: Option<Screen<'_>>,
    pressed: &dyn Pressed,
) -> (Canvas, Hits) {
    let mut c = Canvas::new(w, h);
    c.set_scale(ppp);
    c.clear(Color::BLACK);
    let mut hits = Hits::default();
    let rect = c.rect_points();
    computer(&mut c, rect, full, segs, screen, GLOW, pressed, &mut hits);
    assert_eq!(c.clip_depth(), 0, "push_clip/pop_clip не парны");
    (c, hits)
}

/// Сплошная картинка экрана одного цвета.
fn solid_screen(color: [u8; 4]) -> Vec<u8> {
    color.repeat(384 * 280)
}

/// Пиксели холстов различаются внутри `r` (логические точки).
fn differs_in(a: &Canvas, b: &Canvas, r: Rect) -> bool {
    let s = a.scale();
    let (x0, y0) = ((r.min.x * s) as u32, (r.min.y * s) as u32);
    let (x1, y1) = (
        ((r.max.x * s) as u32).min(a.width()),
        ((r.max.y * s) as u32).min(a.height()),
    );
    (y0..y1).any(|y| (x0..x1).any(|x| a.pixel(x, y) != b.pixel(x, y)))
}

/// Пиксели холстов различаются вне `r` (логические точки).
fn differs_outside(a: &Canvas, b: &Canvas, r: Rect) -> bool {
    let s = a.scale();
    (0..a.height()).any(|y| {
        (0..a.width()).any(|x| {
            let p = pos2((x as f32 + 0.5) / s, (y as f32 + 0.5) / s);
            !r.contains(p) && a.pixel(x, y) != b.pixel(x, y)
        })
    })
}

#[test]
fn draws_at_any_size() {
    let screen = solid_screen(FG);
    for full in [false, true] {
        for (w, h) in [
            (1, 1),
            (3, 2),
            (17, 9),
            (120, 60),
            (480, 400),
            (720, 680),
            (1280, 900),
        ] {
            for ppp in [1.0, 1.5, 2.0] {
                let (_, hits) = draw(
                    w,
                    h,
                    ppp,
                    full,
                    [0x7F; 6],
                    Some((&screen, 384, 280)),
                    &NonePressed,
                );
                assert_eq!(hits.zones.len(), if full { 18 + 59 } else { 18 });
            }
        }
    }
}

/// Все кнопки на месте, зоны внутри холста и не пересекаются.
fn check_hits(hits: &Hits, rect: Rect, full: bool) {
    let set: HashSet<Hit> = hits.zones.iter().map(|z| z.1).collect();
    assert_eq!(set.len(), hits.zones.len(), "клавиша нарисована дважды");
    for d in 0..16 {
        assert!(set.contains(&Hit::Pad(PadKey::Hex(d))), "нет кнопки {d:X}");
    }
    assert!(set.contains(&Hit::Pad(PadKey::StepBack)));
    assert!(set.contains(&Hit::Pad(PadKey::Reset)));
    if full {
        // Все 56 клавиш матрицы, у которых есть код, плюс РУС, УС, СС.
        for n in 0..56u8 {
            let k = KbKey::Matrix(n);
            if k.base_code().is_some() {
                assert!(set.contains(&Hit::Kb(k)), "нет клавиши {:?}", k);
            }
        }
        for k in [KbKey::Rus, KbKey::Us, KbKey::Ss] {
            assert!(set.contains(&Hit::Kb(k)), "нет клавиши {k:?}");
        }
        assert_eq!(hits.zones.len(), 18 + 59);
    } else {
        assert_eq!(hits.zones.len(), 18);
    }
    for (i, (a, ha)) in hits.zones.iter().enumerate() {
        assert!(a.is_positive(), "{ha:?}: пустая зона");
        assert!(rect.contains_rect(*a), "{ha:?}: {a:?} вне {rect:?}");
        for (b, hb) in &hits.zones[i + 1..] {
            assert!(!a.intersects(*b), "{ha:?} пересекается с {hb:?}");
        }
    }
}

#[test]
fn hits_complete_inside_and_disjoint() {
    for full in [false, true] {
        for (w, h, ppp) in [
            (720, 680, 1.0),
            (1280, 900, 1.0),
            (2560, 1800, 2.0),
            (500, 1200, 1.0),
        ] {
            let (c, hits) = draw(w, h, ppp, full, [0; 6], None, &NonePressed);
            check_hits(&hits, c.rect_points(), full);
        }
    }
}

#[test]
fn pressed_key_looks_different() {
    for full in [false, true] {
        let (up, hits) = draw(1280, 900, 1.0, full, [0; 6], None, &NonePressed);
        let mut keys = vec![
            Hit::Pad(PadKey::Hex(5)),
            Hit::Pad(PadKey::StepBack),
            Hit::Pad(PadKey::Reset),
        ];
        if full {
            keys.push(Hit::Kb(KbKey::Rus));
            keys.push(Hit::Kb(KbKey::for_code(b' ').unwrap().0));
            keys.push(Hit::Kb(KbKey::for_code(0x18).unwrap().0));
        }
        for k in keys {
            let (down, hits2) = draw(1280, 900, 1.0, full, [0; 6], None, &Down(vec![k]));
            assert_eq!(hits.zones, hits2.zones, "нажатие не двигает зоны");
            let r = hits.zones.iter().find(|z| z.1 == k).unwrap().0;
            assert!(differs_in(&up, &down, r), "{k:?}: нажатая не отличается");
            // Тень клавиши выходит чуть ниже зоны.
            let around = r.expand2(vec2(1.0, r.height() * 0.1));
            assert!(
                !differs_outside(&up, &down, around),
                "{k:?}: нажатие меняет пиксели вне клавиши"
            );
        }
    }
}

#[test]
fn lit_indicators_differ_from_dark() {
    for full in [false, true] {
        let (dark, _) = draw(1280, 900, 1.0, full, [0; 6], None, &NonePressed);
        let layout = Layout::new(dark.rect_points(), full);
        let area = indicators_area(layout.module);
        for i in 0..6 {
            let mut segs = [0; 6];
            segs[i] = 0x7F;
            let (lit, _) = draw(1280, 900, 1.0, full, segs, None, &NonePressed);
            assert!(differs_in(&dark, &lit, area), "разряд {i} не горит");
            assert!(!differs_outside(&dark, &lit, area), "свечение вне окна");
        }
        // Горящий сегмент — цвета SEG_ON: середина сегмента g первого разряда.
        let (lit, _) = draw(1280, 900, 1.0, full, [0x40; 6], None, &NonePressed);
        let x = layout.module;
        let dx = (MODULE_W - DIG_TOTAL) / 2.0;
        let p = x.p(dx + DIG_W / 2.0, DIG_Y + DIG_H / 2.0);
        assert_eq!(lit.pixel(p.x as u32, p.y as u32), pal::SEG_ON);
        assert_eq!(dark.pixel(p.x as u32, p.y as u32), pal::SEG_OFF);
    }
}

#[test]
fn screen_is_shown() {
    let red = [0xE0, 0x20, 0x30, 0xFF];
    let screen = solid_screen(red);
    for ppp in [1.0, 2.0] {
        let (w, h) = ((1280.0 * ppp) as u32, (900.0 * ppp) as u32);
        let (c, _) = draw(
            w,
            h,
            ppp,
            true,
            [0; 6],
            Some((&screen, 384, 280)),
            &NonePressed,
        );
        let (empty, _) = draw(w, h, ppp, true, [0; 6], None, &NonePressed);
        let layout = Layout::new(c.rect_points(), true);
        let img = tv_image_rect(tv_glass(layout.tv.unwrap()));
        let mid = img.center();
        let px = c.pixel((mid.x * ppp) as u32, (mid.y * ppp) as u32);
        assert_eq!(
            px,
            Color::from_rgb(red[0], red[1], red[2]),
            "середина экрана"
        );
        // Красного больше половины площади картинки (свечение и блик — по краю и сверху).
        let s = ppp;
        let (mut n, mut red_n) = (0, 0);
        for y in (img.min.y * s) as u32..(img.max.y * s) as u32 {
            for x in (img.min.x * s) as u32..(img.max.x * s) as u32 {
                n += 1;
                let p = c.pixel(x, y);
                if p.r() > 0xC0 && p.g() < 0x50 {
                    red_n += 1;
                }
            }
        }
        assert!(red_n * 10 > n * 9, "красных {red_n} из {n}");
        assert!(!differs_outside(&c, &empty, img.expand(1.0)), "экран вылез");
    }
}

/// Кэш-слой статической части + живая часть = весь рисунок, пиксель в пиксель.
#[test]
fn cached_layer_matches_direct() {
    let screen: Vec<u8> = (0..384 * 280)
        .flat_map(|i| {
            if (i / 3 + i / 384) % 2 == 0 {
                FG
            } else {
                [6, 20, 8, 255]
            }
        })
        .collect();
    for full in [false, true] {
        for (w, h, ppp) in [(1280, 900, 1.0), (1000, 777, 1.25), (1440, 1360, 2.0)] {
            let segs = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D];
            let pressed = Down(vec![Hit::Pad(PadKey::Hex(7))]);
            let scr = Some((&screen[..], 384, 280));
            let (direct, hits) = draw(w, h, ppp, full, segs, scr, &pressed);

            let mut layer = Canvas::new(w, h);
            layer.set_scale(ppp);
            let layout = Layout::new(layer.rect_points(), full);
            let mut hits2 = Hits::default();
            computer_static(&mut layer, &layout, GLOW, &pressed, &mut hits2);
            assert_eq!(hits.zones, hits2.zones);

            let mut frame = Canvas::new(w, h);
            frame.set_scale(ppp);
            frame.clear(Color::BLACK);
            frame.draw_canvas(&layer, Pos2::ZERO);
            computer_live(&mut frame, &layout, segs, scr, GLOW);
            for y in 0..h {
                for x in 0..w {
                    assert_eq!(
                        direct.pixel(x, y),
                        frame.pixel(x, y),
                        "full={full} {w}x{h}@{ppp}: пиксель ({x}, {y})"
                    );
                }
            }
        }
    }
}

/// Живая часть не меняет пиксели вне окна индикаторов и экрана.
#[test]
fn live_part_stays_in_place() {
    let screen = solid_screen(FG);
    for full in [false, true] {
        let mut c = Canvas::new(1280, 900);
        let layout = Layout::new(c.rect_points(), full);
        computer_static(&mut c, &layout, GLOW, &NonePressed, &mut Hits::default());
        let before = c.pixmap().clone();
        computer_live(&mut c, &layout, [0xFF; 6], Some((&screen, 384, 280)), GLOW);
        let mut allowed = vec![indicators_area(layout.module)];
        if let Some(t) = layout.tv {
            allowed.push(tv_image_rect(tv_glass(t)));
        }
        for y in 0..900 {
            for x in 0..1280 {
                let p = pos2(x as f32 + 0.5, y as f32 + 0.5);
                if allowed.iter().any(|r| r.contains(p)) {
                    continue;
                }
                assert_eq!(
                    before.pixel(x, y),
                    c.pixmap().pixel(x, y),
                    "full={full}: пиксель ({x}, {y})"
                );
            }
        }
    }
}

/// Зоны нажатия совпадают с прежним рисунком на egui (`draw.rs`) при том же
/// прямоугольнике.
#[test]
fn hits_match_old_egui_drawing() {
    use eframe::egui;

    struct OldNone;
    impl crate::draw::Pressed for OldNone {
        fn is_down(&self, _h: crate::draw::Hit) -> bool {
            false
        }
    }
    let conv = |h: crate::draw::Hit| match h {
        crate::draw::Hit::Pad(p) => Hit::Pad(p),
        crate::draw::Hit::Kb(k) => Hit::Kb(k),
    };
    for full in [false, true] {
        for (x0, y0, w, h) in [
            (0.0, 24.0, 1280.0, 876.0),
            (0.0, 24.0, 720.0, 656.0),
            (3.5, 30.0, 911.0, 1333.0),
        ] {
            let rect = Rect::from_min_size(pos2(x0, y0), vec2(w, h));
            // Старый рисунок: прямоугольник и Xf те же, что в App::computer.
            let erect = egui::Rect::from_min_size(egui::pos2(x0, y0), egui::vec2(w, h));
            let ctx = egui::Context::default();
            let mut old = crate::draw::Hits::default();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                let p = egui::Painter::new(ctx.clone(), egui::LayerId::background(), erect);
                use crate::draw as d;
                if full {
                    let x = d::Xf::fit(erect.shrink(8.0), d::KB_W + 60.0, d::TV_H + d::KB_H + 50.0);
                    d::tv(&p, x.sub(20.0, 10.0, 1.0), None, egui::Color32::GREEN);
                    d::cpu_module(
                        &p,
                        x.sub(d::TV_W + 50.0, 40.0, 0.86),
                        [0; 6],
                        &OldNone,
                        &mut old,
                    );
                    d::keyboard(&p, x.sub(30.0, d::TV_H + 40.0, 1.0), &OldNone, &mut old);
                } else {
                    let x = d::Xf::fit(erect.shrink(12.0), d::MODULE_W + 20.0, d::MODULE_H + 20.0);
                    d::cpu_module(&p, x.sub(10.0, 6.0, 1.0), [0; 6], &OldNone, &mut old);
                }
            });
            let mut c = Canvas::new(w as u32 + 8, (y0 + h) as u32 + 2);
            let mut hits = Hits::default();
            computer(
                &mut c,
                rect,
                full,
                [0; 6],
                None,
                GLOW,
                &NonePressed,
                &mut hits,
            );
            assert_eq!(old.zones.len(), hits.zones.len());
            for ((er, eh), (r, h)) in old.zones.iter().zip(&hits.zones) {
                assert_eq!(conv(*eh), *h);
                let d = (er.min.x - r.min.x).abs()
                    + (er.min.y - r.min.y).abs()
                    + (er.max.x - r.max.x).abs()
                    + (er.max.y - r.max.y).abs();
                assert!(d < 1e-3, "{h:?}: {er:?} ≠ {r:?}");
            }
        }
    }
}

#[test]
fn hits_at_finds_key() {
    let (_, hits) = draw(720, 680, 1.0, false, [0; 6], None, &NonePressed);
    for (r, h) in &hits.zones {
        assert_eq!(hits.at(r.center()), Some(*h));
    }
    assert_eq!(hits.at(pos2(1.0, 1.0)), None);
}

/// Перерисовка одной клавиши поверх слоя = перерисовка всего слоя.
#[test]
fn redraw_key_matches_full_redraw() {
    let mut keys = vec![
        Hit::Pad(PadKey::Hex(0xC)),
        Hit::Pad(PadKey::Hex(3)),
        Hit::Pad(PadKey::StepBack),
        Hit::Pad(PadKey::Reset),
    ];
    let full_keys = [
        Hit::Kb(KbKey::Rus),
        Hit::Kb(KbKey::Ss),
        Hit::Kb(KbKey::for_code(b' ').unwrap().0),
        Hit::Kb(KbKey::for_code(b'J').unwrap().0),
        Hit::Kb(KbKey::for_code(0x0D).unwrap().0),
        Hit::Kb(KbKey::for_code(0x0C).unwrap().0),
    ];
    let static_layer = |w: u32, h: u32, ppp: f32, full: bool, p: &dyn Pressed| {
        let mut c = Canvas::new(w, h);
        c.set_scale(ppp);
        let layout = Layout::new(c.rect_points(), full);
        let mut hits = Hits::default();
        computer_static(&mut c, &layout, GLOW, p, &mut hits);
        (c, layout, hits)
    };
    for full in [false, true] {
        if full {
            keys.extend(full_keys);
        }
        for (w, h, ppp) in [(1280, 900, 1.0), (1000, 777, 1.25), (1440, 1360, 2.0)] {
            let (mut layer, layout, hits) = static_layer(w, h, ppp, full, &NonePressed);
            let orig = layer.pixmap().clone();
            for &k in &keys {
                let (want, _, _) = static_layer(w, h, ppp, full, &Down(vec![k]));
                let area = redraw_key(&mut layer, &layout, &hits, k, true)
                    .unwrap_or_else(|| panic!("full={full} {w}x{h}@{ppp}: {k:?}"));
                assert!(
                    hits.zones
                        .iter()
                        .any(|z| z.1 == k && area.contains_rect(z.0))
                );
                assert!(
                    layer.pixmap().data() == want.pixmap().data(),
                    "full={full} {w}x{h}@{ppp}: {k:?} нажата"
                );
                redraw_key(&mut layer, &layout, &hits, k, false).unwrap();
                assert!(
                    layer.pixmap().data() == orig.data(),
                    "full={full} {w}x{h}@{ppp}: {k:?} отпущена"
                );
            }
        }
    }
    // Крошечный рисунок: клавиши почти касаются — перерисовка одной не делается.
    let (mut layer, layout, hits) = static_layer(60, 50, 1.0, false, &NonePressed);
    assert_eq!(
        redraw_key(&mut layer, &layout, &hits, Hit::Pad(PadKey::Hex(5)), true),
        None
    );
}
