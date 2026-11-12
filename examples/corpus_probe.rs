//! Corpus acceptance runner. Every mutation is in memory; output paths must be supplied explicitly.
use pdfinator::document::Editor;
use serde_json::{Value, json};
use std::path::Path;
fn chars(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}
fn rendered(
    bytes: std::sync::Arc<Vec<u8>>,
    page: usize,
    scale: f32,
) -> Result<pdfcraft_render::RenderedPage, String> {
    let mut r = pdfcraft_render::PageRenderer::new(bytes, Default::default());
    let image = r.render(pdfcraft_render::RenderRequest {
        page,
        scale,
        ..Default::default()
    });
    if let Some(e) = &image.error {
        return Err(e.clone());
    }
    Ok(image)
}
fn cycle(
    e: &mut Editor,
    before: &std::sync::Arc<Vec<u8>>,
    result: Result<(), String>,
    out: &Path,
    label: &str,
) -> Value {
    if let Err(error) = result {
        return json!({"status":if e.bytes==*before {"refused"}else{"rollback_failed"},"error":error});
    }
    let after = e.bytes.clone();
    let reopened = Editor::from_bytes((*after).clone());
    let reopen_ok = reopened
        .as_ref()
        .is_ok_and(|d| d.info.pages.len() == e.info.pages.len());
    let undo_ok = e.undo().is_ok() && e.bytes == *before;
    let redo_ok = e.redo().is_ok() && e.bytes == after;
    let restored = e.undo().is_ok() && e.bytes == *before;
    let _ = std::fs::write(out.join(format!("{label}.pdf")), &*after);
    json!({"status":if reopen_ok&&undo_ok&&redo_ok&&restored {"ok"}else{"verification_failed"},"reopen":reopen_ok,"undo_exact":undo_ok,"redo_exact":redo_ok,"restored":restored})
}
fn exercise(path: &Path, out: &Path) -> Result<Value, String> {
    let mut e = Editor::open(path)?;
    let before = e.bytes.clone();
    let count = e.info.pages.len();
    let sample: std::collections::BTreeSet<_> = [0, count / 2, count - 1].into_iter().collect();
    let mut pages = Vec::new();
    let mut candidate = None;
    let mut selected = None;
    for page in sample {
        let text = e.text(page).map(|t| t.plain_text());
        let blocks = e.blocks(page);
        if candidate.is_none()
            && let Ok(blocks) = &blocks
        {
            for (i, b) in blocks.iter().enumerate() {
                let old = &b.editor_text;
                if b.invisible || old.chars().count() < 4 || old.chars().count() > 600 {
                    continue;
                }
                let mut letters: Vec<_> = old.chars().collect();
                let a = letters.iter().position(|c| c.is_alphabetic());
                if let Some(a) = a
                    && let Some(replacement) = letters
                        .iter()
                        .copied()
                        .find(|c| c.is_alphabetic() && *c != letters[a])
                {
                    letters[a] = replacement;
                    candidate = Some((page, i, letters.into_iter().collect::<String>()));
                    break;
                }
            }
        }
        let items = e.selection_items(page);
        if selected.is_none()
            && let Ok(items) = &items
            && let Some(item) = items
                .iter()
                .find(|i| matches!(i.target, pdfinator::selection::Target::Paint(_)))
                .or_else(|| items.first())
        {
            selected = Some((page, item.target));
        }
        pages.push(json!({"page":page,"width":e.info.pages[page].width,"height":e.info.pages[page].height,"characters":text.as_ref().map(|s|s.chars().count()).ok(),"text_error":text.err(),"blocks":blocks.as_ref().map(|b|b.len()).ok(),"blocks_error":blocks.err(),"items":items.as_ref().map(|v|v.len()).ok(),"items_error":items.err()}));
    }
    let mut renders = Vec::new();
    for page in [0, count - 1]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let p = &e.info.pages[page];
        let scale = (1400. / p.width.max(p.height)).min(1.5);
        match rendered(before.clone(), page, scale) {
            Ok(img) => {
                std::fs::write(
                    out.join(format!("source-{}.png", page + 1)),
                    pdfinator::ocr::encode_png(&img.rgba, img.width, img.height)?,
                )
                .map_err(|e| e.to_string())?;
                renders
                    .push(json!({"page":page,"status":"ok","width":img.width,"height":img.height}));
            }
            Err(error) => renders.push(json!({"page":page,"status":"error","error":error})),
        }
    }
    let mut edit = json!({"status":"not_applicable"});
    let mut move_text = json!({"status":"not_applicable"});
    let mut move_object = json!({"status":"not_applicable"});
    if e.signed {
        edit = json!({"status":"signed_not_modified"});
    } else {
        if let Some((page, index, new)) = candidate {
            let result = e.rewrite(page, index, Some(&new), &Default::default());
            let text_ok = result.is_ok()
                && e.text(page)
                    .is_ok_and(|t| chars(&t.plain_text()).contains(&chars(&new)));
            edit = cycle(&mut e, &before, result, out, "text-edited");
            edit["page"] = json!(page);
            edit["text_found"] = json!(text_ok);
            if edit["status"] == "ok" && !text_ok {
                edit["status"] = json!("text_verification_failed");
            }
            let result = e.move_block(page, index, [3., 2.]);
            move_text = cycle(&mut e, &before, result, out, "text-moved");
            move_text["page"] = json!(page);
        }
        if let Some((page, target)) = selected {
            let result = e.move_selection(page, &[target], [3., 2.]);
            move_object = cycle(&mut e, &before, result, out, "object-moved");
            move_object["page"] = json!(page);
            move_object["target"] = json!(format!("{target:?}"));
        }
    }
    Ok(
        json!({"status":"ok","pages":count,"signed":e.signed,"copy_allowed":e.can_copy(),"samples":pages,"renders":renders,"edit":edit,"move_text":move_text,"move_object":move_object,"source_memory_intact":e.bytes==before}),
    )
}
fn ocr(path: &Path, out: &Path, page: usize) -> Result<Value, String> {
    let e = Editor::open(path)?;
    let p = e.info.pages.get(page).ok_or("Page absente")?;
    let scale = (2200. / p.width.max(p.height)).min(3.);
    let img = rendered(e.bytes.clone(), page, scale)?;
    let recognition = pdfinator::ocr::recognize(&img.rgba, img.width, img.height)?;
    let blocks = recognition.styled_blocks(&img.rgba, img.width, img.height);
    let value = json!({"status":"ok","page":page,"language":recognition.language,"text":recognition.text(),"blocks":blocks.iter().map(|b|json!({"text":b.text,"size_pixels":b.size,"bold":b.bold,"rect":b.rect})).collect::<Vec<_>>()});
    std::fs::write(
        out.join(format!("ocr-{}.json", page + 1)),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(
        json!({"status":"ok","page":page,"characters":recognition.text().chars().count(),"blocks":blocks.len()}),
    )
}
fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 3 {
        eprintln!("Usage: corpus_probe phase source.pdf output-directory [page]");
        std::process::exit(2)
    }
    let phase = args[0].to_string_lossy();
    let path = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    std::fs::create_dir_all(out).unwrap();
    let start = std::time::Instant::now();
    let result = std::panic::catch_unwind(|| match phase.as_ref() {
        "exercise" => exercise(path, out),
        "inspect" => {
            let e = Editor::open(path)?;
            let lines = pdfcraft_edit::text_lines(&e.doc, 0).map_err(|e| e.to_string())?;
            let blocks = e.blocks(0)?;
            Ok(
                json!({"status":"ok","blocks":blocks.iter().map(|b|json!({"text":b.text,"editor_text":b.editor_text,"rect":b.rect,"spans":b.spans.iter().map(|(r,n)|json!({"range":[r.start,r.end],"line":n})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"lines":lines.iter().map(|l|json!({"text":l.text,"font":l.font,"base_font":l.base_font})).collect::<Vec<_>>()}),
            )
        }
        "ocr" => ocr(
            path,
            out,
            args.get(3)
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        ),
        "docx" | "odt" => {
            let format = if phase == "docx" {
                pdfinator::docx::Format::Docx
            } else {
                pdfinator::docx::Format::Odt
            };
            let e = Editor::open(path).unwrap();
            let bytes = if e.info.pages.len() > 20 {
                e.extract_page_group(&[0, e.info.pages.len() / 2, e.info.pages.len() - 1])?
            } else {
                (*e.bytes).clone()
            };
            let count = Editor::from_bytes(bytes.clone())?.info.pages.len();
            let converted = pdfinator::docx::convert_as(bytes, format)?;
            std::fs::write(out.join(format!("export.{phase}")), converted)
                .map_err(|e| e.to_string())?;
            Ok(
                json!({"status":"ok","exported_pages":count,"source_pages":e.info.pages.len(),"sampled":e.info.pages.len()>20}),
            )
        }
        _ => Err("Phase inconnue".into()),
    });
    let mut value = match result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => json!({"status":"error","error":e}),
        Err(_) => json!({"status":"panic"}),
    };
    value["seconds"] = json!(start.elapsed().as_secs_f64());
    println!("{}", serde_json::to_string(&value).unwrap());
}
