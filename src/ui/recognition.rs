//! Convert image text into positioned PDF text over cleaned non-text artwork.
//! Double-click converts directly; the properties command also offers a review.
use super::*;
use std::sync::mpsc::{self, Receiver};

struct Detected {
    result: pdfinator::ocr::Recognition,
    width: u32,
    height: u32,
    scale: f32,
    origin: Pos2,
    blocks: Vec<pdfinator::ocr::Block>,
    cleaned: Vec<u8>,
    png: Vec<u8>,
}
fn preview(pixels: &[u8], width: u32, height: u32) -> egui::ColorImage {
    // OCR keeps its full resolution; the comparison thumbnail stays small enough
    // for integrated graphics and the headless renderer's 2048px texture limit.
    let scale = (1024. / width.max(height) as f32).min(1.);
    let w = ((width as f32 * scale).round() as usize).max(1);
    let h = ((height as f32 * scale).round() as usize).max(1);
    let mut result = Vec::with_capacity(w * h);
    for y in 0..h {
        let source_y = ((y as f64 + 0.5) * height as f64 / h as f64) as usize;
        for x in 0..w {
            let source_x = ((x as f64 + 0.5) * width as f64 / w as f64) as usize;
            let offset = (source_y.min(height as usize - 1) * width as usize
                + source_x.min(width as usize - 1))
                * 4;
            result.push(Color32::from_rgba_premultiplied(
                pixels[offset],
                pixels[offset + 1],
                pixels[offset + 2],
                pixels[offset + 3],
            ));
        }
    }
    egui::ColorImage::new([w, h], result)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn native_labels_over_a_diagram_do_not_block_automatic_ocr_or_get_covered_by_its_background() {
        let (editor, page, target) = if let Some(path) = std::env::var_os("PDFINATOR_MIXED_OCR_PDF")
        {
            (
                Editor::open(std::path::Path::new(&path)).unwrap(),
                1,
                Target::Image(0),
            )
        } else {
            let d = pdfcraft_create::from_text(
                "",
                "Rectangle\nLosange\nQuadrilatere",
                (420., 260.),
                24.,
            )
            .unwrap();
            let mut renderer = pdfcraft_render::PageRenderer::new(
                Arc::new(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()),
                Default::default(),
            );
            let image = renderer.render(pdfcraft_render::RenderRequest {
                page: 0,
                scale: 2.,
                ..Default::default()
            });
            let png = pdfinator::ocr::encode_png(&image.rgba, image.width, image.height).unwrap();
            let d = pdfcraft_create::blank(595., 842., 1).unwrap();
            let mut e =
                Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap())
                    .unwrap();
            e.image_bytes(0, [50., 100., 550., 700.], "diagram.png", &png, None)
                .unwrap();
            e.add_text(
                0,
                AddedText {
                    rect: [80., 610., 400., 650.],
                    text: "Correction native".into(),
                    size: 14.,
                    ..Default::default()
                },
            )
            .unwrap();
            (e, 0, Target::Added(0))
        };
        let before = editor.bytes.clone();
        let native = editor.blocks(page).unwrap();
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        app.editor = Some(editor);
        app.workspace = Workspace::Edit;
        app.tool = Tool::Edit;
        app.page = page;
        let selection = app.selection_for(page, target).unwrap();
        app.start_automatic_recognition(&ctx, selection);
        assert!(app.recognition.is_some(), "{:?}", app.message);
        for _ in 0..3000 {
            let mut out = ctx.run_ui(Default::default(), |_| app.recognition_dialog(&ctx));
            out.textures_delta.clear();
            if app.recognition.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(app.recognition.is_none(), "OCR worker did not finish");
        assert!(
            app.message.is_none(),
            "No blocking OCR toast: {:?}",
            app.message
        );
        let editor = app.editor.as_mut().unwrap();
        assert_ne!(editor.bytes, before);
        let current = editor.blocks(page).unwrap();
        for block in &native {
            assert!(
                current
                    .iter()
                    .any(|b| b.editor_text == block.editor_text && b.rect == block.rect),
                "Native correction must be preserved: {block:?}"
            );
        }
        let labels = editor
            .added(page)
            .iter()
            .filter_map(|c| {
                if let Content::Text(t) = c {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        assert!(labels.contains("losange"), "{labels}");
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        let prefix = if std::env::var_os("PDFINATOR_MIXED_OCR_PDF").is_some() {
            "45-cours"
        } else {
            "45-mixte"
        };
        editor
            .save(&output.join(format!("{prefix}-ocr.pdf")))
            .unwrap();
        let (index, mut text) = editor
            .added(page)
            .into_iter()
            .enumerate()
            .find_map(|(i, c)| match c {
                Content::Text(t) if t.text.to_lowercase().contains("losange") => Some((i, t)),
                _ => None,
            })
            .unwrap();
        text.text = "Rhombe".into();
        editor
            .update_added(page, index, Content::Text(text))
            .unwrap();
        let path = output.join(format!("{prefix}-ocr-modifie.pdf"));
        editor.save(&path).unwrap();
        let reopened = Editor::open(&path).unwrap();
        assert_eq!(
            reopened
                .text(page)
                .unwrap()
                .plain_text()
                .matches("Rhombe")
                .count(),
            1
        );
        editor.undo().unwrap();
        editor.undo().unwrap();
        assert_eq!(editor.bytes, before);
    }
    #[test]
    fn three_images_on_one_page_convert_independently_then_their_words_can_be_edited_and_searched()
    {
        let d = pdfcraft_create::from_text(
            "Images multiples",
            "Légende native conservée",
            (595., 842.),
            12.,
        )
        .unwrap();
        let mut editor =
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
        for (index, city) in ["Paris", "Lyon", "Lille"].into_iter().enumerate() {
            let doc =
                pdfcraft_create::from_text("", &format!("Destination : {city}"), (380., 120.), 20.)
                    .unwrap();
            let mut renderer = pdfcraft_render::PageRenderer::new(
                Arc::new(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()),
                Default::default(),
            );
            let image = renderer.render(pdfcraft_render::RenderRequest {
                page: 0,
                scale: 2.,
                ..Default::default()
            });
            let png = pdfinator::ocr::encode_png(&image.rgba, image.width, image.height).unwrap();
            let top = 720. - index as f64 * 170.;
            editor
                .image_bytes(0, [60., top - 110., 440., top], "texte.png", &png, None)
                .unwrap();
        }
        let original_images = pdfcraft_edit::list_added(&editor.doc);
        for item in original_images.iter().take(2) {
            editor
                .doc
                .update_dict(item.obj, |d| {
                    d.remove(b"PCMark");
                    d.remove(b"PCAdded");
                })
                .unwrap();
        }
        editor =
            Editor::from_bytes(pdfcraft_cos::write_full(&editor.doc, &Default::default()).unwrap())
                .unwrap();
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        editor
            .save(&output.join("33-images-multiples-original.pdf"))
            .unwrap();
        let before = editor.bytes.clone();
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        app.editor = Some(editor);
        app.workspace = Workspace::Edit;
        app.tool = Tool::Edit;
        app.start_page_recognition(&ctx, 0);
        for (index, city) in ["Paris", "Lyon", "Lille"].into_iter().enumerate() {
            let mut session = app
                .recognition
                .take()
                .expect("Next image must still be reachable after earlier indexes change");
            for _ in 0..1000 {
                session.poll(&ctx);
                if session.rx.is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(session.error.is_none(), "{:?}", session.error);
            assert!(
                session.text.contains(city),
                "Image {index}: {}",
                session.text
            );
            for other in ["Paris", "Lyon", "Lille"]
                .into_iter()
                .filter(|s| *s != city)
            {
                assert!(
                    !session.text.contains(other),
                    "Neighbouring image leaked into OCR"
                );
            }
            let queue = session.queue.clone();
            assert_eq!(queue.len(), 2 - index);
            app.apply_recognition(&session).unwrap();
            assert!(
                app.editor
                    .as_ref()
                    .unwrap()
                    .text(0)
                    .unwrap()
                    .plain_text()
                    .contains("Légende native conservée")
            );
            app.next_page_recognition(&ctx, 0, queue);
        }
        assert!(app.recognition.is_none());
        let e = app.editor.as_mut().unwrap();
        let index = e
            .added(0)
            .iter()
            .position(|c| matches!(c,Content::Text(t) if t.text.contains("Paris")))
            .unwrap();
        let Content::Text(mut text) = e.added(0)[index].clone() else {
            panic!()
        };
        text.text = text.text.replace("Paris", "Nantes");
        e.update_added(0, index, Content::Text(text)).unwrap();
        app.refresh(0);
        for city in ["Nantes", "Lyon", "Lille"] {
            app.query = city.into();
            app.request_search(&ctx);
            for _ in 0..1000 {
                app.poll_work(&ctx);
                if app.work.searching.is_none() && app.work.search_due.is_none() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(app.hits.len(), 1, "Search must find {city} once on page 1");
            assert_eq!(app.hits[0].0, 0);
        }
        app.editor
            .as_mut()
            .unwrap()
            .save(&output.join("33-images-multiples-modifiees.pdf"))
            .unwrap();
        let reopened = Editor::open(&output.join("33-images-multiples-modifiees.pdf")).unwrap();
        for city in ["Nantes", "Lyon", "Lille"] {
            assert_eq!(reopened.text(0).unwrap().find(city).len(), 1);
        }
        assert!(reopened.text(0).unwrap().find("Paris").is_empty());
        for _ in 0..4 {
            app.undo_redo(false);
        }
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    }
    #[test]
    fn diagram_corner_letters_become_individually_editable_and_survive_reopening() {
        let editor = if let Some(path) = std::env::var_os("PDFINATOR_REGRESSION_PDF") {
            Editor::open(std::path::Path::new(&path)).unwrap()
        } else {
            let d = pdfcraft_create::blank(360., 200., 1).unwrap();
            let mut e =
                Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap())
                    .unwrap();
            for (letter, x, y) in [
                ("A", 30., 150.),
                ("B", 300., 150.),
                ("C", 300., 40.),
                ("D", 30., 40.),
            ] {
                e.add_text(
                    0,
                    AddedText {
                        rect: [x, y, x + 30., y + 25.],
                        text: letter.into(),
                        size: 18.,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            let mut renderer =
                pdfcraft_render::PageRenderer::new(e.bytes.clone(), Default::default());
            let image = renderer.render(pdfcraft_render::RenderRequest {
                page: 0,
                scale: 1.,
                ..Default::default()
            });
            let png = pdfinator::ocr::encode_png(&image.rgba, image.width, image.height).unwrap();
            let d = pdfcraft_create::blank(595., 842., 1).unwrap();
            let mut e =
                Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap())
                    .unwrap();
            e.image_bytes(0, [60., 500., 420., 700.], "schema.png", &png, None)
                .unwrap();
            e
        };
        let original = editor.bytes.clone();
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        app.editor = Some(editor);
        app.workspace = Workspace::Edit;
        app.tool = Tool::Edit;
        let target = if std::env::var_os("PDFINATOR_REGRESSION_PDF").is_some() {
            Target::Image(0)
        } else {
            Target::Added(0)
        };
        let selection = app.selection_for(0, target).unwrap();
        app.start_recognition(&ctx, selection);
        let mut session = app.recognition.take().unwrap();
        for _ in 0..1000 {
            session.poll(&ctx);
            if session.rx.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(session.error.is_none(), "{:?}", session.error);
        for letter in ["A", "B", "C", "D"] {
            assert_eq!(
                session.blocks.iter().filter(|b| b.text == letter).count(),
                1,
                "Missing {letter}: {:?}",
                session.blocks
            );
        }
        if std::env::var_os("PDFINATOR_REGRESSION_PDF").is_some() {
            for angle in ["36°", "43°"] {
                assert_eq!(
                    session.blocks.iter().filter(|b| b.text == angle).count(),
                    1,
                    "The raised degree sign must not become a baseline zero: {:?}",
                    session.blocks
                );
            }
        }
        app.apply_recognition(&session).unwrap();
        let e = app.editor.as_mut().unwrap();
        for letter in ["A", "B", "C", "D"] {
            assert_eq!(
                e.added(0)
                    .iter()
                    .filter(|c| matches!(c,Content::Text(t) if t.text==letter))
                    .count(),
                1
            );
        }
        let index = e
            .added(0)
            .iter()
            .position(|c| matches!(c,Content::Text(t) if t.text=="A"))
            .unwrap();
        let Content::Text(mut text) = e.added(0)[index].clone() else {
            panic!()
        };
        text.text = "Z".into();
        e.update_added(0, index, Content::Text(text)).unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-output/31-lettres-schema.pdf");
        e.save(&path).unwrap();
        let reopened = Editor::open(&path).unwrap();
        for letter in ["Z", "B", "C", "D"] {
            assert_eq!(
                reopened
                    .added(0)
                    .iter()
                    .filter(|c| matches!(c,Content::Text(t) if t.text==letter))
                    .count(),
                1
            );
        }
        app.undo_redo(false);
        app.undo_redo(false);
        assert_eq!(app.editor.as_ref().unwrap().bytes, original);
    }
    #[test]
    fn double_click_runs_real_french_ocr_then_converts_and_edits_without_a_duplicate() {
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        app.set_workspace(&ctx, Workspace::Edit);
        let text_doc = pdfcraft_create::from_text(
            "OCR",
            "Bonjour Ergonatus 123\nÉcole française : été, à bientôt !",
            (595., 842.),
            22.,
        )
        .unwrap();
        let bytes = pdfcraft_cos::write_full(&text_doc, &Default::default()).unwrap();
        let mut renderer = pdfcraft_render::PageRenderer::new(Arc::new(bytes), Default::default());
        let shot = renderer.render(pdfcraft_render::RenderRequest {
            page: 0,
            scale: 2.,
            ..Default::default()
        });
        assert!(shot.error.is_none());
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, shot.width, shot.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&shot.rgba)
                .unwrap();
        }
        let blank = pdfcraft_create::blank(595., 842., 1).unwrap();
        let mut editor =
            Editor::from_bytes(pdfcraft_cos::write_full(&blank, &Default::default()).unwrap())
                .unwrap();
        editor
            .image_bytes(0, [40., 80., 550., 800.], "scan.png", &png_bytes, None)
            .unwrap();
        app.editor = Some(Editor::from_bytes((*editor.bytes).clone()).unwrap());
        app.fit = Fit::Manual;
        app.zoom = 0.6;
        let before = app.editor.as_ref().unwrap().bytes.clone();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1080., 720.))),
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
        let p = &app.editor.as_ref().unwrap().info.pages[0];
        let s = app.selection_for(0, Target::Added(0)).unwrap();
        let local = s.view_rect(p);
        let pos = Pos2::new(
            app.canvas.center().x - 9. + (local.center().x - p.width / 2.) * app.zoom,
            app.canvas.top() + local.center().y * app.zoom,
        );
        for (step, pressed) in [true, false, true, false].into_iter().enumerate() {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    time: Some(1. + step as f64 * 0.08),
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| app.frame_ui(ui),
            );
            out.textures_delta.clear();
        }
        let mut session = app
            .recognition
            .take()
            .expect("A double-click on the raster image must launch OCR");
        assert!(
            session.automatic,
            "Double-click must bypass the review dialog"
        );
        for _ in 0..1500 {
            session.poll(&ctx);
            if session.rx.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(session.rx.is_none(), "OCR timed out");
        assert!(session.error.is_none(), "{:?}", session.error);
        assert!(session.text.contains("Ergonatus 123"), "{}", session.text);
        assert!(session.text.contains("française"), "{}", session.text);
        assert!(
            session.blocks.len() >= 2,
            "Each OCR line must keep a separate editable block"
        );
        let mut decoder =
            png::Decoder::new(std::io::Cursor::new(session.background.as_ref().unwrap()))
                .read_info()
                .unwrap();
        let mut data = vec![0; decoder.output_buffer_size().unwrap()];
        let info = decoder.next_frame(&mut data).unwrap();
        if let Ok(recognized) =
            pdfinator::ocr::recognize(&data[..info.buffer_size()], info.width, info.height)
        {
            assert!(
                !recognized.text().contains("Ergonatus"),
                "The raster background must not retain a second copy of the text"
            );
        }
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
        assert!(!app.editor.as_ref().unwrap().can_undo());
        let view = (app.zoom, app.scroll_offset);
        app.recognition = Some(session);
        for _ in 0..3 {
            let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
            out.textures_delta.clear();
        }
        assert!(
            app.recognition.is_none(),
            "The completed OCR must apply automatically"
        );
        assert!(app.jump.is_none(), "OCR must not jump to the page top");
        assert_eq!(app.zoom, view.0);
        assert!((app.scroll_offset - view.1).length() < 0.1);
        assert!(app.workspace == Workspace::Edit);
        assert_eq!(
            pdfcraft_edit::page_images(&app.editor.as_ref().unwrap().doc, 0)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            app.editor
                .as_ref()
                .unwrap()
                .text(0)
                .unwrap()
                .plain_text()
                .matches("Ergonatus 123")
                .count(),
            1
        );
        app.undo_redo(false);
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
        app.undo_redo(true);
        let index = app
            .editor
            .as_ref()
            .unwrap()
            .added(0)
            .iter()
            .position(|c| matches!(c, Content::Text(_)))
            .unwrap();
        let selection = app.selection_for(0, Target::Added(index)).unwrap();
        app.begin_text(&ctx, selection);
        app.inline.as_mut().unwrap().selection.draft = "Texte puis corrigé.".into();
        assert!(app.finish_text());
        let mut reopened =
            Editor::from_bytes((*app.editor.as_ref().unwrap().bytes).clone()).unwrap();
        assert_eq!(
            reopened
                .text(0)
                .unwrap()
                .plain_text()
                .matches("puis corrigé")
                .count(),
            1
        );
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output/19-ocr-windows.pdf");
        reopened.save(&path).unwrap();
        assert!(
            Editor::open(&path)
                .unwrap()
                .text(0)
                .unwrap()
                .plain_text()
                .contains("puis corrigé")
        );
    }
    #[test]
    fn recognition_review_fits_small_windows_and_cancel_preserves_the_document() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = App::from_context(&ctx, None);
        app.set_workspace(&ctx, Workspace::Edit);
        let blank = pdfcraft_create::blank(595., 842., 1).unwrap();
        app.editor = Some(
            Editor::from_bytes(pdfcraft_cos::write_full(&blank, &Default::default()).unwrap())
                .unwrap(),
        );
        let before = app.editor.as_ref().unwrap().bytes.clone();
        let mut selection = Selection::text(
            0,
            Target::Added(0),
            [60., 100., 400., 600.],
            String::new(),
            12.,
            [0.; 3],
            false,
        );
        selection.is_image = true;
        app.recognition = Some(Session {
            automatic: false,
            selection,
            bytes: before.clone(),
            rx: None,
            text: "Reconnaissance à relire.\nUn texte français.".into(),
            blocks: Vec::new(),
            background: None,
            area: Rect::from_min_max(Pos2::new(60., 100.), Pos2::new(400., 600.)),
            texture: Some(ctx.load_texture(
                "review-test",
                egui::ColorImage::new([2, 2], vec![Color32::WHITE; 4]),
                Default::default(),
            )),
            error: Some("Relecture nécessaire. ".repeat(100)),
            queue: Vec::new(),
        });
        for _ in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760., 540.))),
                    ..Default::default()
                },
                |ui| app.frame_ui(ui),
            );
            out.textures_delta.clear();
        }
        let rect = egui::containers::AreaState::load(&ctx, egui::Id::new("recognition-dialog"))
            .unwrap()
            .rect();
        assert!(
            rect.left() >= 0. && rect.top() >= 0. && rect.right() <= 760. && rect.bottom() <= 540.,
            "Recognition dialog is outside the minimum window: {rect:?}"
        );
        let mut out = ctx.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
        assert!(app.recognition.is_none());
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
        assert!(!app.editor.as_ref().unwrap().can_undo());
    }
    #[test]
    fn automatic_ocr_never_discards_text_started_while_its_worker_was_running() {
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        let d = pdfcraft_create::blank(595., 842., 1).unwrap();
        let mut e =
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
        let png = pdfinator::ocr::encode_png(&[255; 16], 2, 2).unwrap();
        e.image_bytes(0, [50., 300., 350., 500.], "scan.png", &png, None)
            .unwrap();
        let text = e.added(0).len();
        e.add_text(
            0,
            AddedText {
                rect: [50., 600., 350., 640.],
                text: "Texte existant".into(),
                size: 14.,
                ..Default::default()
            },
        )
        .unwrap();
        app.editor = Some(e);
        app.workspace = Workspace::Edit;
        app.tool = Tool::Edit;
        let selection = app.selection_for(0, Target::Added(0)).unwrap();
        let mut output = ctx.run_ui(Default::default(), |_| {});
        output.textures_delta.clear();
        app.begin_text(&ctx, app.selection_for(0, Target::Added(text)).unwrap());
        app.inline.as_mut().unwrap().selection.draft = "Saisie en cours à conserver".into();
        let before = app.editor.as_ref().unwrap().bytes.clone();
        app.recognition = Some(Session {
            automatic: true,
            selection,
            bytes: before.clone(),
            rx: None,
            text: "Reconnu".into(),
            blocks: vec![pdfinator::ocr::Block {
                text: "Reconnu".into(),
                rect: [60., 350., 150., 380.],
                size: 14.,
                color: [0.; 3],
                words: vec![],
            }],
            background: Some(png),
            area: Rect::from_min_max(Pos2::new(50., 342.), Pos2::new(350., 542.)),
            texture: None,
            error: None,
            queue: vec![],
        });
        app.recognition_dialog(&ctx);
        assert!(app.recognition.is_none());
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
        assert_eq!(
            app.inline.as_ref().unwrap().selection.draft,
            "Saisie en cours à conserver"
        );
    }

    #[test]
    fn recognition_starts_only_in_edit_workspace_with_the_text_editing_tool() {
        let ctx = egui::Context::default();
        let mut app = App::from_context(&ctx, None);
        let blank = pdfcraft_create::blank(595., 842., 1).unwrap();
        app.editor = Some(
            Editor::from_bytes(pdfcraft_cos::write_full(&blank, &Default::default()).unwrap())
                .unwrap(),
        );
        let before = app.editor.as_ref().unwrap().bytes.clone();
        let mut selection = Selection::text(
            0,
            Target::Added(0),
            [60., 100., 400., 600.],
            String::new(),
            12.,
            [0.; 3],
            false,
        );
        selection.is_image = true;
        for workspace in [Workspace::Read, Workspace::Pages, Workspace::Edit] {
            for tool in [
                Tool::Select,
                Tool::Hand,
                Tool::Move,
                Tool::Edit,
                Tool::Image,
                Tool::Text,
                Tool::Highlight,
                Tool::Comment,
            ] {
                if workspace == Workspace::Edit && tool == Tool::Edit {
                    continue;
                }
                app.workspace = workspace;
                app.tool = tool;
                app.message = None;
                app.start_recognition(&ctx, selection.clone());
                assert!(app.recognition.is_none());
                assert!(
                    app.message.is_none(),
                    "Other tools must not even attempt recognition"
                );
                assert_eq!(app.editor.as_ref().unwrap().bytes, before);
            }
        }
    }
}
pub(super) struct Session {
    automatic: bool,
    selection: Selection,
    bytes: Arc<Vec<u8>>,
    rx: Option<Receiver<Result<Detected>>>,
    text: String,
    blocks: Vec<pdfinator::ocr::Block>,
    background: Option<Vec<u8>>,
    area: Rect,
    texture: Option<egui::TextureHandle>,
    error: Option<String>,
    queue: Vec<[f32; 4]>,
}
fn recognize(
    bytes: Arc<Vec<u8>>,
    page: usize,
    geometry: PageInfo,
    area: Rect,
    retained: &[[f32; 4]],
) -> Result<Detected> {
    let area = area.intersect(Rect::from_min_size(
        Pos2::ZERO,
        Vec2::new(geometry.width, geometry.height),
    ));
    if area.width() < 2. || area.height() < 2. {
        return Err("L’image est trop petite ou se trouve hors de la page.".into());
    }
    let scale = 3_f32
        .min(4094. / area.width().max(area.height()))
        .min(8000. / geometry.width.max(geometry.height));
    if !scale.is_finite() || scale <= 0. {
        return Err("Dimensions de page non prises en charge.".into());
    }
    let x = (area.left() * scale).floor().max(0.) as u32;
    let y = (area.top() * scale).floor().max(0.) as u32;
    let width = ((area.right() * scale).ceil() as u32)
        .saturating_sub(x)
        .clamp(1, 4096);
    let height = ((area.bottom() * scale).ceil() as u32)
        .saturating_sub(y)
        .clamp(1, 4096);
    let mut renderer = pdfcraft_render::PageRenderer::new(bytes, Default::default());
    let image = renderer.render(pdfcraft_render::RenderRequest {
        page,
        scale,
        tile: Some(pdfcraft_render::Tile {
            x,
            y,
            w: width,
            h: height,
        }),
        ..Default::default()
    });
    if let Some(error) = image.error {
        return Err(error);
    }
    let mut result = pdfinator::ocr::recognize(&image.rgba, image.width, image.height)?;
    // The image is isolated from native PDF text. Do not resurrect old raster
    // labels hidden under a native correction drawn on top of it.
    for line in &mut result.lines {
        if line.words.is_empty() {
            line.words.push(pdfinator::ocr::Word {
                text: line.text.clone(),
                rect: line.rect,
            });
        }
        line.words.retain(|word| {
            let r = Rect::from_min_max(
                Pos2::new(
                    (x as f32 + word.rect[0]) / scale,
                    (y as f32 + word.rect[1]) / scale,
                ),
                Pos2::new(
                    (x as f32 + word.rect[2]) / scale,
                    (y as f32 + word.rect[3]) / scale,
                ),
            );
            !retained.iter().any(|v| {
                let existing =
                    Rect::from_min_max(Pos2::new(v[0], v[1]), Pos2::new(v[2], v[3])).expand(1.);
                let overlap = r.intersect(existing);
                overlap.is_positive() && overlap.area() >= r.area() * 0.25
            })
        });
        line.text = line
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
    }
    result.lines.retain(|line| !line.words.is_empty());
    let mut blocks = result.blocks();
    let cleaned =
        pdfinator::ocr::erase_letters(&image.rgba, image.width, image.height, &mut blocks)?;
    let png = pdfinator::ocr::encode_png(&cleaned, image.width, image.height)?;
    Ok(Detected {
        result,
        blocks,
        cleaned,
        png,
        width: image.width,
        height: image.height,
        scale,
        origin: Pos2::new(x as f32 / scale, y as f32 / scale),
    })
}
impl Session {
    fn poll(&mut self, ctx: &egui::Context) {
        let Some(result) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) else {
            return;
        };
        self.rx = None;
        match result {
            Ok(detected) => {
                self.text = detected.result.text();
                self.blocks = detected.blocks;
                for block in &mut self.blocks {
                    block.rect = [
                        detected.origin.x + block.rect[0] / detected.scale,
                        detected.origin.y + block.rect[1] / detected.scale,
                        detected.origin.x + block.rect[2] / detected.scale,
                        detected.origin.y + block.rect[3] / detected.scale,
                    ];
                    block.size = (block.size / detected.scale as f64).clamp(3., 96.);
                    let measured = Family::Helvetica
                        .width(&block.text, block.size, false, false)
                        .max(1.);
                    let available = (block.rect[2] - block.rect[0] + 3.) as f64;
                    block.size = block.size.min(block.size * available / measured).max(3.);
                }
                self.area = Rect::from_min_size(
                    detected.origin,
                    Vec2::new(detected.width as f32, detected.height as f32) / detected.scale,
                );
                self.background = Some(detected.png);
                self.texture = Some(ctx.load_texture(
                    "recognized-image",
                    preview(&detected.cleaned, detected.width, detected.height),
                    egui::TextureOptions::LINEAR,
                ));
            }
            Err(error) => self.error = Some(error),
        }
    }
}
impl App {
    pub(super) fn start_automatic_recognition(
        &mut self,
        ctx: &egui::Context,
        selection: Selection,
    ) {
        if self.recognition.is_some() {
            return;
        }
        self.start_recognition(ctx, selection);
        if let Some(session) = &mut self.recognition {
            session.automatic = true;
        }
    }
    fn image_selections(&self, page: usize) -> Vec<Selection> {
        self.editor
            .as_ref()
            .and_then(|e| pdfcraft_edit::editable_objects(&e.doc, page).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|o| o.content_type == "image")
            .filter_map(|o| {
                self.selection_for(
                    page,
                    match o.target.kind {
                        ObjectKind::Added => Target::Added(o.target.index),
                        ObjectKind::Image => Target::Image(o.target.index),
                        ObjectKind::Text => return None,
                    },
                )
            })
            .collect()
    }
    pub(super) fn start_page_recognition(&mut self, ctx: &egui::Context, page: usize) {
        if self.workspace != Workspace::Edit
            || self.tool != Tool::Edit
            || self.recognition.is_some()
        {
            return;
        }
        let Some(p) = self.editor.as_ref().and_then(|e| e.info.pages.get(page)) else {
            return;
        };
        let mut areas = self
            .image_selections(page)
            .iter()
            .map(|s| s.view_rect(p))
            .collect::<Vec<_>>();
        areas.sort_by(|a, b| {
            a.top()
                .total_cmp(&b.top())
                .then_with(|| a.left().total_cmp(&b.left()))
        });
        let queue = areas
            .into_iter()
            .map(|r| [r.left(), r.top(), r.right(), r.bottom()])
            .collect();
        self.next_page_recognition(ctx, page, queue);
    }
    fn next_page_recognition(
        &mut self,
        ctx: &egui::Context,
        page: usize,
        mut queue: Vec<[f32; 4]>,
    ) {
        while !queue.is_empty() {
            let wanted = queue.remove(0);
            let Some(p) = self.editor.as_ref().and_then(|e| e.info.pages.get(page)) else {
                return;
            };
            let selection = self.image_selections(page).into_iter().find(|s| {
                let r = s.view_rect(p);
                [r.left(), r.top(), r.right(), r.bottom()]
                    .iter()
                    .zip(wanted)
                    .all(|(a, b)| (*a - b).abs() < 0.2)
            });
            let Some(selection) = selection else {
                continue;
            };
            // Resolve current image indexes after every conversion; earlier
            // conversions remove original streams and append their text blocks.
            let source = match selection.target {
                Target::Image(i) => pdfinator::ocr::ImageSource::Original(i),
                Target::Added(i) => pdfinator::ocr::ImageSource::Added(i),
                _ => continue,
            };
            if self
                .editor
                .as_ref()
                .is_none_or(|e| e.image_for_ocr(page, source).is_err())
            {
                continue;
            }
            self.start_recognition(ctx, selection);
            if let Some(session) = &mut self.recognition {
                session.queue = queue;
                return;
            }
        }
    }
    pub(super) fn start_recognition(&mut self, ctx: &egui::Context, selection: Selection) {
        if self.workspace != Workspace::Edit
            || self.tool != Tool::Edit
            || self.recognition.is_some()
            || !selection.is_image
            || !self.deselect()
            || !self.permit()
        {
            return;
        }
        let source = match selection.target {
            Target::Image(index) => pdfinator::ocr::ImageSource::Original(index),
            Target::Added(index) => pdfinator::ocr::ImageSource::Added(index),
            _ => return,
        };
        let Some(e) = &self.editor else { return };
        let Some(geometry) = e.info.pages.get(selection.page).cloned() else {
            return;
        };
        let (isolated, area) = match e.image_for_ocr(selection.page, source) {
            Ok(result) => result,
            Err(error) => {
                self.message = Some(error);
                return;
            }
        };
        let retained = match e.image_ocr_retained_text(selection.page, area) {
            Ok(retained) => retained,
            Err(error) => {
                self.message = Some(error);
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let page = selection.page;
        self.recognition = Some(Session {
            automatic: false,
            selection,
            bytes: e.bytes.clone(),
            rx: Some(rx),
            text: String::new(),
            blocks: Vec::new(),
            background: None,
            area: Rect::from_min_max(Pos2::new(area[0], area[1]), Pos2::new(area[2], area[3])),
            texture: None,
            error: None,
            queue: Vec::new(),
        });
        self.message = None;
        std::thread::spawn(move || {
            let result = pdfinator::document::guarded(|| {
                recognize(
                    isolated,
                    page,
                    geometry,
                    Rect::from_min_max(Pos2::new(area[0], area[1]), Pos2::new(area[2], area[3])),
                    &retained,
                )
            });
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub(super) fn recognition_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut session) = self.recognition.take() else {
            return;
        };
        if self
            .editor
            .as_ref()
            .is_none_or(|e| !Arc::ptr_eq(&e.bytes, &session.bytes))
        {
            self.message = Some(
                "Le document a changé. Relancez la reconnaissance sur l’image souhaitée.".into(),
            );
            return;
        }
        session.poll(ctx);
        if session.automatic {
            // No modal and no validation step for the double-click gesture.
            // A tool/document change cancels the pending operation instead of
            // bringing an old editor back into focus when its worker finishes.
            if self.workspace != Workspace::Edit
                || self.tool != Tool::Edit
                || self.selected.is_some()
                || self.inline.as_ref().is_some_and(|i| !i.committed)
                || self.drag.is_some()
                || !self.marquee.selected.is_empty()
                || !self.drawing.selected.is_empty()
            {
                return;
            }
            if session.rx.is_none() {
                if let Some(error) = session.error {
                    self.message = Some(error);
                } else if session.blocks.is_empty() {
                    // All detected words already belong to native PDF labels.
                    // Keep the image selected without a warning or a no-op edit.
                    self.selected = Some(session.selection);
                } else if let Err(error) = self.apply_recognition(&session) {
                    self.message = Some(error);
                }
                return;
            }
            let mut cancel = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
            egui::Area::new(egui::Id::new("recognition-progress"))
                .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-18., -18.))
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Reconnaissance du texte…");
                            cancel |= ui.button("Annuler").clicked();
                        });
                    });
                });
            if !cancel {
                self.recognition = Some(session);
                ctx.request_repaint_after(std::time::Duration::from_millis(40));
            }
            return;
        }
        let mut convert = false;
        let mut skip = false;
        let mut cancel = false;
        let dialog = egui::Modal::new(egui::Id::new("recognition-dialog"))
            .frame(
                egui::Frame::popup(&ctx.style_of(egui::Theme::Light))
                    .corner_radius(14.)
                    .inner_margin(20.),
            )
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 88.).clamp(340., 640.));
                egui::ScrollArea::vertical()
                    .id_salt("recognition-review")
                    .max_height((ctx.content_rect().height() - 190.).max(100.))
                    .show(ui, |ui| {
                        if session.rx.is_some() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Reconnaissance de l’image, hors ligne…");
                            });
                            ctx.request_repaint_after(std::time::Duration::from_millis(40));
                        } else if session.texture.is_none() {
                            if let Some(error) = &session.error {
                                ui.add(egui::Label::new(error).wrap());
                            }
                        } else {
                            ui.columns(2, |columns| {
                                if let Some(texture) = &session.texture {
                                    let scale = (columns[0].available_width()
                                        / session.area.width())
                                    .min(270. / session.area.height());
                                    let (r, _) = columns[0].allocate_exact_size(
                                        session.area.size() * scale,
                                        egui::Sense::hover(),
                                    );
                                    columns[0].painter().image(
                                        texture.id(),
                                        r,
                                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                        Color32::WHITE,
                                    );
                                    for (index, block) in session.blocks.iter().enumerate() {
                                        let rect = Rect::from_min_max(
                                            r.min
                                                + (Pos2::new(block.rect[0], block.rect[1])
                                                    - session.area.min)
                                                    * scale,
                                            r.min
                                                + (Pos2::new(block.rect[2], block.rect[3])
                                                    - session.area.min)
                                                    * scale,
                                        );
                                        let color = Color32::from_rgb(
                                            (block.color[0] * 255.) as u8,
                                            (block.color[1] * 255.) as u8,
                                            (block.color[2] * 255.) as u8,
                                        );
                                        columns[0].painter().text(
                                            rect.left_top(),
                                            egui::Align2::LEFT_TOP,
                                            &block.text,
                                            egui::FontId::proportional(
                                                (block.size as f32 * scale).max(3.),
                                            ),
                                            color,
                                        );
                                        columns[0].painter().rect_stroke(
                                            rect.expand(1.),
                                            1.,
                                            Stroke::new(0.7, icons::BLUE),
                                            StrokeKind::Outside,
                                        );
                                        columns[0].painter().text(
                                            rect.left_top() - Vec2::new(2., 9.),
                                            egui::Align2::LEFT_TOP,
                                            index + 1,
                                            egui::FontId::proportional(8.),
                                            icons::BLUE,
                                        );
                                    }
                                }
                                egui::ScrollArea::vertical()
                                    .id_salt("recognized-blocks")
                                    .max_height(270.)
                                    .show(&mut columns[1], |ui| {
                                        for (index, block) in session.blocks.iter_mut().enumerate()
                                        {
                                            ui.push_id(index, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(format!("{}", index + 1));
                                                    ui.add_sized(
                                                        [ui.available_width() - 48., 26.],
                                                        egui::TextEdit::singleline(&mut block.text),
                                                    );
                                                });
                                                ui.horizontal(|ui| {
                                                    ui.label("Taille");
                                                    ui.add(
                                                        egui::DragValue::new(&mut block.size)
                                                            .range(3.0..=96.)
                                                            .suffix(" pt"),
                                                    );
                                                });
                                                ui.add_space(4.);
                                            });
                                        }
                                    });
                            });
                            session.text = session
                                .blocks
                                .iter()
                                .map(|b| b.text.as_str())
                                .collect::<Vec<_>>()
                                .join("\n");
                            if let Some(error) = &session.error {
                                ui.add_space(8.);
                                ui.add(egui::Label::new(error).wrap());
                            }
                        }
                    });
                ui.add_space(10.);
                ui.separator();
                ui.horizontal(|ui| {
                    cancel = ui
                        .add_sized([90., 34.], egui::Button::new("Annuler"))
                        .clicked();
                    if !session.queue.is_empty() {
                        skip = ui
                            .add_enabled(
                                session.rx.is_none(),
                                egui::Button::new("Image suivante").min_size(Vec2::new(110., 34.)),
                            )
                            .clicked();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        convert = ui
                            .add_enabled(
                                session.rx.is_none()
                                    && session.texture.is_some()
                                    && !session.text.trim().is_empty(),
                                egui::Button::new(
                                    egui::RichText::new(if session.queue.is_empty() {
                                        "Créer les blocs modifiables"
                                    } else {
                                        "Valider et continuer"
                                    })
                                    .color(Color32::WHITE),
                                )
                                .fill(icons::BLUE)
                                .corner_radius(7.)
                                .min_size(Vec2::new(210., 34.)),
                            )
                            .clicked();
                    });
                });
            });
        if cancel || dialog.should_close() {
            return;
        }
        if skip {
            self.next_page_recognition(ctx, session.selection.page, session.queue);
            return;
        }
        if convert {
            if let Err(error) = self.apply_recognition(&session) {
                session.error = Some(error);
                self.recognition = Some(session);
            } else if !session.queue.is_empty() {
                self.next_page_recognition(ctx, session.selection.page, session.queue);
            }
        } else {
            self.recognition = Some(session);
        }
    }
    fn apply_recognition(&mut self, session: &Session) -> Result<()> {
        let e = self.editor.as_mut().ok_or("Aucun document.")?;
        let page = session.selection.page;
        let geometry = e.info.pages.get(page).ok_or("Page introuvable.")?;
        let source = match session.selection.target {
            Target::Image(index) => pdfinator::ocr::ImageSource::Original(index),
            Target::Added(index) => pdfinator::ocr::ImageSource::Added(index),
            _ => return Err("Cette sélection n’est pas une image.".into()),
        };
        let to_rect = |r: [f32; 4]| {
            [
                r[0] as f64,
                (geometry.height - r[3]) as f64,
                r[2] as f64,
                (geometry.height - r[1]) as f64,
            ]
        };
        let texts = session
            .blocks
            .iter()
            .filter(|b| !b.text.trim().is_empty())
            .map(|block| AddedText {
                rect: to_rect([
                    block.rect[0] - 0.5,
                    block.rect[1] - 1.,
                    block.rect[2] + 4.,
                    block.rect[3] + 1.,
                ]),
                text: block.text.clone(),
                size: block.size,
                color: block.color,
                ..Default::default()
            })
            .collect();
        let indices = e.image_to_blocks(
            page,
            source,
            session.background.as_deref().ok_or("Aperçu incomplet.")?,
            to_rect([
                session.area.left(),
                session.area.top(),
                session.area.right(),
                session.area.bottom(),
            ]),
            texts,
        )?;
        self.refresh(page);
        self.workspace = Workspace::Edit;
        self.tool = Tool::Edit;
        self.selected = self.selection_for(
            page,
            Target::Added(*indices.first().ok_or("Aucun bloc reconnu.")?),
        );
        self.inline = None;
        Ok(())
    }
}
