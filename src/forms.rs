//! Dissociate one ordinary Form occurrence without changing shared artwork.
use crate::document::{Editor, Result};
use pdfcraft_content::{Matrix, Op, Pieces};
use pdfcraft_cos::{Dict, Document, Object, Stream};
use std::collections::BTreeMap;

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_OPS: usize = 100_000;
const UNSUPPORTED: &str =
    "Ce groupe utilise une structure qui doit rester groupée pour préserver son rendu.";

fn ordinary(s: &Stream) -> bool {
    s.dict.name(b"Subtype") == Some(b"Form")
        && s.dict
            .get(b"FormType")
            .is_none_or(|v| v.as_int() == Some(1))
        && [
            b"Group".as_slice(),
            b"OC",
            b"Ref",
            b"StructParent",
            b"StructParents",
            b"F",
            b"OPI",
            b"PS",
            b"PCAdded",
            b"PNDrawing",
            b"PNText",
        ]
        .iter()
        .all(|k| s.dict.get(k).is_none())
}

fn editable_placement(m: [f64; 6]) -> bool {
    (m[1] == 0. && m[2] == 0.) || (m[0] == 0. && m[3] == 0.)
}

fn supported_resources(doc: &Document, resources: &Dict) -> bool {
    resources
        .iter()
        .all(|(key, _)| matches!(key.as_slice(), b"Font" | b"XObject" | b"ProcSet"))
        && resources
            .get(b"Font")
            .and_then(|r| doc.dict(r))
            .is_none_or(|fonts| {
                fonts.iter().all(|(_, f)| {
                    doc.dict(f)
                        .is_some_and(|d| d.name(b"Subtype") != Some(b"Type3"))
                })
            })
        && resources
            .get(b"XObject")
            .and_then(|r| doc.dict(r))
            .is_none_or(|objects| {
                objects.iter().all(|(_, o)| {
                    matches!(doc.resolve(o).as_ref(), Object::Stream(s)
                if matches!(s.dict.name(b"Subtype"), Some(b"Form" | b"Image")))
                })
            })
}

fn decode(s: &Stream, remaining: usize) -> Result<Vec<u8>> {
    pdfcraft_filters::decode(&s.filters(), &s.raw, remaining)
        .map_err(|_| "Le flux du groupe est endommagé ou trop volumineux.".into())
}

fn contents(doc: &Document, o: &Object, depth: usize, out: &mut Vec<Object>) -> Result<()> {
    if depth > 16 || out.len() >= 4096 {
        return Err("Cette page contient trop de flux imbriqués.".into());
    }
    match doc.resolve(o).as_ref() {
        Object::Stream(_) => out.push(o.clone()),
        Object::Array(a) => {
            for item in a {
                contents(doc, item, depth + 1, out)?;
            }
        }
        _ => return Err("Flux de page invalide.".into()),
    }
    Ok(())
}

type Names = BTreeMap<(Vec<u8>, Vec<u8>), Vec<u8>>;

fn import_resources(doc: &mut Document, page: &mut Dict, form: &Dict) -> Result<Names> {
    let mut names = Names::new();
    let mut count = 0;
    if !supported_resources(doc, form) {
        return Err(UNSUPPORTED.into());
    }
    for kind in [b"Font".as_slice(), b"XObject"] {
        let Some(source) = form.get(kind).and_then(|r| doc.dict(r)) else {
            continue;
        };
        let mut target = page.get(kind).and_then(|r| doc.dict(r)).unwrap_or_default();
        if target.iter().count() > 4096 {
            return Err(UNSUPPORTED.into());
        }
        for (old, object) in source.iter() {
            count += 1;
            if count > 4096 {
                return Err("Ce groupe contient trop de ressources.".into());
            }
            let mut n = count;
            let new = loop {
                let candidate = format!("PNGroup{n}").into_bytes();
                if target.get(&candidate).is_none() {
                    break candidate;
                }
                n += 1;
            };
            let mut imported = object.clone();
            if kind == b"XObject"
                && let Object::Stream(child) = doc.resolve(object).as_ref()
                && child.dict.name(b"Subtype") == Some(b"Form")
                && child.dict.get(b"Resources").is_none()
            {
                // A nested Form may inherit the original enclosing resources.
                // Give only this imported occurrence its own equivalent context.
                let mut child = child.clone();
                child
                    .dict
                    .set(b"Resources".to_vec(), Object::Dict(form.clone()));
                imported = Object::Ref(doc.add(child));
            }
            target.set(new.clone(), imported);
            names.insert((kind.to_vec(), old.to_vec()), new);
        }
        page.set(kind.to_vec(), Object::Dict(target));
    }
    Ok(names)
}

fn rename(ops: &mut [Op], names: &Names) -> Result<()> {
    let mut graphics = 0usize;
    let mut marked = 0usize;
    let mut text = false;
    for op in ops {
        if op
            .operands
            .iter()
            .any(|o| o.as_dict().is_some_and(|d| d.get(b"PNText").is_some()))
        {
            return Err(UNSUPPORTED.into());
        }
        match op.op.as_slice() {
            b"q" => graphics += 1,
            b"Q" => graphics = graphics.checked_sub(1).ok_or(UNSUPPORTED)?,
            b"BT" if !text => text = true,
            b"ET" if text => text = false,
            b"BT" | b"ET" | b"BI" => return Err(UNSUPPORTED.into()),
            b"BDC" | b"BMC" => marked += 1,
            b"EMC" => marked = marked.checked_sub(1).ok_or(UNSUPPORTED)?,
            _ => (),
        }
        if graphics > 64 || marked > 64 {
            return Err(UNSUPPORTED.into());
        }
        let slot = match op.op.as_slice() {
            b"Tf" => Some((b"Font".as_slice(), 0)),
            b"Do" => Some((b"XObject".as_slice(), 0)),
            b"gs" => Some((b"ExtGState".as_slice(), 0)),
            b"sh" => Some((b"Shading".as_slice(), 0)),
            b"CS" | b"cs" => Some((b"ColorSpace".as_slice(), 0)),
            b"SCN" | b"scn" => op
                .operands
                .len()
                .checked_sub(1)
                .map(|i| (b"Pattern".as_slice(), i)),
            b"BDC" | b"DP" => Some((b"Properties".as_slice(), 1)),
            _ => None,
        };
        if let Some((kind, index)) = slot
            && let Some(old) = op.name(index)
        {
            if let Some(new) = names.get(&(kind.to_vec(), old.to_vec())) {
                op.operands[index] = Object::Name(new.clone());
            } else if !(kind == b"ColorSpace"
                && matches!(
                    old,
                    b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK" | b"Pattern"
                ))
            {
                return Err(UNSUPPORTED.into());
            }
        }
    }
    if graphics != 0 || marked != 0 || text {
        return Err(UNSUPPORTED.into());
    }
    Ok(())
}

fn available(doc: &Document, page: usize, index: usize) -> Result<bool> {
    let images = pdfcraft_edit::page_images(doc, page).map_err(|e| e.to_string())?;
    let Some(image) = images
        .get(index)
        .filter(|i| i.is_form && editable_placement(i.matrix))
    else {
        return Ok(false);
    };
    let Some(object) = image.object else {
        return Ok(false);
    };
    let o = doc.get(object);
    let Object::Stream(s) = o.as_ref() else {
        return Ok(false);
    };
    let resources = s
        .dict
        .get(b"Resources")
        .and_then(|r| doc.dict(r))
        .or_else(|| {
            let p = pdfcraft_model::pages(doc).get(page)?.clone();
            p.dict.get(b"Resources").and_then(|r| doc.dict(r))
        })
        .unwrap_or_default();
    Ok(ordinary(s) && supported_resources(doc, &resources))
}

fn has_text(o: &Object) -> bool {
    match o {
        Object::String(s) => !s.to_text().is_empty(),
        Object::Array(a) => a.iter().any(has_text),
        _ => false,
    }
}

fn dissociate(
    doc: &mut Document,
    page: usize,
    index: usize,
    remaining_form_bytes: &mut usize,
) -> Result<bool> {
    let images = pdfcraft_edit::page_images(doc, page).map_err(|e| e.to_string())?;
    let image = images
        .get(index)
        .filter(|i| i.is_form)
        .ok_or("Sélectionnez un schéma groupé.")?;
    // The drawing editor currently handles rectangular clips in page space.
    // Keep sheared/obliquely rotated clips grouped until their members can be
    // edited without exposing previously hidden parts of the artwork.
    if !editable_placement(image.matrix) {
        return Err(UNSUPPORTED.into());
    }
    let object = image.object.ok_or(UNSUPPORTED)?;
    let form = doc.get(object);
    let Object::Stream(form) = form.as_ref() else {
        return Err(UNSUPPORTED.into());
    };
    if !ordinary(form) {
        return Err(UNSUPPORTED.into());
    }
    let bounds = form
        .dict
        .get(b"BBox")
        .map(|v| doc.resolve(v))
        .and_then(|v| {
            v.as_array()
                .and_then(|a| a.iter().map(Object::as_f64).collect::<Option<Vec<_>>>())
        })
        .filter(|v| v.len() == 4)
        .ok_or(UNSUPPORTED)?;
    let matrix = form
        .dict
        .get(b"Matrix")
        .map(|v| doc.resolve(v))
        .map(|v| {
            v.as_array()
                .and_then(|a| Matrix::from_operands(a))
                .ok_or(UNSUPPORTED)
        })
        .transpose()?
        .unwrap_or(Matrix::IDENTITY);
    if !bounds.iter().chain(&matrix.0).all(|v| v.is_finite()) {
        return Err(UNSUPPORTED.into());
    }
    let bytes = decode(form, *remaining_form_bytes)?;
    *remaining_form_bytes -= bytes.len();
    let parsed = pdfcraft_content::parse(&bytes);
    if parsed.skipped != 0 || parsed.ops.len() > MAX_OPS {
        return Err(UNSUPPORTED.into());
    }
    let mut body = parsed.ops;
    let wrapper = body.iter().filter(|o| o.is("Do")).count() == 1
        && !body.iter().any(|o| match o.op.as_slice() {
            b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"sh" | b"BI" => true,
            b"Tj" | b"TJ" | b"'" | b"\"" => o.operands.iter().any(has_text),
            _ => false,
        });
    let p = pdfcraft_model::pages(doc)
        .get(page)
        .cloned()
        .ok_or("Page introuvable.")?;
    let mut resources = p
        .dict
        .get(b"Resources")
        .and_then(|r| doc.dict(r))
        .unwrap_or_default();
    let source = form
        .dict
        .get(b"Resources")
        .and_then(|r| doc.dict(r))
        .unwrap_or_else(|| resources.clone());
    let aliases = import_resources(doc, &mut resources, &source)?;
    rename(&mut body, &aliases)?;
    let mut streams = Vec::new();
    if let Some(o) = p.dict.get(b"Contents") {
        contents(doc, o, 0, &mut streams)?;
    }
    let mut data = Vec::new();
    let mut remaining = MAX_BYTES;
    for o in &streams {
        let o = doc.resolve(o);
        let Object::Stream(s) = o.as_ref() else {
            return Err(UNSUPPORTED.into());
        };
        let bytes = decode(s, remaining)?;
        remaining -= bytes.len();
        data.push(bytes);
    }
    let pieces = Pieces::join(&data);
    let parsed = pieces.parse();
    if parsed.skipped != 0 || parsed.ops.len() > MAX_OPS {
        return Err(UNSUPPORTED.into());
    }
    let occurrences: Vec<_> = parsed
        .ops
        .iter()
        .filter(|op| op.is("Do") && op.name(0) == Some(image.name.as_bytes()))
        .collect();
    if occurrences.len() != images.iter().filter(|i| i.name == image.name).count() {
        return Err(UNSUPPORTED.into());
    }
    let ordinal = images[..index]
        .iter()
        .filter(|i| i.name == image.name)
        .count();
    let at = occurrences.get(ordinal).ok_or(UNSUPPORTED)?;
    let mut in_text = false;
    let mut path = false;
    for op in &parsed.ops {
        if op.span == at.span {
            break;
        }
        match op.op.as_slice() {
            b"BT" => in_text = true,
            b"ET" => in_text = false,
            b"m" | b"l" | b"c" | b"v" | b"y" | b"re" => path = true,
            b"n" | b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" => path = false,
            _ => (),
        }
    }
    if in_text || path {
        return Err(UNSUPPORTED.into());
    }
    let xo = resources
        .get(b"XObject")
        .and_then(|r| doc.dict(r))
        .ok_or(UNSUPPORTED)?;
    if xo.get(image.name.as_bytes()).and_then(Object::as_ref) != Some(object) {
        return Err(UNSUPPORTED.into());
    }
    let mut replacement = vec![
        Op::new("q", vec![]),
        Op::new("cm", matrix.0.map(Object::Real).to_vec()),
        Op::new(
            "re",
            vec![
                bounds[0].into(),
                bounds[1].into(),
                (bounds[2] - bounds[0]).into(),
                (bounds[3] - bounds[1]).into(),
            ],
        ),
        Op::new("W", vec![]),
        Op::new("n", vec![]),
    ];
    replacement.extend(body);
    replacement.push(Op::new("n", vec![]));
    replacement.push(Op::new("Q", vec![]));
    let rewritten = pieces.splice([(
        at.span.clone(),
        pdfcraft_content::serialize_ops(&replacement),
    )]);
    let mut new_contents = Vec::new();
    for ((old, before), after) in streams.iter().zip(&data).zip(rewritten) {
        if before == &after {
            new_contents.push(old.clone());
            continue;
        }
        let original = doc.resolve(old);
        let Object::Stream(s) = original.as_ref() else {
            return Err(UNSUPPORTED.into());
        };
        if s.dict.get(b"PCAdded").is_some() {
            return Err(UNSUPPORTED.into());
        }
        let mut dict = s.dict.clone();
        dict.remove(b"Filter");
        dict.remove(b"DecodeParms");
        new_contents.push(Object::Ref(doc.add(Stream::flate(dict, &after))));
    }
    doc.update_dict(p.obj, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Array(new_contents));
    })
    .map_err(|e| e.to_string())?;
    pdfcraft_edit::editable_objects(doc, page).map_err(|e| e.to_string())?;
    Ok(wrapper
        && pdfcraft_edit::page_images(doc, page)
            .map_err(|e| e.to_string())?
            .len()
            == images.len()
        && available(doc, page, index)?)
}

impl Editor {
    pub fn can_dissociate_form(&self, page: usize, index: usize) -> bool {
        crate::document::guarded(|| {
            if self.doc.permissions().is_some_and(|p| !p.modify()) {
                return Ok(false);
            }
            available(&self.doc, page, index)
        })
        .unwrap_or(false)
    }
    pub fn dissociate_form(&mut self, page: usize, index: usize) -> Result<()> {
        self.change(false, |doc| {
            let mut remaining = MAX_BYTES;
            for _ in 0..16 {
                if !dissociate(doc, page, index, &mut remaining)? {
                    return Ok(());
                }
            }
            Err("Ce schéma contient trop de niveaux de groupes imbriqués.".into())
        })
    }
}
