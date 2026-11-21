//! The host's plates over the app frame (error overlay, dialog, toast) with the 0.1 geometry (`Emu._plates`).
use super::canvas::py_round;
use super::raster::Style;
use super::{Canvas, Frame, TextItem};
use crate::emu::Pending;

pub(crate) type Rect = (i32, i32, i32, i32);

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Plate {
    pub items: Vec<TextItem>,
    /// Outlined boxes, inclusive corners.
    pub boxes: Vec<Rect>,
    /// Dialog buttons: box, label, the answer it gives.
    pub buttons: Vec<(Rect, &'static str, bool)>,
}

/// What the plates show; `toast` only while it is visible.
pub(crate) struct Overlays<'a> {
    pub error: Option<&'a (String, String)>,
    pub pending: Option<&'a Pending>,
    pub toast: Option<&'a str>,
}

/// 0.1 `_item`: one line centred on `cx`.
fn item(c: &Canvas, text: &str, font: &'static str, cx: f64, y: i32) -> TextItem {
    let f = c.fonts();
    let w = f.measure(font, text);
    TextItem {
        text: text.to_string(),
        x: py_round(cx - f64::from(w) / 2.0),
        y,
        w,
        h: f.line_height(font),
        font,
    }
}

pub(crate) fn plates(c: &Canvas, o: &Overlays, dialog: bool) -> Vec<Plate> {
    let w = f64::from(c.w);
    let mut out = vec![];
    if let Some((title, msg)) = o.error {
        let mut items: Vec<TextItem> = vec![];
        let mut y = c.h as i32 / 2 - 60;
        let lines = c.wrap("small", msg, w - 64.0);
        let rows = [(title.as_str(), "large")]
            .into_iter()
            .chain(lines.iter().take(4).map(|l| (l.as_str(), "small")));
        for (text, font) in rows {
            let it = item(c, text, font, w / 2.0, y);
            y += it.h + if font == "large" { 8 } else { 0 };
            items.push(it);
        }
        out.push(Plate {
            items,
            ..Plate::default()
        });
    }
    if let (true, Some(p)) = (dialog, o.pending) {
        out.push(dialog_plate(c, p));
    }
    if let Some(text) = o.toast {
        let lh = c.fonts().line_height("medium");
        let mut lines = c.wrap("medium", text, w - 96.0);
        lines.truncate(3);
        let mut y = c.h as i32 - 48 - 16 - lh * lines.len() as i32;
        let mut items = vec![];
        for line in &lines {
            items.push(item(c, line, "medium", w / 2.0, y));
            y += lh;
        }
        out.push(Plate {
            items,
            ..Plate::default()
        });
    }
    out
}

pub(crate) fn dialog_plate(c: &Canvas, p: &Pending) -> Plate {
    let w = f64::from(c.w);
    let mut plate = Plate::default();
    let mut y = c.h as i32 / 2 - 100;
    if !p.title.is_empty() {
        let it = item(c, &p.title, "large", w / 2.0, y);
        y += it.h + 12;
        plate.items.push(it);
    }
    if matches!(p.kind, "text" | "edit") {
        let lh = c.fonts().line_height("medium");
        plate.boxes.push((40, y - 6, c.w as i32 - 41, y + lh + 6));
        let line = c
            .wrap("medium", &p.text, w - 100.0)
            .pop()
            .unwrap_or_default();
        if !line.is_empty() {
            plate.items.push(TextItem {
                x: 50,
                ..item(c, &line, "medium", w / 2.0, y)
            });
        }
        y += lh + 24;
    } else if !p.text.is_empty() {
        for line in c.wrap("small", &p.text, w - 64.0).iter().take(4) {
            let it = item(c, line, "small", w / 2.0, y);
            y += it.h;
            plate.items.push(it);
        }
        y += 16;
    }
    let labels = match p.kind {
        "text" | "edit" => ["OK", "Cancel"],
        _ => ["Yes", "No"],
    };
    for ((label, ok), cx) in labels
        .into_iter()
        .zip([true, false])
        .zip([w / 3.0, 2.0 * w / 3.0])
    {
        let it = item(c, label, "medium", cx, y);
        let rect = (
            py_round(cx - 70.0),
            y - 8,
            py_round(cx + 70.0),
            y + it.h + 8,
        );
        plate.items.push(it);
        plate.boxes.push(rect);
        plate.buttons.push((rect, label, ok));
    }
    plate
}

/// Draws the plates as 0.1 `Emu.shot`: a white panel with a 3 px border, 2 px button boxes, the text.
pub(crate) fn draw(c: &Canvas, frame: &mut Frame, plates: &[Plate]) {
    let fonts = c.fonts();
    let rect = |f: &mut Frame, (x0, y0, x1, y1): Rect, ink: u8, fill: bool, width: i32| {
        let (x0, y0) = (f64::from(x0), f64::from(y0));
        let (w, h) = (f64::from(x1) - x0 + 1.0, f64::from(y1) - y0 + 1.0);
        f.rect(x0, y0, w, h, 0.0, Style { ink, fill, width });
    };
    for p in plates {
        let tops = p
            .items
            .iter()
            .map(|i| i.y)
            .chain(p.boxes.iter().map(|b| b.1));
        let bottoms = p
            .items
            .iter()
            .map(|i| i.y + i.h)
            .chain(p.boxes.iter().map(|b| b.3));
        let (top, bottom) = (
            tops.min().unwrap_or(0) - 16,
            bottoms.max().unwrap_or(0) + 16,
        );
        let panel = (16, top, c.w as i32 - 17, bottom);
        rect(frame, panel, 0, true, 1);
        rect(frame, panel, 1, false, 3);
        for &b in &p.boxes {
            rect(frame, b, 1, false, 2);
        }
        for it in &p.items {
            fonts.draw(frame, it.font, it.x, it.y, &it.text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> Canvas {
        Canvas::new(480, 800)
    }

    fn pending(kind: &'static str, title: &str, text: &str) -> Pending {
        Pending {
            kind,
            title: title.into(),
            text: text.into(),
        }
    }

    fn words(p: &Plate) -> Vec<&str> {
        p.items.iter().map(|i| i.text.as_str()).collect()
    }

    #[test]
    fn confirm_plate_has_title_text_and_two_buttons() {
        let c = canvas();
        let p = dialog_plate(&c, &pending("confirm", "Delete?", "All notes go"));
        assert_eq!(words(&p), ["Delete?", "All notes go", "Yes", "No"]);
        let (title, text, yes) = (&p.items[0], &p.items[1], &p.items[2]);
        assert_eq!((title.y, title.h, title.font), (300, 30, "large"));
        assert_eq!((text.y, text.font), (342, "small"));
        // buttons sit 16 px under the text, centred on w/3 and 2w/3, 140 px wide
        assert_eq!(yes.y, 378);
        assert_eq!(p.buttons[0], ((90, 370, 230, 410), "Yes", true));
        assert_eq!(p.buttons[1], ((250, 370, 390, 410), "No", false));
        assert!((yes.x + yes.w / 2 - 160).abs() <= 1);
    }

    #[test]
    fn text_dialog_shows_the_field_box_and_ok_cancel() {
        let c = canvas();
        let p = dialog_plate(&c, &pending("text", "Name", "Ann"));
        assert_eq!(words(&p), ["Name", "Ann", "OK", "Cancel"]);
        assert_eq!(p.boxes[0], (40, 336, 439, 372));
        assert_eq!((p.items[1].x, p.items[1].y), (50, 342));
        let empty = dialog_plate(&c, &pending("edit", "Note", ""));
        assert_eq!(words(&empty), ["Note", "OK", "Cancel"]);
    }

    #[test]
    fn toast_sits_above_the_bottom_and_error_wraps_four_lines() {
        let c = canvas();
        let long = "word ".repeat(80);
        let error = ("App error, closed".to_string(), long.clone());
        let o = Overlays {
            error: Some(&error),
            pending: None,
            toast: Some("Saved"),
        };
        let ps = plates(&c, &o, true);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].items.len(), 5);
        assert_eq!(
            (ps[0].items[0].y, ps[0].items[1].y, ps[0].items[2].y),
            (340, 378, 398)
        );
        assert_eq!(words(&ps[1]), ["Saved"]);
        assert_eq!(ps[1].items[0].y, 800 - 48 - 16 - 24);
    }

    #[test]
    fn plates_skip_the_dialog_when_asked() {
        let c = canvas();
        let p = pending("leave", "Leave app?", "");
        let o = Overlays {
            error: None,
            pending: Some(&p),
            toast: None,
        };
        assert_eq!(plates(&c, &o, true).len(), 1);
        assert!(plates(&c, &o, false).is_empty());
        let mut f = Frame::new(480, 800);
        draw(&c, &mut f, &plates(&c, &o, true));
        // panel border at x 16..18, white inside
        let y = (800 / 2 - 100 - 10) as u32;
        assert_eq!([15, 16, 18, 19].map(|x| f.get(x, y)), [0, 1, 1, 0]);
    }
}
