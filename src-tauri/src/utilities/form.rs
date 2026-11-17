//! A PDF form filled in: the values written into its fields, or burnt into
//! its pages and the form taken away (flattened).
//!
//! The screen finds the fields with pdf.js, which names each by its widget —
//! the box on the page — as `"12R"` (object 12) or `"12R3"` (generation 3).
//! A value belongs to the field the widget is part of: the widget itself when
//! it has a name (`/T`), otherwise the parent that does.
//!
//! # Filled, the form kept
//!
//! - **Text and lists:** `/V` is the text. The old appearance is taken off and
//!   `/NeedAppearances` asks the reader to draw the new value, in its own font:
//!   the standard PDF fonts have no Vietnamese, so drawing it here could not
//!   get "Nguyễn" right, and every reader that fills forms honours the flag.
//! - **Checkboxes and radio buttons:** `/V` is the "on" state the widget's
//!   appearance is named by, and each widget shows on or `/Off` (`/AS`).
//!
//! # Flattened
//!
//! The screen draws each value as a picture and `stamp::place` puts it where
//! its box is; then every widget comes off its page and the form off the
//! catalog. What is left cannot be changed by filling it in again — which is
//! the point of flattening.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

use super::pdf::PdfError;

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Text,
    Check,
    Radio,
    Choice,
}

/// One widget's value, as the screen has it.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filled {
    /// pdf.js's id for the widget: `"12R"`, or `"12R3"` for generation 3.
    pub widget: String,
    pub kind: FieldKind,
    /// Text and lists: the text. Checkboxes and radio buttons: `"on"` or `""`.
    pub value: String,
}

/// `"12R"` → (12, 0); `"12R3"` → (12, 3).
pub fn widget_id(id: &str) -> Option<ObjectId> {
    let (number, generation) = id.split_once('R')?;
    let number = number.parse().ok()?;
    let generation = if generation.is_empty() { 0 } else { generation.parse().ok()? };
    Some((number, generation))
}

/// The field a widget's value lives on: the widget if it is named, else the
/// nearest named parent.
fn field_of(doc: &Document, widget: ObjectId) -> ObjectId {
    let mut id = widget;
    for _ in 0..32 {
        let Ok(dict) = doc.get_dictionary(id) else { break };
        if dict.has(b"T") {
            return id;
        }
        match dict.get(b"Parent").and_then(Object::as_reference) {
            Ok(parent) => id = parent,
            Err(_) => break,
        }
    }
    widget
}

/// The name of a widget's "on" appearance: whatever `/AP /N` has besides `/Off`.
fn on_state(doc: &Document, widget: ObjectId) -> Option<Vec<u8>> {
    let dict = doc.get_dictionary(widget).ok()?;
    let ap = match dict.get(b"AP").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    let normal = match ap.get(b"N").ok()? {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };
    normal.iter().map(|(k, _)| k.clone()).find(|k| k.as_slice() != b"Off")
}

fn set(doc: &mut Document, id: ObjectId, key: &str, value: Object) -> Result<(), PdfError> {
    doc.get_dictionary_mut(id).map_err(|e| PdfError::Unreadable(format!("a form field: {e}")))?.set(key, value);
    Ok(())
}

/// Write the values into the form, and ask readers to draw them.
pub fn fill(doc: &mut Document, values: &[Filled]) -> Result<(), PdfError> {
    let mut touched = false;
    // The radio groups that have a button on; the others are set to off.
    let groups_on: HashSet<ObjectId> = values
        .iter()
        .filter(|v| v.kind == FieldKind::Radio && !v.value.is_empty())
        .filter_map(|v| widget_id(&v.widget))
        .map(|w| field_of(doc, w))
        .collect();
    for filled in values {
        let Some(widget) = widget_id(&filled.widget).filter(|id| doc.get_dictionary(*id).is_ok()) else { continue };
        let field = field_of(doc, widget);
        match filled.kind {
            FieldKind::Text | FieldKind::Choice => {
                set(doc, field, "V", super::catalog::text_string(&filled.value))?;
                // The old picture of the old value would hide the new one.
                if let Ok(dict) = doc.get_dictionary_mut(widget) {
                    dict.remove(b"AP");
                }
            }
            FieldKind::Check => {
                let on = on_state(doc, widget).unwrap_or_else(|| b"Yes".to_vec());
                let state = if filled.value.is_empty() { b"Off".to_vec() } else { on };
                set(doc, field, "V", Object::Name(state.clone()))?;
                set(doc, widget, "AS", Object::Name(state))?;
            }
            FieldKind::Radio => {
                let state = match on_state(doc, widget) {
                    Some(on) if !filled.value.is_empty() => on,
                    _ => b"Off".to_vec(),
                };
                if state.as_slice() != b"Off" {
                    set(doc, field, "V", Object::Name(state.clone()))?;
                } else if !groups_on.contains(&field) {
                    set(doc, field, "V", Object::Name(b"Off".to_vec()))?;
                }
                set(doc, widget, "AS", Object::Name(state))?;
            }
        }
        touched = true;
    }
    if touched {
        if let Some(form) = acro_form(doc) {
            set(doc, form, "NeedAppearances", Object::Boolean(true))?;
        } else if let Some(Object::Dictionary(form)) = catalog_mut(doc).and_then(|c| c.get_mut(b"AcroForm").ok()) {
            form.set("NeedAppearances", true);
        }
    }
    Ok(())
}

fn catalog_mut(doc: &mut Document) -> Option<&mut lopdf::Dictionary> {
    let id = doc.trailer.get(b"Root").and_then(Object::as_reference).ok()?;
    doc.get_dictionary_mut(id).ok()
}

/// The form dictionary, when it is an object of its own.
fn acro_form(doc: &Document) -> Option<ObjectId> {
    doc.catalog().ok()?.get(b"AcroForm").and_then(Object::as_reference).ok()
}

/// Take every widget off its page and the form off the catalog: what is left
/// is the page as drawn (the values having been placed on it first).
pub fn strip(doc: &mut Document) -> Result<(), PdfError> {
    let pages: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page in pages {
        let annots = match doc.get_dictionary(page).ok().and_then(|d| d.get(b"Annots").ok()).cloned() {
            Some(Object::Reference(id)) => doc.get_object(id).ok().and_then(|o| o.as_array().ok()).cloned(),
            Some(Object::Array(items)) => Some(items),
            _ => None,
        };
        let Some(annots) = annots else { continue };
        let kept: Vec<Object> = annots
            .into_iter()
            .filter(|a| {
                let dict = match a {
                    Object::Reference(id) => doc.get_dictionary(*id).ok(),
                    Object::Dictionary(d) => Some(d),
                    _ => None,
                };
                dict.is_none_or(|d| d.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Widget"))
            })
            .collect();
        let dict = doc.get_dictionary_mut(page).map_err(|e| PdfError::Unreadable(e.to_string()))?;
        if kept.is_empty() {
            dict.remove(b"Annots");
        } else {
            dict.set("Annots", kept);
        }
    }
    if let Some(catalog) = catalog_mut(doc) {
        catalog.remove(b"AcroForm");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utilities::pdf::tests::{round_trip, sample};
    use lopdf::{dictionary, Dictionary, Stream};

    /// A page with a text field, a checkbox and a two-button radio group.
    fn form() -> (Document, [ObjectId; 4]) {
        let mut doc = sample(1, "p");
        let page = doc.get_pages()[&1];
        let look = |doc: &mut Document| doc.add_object(Stream::new(Dictionary::new(), b"0 g".to_vec()));
        let (yes, off, old_look) = (look(&mut doc), look(&mut doc), look(&mut doc));
        let text = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal("name"),
            "Rect" => vec![72.into(), 700.into(), 300.into(), 720.into()], "P" => page,
            "AP" => dictionary! { "N" => old_look },
        });
        let check = doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Btn", "T" => Object::string_literal("agree"),
            "Rect" => vec![72.into(), 650.into(), 84.into(), 662.into()], "P" => page, "AS" => "Off",
            "AP" => dictionary! { "N" => dictionary! { "Yes" => yes, "Off" => off } },
        });
        let group = doc.new_object_id();
        let radio = |doc: &mut Document, on: &str| doc.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "Parent" => group, "AS" => "Off",
            "Rect" => vec![72.into(), 600.into(), 84.into(), 612.into()], "P" => page,
            "AP" => dictionary! { "N" => dictionary! { on => yes, "Off" => off } },
        });
        let (male, female) = (radio(&mut doc, "Nam"), radio(&mut doc, "Nu"));
        doc.objects.insert(group, Object::Dictionary(dictionary! {
            "FT" => "Btn", "Ff" => 49152, "T" => Object::string_literal("sex"), "Kids" => vec![male.into(), female.into()],
        }));
        doc.get_dictionary_mut(page).unwrap().set("Annots", vec![text.into(), check.into(), male.into(), female.into()]);
        let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        doc.get_dictionary_mut(root).unwrap().set("AcroForm", dictionary! { "Fields" => vec![text.into(), check.into(), group.into()] });
        (doc, [text, check, male, female])
    }

    fn id(o: ObjectId) -> String {
        format!("{}R", o.0)
    }

    fn get<'a>(doc: &'a Document, o: ObjectId, key: &[u8]) -> &'a Object {
        doc.get_dictionary(o).unwrap().get(key).unwrap()
    }

    #[test]
    fn pdf_js_ids_name_objects() {
        assert_eq!(widget_id("12R"), Some((12, 0)));
        assert_eq!(widget_id("12R3"), Some((12, 3)));
        assert_eq!(widget_id("R"), None);
        assert_eq!(widget_id("x"), None);
    }

    #[test]
    fn values_go_into_their_fields() {
        let (mut doc, [text, check, male, female]) = form();
        let values = [
            Filled { widget: id(text), kind: FieldKind::Text, value: "Nguyễn Văn A".into() },
            Filled { widget: id(check), kind: FieldKind::Check, value: "on".into() },
            Filled { widget: id(male), kind: FieldKind::Radio, value: "".into() },
            Filled { widget: id(female), kind: FieldKind::Radio, value: "on".into() },
        ];
        fill(&mut doc, &values).unwrap();
        let doc = round_trip(doc);
        // Vietnamese as UTF-16, and the old appearance gone so the reader draws it.
        assert_eq!(&get(&doc, text, b"V").as_str().unwrap()[..2], [0xFE, 0xFF]);
        assert!(doc.get_dictionary(text).unwrap().get(b"AP").is_err());
        assert_eq!(get(&doc, check, b"V").as_name().unwrap(), b"Yes");
        assert_eq!(get(&doc, check, b"AS").as_name().unwrap(), b"Yes");
        let group = get(&doc, male, b"Parent").as_reference().unwrap();
        assert_eq!(get(&doc, group, b"V").as_name().unwrap(), b"Nu");
        assert_eq!(get(&doc, male, b"AS").as_name().unwrap(), b"Off");
        assert_eq!(get(&doc, female, b"AS").as_name().unwrap(), b"Nu");
        let form = doc.catalog().unwrap().get(b"AcroForm").unwrap().as_dict().unwrap();
        assert!(form.get(b"NeedAppearances").unwrap().as_bool().unwrap());
    }

    #[test]
    fn a_ticked_box_can_be_unticked() {
        let (mut doc, [_, check, ..]) = form();
        fill(&mut doc, &[Filled { widget: id(check), kind: FieldKind::Check, value: "on".into() }]).unwrap();
        fill(&mut doc, &[Filled { widget: id(check), kind: FieldKind::Check, value: "".into() }]).unwrap();
        assert_eq!(get(&doc, check, b"AS").as_name().unwrap(), b"Off");
    }

    #[test]
    fn flattening_takes_the_form_away() {
        let (mut doc, _) = form();
        strip(&mut doc).unwrap();
        let doc = round_trip(doc);
        let page = doc.get_pages()[&1];
        assert!(doc.get_dictionary(page).unwrap().get(b"Annots").is_err());
        assert!(doc.catalog().unwrap().get(b"AcroForm").is_err());
    }

    /// A real form: `UTILITIES_FORM` filled with `UTILITIES_FORM_VALUES` (JSON,
    /// as the screen sends it), written to `UTILITIES_FORM_OUT`.
    #[test]
    #[ignore]
    fn real_form() {
        let values: Vec<Filled> = serde_json::from_str(&std::env::var("UTILITIES_FORM_VALUES").unwrap()).unwrap();
        let mut doc = crate::utilities::pdf::open(std::path::Path::new(&std::env::var("UTILITIES_FORM").unwrap())).unwrap();
        fill(&mut doc, &values).unwrap();
        crate::utilities::pdf::save(&mut doc, std::path::Path::new(&std::env::var("UTILITIES_FORM_OUT").unwrap())).unwrap();
    }
}
