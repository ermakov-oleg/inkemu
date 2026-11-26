//! Dialogs waiting for the user and toasts (0.1 `_set_pending`, `answer`, `type_text`, `_dialog_hit`, `_show_toast`).
use std::rc::Rc;

use crate::draw::plates;
use crate::emu::{with, Host, Pending};
use crate::input::INPUT_MS;
use crate::log::py_repr_str;

pub(crate) const TOAST_MS: i64 = 2000;

pub(crate) type AnswerFn = Rc<dyn Fn(&Host, bool)>;
pub(crate) type TypeFn = Rc<dyn Fn(&Host, &str)>;

/// What answering or typing into the pending dialog does.
#[derive(Clone)]
pub(crate) struct Handler {
    pub answer: AnswerFn,
    pub type_text: Option<TypeFn>,
    /// What a front end's text field submits: the whole text, then the confirm key.
    pub submit: Option<TypeFn>,
    /// Answering OK leaves the dialog open (an `enter = "newline"` edit inserts a line break).
    pub ok_keeps_open: bool,
    /// A docked edit's text area, `x, y, w, h`: a pointer going down in it belongs to the host (§4).
    pub rect: Option<(i64, i64, i64, i64)>,
}

/// Python `repr` of the 0.1 `pending` dict.
fn repr(p: Option<&Pending>) -> String {
    match p {
        None => "None".into(),
        Some(p) => format!(
            "{{'kind': {}, 'title': {}, 'text': {}}}",
            py_repr_str(p.kind),
            py_repr_str(&p.title),
            py_repr_str(&p.text)
        ),
    }
}

impl Host<'_> {
    /// Opens a dialog, replacing an open one; returns its id, the new `pending_id`.
    pub(crate) fn set_pending(
        &self,
        kind: &'static str,
        title: String,
        text: String,
        h: Handler,
    ) -> u64 {
        with(self.st, |s| {
            if let Some(p) = &s.pending {
                let msg = format!("a {} dialog was still open; replaced by {kind}", p.kind);
                s.warn(&msg);
            }
            s.log_line(&format!("pending {kind}: {title}"));
            s.pending = Some(Pending { kind, title, text });
            s.pending_h = Some(h);
            s.pending_id += 1;
            s.pending_id
        })
    }

    pub(crate) fn clear_pending(&self) {
        with(self.st, |s| s.clear_pending());
    }

    /// True while the dialog opened as `id` is still the open one.
    pub(crate) fn is_pending(&self, id: u64) -> bool {
        with(self.st, |s| s.pending_h.is_some() && s.pending_id == id)
    }

    pub(crate) fn show_toast(&self, text: &str) {
        with(self.st, |s| {
            s.log.toasts.push(text.to_string());
            s.toast = Some((text.to_string(), s.sched.clock.ms + TOAST_MS));
            s.log_line(&format!("toast: {text}"));
        });
    }

    /// The kind of the open dialog, if any.
    pub(crate) fn pending_kind(&self) -> Option<&'static str> {
        with(self.st, |s| s.pending.as_ref().map(|p| p.kind))
    }

    pub fn answer(&self, ok: bool) -> Result<(), String> {
        self.answer_now(ok)?;
        self.wait(INPUT_MS)
    }

    /// [`Host::answer`] without the time an input takes.
    pub(crate) fn answer_now(&self, ok: bool) -> Result<(), String> {
        let h = with(self.st, |s| {
            s.pending.as_ref()?;
            let keep = ok && s.pending_h.as_ref().is_some_and(|h| h.ok_keeps_open);
            if keep {
                return s.pending_h.clone();
            }
            let h = s.pending_h.take();
            s.pending = None;
            h
        });
        let Some(h) = h else {
            return Err("no dialog is waiting for an answer".into());
        };
        (h.answer)(self, ok);
        self.settle();
        Ok(())
    }

    pub fn type_text(&self, text: &str) -> Result<(), String> {
        self.text_input(text, |h| h.type_text)
    }

    /// Like [`Host::type_text`], but an edit's text is replaced by `text` instead of appended to.
    pub fn submit_text(&self, text: &str) -> Result<(), String> {
        self.text_input(text, |h| h.submit)
    }

    fn text_input(&self, text: &str, pick: fn(Handler) -> Option<TypeFn>) -> Result<(), String> {
        let (p, h) = with(self.st, |s| (s.pending.clone(), s.pending_h.clone()));
        let typing = h.and_then(pick).filter(|_| p.is_some());
        let Some(typing) = typing else {
            return Err(format!(
                "no text input is waiting; pending: {}",
                repr(p.as_ref())
            ));
        };
        typing(self, text);
        self.settle();
        self.wait(INPUT_MS)
    }

    /// (label, answer) of the dialog button at x, y.
    pub(crate) fn dialog_hit(&self, x: i32, y: i32) -> Option<(&'static str, bool)> {
        with(self.st, |s| {
            let p = s.pending.as_ref()?;
            plates::dialog_plate(&s.canvas, p)
                .buttons
                .into_iter()
                .find(|((x0, y0, x1, y1), _, _)| {
                    (*x0..=*x1).contains(&x) && (*y0..=*y1).contains(&y)
                })
                .map(|(_, label, ok)| (label, ok))
        })
    }
}
