//! Mixed selections keep original PDF paints, glyphs, fonts and clipping paths.
//! Paths keep their native operators: there is no rasterization or flattening.
use crate::document::{Editor, Result};
use pdfcraft_content::{Matrix, Op, Pieces};
use pdfcraft_cos::{Dict, Document, Object, Stream};
use pdfcraft_edit::{ObjectKind, ObjectTarget};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Object(ObjectTarget),
    Paint(usize),
    Annotation(usize),
}
#[derive(Clone, Debug)]
pub struct Item {
    pub target: Target,
    pub rect: [f64; 4],
}
struct Paint {
    id: usize,
    rect: [f64; 4],
    prefix: usize,
    path: Vec<usize>,
    depth: usize,
    marked: usize,
    matrix: Matrix,
    in_place: bool,
    clip: Option<[f64; 4]>,
}
struct Inventory {
    contents: Vec<Object>,
    pieces: Pieces,
    ops: Vec<Op>,
    prefix: Vec<Op>,
    paints: Vec<Paint>,
    scopes: Vec<Scope>,
}
struct Scope {
    start: usize,
    end: usize,
    matrix: Matrix,
    clip: Option<[f64; 4]>,
}
fn intersect(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}
fn inventory(doc: &Document, page: usize) -> Result<Inventory> {
    let p = pdfcraft_model::pages(doc)
        .get(page)
        .cloned()
        .ok_or("Page introuvable.")?;
    let contents = p
        .dict
        .get(b"Contents")
        .map(|c| match doc.resolve(c).as_ref() {
            Object::Array(a) => a.clone(),
            _ => vec![c.clone()],
        })
        .unwrap_or_default();
    let mut data = Vec::new();
    let mut excluded = BTreeSet::new();
    for (index, c) in contents.iter().enumerate() {
        let o = doc.resolve(c);
        let Object::Stream(s) = o.as_ref() else {
            return Err("Flux PDF invalide.".into());
        };
        data.push(s.decoded().map_err(|e| e.to_string())?);
        if s.dict.get(b"PCAdded").is_some() {
            excluded.insert(index);
        }
    }
    let pieces = Pieces::join(&data);
    let parsed = pieces.parse();
    if parsed.skipped > 0 {
        return Err("Ce contenu PDF ne peut pas être sélectionné de manière fiable.".into());
    }
    let ops = parsed.ops;
    let mut prefix = Vec::new();
    let mut paints = Vec::new();
    let mut matrix = Matrix::IDENTITY;
    let mut width = 1.;
    let mut clip: Option<[f64; 4]> = None;
    let mut blocked = false;
    let mut stack = Vec::new();
    let mut path = Vec::new();
    let mut b = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut clipping = false;
    let mut in_text = false;
    let mut managed = Vec::new();
    let mut stable_path = true;
    let mut scopes = Vec::new();
    let mut scope_stack = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        let own = excluded.contains(&pieces.pieces_of(op).0);
        match op.op.as_slice() {
            b"BDC" | b"BMC" => {
                managed.push(
                    op.operands
                        .get(1)
                        .and_then(Object::as_dict)
                        .is_some_and(|d| d.get(b"PNText").is_some()),
                );
                prefix.push(op.clone());
            }
            b"EMC" => {
                managed.pop();
                prefix.push(op.clone());
            }
            b"BT" => in_text = true,
            b"ET" => in_text = false,
            b"Tr" if op.num(0).is_some_and(|v| v >= 4.) => blocked = true,
            b"q" => {
                scope_stack.push((i, matrix, clip));
                stable_path &= path.is_empty();
                stack.push((matrix, width, clip, blocked));
                prefix.push(op.clone());
            }
            b"Q" => {
                if let Some((start, matrix, clip)) = scope_stack.pop() {
                    scopes.push(Scope {
                        start,
                        end: i,
                        matrix,
                        clip,
                    });
                }
                stable_path &= path.is_empty();
                if let Some(s) = stack.pop() {
                    (matrix, width, clip, blocked) = s;
                }
                prefix.push(op.clone());
            }
            b"cm" => {
                stable_path &= path.is_empty();
                if let Some(m) = op.nums::<6>() {
                    matrix = Matrix(m).then(&matrix);
                }
                prefix.push(op.clone());
            }
            b"w" => {
                width = op.num(0).unwrap_or(1.);
                prefix.push(op.clone());
            }
            b"J" | b"j" | b"M" | b"d" | b"ri" | b"i" | b"gs" | b"CS" | b"cs" | b"SC" | b"SCN"
            | b"sc" | b"scn" | b"G" | b"g" | b"RG" | b"rg" | b"K" | b"k" => prefix.push(op.clone()),
            b"m" | b"l" | b"c" | b"v" | b"y" | b"re" | b"h" => {
                path.push(i);
                let mut point = |x, y| {
                    let (x, y) = matrix.apply(x, y);
                    b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                };
                if op.is("re") {
                    if let Some([x, y, w, h]) = op.nums::<4>() {
                        for (x, y) in [(x, y), (x + w, y), (x + w, y + h), (x, y + h)] {
                            point(x, y);
                        }
                    }
                } else {
                    for pair in op.operands.as_chunks::<2>().0 {
                        if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                            point(x, y);
                        }
                    }
                }
            }
            b"W" | b"W*" => {
                path.push(i);
                clipping = true;
            }
            b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"n" => {
                let mut rect = b;
                if matches!(op.op.as_slice(), b"S" | b"s" | b"B" | b"B*" | b"b" | b"b*") {
                    let m = matrix.0;
                    // PDF width 0 is a visible device hairline, not an empty box.
                    let margin =
                        (width.abs() * m[0].hypot(m[1]).max(m[2].hypot(m[3])) / 2.).max(0.125);
                    rect = [
                        rect[0] - margin,
                        rect[1] - margin,
                        rect[2] + margin,
                        rect[3] + margin,
                    ];
                }
                let whole = clip.is_none_or(|c| {
                    rect[0] >= c[0] && rect[1] >= c[1] && rect[2] <= c[2] && rect[3] <= c[3]
                });
                if let Some(c) = clip {
                    rect = intersect(rect, c);
                }
                if !op.is("n")
                    && !own
                    && !in_text
                    && !managed.contains(&true)
                    && !blocked
                    && rect.iter().all(|v| v.is_finite())
                    && rect[2] > rect[0]
                    && rect[3] > rect[1]
                {
                    paints.push(Paint {
                        id: i,
                        rect,
                        prefix: prefix.len(),
                        path: path.clone(),
                        depth: stack.len(),
                        marked: managed.len(),
                        matrix,
                        in_place: whole && stable_path && !clipping,
                        clip,
                    });
                }
                if clipping {
                    clip = Some(clip.map_or(b, |c| intersect(c, b)));
                    prefix.extend(path.iter().map(|j| ops[*j].clone()));
                    prefix.push(Op::new("n", vec![]));
                }
                path.clear();
                b = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                clipping = false;
                stable_path = true;
            }
            // Shadings and inline images are also native paints, rather than holes
            // in the inventory. A shading's visible clipping extent is its bounds.
            b"sh" | b"BI" if !own && !blocked => {
                let rect = if op.is("BI") {
                    matrix.bbox([0., 0., 1., 1.])
                } else {
                    clip.unwrap_or_else(|| p.crop(doc))
                };
                paints.push(Paint {
                    id: i,
                    rect,
                    prefix: prefix.len(),
                    path: vec![],
                    depth: stack.len(),
                    marked: managed.len(),
                    matrix,
                    in_place: false,
                    clip,
                });
            }
            _ => {}
        }
    }
    if paints.len() > 10000 || prefix.len() > 100000 {
        return Err("Cette page contient trop d’objets pour une sélection simultanée.".into());
    }
    Ok(Inventory {
        contents,
        pieces,
        ops,
        prefix,
        paints,
        scopes,
    })
}
fn annotations(doc: &Document, page: usize) -> Result<Vec<Object>> {
    let p = pdfcraft_model::pages(doc)
        .get(page)
        .cloned()
        .ok_or("Page introuvable.")?;
    Ok(p.dict
        .get(b"Annots")
        .and_then(|a| doc.resolve(a).as_array().cloned())
        .unwrap_or_default())
}
fn items(doc: &Document, page: usize) -> Result<Vec<Item>> {
    let mut items = pdfcraft_edit::editable_objects(doc, page)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|o| Item {
            target: Target::Object(o.target),
            rect: o.rect,
        })
        .collect::<Vec<_>>();
    items.extend(inventory(doc, page)?.paints.into_iter().map(|p| Item {
        target: Target::Paint(p.id),
        rect: p.rect,
    }));
    for (index, o) in annotations(doc, page)?.iter().enumerate() {
        if let Some(rect) = doc
            .resolve(o)
            .as_dict()
            .and_then(|d| d.get(b"Rect"))
            .and_then(Object::as_array)
            .and_then(|a| a.iter().map(Object::as_f64).collect::<Option<Vec<_>>>())
            .and_then(|a| <[f64; 4]>::try_from(a).ok())
        {
            items.push(Item {
                target: Target::Annotation(index),
                rect,
            });
        }
    }
    Ok(items)
}
fn paints_change(
    doc: &mut Document,
    page: usize,
    targets: &BTreeSet<usize>,
    delta: Option<[f64; 2]>,
) -> Result<()> {
    if targets.is_empty() {
        return Ok(());
    }
    let all = inventory(doc, page)?;
    if all
        .paints
        .iter()
        .filter(|p| targets.contains(&p.id))
        .map(|p| p.prefix + p.path.len())
        .sum::<usize>()
        > 2_000_000
    {
        return Err("Cette sélection est trop complexe. Déplacez-la en plusieurs parties.".into());
    }
    let p = pdfcraft_model::pages(doc)[page].clone();
    let mut edits = Vec::new();
    let mut appended = Vec::new();
    let mut kept_owned = std::collections::BTreeMap::new();
    let mut in_placed = BTreeSet::new();
    if let Some(delta) = delta {
        // Move a complete clipped graphics scope in its original stacking order.
        // This keeps grid lines below their labels and translates the clip too.
        let mut scopes = all.scopes.iter().collect::<Vec<_>>();
        scopes.sort_by_key(|s| std::cmp::Reverse(s.end - s.start));
        for scope in scopes {
            let paints = all
                .paints
                .iter()
                .filter(|p| p.id > scope.start && p.id < scope.end)
                .collect::<Vec<_>>();
            let stays_inside = |p: &Paint| {
                p.clip.is_none_or(|c| {
                    p.rect[0] + delta[0] >= c[0]
                        && p.rect[1] + delta[1] >= c[1]
                        && p.rect[2] + delta[0] <= c[2]
                        && p.rect[3] + delta[1] <= c[3]
                })
            };
            if paints.is_empty()
                || paints
                    .iter()
                    .any(|p| !targets.contains(&p.id) || in_placed.contains(&p.id))
                || paints.iter().all(|p| p.in_place && stays_inside(p))
                || all.ops[scope.start..=scope.end]
                    .iter()
                    .any(|o| matches!(o.op.as_slice(), b"BT" | b"Do" | b"BI" | b"sh"))
            {
                continue;
            }
            let fits = paints.iter().all(|p| {
                scope.clip.is_none_or(|c| {
                    p.rect[0] + delta[0] >= c[0]
                        && p.rect[1] + delta[1] >= c[1]
                        && p.rect[2] + delta[0] <= c[2]
                        && p.rect[3] + delta[1] <= c[3]
                })
            });
            if !fits {
                continue;
            }
            let inverse = scope
                .matrix
                .invert()
                .ok_or("Transformation non inversible.")?;
            let a = inverse.apply(0., 0.);
            let b = inverse.apply(delta[0], delta[1]);
            let before = [
                Op::new("q", vec![]),
                Op::new(
                    "cm",
                    [1., 0., 0., 1., b.0 - a.0, b.1 - a.1]
                        .map(Object::Real)
                        .to_vec(),
                ),
                all.ops[scope.start].clone(),
            ];
            edits.push((
                all.ops[scope.start].span.clone(),
                pdfcraft_content::serialize_ops(&before),
            ));
            edits.push((
                all.ops[scope.end].span.clone(),
                pdfcraft_content::serialize_ops(&[
                    all.ops[scope.end].clone(),
                    Op::new("Q", vec![]),
                ]),
            ));
            in_placed.extend(paints.into_iter().map(|p| p.id));
        }
    }
    for paint in all.paints.iter().filter(|p| targets.contains(&p.id)) {
        if in_placed.contains(&paint.id) {
            continue;
        }
        if let Some(delta) = delta {
            let r = paint.rect;
            let moved = [
                r[0] + delta[0],
                r[1] + delta[1],
                r[2] + delta[0],
                r[3] + delta[1],
            ];
            if paint.in_place
                && paint.clip.is_none_or(|c| {
                    moved[0] >= c[0] && moved[1] >= c[1] && moved[2] <= c[2] && moved[3] <= c[3]
                })
            {
                let inverse = paint
                    .matrix
                    .invert()
                    .ok_or("Transformation non inversible.")?;
                let a = inverse.apply(0., 0.);
                let b = inverse.apply(delta[0], delta[1]);
                let mut content = vec![
                    Op::new("q", vec![]),
                    Op::new(
                        "cm",
                        [1., 0., 0., 1., b.0 - a.0, b.1 - a.1]
                            .map(Object::Real)
                            .to_vec(),
                    ),
                ];
                content.extend(paint.path.iter().map(|i| all.ops[*i].clone()));
                content.push(all.ops[paint.id].clone());
                content.push(Op::new("Q", vec![]));
                for index in &paint.path {
                    edits.push((all.ops[*index].span.clone(), Vec::new()));
                }
                edits.push((
                    all.ops[paint.id].span.clone(),
                    pdfcraft_content::serialize_ops(&content),
                ));
                kept_owned
                    .entry(all.pieces.pieces_of(&all.ops[paint.id]).0)
                    .or_insert(delta);
                in_placed.insert(paint.id);
                continue;
            }
        }
        // Retain the path's clipping side-effect for following unselected objects.
        edits.push((all.ops[paint.id].span.clone(), b"n\n".to_vec()));
        if let Some(delta) = delta {
            let mut content = vec![
                Op::new("q", vec![]),
                Op::new(
                    "cm",
                    [1., 0., 0., 1., delta[0], delta[1]]
                        .map(Object::Real)
                        .to_vec(),
                ),
            ];
            content.extend_from_slice(&all.prefix[..paint.prefix]);
            content.extend(paint.path.iter().map(|i| all.ops[*i].clone()));
            content.push(all.ops[paint.id].clone());
            content.extend((0..paint.marked).map(|_| Op::new("EMC", vec![])));
            content.extend((0..paint.depth).map(|_| Op::new("Q", vec![])));
            content.push(Op::new("Q", vec![]));
            appended.extend(content);
        }
    }
    if all
        .paints
        .iter()
        .filter(|p| targets.contains(&p.id))
        .count()
        != targets.len()
    {
        return Err("La sélection a changé. Sélectionnez de nouveau les éléments.".into());
    }
    edits.sort_by_key(|(r, _)| r.start);
    let chunks = all.pieces.splice(edits);
    let mut contents = Vec::new();
    // Isolate the old page state, including an unbalanced top-level clipping path.
    let start = doc.add(Object::Stream(Stream::flate(Dict::new(), b"q\n")));
    contents.push(Object::Ref(start));
    for (stream_index, (old, bytes)) in all.contents.iter().zip(chunks).enumerate() {
        let o = doc.resolve(old);
        let Object::Stream(s) = o.as_ref() else {
            return Err("Flux invalide.".into());
        };
        if s.decoded().map_err(|e| e.to_string())? == bytes {
            contents.push(old.clone());
            continue;
        }
        let mut d = s.dict.clone();
        d.remove(b"Filter");
        d.remove(b"DecodeParms");
        let all_owned_moved = all
            .paints
            .iter()
            .filter(|p| all.pieces.pieces_of(&all.ops[p.id]).0 == stream_index)
            .all(|p| in_placed.contains(&p.id));
        if let Some(delta) = kept_owned.get(&stream_index).filter(|_| all_owned_moved)
            && let Some(Object::String(value)) = d.get(b"PNDrawing")
            && let Ok(mut drawing) =
                serde_json::from_str::<crate::drawing::Drawing>(&value.to_text())
        {
            drawing.translate(*delta);
            d.set(
                b"PNDrawing".to_vec(),
                Object::String(pdfcraft_cos::PdfString::text(
                    &serde_json::to_string(&drawing).map_err(|e| e.to_string())?,
                )),
            );
        } else {
            d.remove(b"PNDrawing");
        }
        contents.push(Object::Ref(
            doc.add(Object::Stream(Stream::flate(d, &bytes))),
        ));
    }
    let mut end = b"Q\n".to_vec();
    end.extend(pdfcraft_content::serialize_ops(&appended));
    contents.push(Object::Ref(
        doc.add(Object::Stream(Stream::flate(Dict::new(), &end))),
    ));
    doc.update_dict(p.obj, |d| {
        d.set(b"Contents".to_vec(), Object::Array(contents));
    })
    .map_err(|e| e.to_string())
}
fn delete_objects(doc: &mut Document, page: usize, targets: &[ObjectTarget]) -> Result<()> {
    let mut targets = targets.to_vec();
    targets.sort();
    targets.dedup();
    targets.reverse();
    for t in targets {
        match t.kind {
            ObjectKind::Text => {
                pdfcraft_edit::text::clear_block(doc, page, t.index).map_err(|e| e.to_string())?
            }
            ObjectKind::Image => {
                pdfcraft_edit::change_image(doc, page, t.index, &pdfcraft_edit::ImageChange::Delete)
                    .map_err(|e| e.to_string())?
            }
            ObjectKind::Added => {
                pdfcraft_edit::delete_content(doc, page, t.index).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}
fn change(
    doc: &mut Document,
    page: usize,
    targets: &[Target],
    delta: Option<[f64; 2]>,
) -> Result<()> {
    let inventory = items(doc, page)?;
    if targets.is_empty()
        || targets
            .iter()
            .any(|t| !inventory.iter().any(|i| i.target == *t))
    {
        return Err("La sélection n’existe plus.".into());
    }
    let paints = targets
        .iter()
        .filter_map(|t| {
            if let Target::Paint(i) = t {
                Some(*i)
            } else {
                None
            }
        })
        .collect();
    paints_change(doc, page, &paints, delta)?;
    let objects = targets
        .iter()
        .filter_map(|t| {
            if let Target::Object(o) = t {
                Some(*o)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if let Some(delta) = delta {
        let mut rich = Vec::new();
        let mut plain = Vec::new();
        for t in objects {
            let data = if t.kind == ObjectKind::Text {
                pdfcraft_edit::text::formatting_data(doc, page, t.index)
                    .ok()
                    .flatten()
                    .and_then(|s| serde_json::from_str::<crate::rich::TextData>(&s).ok())
            } else {
                None
            };
            if let Some(data) = data {
                rich.push(data);
            } else {
                plain.push(t);
            }
        }
        if !plain.is_empty() {
            pdfcraft_edit::objects::move_objects(doc, page, &plain, delta)
                .map_err(|e| e.to_string())?;
        }
        for mut data in rich {
            let blocks = pdfcraft_edit::text_blocks(doc, page).map_err(|e| e.to_string())?;
            let index = (0..blocks.len())
                .find(|i| {
                    pdfcraft_edit::text::formatting_data(doc, page, *i)
                        .ok()
                        .flatten()
                        .and_then(|s| serde_json::from_str::<crate::rich::TextData>(&s).ok())
                        .is_some_and(|d| d.id == data.id)
                })
                .ok_or("Bloc de texte introuvable.")?;
            data.rect = [
                data.rect[0] + delta[0],
                data.rect[1] + delta[1],
                data.rect[2] + delta[0],
                data.rect[3] + delta[1],
            ];
            Editor::write_formatted_text(doc, page, Some(index), None, &mut data, &[])?;
        }
    } else {
        delete_objects(doc, page, &objects)?;
    }
    let selected_annotations = targets
        .iter()
        .filter_map(|t| {
            if let Target::Annotation(i) = t {
                Some(*i)
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>();
    if !selected_annotations.is_empty() {
        let mut kept = Vec::new();
        for (index, o) in annotations(doc, page)?.into_iter().enumerate() {
            if !selected_annotations.contains(&index) {
                kept.push(o);
                continue;
            }
            if let Some(delta) = delta {
                let mut dict = doc
                    .resolve(&o)
                    .as_dict()
                    .cloned()
                    .ok_or("Annotation invalide.")?;
                for key in [b"Rect".as_slice(), b"QuadPoints", b"Vertices", b"L", b"CL"] {
                    if let Some(values) = dict.get(key).and_then(Object::as_array) {
                        let values = values
                            .iter()
                            .enumerate()
                            .map(|(i, v)| {
                                v.as_f64()
                                    .map(|v| Object::Real(v + delta[i % 2]))
                                    .unwrap_or_else(|| v.clone())
                            })
                            .collect();
                        dict.set(key.to_vec(), Object::Array(values));
                    }
                }
                if let Some(lines) = dict.get(b"InkList").and_then(Object::as_array) {
                    let lines = lines
                        .iter()
                        .map(|line| match line.as_array() {
                            Some(v) => Object::Array(
                                v.iter()
                                    .enumerate()
                                    .map(|(i, n)| {
                                        n.as_f64()
                                            .map(|n| Object::Real(n + delta[i % 2]))
                                            .unwrap_or_else(|| n.clone())
                                    })
                                    .collect(),
                            ),
                            None => line.clone(),
                        })
                        .collect();
                    dict.set(b"InkList".to_vec(), Object::Array(lines));
                }
                if let Some(r) = o.as_ref() {
                    doc.set(r, Object::Dict(dict));
                    kept.push(o);
                } else {
                    kept.push(Object::Dict(dict));
                }
            }
        }
        let p = pdfcraft_model::pages(doc)[page].obj;
        doc.update_dict(p, |d| d.set(b"Annots".to_vec(), Object::Array(kept)))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
impl Editor {
    pub fn selection_items(&self, page: usize) -> Result<Vec<Item>> {
        items(&self.doc, page)
    }
    pub fn move_selection(
        &mut self,
        page: usize,
        targets: &[Target],
        delta: [f64; 2],
    ) -> Result<()> {
        if !delta.iter().all(|v| v.is_finite() && v.abs() < 10000000.) {
            return Err("Déplacement invalide.".into());
        }
        if delta == [0., 0.] {
            return Ok(());
        }
        self.change(false, |doc| change(doc, page, targets, Some(delta)))
    }
    pub fn delete_selection(&mut self, page: usize, targets: &[Target]) -> Result<()> {
        self.change(false, |doc| change(doc, page, targets, None))
    }
    pub fn selection_preview(
        &self,
        page: usize,
        targets: &[Target],
        only: bool,
        black: bool,
    ) -> Result<std::sync::Arc<Vec<u8>>> {
        let mut doc = self.doc.clone();
        let removed = if only {
            items(&doc, page)?
                .into_iter()
                .map(|i| i.target)
                .filter(|t| !targets.contains(t))
                .collect::<Vec<_>>()
        } else {
            targets.to_vec()
        };
        if !removed.is_empty() {
            change(&mut doc, page, &removed, None)?;
        }
        if black {
            let p = pdfcraft_model::pages(&doc)[page].clone();
            let c = p
                .dict
                .get(b"Contents")
                .map(|o| match doc.resolve(o).as_ref() {
                    Object::Array(a) => a.clone(),
                    _ => vec![o.clone()],
                })
                .unwrap_or_default();
            let b = doc.add(Object::Stream(Stream::flate(
                Dict::new(),
                b"q 0 g -100000 -100000 200000 200000 re f Q\n",
            )));
            let mut all = vec![Object::Ref(b)];
            all.extend(c);
            doc.update_dict(p.obj, |d| d.set(b"Contents".to_vec(), Object::Array(all)))
                .map_err(|e| e.to_string())?;
        }
        pdfcraft_cos::write_full(&doc, &Default::default())
            .map(std::sync::Arc::new)
            .map_err(|e| e.to_string())
    }
}
