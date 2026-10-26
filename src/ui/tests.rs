use super::*;
fn input_frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> egui::FullOutput {
    let mut out = ctx.run_ui(
        egui::RawInput {
            events,
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    out
}
fn pointer_button(pos: Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}
fn drag_pointer(ctx: &egui::Context, app: &mut App, start: Pos2, end: Pos2) {
    input_frame(
        ctx,
        app,
        vec![
            egui::Event::PointerMoved(start),
            pointer_button(start, egui::PointerButton::Primary, true),
        ],
    );
    input_frame(ctx, app, vec![egui::Event::PointerMoved(end)]);
    input_frame(
        ctx,
        app,
        vec![pointer_button(end, egui::PointerButton::Primary, false)],
    );
}
fn sheet_origin(app: &App) -> Pos2 {
    let p = &app.editor.as_ref().unwrap().info.pages[app.page];
    Pos2::new(
        app.canvas.left() + ((app.canvas.width() - 18. - p.width * app.zoom) / 2.).max(0.)
            - app.scroll_offset.x,
        app.canvas.top() - app.scroll_offset.y,
    )
}
#[test]
fn reading_cursor_is_an_ibeam_over_text_and_an_arrow_over_blank_page_and_margin() {
    let (ctx, mut app) = setup();
    app.set_workspace(&ctx, Workspace::Read);
    app.set_tool(&ctx, Tool::Select);
    input_frame(&ctx, &mut app, vec![]);
    let text = app.editor.as_ref().unwrap().text(0).unwrap();
    let glyph = text
        .glyphs
        .iter()
        .find(|g| !g.text.trim().is_empty())
        .unwrap();
    let at = sheet_origin(&app)
        + Vec2::new(
            (glyph.rect[0] + glyph.rect[2]) * 0.5,
            (glyph.rect[1] + glyph.rect[3]) * 0.5,
        ) * app.zoom;
    for (pos, cursor) in [
        (at, egui::CursorIcon::Text),
        (
            sheet_origin(&app) + Vec2::new(15., 200.) * app.zoom,
            egui::CursorIcon::Default,
        ),
        (
            app.canvas.min + Vec2::new(1., 2.),
            egui::CursorIcon::Default,
        ),
    ] {
        let out = input_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(pos)]);
        assert_eq!(out.platform_output.cursor_icon, cursor);
    }
}
#[test]
fn a_selected_block_has_an_open_hand_then_a_closed_hand_while_dragging() {
    let (ctx, mut app) = setup();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Select);
    input_frame(&ctx, &mut app, vec![]);
    let p = app.editor.as_ref().unwrap().info.pages[0].clone();
    let block = app
        .selection_for(0, Target::Block(0))
        .unwrap()
        .view_rect(&p);
    let origin = sheet_origin(&app);
    let zoom = app.zoom;
    drag_pointer(
        &ctx,
        &mut app,
        origin + (block.min.to_vec2() - Vec2::splat(8.)) * zoom,
        origin + (block.max.to_vec2() + Vec2::splat(8.)) * zoom,
    );
    input_frame(&ctx, &mut app, vec![]);
    let center = sheet_origin(&app) + block.center().to_vec2() * app.zoom;
    let out = input_frame(&ctx, &mut app, vec![egui::Event::PointerMoved(center)]);
    assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::Grab);
    let out = input_frame(
        &ctx,
        &mut app,
        vec![pointer_button(center, egui::PointerButton::Primary, true)],
    );
    assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::Grabbing);
    input_frame(
        &ctx,
        &mut app,
        vec![pointer_button(center, egui::PointerButton::Primary, false)],
    );
}
#[test]
fn diagonal_polygon_is_selected_as_a_line_then_dragged_resized_and_restyled() {
    use pdfinator::drawing::{Kind, StrokeStyle};
    for tool in [Tool::Edit, Tool::Move, Tool::Draw] {
        let (ctx, mut app) = setup();
        ctx.enable_accesskit();
        let e = if let Some(path) = std::env::var_os("PDFINATOR_OBLIQUE_PDF") {
            Editor::open(std::path::Path::new(&path)).unwrap()
        } else {
            let mut e = app.editor.take().unwrap();
            let p = pdfcraft_model::pages(&e.doc).remove(0);
            let old = p.dict.get(b"Contents").unwrap().clone();
            let raw=b"q 20 400 160 150 re W n 1 1 1 rg 21 401 158 148 re f 0 g 40 449.7 m 140 499.7 l 140.3 500.3 l 40.3 450.3 l h f Q";
            let s = e
                .doc
                .add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::flate(
                    pdfcraft_cos::Dict::new(),
                    raw,
                )));
            e.doc
                .update_dict(p.obj, |d| {
                    d.set(
                        b"Contents".to_vec(),
                        pdfcraft_cos::Object::Array(vec![old, pdfcraft_cos::Object::Ref(s)]),
                    )
                })
                .unwrap();
            Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap())
                .unwrap()
        };
        let before = e.bytes.clone();
        let line = e
            .drawings(0)
            .unwrap()
            .into_iter()
            .find(|i| {
                i.drawing.kind == Kind::Line
                    && i.drawing.u[0].abs() > 20.
                    && i.drawing.u[1].abs() > 20.
            })
            .unwrap();
        app.editor = Some(e);
        app.raster
            .document(app.editor.as_ref().unwrap().bytes.clone(), true, None);
        app.set_workspace(&ctx, Workspace::Edit);
        app.set_tool(&ctx, tool);
        app.drawing.mode = None;
        app.fit = Fit::Manual;
        app.zoom = 0.6;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1080., 1100.))),
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
        for _ in 0..3 {
            input_frame(&ctx, &mut app, vec![]);
        }
        let p = app.editor.as_ref().unwrap().info.pages[0].clone();
        let points = line.drawing.view_points(&p);
        let mid = (Pos2::new(points[0][0], points[0][1])
            + Pos2::new(points[1][0], points[1][1]).to_vec2())
            * 0.5;
        let pos = sheet_origin(&app) + mid.to_vec2() * app.zoom;
        input_frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                pointer_button(pos, egui::PointerButton::Primary, true),
            ],
        );
        input_frame(
            &ctx,
            &mut app,
            vec![pointer_button(pos, egui::PointerButton::Primary, false)],
        );
        for _ in 0..3 {
            input_frame(&ctx, &mut app, vec![]);
        }
        assert_eq!(app.drawing.selected.len(), 1, "{tool:?}: {:?}", app.message);
        assert_eq!(
            app.drawing.selected[0].drawing.kind,
            Kind::Line,
            "The oblique line must win over its white background"
        );
        let pos = sheet_origin(&app) + mid.to_vec2() * app.zoom;
        drag_pointer(&ctx, &mut app, pos, pos + Vec2::new(80., 50.));
        for _ in 0..200 {
            input_frame(&ctx, &mut app, vec![]);
            if app.drawing.drag.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.message.is_none(), "{tool:?}: {:?}", app.message);
        assert_eq!(app.drawing.selected.len(), 1);
        let moved = app.drawing.selected[0].drawing.clone();
        assert!((moved.a[0] - line.drawing.a[0]).abs() > 50.);
        let endpoint = moved.view_points(&p)[1];
        let end = sheet_origin(&app) + Vec2::new(endpoint[0], endpoint[1]) * app.zoom;
        drag_pointer(&ctx, &mut app, end, end + Vec2::new(25., -20.));
        for _ in 0..200 {
            input_frame(&ctx, &mut app, vec![]);
            if app.drawing.drag.is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.message.is_none(), "{:?}", app.message);
        let updated = app.drawing.selected.clone();
        assert_ne!(updated[0].drawing.u, moved.u);
        let click = |app: &mut App, label: &str| {
            let out = input_frame(&ctx, app, vec![]);
            let b = out
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some(label))
                .unwrap_or_else(|| panic!("Missing {label}"))
                .1
                .bounds()
                .unwrap();
            let pos = Pos2::new((b.x0 + b.x1) as f32 / 2., (b.y0 + b.y1) as f32 / 2.);
            input_frame(
                &ctx,
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    pointer_button(pos, egui::PointerButton::Primary, true),
                ],
            );
            input_frame(
                &ctx,
                app,
                vec![pointer_button(pos, egui::PointerButton::Primary, false)],
            );
        };
        click(&mut app, "Choisir une épaisseur");
        click(&mut app, "4,5 pt");
        click(&mut app, "Choisir un type de trait");
        click(&mut app, "Pointillés");
        click(&mut app, "Rouge");
        assert!(app.message.is_none(), "{:?}", app.message);
        assert!(
            app.editor
                .as_ref()
                .unwrap()
                .drawings(0)
                .unwrap()
                .iter()
                .any(|i| (i.drawing.width - 4.5).abs() < 0.01
                    && i.drawing.style == StrokeStyle::Dotted
                    && (i.drawing.color[0] - 0.8).abs() < 0.001)
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("test-output/46-interface-oblique-{tool:?}.pdf"));
        app.editor.as_mut().unwrap().save(&path).unwrap();
        for _ in 0..5 {
            app.undo_redo(false);
        }
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    }
}
#[test]
fn block_border_and_corner_open_clipboard_menu_and_delete_removes_source_with_undo() {
    for corner in [false, true] {
        let (ctx, mut app) = setup();
        ctx.enable_accesskit();
        app.set_workspace(&ctx, Workspace::Edit);
        app.fit = Fit::Manual;
        app.zoom = 0.7;
        let before = app.editor.as_ref().unwrap().bytes.clone();
        app.begin_text(&ctx, app.selection_for(0, Target::Block(0)).unwrap());
        for _ in 0..100 {
            input_frame(&ctx, &mut app, vec![]);
            if app
                .inline
                .as_ref()
                .is_some_and(|i| i.screen.is_finite() && i.screen.is_positive())
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let bounds = app.inline.as_ref().unwrap().screen;
        assert!(
            bounds.is_finite() && bounds.is_positive(),
            "Inline not visible: {bounds:?}, canvas {:?}",
            app.canvas
        );
        let pos = if corner {
            bounds.right_bottom()
        } else {
            bounds.center_top()
        };
        input_frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                pointer_button(pos, egui::PointerButton::Secondary, true),
            ],
        );
        input_frame(
            &ctx,
            &mut app,
            vec![pointer_button(pos, egui::PointerButton::Secondary, false)],
        );
        let out = input_frame(&ctx, &mut app, vec![]);
        let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
        for name in [
            "Copier Ctrl+C",
            "Couper Ctrl+X",
            "Coller Ctrl+V",
            "Supprimer Suppr",
        ] {
            assert!(
                nodes.iter().any(|(_, node)| node.label() == Some(name)),
                "Missing {name}"
            );
        }
        assert!(
            app.drag.is_none(),
            "A right click must never start dragging"
        );
        click_named_control(&ctx, &mut app, "Copier Ctrl+C");
        assert!(app.copied_block.is_some());
        // Selecting the border leaves character editing, so Delete removes the block.
        let out = input_frame(
            &ctx,
            &mut app,
            vec![egui::Event::Key {
                key: Key::Delete,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        drop(out);
        let e = app.editor.as_mut().unwrap();
        assert!(
            e.text(0).unwrap().plain_text().trim().is_empty(),
            "{:?}",
            app.message
        );
        assert!(app.selected.is_none());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test-output/26-bloc-supprime.pdf");
        e.save(&path).unwrap();
        assert!(
            Editor::open(&path)
                .unwrap()
                .text(0)
                .unwrap()
                .plain_text()
                .trim()
                .is_empty()
        );
        e.undo().unwrap();
        assert_eq!(e.bytes, before);
    }
}
#[test]
fn context_menu_cut_and_paste_keep_an_editable_block() {
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    app.set_workspace(&ctx, Workspace::Edit);
    app.selected = app.selection_for(0, Target::Block(0));
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    let s = app.selected.as_ref().unwrap();
    let r = s.view_rect(&app.editor.as_ref().unwrap().info.pages[0]);
    let pos = sheet_origin(&app) + r.center_top().to_vec2() * app.zoom;
    input_frame(
        &ctx,
        &mut app,
        vec![
            egui::Event::PointerMoved(pos),
            pointer_button(pos, egui::PointerButton::Secondary, true),
        ],
    );
    input_frame(
        &ctx,
        &mut app,
        vec![pointer_button(pos, egui::PointerButton::Secondary, false)],
    );
    click_named_control(&ctx, &mut app, "Couper Ctrl+X");
    assert!(
        app.editor
            .as_ref()
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .trim()
            .is_empty()
    );
    app.paste_copied_block();
    assert!(app.selected.is_some());
    assert_eq!(
        app.editor
            .as_ref()
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .matches("original")
            .count(),
        1
    );
    let s = app.selected.clone().unwrap();
    app.begin_text(&ctx, s);
    let selection = &mut app.inline.as_mut().unwrap().selection;
    let replacement = "Bloc collé puis modifié.";
    pdfinator::rich::remap(&mut selection.rich, &selection.draft, replacement);
    selection.draft = replacement.into();
    assert!(app.finish_text(), "{:?}", app.message);
    assert!(
        app.editor
            .as_ref()
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .contains("puis modifié")
    );
}
#[test]
fn new_text_box_can_be_drawn_moved_and_resized_while_empty_then_typed_and_saved() {
    let (ctx, mut app) = setup();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Text);
    app.fit = Fit::Manual;
    app.zoom = 0.6;
    input_frame(&ctx, &mut app, vec![]);
    let before = app.editor.as_ref().unwrap().bytes.clone();
    let origin = sheet_origin(&app);
    let zoom = app.zoom;
    drag_pointer(
        &ctx,
        &mut app,
        origin + Vec2::new(80., 300.) * zoom,
        origin + Vec2::new(380., 450.) * zoom,
    );
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    assert!(app.finish_text());
    let s = app
        .selected
        .clone()
        .expect("The empty frame must survive leaving text input");
    assert!(matches!(s.target, Target::New));
    assert!(s.draft.is_empty());
    let p = app.editor.as_ref().unwrap().info.pages[0].clone();
    let original = s.view_rect(&p);
    assert!((original.width() - 300.).abs() < 1., "{original:?}");
    assert!((original.height() - 150.).abs() < 1., "{original:?}");
    let edge = sheet_origin(&app) + original.center_top().to_vec2() * app.zoom;
    drag_pointer(&ctx, &mut app, edge, edge + Vec2::new(30., 18.));
    let moved = app.selected.as_ref().unwrap().view_rect(&p);
    assert!(
        (moved.min - original.min - Vec2::new(50., 30.)).length() < 1.,
        "{original:?} -> {moved:?}"
    );
    let corner = sheet_origin(&app) + moved.right_bottom().to_vec2() * app.zoom;
    drag_pointer(&ctx, &mut app, corner, corner + Vec2::new(24., 12.));
    let resized = app.selected.as_ref().unwrap().view_rect(&p);
    assert!((resized.size() - moved.size() - Vec2::new(40., 20.)).length() < 1.);
    assert_eq!(
        app.editor.as_ref().unwrap().bytes,
        before,
        "An empty draft must not alter the PDF"
    );
    assert!(!app.editor.as_ref().unwrap().dirty());
    let selected = app.selected.clone().unwrap();
    app.begin_text(&ctx, selected);
    app.inline.as_mut().unwrap().selection.draft = "Été français : œ, ç et à.".into();
    assert!(app.finish_text(), "{:?}", app.message);
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output/26-cadre-trace.pdf");
    app.editor.as_mut().unwrap().save(&path).unwrap();
    let reopened = Editor::open(&path).unwrap();
    assert!(
        reopened
            .text(0)
            .unwrap()
            .plain_text()
            .contains("Été français")
    );
    assert_eq!(
        reopened
            .text(0)
            .unwrap()
            .plain_text()
            .matches("français")
            .count(),
        1
    );
}
fn setup() -> (egui::Context, App) {
    let ctx = egui::Context::default();
    let mut app = App::from_context(&ctx, None);
    let d =
        pdfcraft_create::from_text("UI", "Texte original à corriger.", (595., 842.), 14.).unwrap();
    let bytes = pdfcraft_cos::write_full(&d, &pdfcraft_cos::SaveOptions::default()).unwrap();
    app.editor = Some(Editor::from_bytes(bytes).unwrap());
    app.raster
        .document(app.editor.as_ref().unwrap().bytes.clone(), true, None);
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1080., 720.))),
            ..Default::default()
        },
        |ui| {
            app.frame_ui(ui);
        },
    );
    output.textures_delta.clear();
    (ctx, app)
}
#[test]
fn selection_rectangle_moves_text_grid_arrowheads_and_image_together_at_three_zooms() {
    use pdfinator::selection::Target as Mixed;
    for (zoom, tool) in [0.5, 0.8, 1.4]
        .into_iter()
        .flat_map(|zoom| [Tool::Select, Tool::Edit].map(|tool| (zoom, tool)))
    {
        let (ctx, mut app) = setup();
        let editor = if let Some(path) = std::env::var_os("PDFINATOR_REGRESSION_PDF") {
            Editor::open(std::path::Path::new(&path)).unwrap()
        } else {
            let d = pdfcraft_create::from_text("Graphique", "A B C", (595., 842.), 12.).unwrap();
            let mut e =
                Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap())
                    .unwrap();
            let b = e.blocks(0).unwrap().remove(0);
            e.move_block(0, 0, [360. - b.rect[0], 610. - b.rect[1]])
                .unwrap();
            let p = pdfcraft_model::pages(&e.doc)[0].clone();
            let old = p.dict.get(b"Contents").unwrap().clone();
            let mut raw = String::from("q 325 547 210 125 re W n 0.6 G 0.5 w\n");
            for x in (325..=535).step_by(15) {
                raw.push_str(&format!("{x} 547 m {x} 672 l S\n"));
            }
            for y in (547..=672).step_by(15) {
                raw.push_str(&format!("325 {y} m 535 {y} l S\n"));
            }
            raw.push_str("Q 0 g 425 680 m 421 671 l 429 671 l h f\n");
            let r = e
                .doc
                .add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::flate(
                    pdfcraft_cos::Dict::new(),
                    raw.as_bytes(),
                )));
            e.doc
                .update_dict(p.obj, |d| {
                    d.set(
                        b"Contents".to_vec(),
                        pdfcraft_cos::Object::Array(vec![old, pdfcraft_cos::Object::Ref(r)]),
                    )
                })
                .unwrap();
            Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap())
                .unwrap()
        };
        let original = editor.bytes.clone();
        let text = (0..editor.info.pages.len())
            .map(|p| editor.text(p).unwrap().plain_text())
            .collect::<Vec<_>>();
        app.editor = Some(editor);
        app.raster.document(original.clone(), true, None);
        app.set_workspace(&ctx, Workspace::Edit);
        app.set_tool(&ctx, tool);
        app.fit = Fit::Manual;
        app.zoom = zoom;
        app.thumbnails = false;
        for _ in 0..4 {
            input_frame(&ctx, &mut app, vec![]);
        }
        let screen = |app: &App, user: [f32; 2]| {
            let p = &app.editor.as_ref().unwrap().info.pages[0];
            let v = p.user_to_view(user[0], user[1]);
            sheet_origin(app) + Vec2::new(v[0], v[1]) * app.zoom
        };
        let start = screen(&app, [315., 690.]);
        let end = screen(&app, [550., 540.]);
        drag_pointer(&ctx, &mut app, start, end);
        assert!(
            app.marquee.selected.len() >= 20,
            "zoom {zoom}, selected {:?}, message {:?}",
            app.marquee.selected,
            app.message
        );
        assert!(app.marquee.selected.iter().any(|i| matches!(
            i.target,
            Mixed::Object(pdfcraft_edit::ObjectTarget {
                kind: ObjectKind::Text,
                ..
            })
        )));
        assert!(
            app.marquee
                .selected
                .iter()
                .any(|i| matches!(i.target, Mixed::Paint(_)))
        );
        for _ in 0..1000 {
            input_frame(&ctx, &mut app, vec![]);
            if app.marquee.preview_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            app.marquee.preview_ready(),
            "Mixed preview failed under parallel rendering load: {:?}",
            app.message
        );
        let selected = app.marquee.selected.clone();
        let start = screen(&app, [430., 615.]);
        let delta = Vec2::new(12. * zoom, 18. * zoom);
        drag_pointer(&ctx, &mut app, start, start + delta);
        assert!(app.message.is_none(), "{:?}", app.message);
        let e = app.editor.as_mut().unwrap();
        let moved = e.selection_items(0).unwrap();
        for old in selected {
            let expected = [
                old.rect[0] + 12.,
                old.rect[1] - 18.,
                old.rect[2] + 12.,
                old.rect[3] - 18.,
            ];
            assert!(
                moved.iter().any(|i| i
                    .rect
                    .iter()
                    .zip(expected)
                    .all(|(a, b)| (*a - b).abs() < 0.1)),
                "Missing {:?} at zoom {zoom}",
                old
            );
        }
        for (page, original) in text.iter().enumerate() {
            // Moving a diagram changes its spatial reading order relative to the
            // exercise's dotted answer lines. Preserve every non-whitespace glyph.
            let glyphs = |s: String| {
                let mut c = s.chars().filter(|c| !c.is_whitespace()).collect::<Vec<_>>();
                c.sort_unstable();
                c
            };
            assert_eq!(
                glyphs(e.text(page).unwrap().plain_text()),
                glyphs(original.clone())
            );
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "test-output/30-selection-graphique-{}.pdf",
            (zoom * 100.) as u32
        ));
        e.save(&path).unwrap();
        let reopened = Editor::open(&path).unwrap();
        assert_eq!(reopened.info.pages.len(), text.len());
        app.undo_redo(false);
        assert_eq!(app.editor.as_ref().unwrap().bytes, original);
        if zoom == 0.8 {
            let start = screen(&app, [-12., 810.]);
            let end = screen(&app, [560., 540.]);
            drag_pointer(&ctx, &mut app, start, end);
            assert!(
                app.marquee.selected.len() >= 20,
                "A rectangle may start in the gray margin"
            );
            input_frame(
                &ctx,
                &mut app,
                vec![egui::Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL,
                }],
            );
            assert_eq!(
                app.marquee.selected.len(),
                app.editor
                    .as_ref()
                    .unwrap()
                    .selection_items(0)
                    .unwrap()
                    .len()
            );
        }
    }
}
#[test]
fn writer_frames_select_move_resize_and_change_width_directly_in_modifier() {
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    let original = if let Some(path) = std::env::var_os("PDFINATOR_REGRESSION_PDF") {
        Editor::open(std::path::Path::new(&path)).unwrap()
    } else {
        let mut d = pdfcraft_create::blank(595., 842., 2).unwrap();
        let p = pdfcraft_model::pages(&d)[1].obj;
        let s = d.add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::flate(pdfcraft_cos::Dict::new(),
            b"q 0 0.1 595.2 841.8 re W* n 1 w 0 G 437.3 677.8 m 293.5 677.8 l 293.5 827.2 l 581 827.2 l 581 677.8 l 437.3 677.8 l h S 437.6 488.3 m 293.5 488.3 l 293.5 657 l 581.6 657 l 581.6 488.3 l 437.6 488.3 l h S 392.7 780.5 m 400.7 772.5 l S 399.6 781.4 m 393.8 771.7 l S Q")));
        d.update_dict(p, |p| {
            p.set(b"Contents".to_vec(), pdfcraft_cos::Object::Ref(s))
        })
        .unwrap();
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap()
    };
    let before = original.bytes.clone();
    let text = (0..original.info.pages.len())
        .map(|p| original.text(p).unwrap().plain_text())
        .collect::<Vec<_>>();
    let items = original.drawings(1).unwrap();
    let top = items
        .iter()
        .find(|i| (i.drawing.a[1] - 827.2).abs() < 0.1 && i.drawing.u[0] > 280.)
        .expect("Frame shown in the screenshot must be editable")
        .drawing
        .clone();
    let count = items.len();
    app.raster.document(original.bytes.clone(), true, None);
    app.editor = Some(original);
    app.set_workspace(&ctx, Workspace::Edit);
    app.fit = Fit::Manual;
    app.zoom = 0.7;
    app.thumbnails = false;
    app.page = 1;
    app.jump = Some(1);
    for _ in 0..90 {
        input_frame(&ctx, &mut app, vec![]);
    }
    let screen = |app: &App, point: [f64; 2]| {
        let e = app.editor.as_ref().unwrap();
        let p = &e.info.pages[1];
        let q = p.user_to_view(point[0] as f32, point[1] as f32);
        Pos2::new(
            app.canvas.left() + ((app.canvas.width() - 18. - p.width * app.zoom) / 2.).max(0.)
                - app.scroll_offset.x
                + q[0] * app.zoom,
            app.canvas.top() + e.info.pages[0].height * app.zoom + 20. - app.scroll_offset.y
                + q[1] * app.zoom,
        )
    };
    let at = screen(&app, top.at(0.5, 0.));
    for pressed in [true, false] {
        input_frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(at),
                pointer_button(at, egui::PointerButton::Primary, pressed),
            ],
        );
    }
    assert_eq!(app.tool, Tool::Edit);
    assert_eq!(app.drawing.selected.len(), 1, "{:?}", app.message);
    assert!(app.inline.is_none() && app.selected.is_none());
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    let start = screen(&app, top.at(0.5, 0.));
    drag_pointer(&ctx, &mut app, start, start + Vec2::new(7., 14.));
    assert_eq!(app.drawing.selected.len(), 1, "{:?}", app.message);
    let moved = app.drawing.selected[0].drawing.clone();
    assert!(
        (moved.a[0] - top.a[0] - 10.).abs() < 0.1 && (moved.a[1] - top.a[1] + 20.).abs() < 0.1,
        "{top:?} -> {moved:?}"
    );
    for _ in 0..100 {
        input_frame(&ctx, &mut app, vec![]);
        if app.drawing.drag.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let end = screen(&app, moved.at(1., 0.));
    drag_pointer(&ctx, &mut app, end, end + Vec2::new(-14., 0.));
    let resized = app.drawing.selected[0].drawing.clone();
    assert!((resized.u[0] - moved.u[0] + 20.).abs() < 0.1);
    click_named_control(&ctx, &mut app, "Choisir une épaisseur");
    click_named_control(&ctx, &mut app, "3 pt");
    assert!(
        (app.drawing.selected[0]
            .drawing
            .view_width(&app.editor.as_ref().unwrap().info.pages[1])
            - 3.)
            .abs()
            < 0.01
    );
    let e = app.editor.as_mut().unwrap();
    assert_eq!(
        e.drawings(1).unwrap().len(),
        count,
        "No duplicate stroke must remain"
    );
    assert_eq!(
        (0..e.info.pages.len())
            .map(|p| e.text(p).unwrap().plain_text())
            .collect::<Vec<_>>(),
        text
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-output/27-selection-traits.pdf");
    e.save(&path).unwrap();
    let reopened = Editor::open(&path).unwrap();
    assert_eq!(reopened.drawings(1).unwrap().len(), count);
    assert!(
        reopened
            .drawings(1)
            .unwrap()
            .iter()
            .any(|i| (i.drawing.width - 3.).abs() < 0.01
                && (i.drawing.a[0] - moved.a[0]).abs() < 0.1)
    );
    for _ in 0..3 {
        e.undo().unwrap();
    }
    assert_eq!(e.bytes, before);
}
#[test]
fn writer_green_cell_and_table_can_be_selected_recoloured_and_moved_with_their_text() {
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    let editor = if let Some(path) = std::env::var_os("PDFINATOR_REGRESSION_PDF") {
        Editor::open(std::path::Path::new(&path)).unwrap()
    } else {
        let d = pdfcraft_create::from_text("Tableau", "Sujet 2", (595., 842.), 10.).unwrap();
        let mut e =
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
        let b = e.blocks(0).unwrap().remove(0);
        e.move_block(0, 0, [195. - b.rect[0], 750. - b.rect[1]])
            .unwrap();
        let p = pdfcraft_model::pages(&e.doc)[0].clone();
        let old = p.dict.get(b"Contents").unwrap().clone();
        let s=e.doc.add(pdfcraft_cos::Object::Stream(pdfcraft_cos::Stream::flate(pdfcraft_cos::Dict::new(),
            b"q 0 0.1 595.2 841.8 re W* n 0.78 0.85 0.64 rg 15 747.2 371.3 16.2 re f* 386.3 747.2 187.5 16.2 re f* 0 g 15 700 558.8 0.6 re f* 15 763.4 558.8 0.6 re f* 15 747.2 558.8 0.6 re f* 15 700 0.6 63.4 re f* 573.2 700 0.6 63.4 re f* 386.3 700 0.6 63.4 re f* Q")));
        e.doc
            .update_dict(p.obj, |p| {
                p.set(
                    b"Contents".to_vec(),
                    pdfcraft_cos::Object::Array(vec![pdfcraft_cos::Object::Ref(s), old]),
                )
            })
            .unwrap();
        Editor::from_bytes(pdfcraft_cos::write_full(&e.doc, &Default::default()).unwrap()).unwrap()
    };
    let before = editor.bytes.clone();
    let text = (0..editor.info.pages.len())
        .map(|p| editor.text(p).unwrap().plain_text())
        .collect::<Vec<_>>();
    let green = editor
        .drawings(0)
        .unwrap()
        .into_iter()
        .find(|i| {
            i.drawing.kind == pdfinator::drawing::Kind::FilledRectangle
                && (i.drawing.a[1] - 747.2).abs() < 0.1
                && i.drawing.a[0] < 20.
        })
        .expect("Green header cell must be editable");
    app.raster.document(editor.bytes.clone(), true, None);
    app.editor = Some(editor);
    app.set_workspace(&ctx, Workspace::Edit);
    app.fit = Fit::Manual;
    app.zoom = 0.8;
    app.thumbnails = false;
    app.page = 0;
    app.jump = Some(0);
    for _ in 0..4 {
        input_frame(&ctx, &mut app, vec![]);
    }
    let screen = |app: &App, point: [f32; 2]| {
        let p = &app.editor.as_ref().unwrap().info.pages[0];
        let q = p.user_to_view(point[0], point[1]);
        Pos2::new(
            app.canvas.left() + ((app.canvas.width() - 18. - p.width * app.zoom) / 2.).max(0.)
                - app.scroll_offset.x
                + q[0] * app.zoom,
            app.canvas.top() - app.scroll_offset.y + q[1] * app.zoom,
        )
    };
    let click_green = |app: &mut App| {
        let at = screen(app, [50., 755.]);
        for pressed in [true, false] {
            input_frame(
                &ctx,
                app,
                vec![
                    egui::Event::PointerMoved(at),
                    pointer_button(at, egui::PointerButton::Primary, pressed),
                ],
            );
        }
    };
    click_green(&mut app);
    assert_eq!(app.drawing.selected.len(), 1);
    assert_eq!(
        app.drawing.selected[0].drawing.kind,
        pdfinator::drawing::Kind::FilledRectangle
    );
    assert!(app.selected.is_none() && app.inline.is_none());
    click_named_control(&ctx, &mut app, "Bleu");
    let changed = &app.drawing.selected[0].drawing;
    assert!((changed.color[2] - 0.95).abs() < 1e-5);
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output/28-case-coloree.pdf");
    app.editor.as_mut().unwrap().save(&path).unwrap();
    let reopened = Editor::open(&path).unwrap();
    assert!(
        reopened
            .drawings(0)
            .unwrap()
            .iter()
            .any(|i| i.drawing.kind == green.drawing.kind
                && (i.drawing.a[1] - green.drawing.a[1]).abs() < 0.1
                && (i.drawing.color[2] - 0.95).abs() < 1e-5)
    );
    app.undo_redo(false);
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    for _ in 0..4 {
        input_frame(&ctx, &mut app, vec![]);
    }
    click_green(&mut app);
    click_named_control(&ctx, &mut app, "Sélectionner le tableau");
    assert!(app.drawing.selected.len() >= 6);
    if std::env::var_os("PDFINATOR_REGRESSION_PDF").is_some() {
        assert_eq!(
            app.drawing.selected.len(),
            32,
            "Both green cells and all 30 border polygons must be included"
        );
    }
    let bounds = app
        .drawing
        .selected
        .iter()
        .flat_map(|i| [i.drawing.at(0., 0.), i.drawing.at(1., 1.)])
        .fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |r, p| {
                [
                    r[0].min(p[0]),
                    r[1].min(p[1]),
                    r[2].max(p[0]),
                    r[3].max(p[1]),
                ]
            },
        );
    let old_blocks = app.editor.as_ref().unwrap().blocks(0).unwrap();
    let contained = old_blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            b.rect[0] >= bounds[0] - 1.
                && b.rect[1] >= bounds[1] - 1.
                && b.rect[2] <= bounds[2] + 1.
                && b.rect[3] <= bounds[3] + 1.
        })
        .map(|(i, b)| (i, b.rect))
        .collect::<Vec<_>>();
    assert!(!contained.is_empty(), "The table text must be included");
    let start = screen(&app, [50., 755.]);
    drag_pointer(&ctx, &mut app, start, start + Vec2::new(4., 8.));
    let e = app.editor.as_ref().unwrap();
    let after = e.blocks(0).unwrap();
    for (i, r) in &contained {
        assert!(
            (after[*i].rect[0] - r[0] - 5.).abs() < 0.1
                && (after[*i].rect[1] - r[1] + 10.).abs() < 0.1,
            "{r:?} -> {:?}, error {:?}",
            after[*i].rect,
            app.message
        );
    }
    assert_eq!(
        (0..e.info.pages.len())
            .map(|p| e.text(p).unwrap().plain_text())
            .collect::<Vec<_>>(),
        text
    );
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output/28-tableau-deplace.pdf");
    app.editor.as_mut().unwrap().save(&path).unwrap();
    let reopened = Editor::open(&path).unwrap();
    for (i, r) in &contained {
        assert!((reopened.blocks(0).unwrap()[*i].rect[0] - r[0] - 5.).abs() < 0.1);
    }
    app.undo_redo(false);
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    for _ in 0..4 {
        input_frame(&ctx, &mut app, vec![]);
    }
    click_green(&mut app);
    click_named_control(&ctx, &mut app, "Sélectionner le tableau");
    click_named_control(&ctx, &mut app, "Supprimer le tableau");
    let e = app.editor.as_ref().unwrap();
    assert!(
        !e.blocks(0)
            .unwrap()
            .iter()
            .any(|b| b.rect[0] >= bounds[0] - 1.
                && b.rect[1] >= bounds[1] - 1.
                && b.rect[2] <= bounds[2] + 1.
                && b.rect[3] <= bounds[3] + 1.)
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-output/28-tableau-supprime.pdf");
    app.editor.as_mut().unwrap().save(&path).unwrap();
    app.undo_redo(false);
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
}

#[test]
fn highlighter_uses_a_text_cursor_only_on_the_pdf_page() {
    let (ctx, mut app) = setup();
    app.set_tool(&ctx, Tool::Highlight);
    let inside = app.canvas.center();
    let outside = Pos2::new(app.canvas.left() + 1., app.canvas.top() + 3.);
    for (pos, expected) in [
        (inside, egui::CursorIcon::Text),
        (outside, egui::CursorIcon::Default),
    ] {
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::PointerMoved(pos)],
                    ..Default::default()
                },
                |ui| app.frame_ui(ui),
            );
            out.textures_delta.clear();
            assert_eq!(out.platform_output.cursor_icon, expected);
        }
    }
    app.set_tool(&ctx, Tool::Select);
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::PointerMoved(inside)],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::Default);
    out.textures_delta.clear();
}

fn click_named_control(ctx: &egui::Context, app: &mut App, label: &str) {
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    let bounds = nodes
        .iter()
        .find(|(_, n)| n.label() == Some(label))
        .unwrap_or_else(|| {
            panic!(
                "Missing control {label}; labels: {:?}",
                nodes
                    .iter()
                    .filter_map(|(_, n)| n.label())
                    .collect::<Vec<_>>()
            )
        })
        .1
        .bounds()
        .unwrap();
    let pos = Pos2::new(
        (bounds.x0 + bounds.x1) as f32 / 2.,
        (bounds.y0 + bounds.y1) as f32 / 2.,
    );
    for pressed in [true, false] {
        let mut out = ctx.run_ui(
            egui::RawInput {
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
}

#[test]
fn drawing_palette_and_preset_menus_edit_the_selected_line_and_undo_exactly() {
    use pdfinator::drawing::{Drawing, Kind, StrokeStyle};
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Draw);
    let e = app.editor.as_mut().unwrap();
    let p = e.info.pages[0].clone();
    e.add_drawing(
        0,
        Drawing::from_view(
            &p,
            Kind::Line,
            [80., 250.],
            [400., 250.],
            1.5,
            [0.; 3],
            1,
            1,
        ),
    )
    .unwrap();
    app.drawing.selected = e.drawings(0).unwrap();
    app.drawing.page = 0;
    let before = e.bytes.clone();
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    assert!(app.right_rect.right() <= ctx.content_rect().right());
    assert!(
        !out.platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n
                .label()
                .is_some_and(|s| s.starts_with("Choisissez une forme")))
    );
    click_named_control(&ctx, &mut app, "Rouge");
    let color = app.editor.as_ref().unwrap().drawings(0).unwrap()[0]
        .drawing
        .color;
    assert!(
        color
            .iter()
            .zip([0.8, 0.06, 0.04])
            .all(|(actual, expected)| (*actual - expected).abs() < 0.00001)
    );
    click_named_control(&ctx, &mut app, "Choisir une épaisseur");
    click_named_control(&ctx, &mut app, "3 pt");
    assert!(
        (app.editor.as_ref().unwrap().drawings(0).unwrap()[0]
            .drawing
            .view_width(&p)
            - 3.)
            .abs()
            < 0.01
    );
    click_named_control(&ctx, &mut app, "Choisir un type de trait");
    click_named_control(&ctx, &mut app, "Pointillés");
    let e = app.editor.as_ref().unwrap();
    assert_eq!(e.drawings(0).unwrap()[0].drawing.style, StrokeStyle::Dotted);
    let reopened = Editor::from_bytes((*e.bytes).clone()).unwrap();
    assert_eq!(
        reopened.drawings(0).unwrap()[0].drawing.style,
        StrokeStyle::Dotted
    );
    for _ in 0..3 {
        app.undo_redo(false);
    }
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
}

#[test]
fn blank_page_button_is_beside_drawing_and_opens_the_new_page_without_changing_workspace() {
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    app.set_workspace(&ctx, Workspace::Edit);
    let before = app.editor.as_ref().unwrap().bytes.clone();
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    let bounds = |label| {
        nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label))
            .unwrap()
            .1
            .bounds()
            .unwrap()
    };
    let drawing = bounds("Dessin");
    let blank = bounds("Ajouter une page blanche");
    assert_eq!(drawing.y0, blank.y0);
    assert!(blank.x0 > drawing.x0);
    click_named_control(&ctx, &mut app, "Ajouter une page blanche");
    for _ in 0..90 {
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
        if app.page == 1 {
            break;
        }
    }
    assert!(app.workspace == Workspace::Edit);
    assert_eq!(app.page, 1);
    let e = app.editor.as_ref().unwrap();
    assert_eq!(e.info.pages.len(), 2);
    assert!(e.text(1).unwrap().plain_text().trim().is_empty());
    app.undo_redo(false);
    assert_eq!(app.page, 0);
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
}

#[test]
fn changing_colour_of_several_tables_does_not_change_their_row_or_column_counts() {
    use pdfinator::drawing::{Drawing, Kind};
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Draw);
    let e = app.editor.as_mut().unwrap();
    let p = e.info.pages[0].clone();
    for (x, rows, columns) in [(60., 3, 5), (300., 2, 4)] {
        e.add_drawing(
            0,
            Drawing::from_view(
                &p,
                Kind::Table,
                [x, 250.],
                [x + 180., 400.],
                1.5,
                [0.; 3],
                rows,
                columns,
            ),
        )
        .unwrap();
    }
    app.drawing.selected = e.drawings(0).unwrap();
    app.drawing.page = 0;
    click_named_control(&ctx, &mut app, "Bleu");
    let items = app.editor.as_ref().unwrap().drawings(0).unwrap();
    assert_eq!(
        items
            .iter()
            .map(|i| (i.drawing.rows, i.drawing.columns))
            .collect::<Vec<_>>(),
        [(3, 5), (2, 4)]
    );
    assert!(
        items
            .iter()
            .all(|i| (i.drawing.color[2] - 0.95).abs() < 0.00001)
    );
}

#[test]
fn drawing_tool_creates_a_vector_line_and_dragging_its_end_changes_length_without_a_copy() {
    let (ctx, mut app) = setup();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Draw);
    app.fit = Fit::Manual;
    app.zoom = 0.6;
    app.drawing.mode = Some(pdfinator::drawing::Kind::Line);
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let before = app.editor.as_ref().unwrap().bytes.clone();
    let origin = Pos2::new(
        app.canvas.left() + ((app.canvas.width() - 18. - 595. * app.zoom) / 2.).max(0.)
            - app.scroll_offset.x,
        app.canvas.top(),
    );
    let start = origin + Vec2::new(80., 340.) * app.zoom;
    let end = origin + Vec2::new(330., 340.) * app.zoom;
    let drag = |app: &mut App, a: Pos2, b: Pos2| {
        for events in [
            vec![
                egui::Event::PointerMoved(a),
                egui::Event::PointerButton {
                    pos: a,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            vec![egui::Event::PointerMoved(b)],
            vec![egui::Event::PointerButton {
                pos: b,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
        ] {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| app.frame_ui(ui),
            );
            out.textures_delta.clear();
        }
    };
    drag(&mut app, start, end);
    let items = app.editor.as_ref().unwrap().drawings(0).unwrap();
    assert_eq!(items.len(), 1, "{:?}", app.message);
    let original = items[0].drawing.clone();
    assert!((original.u[0].hypot(original.u[1]) - 250.).abs() < 1.);
    for _ in 0..100 {
        if app.drawing.drag.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
    }
    assert!(app.drawing.drag.is_none());
    drag(&mut app, end, end + Vec2::new(60., 0.));
    let items = app.editor.as_ref().unwrap().drawings(0).unwrap();
    assert_eq!(items.len(), 1);
    assert!((items[0].drawing.u[0].hypot(items[0].drawing.u[1]) - 350.).abs() < 1.);
    app.undo_redo(false);
    app.undo_redo(false);
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
}

#[test]
fn hand_cursor_is_limited_to_the_sheet_and_returns_to_an_arrow_in_the_margin() {
    let (ctx, mut app) = setup();
    for (workspace, tool) in [
        (Workspace::Read, Tool::Hand),
        (Workspace::Edit, Tool::Hand),
        (Workspace::Edit, Tool::Move),
    ] {
        app.set_workspace(&ctx, workspace);
        app.set_tool(&ctx, tool);
        for (pos, pressed, cursor) in [
            (app.canvas.center(), false, egui::CursorIcon::Grab),
            (app.canvas.center(), true, egui::CursorIcon::Grabbing),
            (
                app.canvas.min + Vec2::new(1., 3.),
                true,
                egui::CursorIcon::Default,
            ),
            (
                app.canvas.min + Vec2::new(1., 3.),
                false,
                egui::CursorIcon::Default,
            ),
        ] {
            let mut out = ctx.run_ui(
                egui::RawInput {
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
            assert_eq!(out.platform_output.cursor_icon, cursor);
        }
    }
}

#[test]
fn title_bar_double_click_toggles_the_native_maximized_state() {
    for maximized in [false, true] {
        let (ctx, mut app) = setup();
        ctx.enable_accesskit();
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
        let b = out
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some("Barre de titre"))
            .unwrap()
            .1
            .bounds()
            .unwrap();
        let pos = Pos2::new((b.x0 + b.x1) as f32 / 2., (b.y0 + b.y1) as f32 / 2.);
        let mut changed = false;
        for (index, pressed) in [true, false, true, false].into_iter().enumerate() {
            let mut input = egui::RawInput {
                time: Some(1. + index as f64 * 0.08),
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
            };
            input
                .viewports
                .entry(egui::ViewportId::ROOT)
                .or_default()
                .maximized = Some(maximized);
            let mut out = ctx.run_ui(input, |ui| app.frame_ui(ui));
            out.textures_delta.clear();
            changed |= out
                .viewport_output
                .values()
                .flat_map(|v| v.commands.iter())
                .any(|cmd| matches!(cmd,egui::ViewportCommand::Maximized(v) if *v!=maximized));
        }
        assert!(
            changed,
            "No native maximise/restore command for maximized={maximized}"
        );
    }
}

#[test]
fn document_tabs_drag_sideways_without_exchanging_contents_or_histories() {
    let (ctx, mut app) = setup();
    app.editor.as_mut().unwrap().path = Some(PathBuf::from("A.pdf"));
    app.zoom = 1.3;
    app.editor
        .as_mut()
        .unwrap()
        .rewrite(0, 0, Some("Modification dans A."), &Default::default())
        .unwrap();
    let a_bytes = app.editor.as_ref().unwrap().bytes.clone();
    for label in ["B", "C"] {
        let d = pdfcraft_create::from_text(label, &format!("Document {label}."), (595., 842.), 14.)
            .unwrap();
        let mut e =
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
        e.path = Some(PathBuf::from(format!("{label}.pdf")));
        app.insert_document(e);
    }
    let c_bytes = app.editor.as_ref().unwrap().bytes.clone();
    ctx.enable_accesskit();
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
    let point = |name: &str, right: bool| {
        let b = nodes
            .iter()
            .find(|(_, n)| n.label().is_some_and(|s| s.starts_with(name)))
            .unwrap()
            .1
            .bounds()
            .unwrap();
        Pos2::new(
            if right {
                b.x1 as f32 - 2.
            } else {
                (b.x0 + b.x1) as f32 / 2.
            },
            (b.y0 + b.y1) as f32 / 2.,
        )
    };
    let start = point("A.pdf", false);
    let end = point("C.pdf", true);
    for events in [
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        vec![egui::Event::PointerMoved(end)],
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    ] {
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
    }
    assert_eq!(
        (0..3).map(|i| app.document_label(i)).collect::<Vec<_>>(),
        ["B.pdf", "C.pdf", "A.pdf"]
    );
    assert_eq!(app.editor.as_ref().unwrap().bytes, c_bytes);
    assert_eq!(app.active_document, 1);
    assert!(app.activate_document(2));
    assert_eq!(app.editor.as_ref().unwrap().bytes, a_bytes);
    assert!((app.zoom - 1.3).abs() < 0.01);
    assert!(app.editor.as_ref().unwrap().can_undo());
}

#[test]
fn block_clipboard_follows_tabs_pastes_editable_content_and_respects_read_mode() {
    let (ctx, mut app) = setup();
    app.set_workspace(&ctx, Workspace::Edit);
    let selection = app.selection_for(0, Target::Block(0)).unwrap();
    app.begin_text(&ctx, selection);
    app.inline.as_mut().unwrap().selection.draft = "Un bloc français corrigé.".into();
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Copy],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    assert_eq!(
        app.copied_block.as_ref().unwrap().data.text,
        "Un bloc français corrigé."
    );
    let source = app.editor.as_ref().unwrap().bytes.clone();
    let blank = pdfcraft_create::blank(595., 842., 1).unwrap();
    app.insert_document(
        Editor::from_bytes(pdfcraft_cos::write_full(&blank, &Default::default()).unwrap()).unwrap(),
    );
    let before = app.editor.as_ref().unwrap().bytes.clone();
    let paste = egui::Event::Paste("Un bloc français corrigé.".into());
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![paste.clone()],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    app.set_workspace(&ctx, Workspace::Edit);
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![paste],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    let e = app.editor.as_ref().unwrap();
    assert_eq!(
        e.text(0)
            .unwrap()
            .plain_text()
            .matches("Un bloc français corrigé")
            .count(),
        1
    );
    assert!(matches!(
        app.selected.as_ref().unwrap().target,
        Target::Block(_)
    ));
    app.undo_redo(false);
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    assert_eq!(app.page, 0);
    assert_eq!(app.page_destination, 1);
    app.undo_redo(true);
    assert!(app.activate_document(0));
    assert_eq!(app.editor.as_ref().unwrap().bytes, source);
}

#[test]
fn adding_pdf_files_from_pages_updates_thumbnails_and_can_be_undone() {
    let (ctx, mut app) = setup();
    app.set_workspace(&ctx, Workspace::Pages);
    let dir = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for (n, count) in [(1, 1), (2, 2)] {
        let path = dir.path().join(format!("ajout-{n}.pdf"));
        let d = pdfcraft_create::blank(420., 595., count).unwrap();
        std::fs::write(
            &path,
            pdfcraft_cos::write_full(&d, &Default::default()).unwrap(),
        )
        .unwrap();
        paths.push(path);
    }
    let before = app.editor.as_ref().unwrap().bytes.clone();
    app.append_pdf_paths(&paths);
    assert!(app.message.is_none(), "{:?}", app.message);
    assert_eq!(app.editor.as_ref().unwrap().info.pages.len(), 4);
    assert_eq!(app.page, 1);
    ctx.enable_accesskit();
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let update = out.platform_output.accesskit_update.unwrap();
    for n in 1..=4 {
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, node)| node.label()
                    == Some(&format!("Page {n} · double-cliquer pour lire")))
        );
    }
    assert!(app.workspace == Workspace::Pages);
    app.undo_redo(false);
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    assert_eq!(app.page, 0);
    assert_eq!(app.page_destination, 1);
    app.undo_redo(true);
    assert_eq!(app.editor.as_ref().unwrap().info.pages.len(), 4);
}

#[test]
fn rail_commands_fit_small_windows_and_pages_only_exposes_relevant_actions() {
    for height in [540., 720., 1080.] {
        for workspace in [Workspace::Read, Workspace::Edit, Workspace::Pages] {
            let (ctx, mut app) = setup();
            ctx.enable_accesskit();
            app.set_workspace(&ctx, workspace);
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(760., height))),
                    ..Default::default()
                },
                |ui| app.frame_ui(ui),
            );
            out.textures_delta.clear();
            let update = out.platform_output.accesskit_update.as_ref().unwrap();
            let wanted = if workspace == Workspace::Pages {
                vec![
                    "Ouvrir · Ctrl+O",
                    "Enregistrer sous · Ctrl+Maj+S",
                    "Annuler · Ctrl+Z",
                    "Tourner à gauche · 90°",
                    "Tourner à droite · 90°",
                    "Avancer cette page",
                    "Reculer cette page",
                    "Appliquer cette position",
                    "Nouvelle position de la page",
                    "Ajouter des PDF à la fin · Ctrl+Maj+O",
                ]
            } else {
                vec![
                    "Ouvrir · Ctrl+O",
                    "Enregistrer sous · Ctrl+Maj+S",
                    "Page entière",
                    "Main · déplacer la vue",
                    if workspace == Workspace::Edit {
                        "Déplacer"
                    } else {
                        "Commentaire"
                    },
                ]
            };
            for label in wanted {
                let node = update
                    .nodes
                    .iter()
                    .map(|(_, n)| n)
                    .find(|n| n.label() == Some(label))
                    .unwrap_or_else(|| panic!("Missing {label}"));
                let bounds = node.bounds().unwrap();
                assert!(
                    bounds.y1 <= height as f64 && bounds.x1 <= 92.,
                    "{label}: {bounds:?} at height {height}"
                );
            }
            if workspace == Workspace::Edit {
                for label in [
                    "Copier le bloc · Ctrl+C",
                    "Coller le bloc · Ctrl+V",
                    "Dessin",
                    "Ajouter une page blanche",
                ] {
                    let bounds = update
                        .nodes
                        .iter()
                        .find(|(_, n)| n.label() == Some(label))
                        .unwrap()
                        .1
                        .bounds()
                        .unwrap();
                    assert!(
                        bounds.y1 <= height as f64 && bounds.x1 <= 92.,
                        "{label}: {bounds:?}"
                    );
                }
            }
            if workspace != Workspace::Pages {
                let bounds = |label| {
                    update
                        .nodes
                        .iter()
                        .find(|(_, n)| n.label() == Some(label))
                        .unwrap()
                        .1
                        .bounds()
                        .unwrap()
                };
                let select = bounds("Sélectionner");
                let hand = bounds("Main · déplacer la vue");
                assert_eq!(select.y0, hand.y0);
                assert!(select.x1 <= hand.x0, "Main must be beside Select");
            }
            if workspace == Workspace::Pages {
                for label in [
                    "Augmenter le zoom",
                    "Rechercher · Ctrl+F",
                    "Imprimer · Ctrl+P",
                    "Afficher les miniatures",
                    "Lire cette page · Entrée",
                ] {
                    assert!(
                        !update.nodes.iter().any(|(_, n)| n.label() == Some(label)),
                        "Useless in Pages: {label}"
                    );
                }
                assert_eq!(app.right_rect, Rect::NOTHING);
            }
            assert!(!update.nodes.iter().any(|(_, n)| {
                n.label()
                    .is_some_and(|s| s.contains("Glissez une vignette"))
            }));
            out.textures_delta.clear();
        }
    }
}

#[test]
fn dismissing_search_removes_only_temporary_highlights() {
    for action in 0..5 {
        let (ctx, mut app) = setup();
        let text = app.editor.as_ref().unwrap().text(0).unwrap();
        app.editor
            .as_mut()
            .unwrap()
            .highlight(0, &text, 0..5)
            .unwrap();
        let before = app.editor.as_ref().unwrap().bytes.clone();
        let annotations = app.editor.as_ref().unwrap().info.annotations.len();
        app.query = "original".into();
        app.request_search(&ctx);
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
        let yellow = Color32::from_rgba_unmultiplied(255, 210, 0, 95);
        assert!(!app.hits.is_empty());
        assert!(
            out.shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::epaint::Shape::Rect(r) if r.fill == yellow))
        );
        let mut events = Vec::new();
        match action {
            0 => app.close_search(),
            1 => {
                assert!(app.set_tool(&ctx, Tool::Select));
            }
            2 => {
                assert!(app.set_tool(&ctx, Tool::Hand));
            }
            3 => events.push(egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }),
            _ => {
                app.fit = Fit::Manual;
                app.zoom = 0.5;
                input_frame(&ctx, &mut app, vec![]);
                let pos = app.canvas.min + Vec2::new(3., 100.);
                events = vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ];
            }
        }
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
        assert!(
            !app.search && app.hits.is_empty(),
            "Dismiss action {action}"
        );
        assert!(
            !out.shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::epaint::Shape::Rect(r) if r.fill == yellow))
        );
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
        assert_eq!(
            app.editor.as_ref().unwrap().info.annotations.len(),
            annotations
        );
    }
}

#[test]
fn hand_pans_the_view_on_both_axes_without_editing_or_selecting_text() {
    let (ctx, mut app) = setup();
    app.fit = Fit::Manual;
    app.zoom = 2.;
    app.thumbnails = false;
    app.scroll_offset = Vec2::new(150., 200.);
    assert!(app.set_tool(&ctx, Tool::Hand));
    for _ in 0..2 {
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
    }
    let before = app.editor.as_ref().unwrap().bytes.clone();
    let offset = app.scroll_offset;
    let start = app.canvas.center();
    let end = start - Vec2::new(70., 95.);
    for (stage, events) in [
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        vec![egui::Event::PointerMoved(end)],
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    ]
    .into_iter()
    .enumerate()
    {
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
        if stage == 1 {
            assert!(
                (app.scroll_offset.x - offset.x - 70.).abs() < 1.,
                "Horizontal pan: {:?}",
                app.scroll_offset
            );
            assert!(
                (app.scroll_offset.y - offset.y - 95.).abs() < 1.,
                "Vertical pan: {:?}",
                app.scroll_offset
            );
            assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::Grabbing);
        }
    }
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    assert!(app.selected.is_none() && app.text_selection.is_none());
    assert!(!app.editor.as_ref().unwrap().can_undo());
}

#[test]
fn edit_sidebar_drags_pages_before_and_after_without_changing_workspace() {
    for workspace in [Workspace::Read, Workspace::Edit] {
        for move_before in [false, true] {
            let (ctx, mut app) = setup();
            ctx.enable_accesskit();
            let mut d = pdfcraft_create::blank(595., 842., 3).unwrap();
            for page in 0..3 {
                pdfcraft_edit::add_content(
                    &mut d,
                    page,
                    &Content::Text(AddedText {
                        rect: [40., 700., 500., 740.],
                        text: format!("Contenu page {}", page + 1),
                        ..Default::default()
                    }),
                )
                .unwrap();
            }
            app.editor = Some(
                Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap())
                    .unwrap(),
            );
            app.raster = Default::default();
            app.set_workspace(&ctx, workspace);
            let source = if move_before { 2 } else { 0 };
            let texture = ctx.load_texture(
                "sidebar-source",
                egui::ColorImage::new([2, 2], vec![Color32::WHITE; 4]),
                Default::default(),
            );
            let texture_id = texture.id();
            app.raster.thumbs.insert(source, texture);
            let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
            out.textures_delta.clear();
            let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
            let bounds = |page: usize| {
                let label = format!("Lire la page {}", page + 1);
                let b = nodes
                    .iter()
                    .find(|(_, n)| n.label() == Some(&label))
                    .unwrap()
                    .1
                    .bounds()
                    .unwrap();
                Rect::from_min_max(
                    Pos2::new(b.x0 as f32, b.y0 as f32),
                    Pos2::new(b.x1 as f32, b.y1 as f32),
                )
            };
            let start = bounds(source).center();
            let target = bounds(if move_before { 0 } else { 1 });
            let end = Pos2::new(
                target.center().x,
                if move_before {
                    target.top() + 15.
                } else {
                    target.bottom() - 15.
                },
            );
            for (stage, events) in [
                vec![
                    egui::Event::PointerMoved(start),
                    egui::Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
                vec![egui::Event::PointerMoved(end)],
                vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
            ]
            .into_iter()
            .enumerate()
            {
                let mut out = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| app.frame_ui(ui),
                );
                out.textures_delta.clear();
                if stage == 1 {
                    assert_eq!(app.page_drag, Some(source));
                    assert!(out.shapes.iter().any(|s| matches!(&s.shape,
                    egui::epaint::Shape::LineSegment { points, stroke }
                        if stroke.width == 4. && stroke.color == icons::BLUE && points[0].y == points[1].y
                )), "Sidebar insertion line must be horizontal");
                    assert!(out.shapes.iter().any(|s| matches!(&s.shape,
                    egui::epaint::Shape::Mesh(m) if m.texture_id == texture_id && m.calc_bounds().right() > 197. && s.clip_rect.right() > 197.
                )), "Dragged thumbnail must remain visible beyond the narrow sidebar");
                    assert!(!app.editor.as_ref().unwrap().can_undo());
                }
            }
            assert!(app.workspace == workspace);
            assert_eq!(
                app.tool,
                if workspace == Workspace::Edit {
                    Tool::Edit
                } else {
                    Tool::Select
                }
            );
            assert!(app.page_drag.is_none());
            let e = app.editor.as_ref().unwrap();
            let expected = if move_before { [3, 1, 2] } else { [2, 1, 3] };
            let reopened = Editor::from_bytes((*e.bytes).clone()).unwrap();
            for (page, number) in expected.into_iter().enumerate() {
                assert!(
                    reopened
                        .text(page)
                        .unwrap()
                        .plain_text()
                        .contains(&format!("Contenu page {number}"))
                );
            }
            assert!(e.can_undo());
            let after = e.bytes.clone();
            app.undo_redo(false);
            assert!(
                app.editor
                    .as_ref()
                    .unwrap()
                    .text(0)
                    .unwrap()
                    .plain_text()
                    .contains("page 1")
            );
            app.undo_redo(true);
            assert_eq!(app.editor.as_ref().unwrap().bytes, after);
        }
    }
}

#[test]
fn print_dialog_fits_small_windows_with_long_printer_names_and_escape_closes_it() {
    for (width, height) in [(760., 540.), (1080., 720.)] {
        let (ctx, mut app) = setup();
        ctx.enable_accesskit();
        let before = app.editor.as_ref().unwrap().bytes.clone();
        app.printing.open = true;
        app.printing.printers = vec![craft_print::Printer {
            name: format!(
                "HP Smart Tank 7000 series — {}",
                "Imprimante avec un très long nom ".repeat(8)
            ),
            default: true,
            two_sided: Some(true),
        }];
        for _ in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height))),
                    ..Default::default()
                },
                |ui| app.frame_ui(ui),
            );
            out.textures_delta.clear();
            let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
            for label in ["Imprimer", "Fermer"] {
                let node = nodes
                    .iter()
                    .map(|(_, n)| n)
                    .find(|n| n.label() == Some(label) && n.role() == egui::accesskit::Role::Button)
                    .unwrap();
                let b = node.bounds().unwrap();
                assert!(
                    b.x0 >= 0. && b.y0 >= 0. && b.x1 <= width as f64 && b.y1 <= height as f64,
                    "{label} is outside the window: {b:?}"
                );
            }
        }
        let rect = egui::containers::AreaState::load(&ctx, egui::Id::new("print-dialog"))
            .unwrap()
            .rect();
        assert!(
            rect.left() >= 0.
                && rect.top() >= 0.
                && rect.right() <= width
                && rect.bottom() <= height,
            "Oversized print dialog: {rect:?}"
        );
        assert_eq!(app.right_rect, Rect::NOTHING);
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
        assert!(!app.printing.open && !app.printing.busy);
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    }
}

#[test]
fn switching_to_reading_commits_and_closes_text_properties() {
    let (ctx, mut app) = setup();
    app.workspace = Workspace::Edit;
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    app.begin_text(&ctx, s);
    app.inline.as_mut().unwrap().selection.draft = "Correction conservée en lecture.".into();
    app.set_workspace(&ctx, Workspace::Read);
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    assert!(app.workspace == Workspace::Read && app.tool == Tool::Select);
    assert!(app.selected.is_none());
    assert!(app.inline.as_ref().is_none_or(|i| i.committed));
    assert!(app.right_rect == Rect::NOTHING);
    assert!(
        app.editor
            .as_ref()
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .contains("conservée en lecture")
    );
}

#[test]
fn select_closes_and_commits_a_comment() {
    let (ctx, mut app) = setup();
    let index = app
        .editor
        .as_mut()
        .unwrap()
        .add_comment(0, [80., 700.], "Avant")
        .unwrap();
    app.tool = Tool::Comment;
    app.selected = app.selection_for(0, Target::Comment(index));
    app.selected.as_mut().unwrap().draft = "Commentaire conservé.".into();
    assert!(app.set_tool(&ctx, Tool::Select));
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    assert!(app.selected.is_none());
    assert!(app.right_rect == Rect::NOTHING);
    assert_eq!(
        app.editor.as_ref().unwrap().info.annotations[0]
            .contents
            .as_deref(),
        Some("Commentaire conservé.")
    );
}

#[test]
fn gray_background_click_commits_and_deselects_in_edit_mode() {
    let (ctx, mut app) = setup();
    app.workspace = Workspace::Edit;
    app.tool = Tool::Edit;
    app.fit = Fit::Manual;
    app.zoom = 0.5;
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    app.begin_text(&ctx, s);
    app.inline.as_mut().unwrap().selection.draft = "Correction par clic sur le gris.".into();
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let pos = Pos2::new(app.canvas.left() + 4., app.canvas.center().y);
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    assert!(app.selected.is_none());
    assert!(app.inline.as_ref().is_none_or(|i| i.committed));
    assert!(
        app.editor
            .as_ref()
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .contains("clic sur le gris")
    );
}

#[test]
fn failed_commit_does_not_discard_the_editor_during_mode_change() {
    let (ctx, mut app) = setup();
    app.workspace = Workspace::Edit;
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    app.begin_text(&ctx, s);
    app.inline.as_mut().unwrap().selection.draft = "Impossible : \u{10ffff}".into();
    let before = app.editor.as_ref().unwrap().bytes.clone();
    app.set_workspace(&ctx, Workspace::Read);
    assert!(app.workspace == Workspace::Edit);
    assert!(app.selected.is_some());
    assert!(!app.inline.as_ref().unwrap().committed);
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
}

#[test]
fn move_tool_drags_a_text_body_without_opening_text_properties() {
    let (ctx, mut app) = setup();
    app.workspace = Workspace::Edit;
    app.set_tool(&ctx, Tool::Move);
    app.fit = Fit::Manual;
    app.zoom = 0.8;
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let original = app.editor.as_ref().unwrap().blocks(0).unwrap()[0].rect;
    let p = &app.editor.as_ref().unwrap().info.pages[0];
    let local = app.selection_for(0, Target::Block(0)).unwrap().view_rect(p);
    let start = Pos2::new(
        app.canvas.center().x - 9. + (local.center().x - p.width / 2.) * app.zoom,
        app.canvas.top() + local.center().y * app.zoom,
    );
    let end = start + Vec2::new(32., 16.);
    for events in [
        vec![
            egui::Event::PointerMoved(start),
            egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        vec![egui::Event::PointerMoved(end)],
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    ] {
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
    }
    let e = app.editor.as_ref().unwrap();
    let moved = e.blocks(0).unwrap()[0].rect;
    assert!(
        (moved[0] - original[0] - 40.).abs() < 0.1,
        "{original:?} -> {moved:?}"
    );
    assert!((moved[1] - original[1] + 20.).abs() < 0.1);
    assert_eq!(
        e.text(0).unwrap().plain_text().matches("original").count(),
        1
    );
    assert!(app.right_rect == Rect::NOTHING);
}

#[test]
fn font_menu_contains_named_local_faces_and_embeds_the_chosen_face() {
    let (ctx, mut app) = setup();
    app.fonts = typography::FontBook::discover();
    assert!(app.fonts.entries.len() > 10);
    let font = app
        .fonts
        .entries
        .iter()
        .find(|f| f.name == "Arial")
        .unwrap()
        .clone();
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    app.selected = Some(s.clone());
    app.begin_text(&ctx, s);
    app.choose_font(&ctx, font);
    assert!(app.finish_text(), "{:?}", app.message);
    let e = app.editor.as_ref().unwrap();
    let data = e.text_formatting(0, 0).unwrap();
    assert_eq!(data.spans[0].format.font_name.as_deref(), Some("Arial"));
    assert!(
        pdfcraft_edit::text_lines(&e.doc, 0)
            .unwrap()
            .iter()
            .all(|l| l.base_font.contains("Arial"))
    );
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-output/16-police-arial.pdf");
    app.editor.as_mut().unwrap().save(&path).unwrap();
    let e = Editor::open(&path).unwrap();
    assert!(e.text(0).unwrap().plain_text().contains("à corriger"));
    assert_eq!(
        e.text_formatting(0, 0).unwrap().spans[0]
            .format
            .font_name
            .as_deref(),
        Some("Arial")
    );
}

#[test]
fn font_can_be_chosen_before_typing_new_text() {
    let (ctx, mut app) = setup();
    let s = Selection::text(
        0,
        Target::New,
        [80., 500., 340., 530.],
        String::new(),
        14.,
        [0.; 3],
        false,
    );
    app.selected = Some(s.clone());
    app.begin_text(&ctx, s);
    app.fonts = typography::FontBook::discover();
    let font = app
        .fonts
        .entries
        .iter()
        .find(|f| f.name == "Arial")
        .unwrap()
        .clone();
    app.choose_font(&ctx, font);
    assert_eq!(
        app.inline
            .as_ref()
            .unwrap()
            .selection
            .active_format()
            .font_name
            .as_deref(),
        Some("Arial")
    );
    app.inline.as_mut().unwrap().selection.draft = "Une nouvelle phrase, à côté.".into();
    assert!(app.finish_text(), "{:?}", app.message);
    let e = app.editor.as_ref().unwrap();
    assert_eq!(
        e.text(0).unwrap().plain_text().matches("nouvelle").count(),
        1
    );
    assert!(
        pdfcraft_edit::text_lines(&e.doc, 0)
            .unwrap()
            .iter()
            .any(|l| l.text.contains("nouvelle") && l.base_font.contains("Arial"))
    );
}

#[test]
fn font_size_menu_formats_the_selection_or_whole_block_and_accepts_a_custom_french_value() {
    use egui::text::{CCursor, CCursorRange};
    for whole_block in [false, true] {
        let (ctx, mut app) = setup();
        ctx.enable_accesskit();
        app.set_workspace(&ctx, Workspace::Edit);
        app.begin_text(&ctx, app.selection_for(0, Target::Block(0)).unwrap());
        for _ in 0..200 {
            input_frame(&ctx, &mut app, vec![]);
            if app
                .inline
                .as_ref()
                .is_some_and(|i| i.screen.is_positive() && !i.focus)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let id = egui::Id::new("direct-text");
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
        state.cursor.set_char_range(Some(if whole_block {
            CCursorRange::one(CCursor::new(5))
        } else {
            CCursorRange::two(CCursor::new(0), CCursor::new(5))
        }));
        state.store(&ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
        input_frame(&ctx, &mut app, vec![]);
        let text = app.inline.as_ref().unwrap().selection.draft.clone();
        click_named_control(&ctx, &mut app, "Choisir une taille de police");
        click_named_control(&ctx, &mut app, "18 pt");
        input_frame(&ctx, &mut app, vec![]);
        assert!(
            ctx.memory(|m| m.has_focus(id)),
            "Preset choice must return to the text caret"
        );
        assert!(!egui::Popup::is_any_open(&ctx));
        assert_eq!(
            app.inline.as_ref().unwrap().selection.text_range,
            if whole_block { None } else { Some(0..5) },
            "Choosing a size must preserve the character selection for another change"
        );
        let range = if whole_block { 0..text.len() } else { 0..5 };
        let selection = &app.inline.as_ref().unwrap().selection;
        assert!(
            selection
                .rich
                .iter()
                .any(|s| s.range == range && s.format.size == Some(18.))
        );
        click_named_control(&ctx, &mut app, "Choisir une taille de police");
        click_named_control(&ctx, &mut app, "Autre taille de police");
        input_frame(&ctx, &mut app, vec![]);
        assert!(
            egui::Popup::is_any_open(&ctx),
            "Custom input must keep the menu open"
        );
        assert!(
            !ctx.memory(|m| m.has_focus(id)),
            "Custom input must retain its own focus"
        );
        input_frame(&ctx, &mut app, vec![egui::Event::Text("13,5".into())]);
        input_frame(
            &ctx,
            &mut app,
            vec![egui::Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        for _ in 0..2 {
            input_frame(&ctx, &mut app, vec![]);
        }
        assert!(ctx.memory(|m| m.has_focus(id)));
        assert!(!egui::Popup::is_any_open(&ctx));
        let selection = &app.inline.as_ref().unwrap().selection;
        assert_eq!(
            selection.draft, text,
            "Typing a size must not write into the document"
        );
        assert!(
            selection
                .rich
                .iter()
                .any(|s| s.range == range && s.format.size == Some(13.5)),
            "{:?}",
            selection.rich
        );
        assert!(app.finish_text(), "{:?}", app.message);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "test-output/35-taille-{}.pdf",
            if whole_block { "bloc" } else { "selection" }
        ));
        app.editor.as_mut().unwrap().save(&path).unwrap();
        let reopened = Editor::open(&path).unwrap();
        assert_eq!(reopened.text(0).unwrap().plain_text().trim(), text);
        assert!(
            reopened
                .text_formatting(0, 0)
                .unwrap()
                .spans
                .iter()
                .any(|s| s.range == range && s.format.size == Some(13.5))
        );
    }
}

#[test]
fn simplified_text_panel_bullets_and_spacing_preserve_selection_and_save() {
    use egui::text::{CCursor, CCursorRange};
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    app.set_workspace(&ctx, Workspace::Edit);
    let mut s = app.selection_for(0, Target::Block(0)).unwrap();
    s.draft = "Premier élément\nDeuxième élément\nTroisième élément".into();
    s.rect[1] -= 100.;
    app.begin_text(&ctx, s);
    for _ in 0..200 {
        input_frame(&ctx, &mut app, vec![]);
        if app
            .inline
            .as_ref()
            .is_some_and(|i| i.screen.is_positive() && !i.focus)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let out = input_frame(&ctx, &mut app, vec![]);
    let labels: Vec<_> = out
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label())
        .collect();
    for removed in [
        "Texte",
        "Bloc entier",
        "Texte sélectionné",
        "Valider",
        "Choisir un fichier de police…",
    ] {
        assert!(!labels.contains(&removed), "{removed}: {labels:?}");
    }
    let id = egui::Id::new("direct-text");
    let text = app.inline.as_ref().unwrap().selection.draft.clone();
    let start = text.find("Deuxième").unwrap();
    let a = text[..start].chars().count();
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
    state.cursor.set_char_range(Some(CCursorRange::two(
        CCursor::new(a),
        CCursor::new(a + 7),
    )));
    state.store(&ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
    input_frame(&ctx, &mut app, vec![]);
    click_named_control(&ctx, &mut app, "Puces");
    input_frame(&ctx, &mut app, vec![]);
    let s = &app.inline.as_ref().unwrap().selection;
    assert_eq!(
        s.draft,
        "Premier élément\n• Deuxième élément\nTroisième élément"
    );
    assert_eq!(&s.draft[s.text_range.clone().unwrap()], "Deuxièm");
    click_named_control(&ctx, &mut app, "Puces");
    input_frame(&ctx, &mut app, vec![]);
    assert_eq!(app.inline.as_ref().unwrap().selection.draft, text);
    click_named_control(&ctx, &mut app, "Puces");
    click_named_control(&ctx, &mut app, "Choisir un interligne");
    click_named_control(&ctx, &mut app, "2 ×");
    input_frame(&ctx, &mut app, vec![]);
    assert!(ctx.memory(|m| m.has_focus(id)));
    assert_eq!(
        app.inline.as_ref().unwrap().selection.line_spacing,
        Some(2.)
    );
    let mut baselines = Vec::new();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let inline = app.inline.as_ref().unwrap();
        let galley = inline
            .layout(ui, &inline.selection.draft, 1., 450.)
            .unwrap();
        baselines = galley
            .rows
            .iter()
            .map(|r| r.pos.y + r.row.glyphs[0].pos.y)
            .collect();
    });
    output.textures_delta.clear();
    assert_eq!(baselines.len(), 3);
    assert!(
        baselines
            .windows(2)
            .all(|v| ((v[1] - v[0]) - 28.).abs() < 0.1),
        "{baselines:?}"
    );
    click_named_control(&ctx, &mut app, "Choisir un interligne");
    click_named_control(&ctx, &mut app, "Autre interligne");
    input_frame(&ctx, &mut app, vec![egui::Event::Text("1,75".into())]);
    input_frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    input_frame(&ctx, &mut app, vec![]);
    assert_eq!(
        app.inline.as_ref().unwrap().selection.line_spacing,
        Some(1.75)
    );
    assert!(app.finish_text(), "{:?}", app.message);
    let e = app.editor.as_mut().unwrap();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output/36-puces-interface.pdf");
    e.save(&path).unwrap();
    let e = Editor::open(&path).unwrap();
    assert_eq!(e.text_formatting(0, 0).unwrap().line_spacing, Some(1.75));
    assert_eq!(e.text(0).unwrap().plain_text().matches('•').count(), 1);
}

#[test]
fn word_export_is_beside_search_in_read_and_edit_and_uses_an_immutable_snapshot() {
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    for mode in [Workspace::Read, Workspace::Edit, Workspace::Pages] {
        app.set_workspace(&ctx, mode);
        let out = input_frame(&ctx, &mut app, vec![]);
        let count = out
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, n)| n.label() == Some("Exporter en DOCX ou ODT"))
            .count();
        assert_eq!(count, usize::from(mode != Workspace::Pages));
        if mode != Workspace::Pages {
            let nodes = &out.platform_output.accesskit_update.as_ref().unwrap().nodes;
            let bounds = |label| {
                nodes
                    .iter()
                    .find(|(_, n)| n.label() == Some(label))
                    .unwrap()
                    .1
                    .bounds()
                    .unwrap()
            };
            let search = bounds("Rechercher · Ctrl+F");
            let export = bounds("Exporter en DOCX ou ODT");
            assert!((search.y0 - export.y0).abs() < 1.);
            assert!(export.x0 >= search.x1);
        }
    }
    let original = app.editor.as_ref().unwrap().bytes.clone();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-output/38-export interface.docx");
    app.start_word_export(&ctx, path.clone());
    for _ in 0..1000 {
        app.poll_word_export(&ctx);
        if app.word_export.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(app.word_export.is_none());
    assert!(
        app.message
            .as_deref()
            .unwrap_or_default()
            .starts_with("Document créé"),
        "{:?}",
        app.message
    );
    assert!(std::fs::read(path).unwrap().starts_with(b"PK"));
    assert_eq!(app.editor.as_ref().unwrap().bytes, original);
}

#[test]
fn selected_characters_and_whole_block_formatting_are_distinct() {
    let (ctx, mut app) = setup();
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    app.selected = Some(s.clone());
    app.begin_text(&ctx, s);
    app.inline.as_mut().unwrap().selection.text_range = Some(0..5);
    app.apply_format(
        &ctx,
        pdfinator::rich::Format {
            color: Some([0.8, 0.1, 0.2]),
            ..Default::default()
        },
        None,
    );
    let i = app.inline.as_mut().unwrap();
    assert_eq!(i.selection.rich[0].range, 0..5);
    i.selection.scope_all = true;
    app.apply_format(
        &ctx,
        pdfinator::rich::Format {
            underline: Some(true),
            ..Default::default()
        },
        None,
    );
    assert!(app.finish_text(), "{:?}", app.message);
    let e = app.editor.as_ref().unwrap();
    let data = e.text_formatting(0, 0).unwrap();
    assert!(data.spans.iter().all(|s| s.format.underline == Some(true)));
    assert_eq!(
        data.spans
            .iter()
            .find(|s| s.format.color.is_some())
            .unwrap()
            .range,
        0..5
    );
    assert_eq!(
        e.text(0).unwrap().plain_text().matches("corriger").count(),
        1
    );
}

#[test]
fn unsupported_source_glyph_uses_named_covering_font_without_blocking_message() {
    let (ctx, mut app) = setup();
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    app.selected = Some(s.clone());
    app.begin_text(&ctx, s);
    app.inline.as_mut().unwrap().selection.draft = "Une correction : Ω, œ, l’été.".into();
    assert!(app.finish_text(), "{:?}", app.message);
    assert!(app.message.is_none(), "{:?}", app.message);
    let e = app.editor.as_ref().unwrap();
    let data = e.text_formatting(0, 0).unwrap();
    assert!(!data.font_name.is_empty());
    assert!(e.text(0).unwrap().plain_text().contains('Ω'));
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-output/17-police-adaptee.pdf");
    app.editor.as_mut().unwrap().save(&path).unwrap();
}

#[test]
fn image_body_context_menu_crops_with_a_handle_in_select_edit_and_move() {
    for tool in [Tool::Select, Tool::Edit, Tool::Move, Tool::Hand] {
        let (ctx, mut app) = setup();
        ctx.enable_accesskit();
        app.set_workspace(&ctx, Workspace::Edit);
        app.set_tool(&ctx, tool);
        app.fit = Fit::Manual;
        app.zoom = 0.8;
        let png = pdfinator::ocr::encode_png(&[220, 40, 30, 255].repeat(60 * 40), 60, 40).unwrap();
        app.editor
            .as_mut()
            .unwrap()
            .image_bytes(0, [100., 350., 400., 550.], "test.png", &png, None)
            .unwrap();
        app.refresh(0);
        for _ in 0..3 {
            input_frame(&ctx, &mut app, vec![]);
        }
        let s = app.selection_for(0, Target::Added(0)).unwrap();
        let p = &app.editor.as_ref().unwrap().info.pages[0];
        let local = s.view_rect(p);
        let pos = sheet_origin(&app) + local.center().to_vec2() * app.zoom;
        let out = input_frame(&ctx, &mut app, vec![]);
        assert!(
            !out.platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, n)| n.label() == Some("Rogner une image"))
        );
        for pressed in [true, false] {
            input_frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    pointer_button(pos, egui::PointerButton::Secondary, pressed),
                ],
            );
        }
        let menu = input_frame(&ctx, &mut app, vec![]);
        for label in ["Copier Ctrl+C", "Couper Ctrl+X", "Coller Ctrl+V", "Rogner"] {
            assert!(
                menu.platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .any(|(_, n)| n.label() == Some(label)),
                "{label}, {tool:?}, labels {:?}, context {:?}",
                menu.platform_output
                    .accesskit_update
                    .as_ref()
                    .unwrap()
                    .nodes
                    .iter()
                    .filter_map(|(_, n)| n.label())
                    .collect::<Vec<_>>(),
                app.image_context.as_ref().map(|s| s.target)
            );
        }
        click_named_control(&ctx, &mut app, "Rogner");
        assert!(app.image_crop.is_some(), "{tool:?}: {:?}", app.message);
        for _ in 0..3 {
            input_frame(&ctx, &mut app, vec![]);
        }
        let start = sheet_origin(&app) + local.max.to_vec2() * app.zoom;
        let end = start - Vec2::new(60., 40.) * app.zoom;
        drag_pointer(&ctx, &mut app, start, end);
        let before = app.editor.as_ref().unwrap().bytes.clone();
        click_named_control(&ctx, &mut app, "Appliquer le rognage");
        assert!(app.image_crop.is_none(), "{:?}", app.message);
        let e = app.editor.as_mut().unwrap();
        let (_, cropped) = e
            .image_for_ocr(0, pdfinator::ocr::ImageSource::Original(0))
            .unwrap();
        assert!(
            (cropped[2] - cropped[0] - 240.).abs() < 0.1
                && (cropped[3] - cropped[1] - 160.).abs() < 0.1,
            "{tool:?} {cropped:?}"
        );
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("test-output/43-rogner-{tool:?}.pdf"));
        e.save(&path).unwrap();
        e.undo().unwrap();
        assert_eq!(e.bytes, before);
    }
}

#[test]
fn image_context_cut_and_paste_uses_the_image_snapshot_in_another_pdf() {
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Select);
    app.fit = Fit::Manual;
    app.zoom = 0.8;
    let png = pdfinator::ocr::encode_png(&[20, 180, 60, 255].repeat(60 * 40), 60, 40).unwrap();
    app.editor
        .as_mut()
        .unwrap()
        .image_bytes(0, [100., 350., 400., 550.], "test.png", &png, None)
        .unwrap();
    app.refresh(0);
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    let pos = sheet_origin(&app) + Vec2::new(250., 392.) * app.zoom;
    for pressed in [true, false] {
        input_frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                pointer_button(pos, egui::PointerButton::Secondary, pressed),
            ],
        );
    }
    click_named_control(&ctx, &mut app, "Couper Ctrl+X");
    assert!(app.copied_image.is_some());
    assert!(
        pdfcraft_edit::page_images(&app.editor.as_ref().unwrap().doc, 0)
            .unwrap()
            .is_empty()
    );
    let d = pdfcraft_create::blank(595., 842., 1).unwrap();
    app.insert_document(
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap(),
    );
    app.set_workspace(&ctx, Workspace::Edit);
    // Match the Windows event bridge for an image clipboard: only key-up
    // is forwarded, because key-down attempts a text paste in egui-winit.
    input_frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::V,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::CTRL,
        }],
    );
    assert_eq!(
        pdfcraft_edit::page_images(&app.editor.as_ref().unwrap().doc, 0)
            .unwrap()
            .len(),
        1,
        "{:?}",
        app.message
    );
    app.undo_redo(false);
    assert!(
        pdfcraft_edit::page_images(&app.editor.as_ref().unwrap().doc, 0)
            .unwrap()
            .is_empty()
    );
    app.activate_document(0);
    app.undo_redo(false);
    assert_eq!(
        pdfcraft_edit::page_images(&app.editor.as_ref().unwrap().doc, 0)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn thumbnail_context_menu_duplicates_and_undoes_in_each_workspace() {
    for mode in [Workspace::Read, Workspace::Edit, Workspace::Pages] {
        let (ctx, mut app) = setup();
        ctx.enable_accesskit();
        app.set_workspace(&ctx, mode);
        app.thumbnails = true;
        for _ in 0..3 {
            input_frame(&ctx, &mut app, vec![]);
        }
        let out = input_frame(&ctx, &mut app, vec![]);
        let label = if mode == Workspace::Pages {
            "Page 1 · double-cliquer pour lire"
        } else {
            "Lire la page 1"
        };
        let r = out
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label))
            .unwrap()
            .1
            .bounds()
            .unwrap();
        let pos = Pos2::new(((r.x0 + r.x1) / 2.) as f32, ((r.y0 + r.y1) / 2.) as f32);
        for pressed in [true, false] {
            input_frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    pointer_button(pos, egui::PointerButton::Secondary, pressed),
                ],
            );
        }
        let before = app.editor.as_ref().unwrap().bytes.clone();
        click_named_control(&ctx, &mut app, "Dupliquer");
        assert_eq!(
            app.editor.as_ref().unwrap().info.pages.len(),
            2,
            "{}: {:?}",
            label,
            app.message
        );
        assert!(app.workspace == mode);
        app.undo_redo(false);
        assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    }
}

#[test]
fn modified_tabs_keep_the_open_button_visible_without_moving_the_next_tab() {
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    for n in 0..6 {
        let d = pdfcraft_create::from_text("", "Document", (595., 842.), 12.).unwrap();
        let mut e =
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
        e.path = Some(PathBuf::from(format!("Document long {n}.pdf")));
        app.insert_document(e);
    }
    app.activate_document(0);
    let frame = |app: &mut App| {
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(920., 640.))),
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
        out
    };
    for _ in 0..3 {
        frame(&mut app);
    }
    let before = frame(&mut app);
    let bounds = |out: &egui::FullOutput, label: &str| {
        out.platform_output
            .accesskit_update
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some(label))
            .unwrap()
            .1
            .bounds()
            .unwrap()
    };
    let plus = bounds(&before, "Ouvrir un autre PDF");
    assert!(plus.x1 < 920. && plus.x1 - plus.x0 > 20.);
    app.editor
        .as_mut()
        .unwrap()
        .rewrite(0, 0, Some("Document modifié"), &Default::default())
        .unwrap();
    let after = frame(&mut app);
    assert_eq!(bounds(&after, "Ouvrir un autre PDF"), plus);
    assert!(after.shapes.iter().any(|s|matches!(&s.shape,egui::epaint::Shape::Circle(c) if c.fill==Color32::from_rgb(245,65,80))));
}

#[test]
fn collapsing_properties_keeps_the_selected_drawing_tool_and_scale() {
    let (ctx, mut app) = setup();
    ctx.enable_accesskit();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Draw);
    app.fit = Fit::Manual;
    app.zoom = 0.7;
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    let width = app.canvas.width();
    click_named_control(&ctx, &mut app, "Replier ›");
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    assert!(app.properties_collapsed);
    assert_eq!(app.tool, Tool::Draw);
    assert_eq!(app.zoom, 0.7);
    assert!(app.canvas.width() > width + 150.);
    click_named_control(&ctx, &mut app, "‹");
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    assert!(!app.properties_collapsed);
    assert_eq!(app.tool, Tool::Draw);
    assert!((app.canvas.width() - width).abs() < 1.);
}

#[test]
fn document_tabs_keep_independent_content_history_and_view() {
    let (_, mut app) = setup();
    app.editor
        .as_mut()
        .unwrap()
        .rewrite(0, 0, Some("Premier document corrigé."), &Default::default())
        .unwrap();
    app.zoom = 0.75;
    app.fit = Fit::Manual;
    let first = app.editor.as_ref().unwrap().bytes.clone();
    let doc =
        pdfcraft_create::from_text("Second", "Deuxième document.", (595., 842.), 14.).unwrap();
    app.insert_document(
        Editor::from_bytes(pdfcraft_cos::write_full(&doc, &Default::default()).unwrap()).unwrap(),
    );
    assert_eq!(app.documents.len(), 2);
    assert!(!app.document_dirty(1));
    assert!(app.document_dirty(0));
    app.zoom = 1.5;
    app.fit = Fit::Manual;
    assert!(app.activate_document(0));
    assert_eq!(app.editor.as_ref().unwrap().bytes, first);
    assert_eq!(app.zoom, 0.75);
    assert!(app.editor.as_ref().unwrap().can_undo());
    assert!(app.activate_document(1));
    assert_eq!(app.zoom, 1.5);
    assert!(!app.editor.as_ref().unwrap().can_undo());
    app.close_document(1);
    assert_eq!(app.documents.len(), 1);
    assert_eq!(app.editor.as_ref().unwrap().bytes, first);
}

#[test]
fn bold_text_can_be_enlarged_with_a_corner_without_crashing_or_losing_its_style() {
    for (added, corner) in [(false, 0), (false, 2), (true, 1), (true, 3)] {
        let (ctx, mut app) = setup();
        app.set_workspace(&ctx, Workspace::Edit);
        app.fit = Fit::Manual;
        app.zoom = 0.8;
        app.fonts = typography::FontBook::discover();
        let target = if added {
            let e = app.editor.as_mut().unwrap();
            let index = e.added(0).len();
            e.add_text(
                0,
                AddedText {
                    rect: [60., 550., 320., 585.],
                    text: "Bloc ajouté à agrandir".into(),
                    size: 14.,
                    ..Default::default()
                },
            )
            .unwrap();
            // Formatting converts the added object into native rich text. Its old
            // index now identifies this neighbour and must never be resized instead.
            e.add_text(
                0,
                AddedText {
                    rect: [60., 450., 320., 485.],
                    text: "Voisin intact".into(),
                    size: 14.,
                    ..Default::default()
                },
            )
            .unwrap();
            Target::Added(index)
        } else {
            Target::Block(0)
        };
        let original = app.editor.as_ref().unwrap().bytes.clone();
        let neighbour = app.editor.as_ref().unwrap().added(0).iter().find_map(|c| {
            if let Content::Text(t) = c {
                (t.text == "Voisin intact").then_some(t.clone())
            } else {
                None
            }
        });
        let source = app.selection_for(0, target).unwrap();
        let draft = source.draft.clone();
        let source_width = source
            .view_rect(&app.editor.as_ref().unwrap().info.pages[0])
            .width();
        app.begin_text(&ctx, source);
        app.inline.as_mut().unwrap().selection.scope_all = true;
        app.font_variant(&ctx, true, false);
        assert!(
            !app.inline
                .as_ref()
                .unwrap()
                .selection
                .font_bindings
                .is_empty(),
            "The regression must exercise a newly embedded bold font"
        );
        for _ in 0..200 {
            input_frame(&ctx, &mut app, vec![]);
            if app
                .inline
                .as_ref()
                .is_some_and(|i| i.screen.is_positive() && !i.focus)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let r = app.inline.as_ref().unwrap().screen;
        assert!(r.is_positive());
        let start = [
            r.left_top(),
            r.right_top(),
            r.right_bottom(),
            r.left_bottom(),
        ][corner];
        let delta = Vec2::new(
            if corner == 0 || corner == 3 {
                -48.
            } else {
                48.
            },
            if corner < 2 { -24. } else { 24. },
        );
        drag_pointer(&ctx, &mut app, start, start + delta);
        assert!(app.message.is_none(), "{:?}", app.message);
        let e = app.editor.as_mut().unwrap();
        let blocks = e.blocks(0).unwrap();
        let index = blocks
            .iter()
            .position(|b| b.editor_text == draft)
            .expect("The selected block must survive");
        let b = &blocks[index];
        assert!(
            b.rect[2] - b.rect[0] > source_width as f64 + 40.,
            "Block not enlarged: {:?}",
            b.rect
        );
        assert!(
            (b.size - 14.).abs() < 0.01,
            "Resizing must not stretch the font"
        );
        let rich = e.text_formatting(0, index).unwrap();
        assert!(rich.spans.iter().any(|s| s.format.bold == Some(true)));
        if let Some(neighbour) = neighbour {
            assert!(e.added(0).iter().any(|c| matches!(c, Content::Text(t) if t.text == neighbour.text && t.rect == neighbour.rect)));
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "test-output/34-gras-{}-{corner}.pdf",
            if added { "ajout" } else { "origine" }
        ));
        e.save(&path).unwrap();
        let reopened = Editor::open(&path).unwrap();
        assert_eq!(
            reopened
                .text(0)
                .unwrap()
                .plain_text()
                .matches(&draft)
                .count(),
            1
        );
        e.undo().unwrap();
        e.undo().unwrap();
        assert_eq!(
            e.bytes, original,
            "One undo per formatting/resize operation"
        );
    }
}

#[test]
fn shrinking_text_container_reflows_without_changing_font_size() {
    let (ctx, app) = setup();
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    let mut preview = inline::Inline::new(&ctx, app.editor.as_ref().unwrap(), s, 1.).unwrap();
    preview.reflow = true;
    let mut out = ctx.run_ui(Default::default(), |ui| {
        let a = preview
            .layout(
                ui,
                "Un paragraphe assez long pour changer de ligne.",
                1.,
                400.,
            )
            .unwrap();
        let b = preview
            .layout(
                ui,
                "Un paragraphe assez long pour changer de ligne.",
                1.,
                100.,
            )
            .unwrap();
        assert!(b.rows.len() > a.rows.len());
        assert_eq!(
            a.job.sections[0].format.font_id.size,
            b.job.sections[0].format.font_id.size
        );
        assert!((b.job.sections[0].format.font_id.size - 14.).abs() < 0.01);
    });
    out.textures_delta.clear();
}

#[test]
fn pinch_and_ctrl_wheel_zoom_document_without_zooming_interface() {
    let (ctx, mut app) = setup();
    let pointer = app.canvas.center();
    let old = app.zoom;
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::PointerMoved(pointer), egui::Event::Zoom(1.25)],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    assert!((app.zoom - old * 1.25).abs() < 0.01);
    assert!(app.fit == Fit::Manual);
    assert_eq!(ctx.zoom_factor(), 1.);
    let old = app.zoom;
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![
                egui::Event::ModifiersChanged(Modifiers::CTRL),
                egui::Event::PointerMoved(pointer),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0., 80.),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::CTRL,
                },
            ],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    assert!(app.zoom > old);
    assert_eq!(ctx.zoom_factor(), 1.);
}

#[test]
fn overview_shows_page_thumbnails_and_opens_selected_page() {
    let (ctx, mut app) = setup();
    let d = pdfcraft_create::blank(595., 842., 6).unwrap();
    app.editor = Some(
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap(),
    );
    app.workspace = Workspace::Pages;
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    assert!(!app.thumbnail_visible.is_empty());
    app.page = 4;
    app.open_page(4);
    assert!(app.workspace == Workspace::Read);
    assert_eq!(app.jump, Some(4));
    assert!(app.thumbnails);
}

#[test]
fn dragging_a_thumbnail_reorders_the_actual_pdf() {
    let (ctx, mut app) = setup();
    let mut d = pdfcraft_create::blank(595., 842., 3).unwrap();
    for page in 0..3 {
        pdfcraft_edit::add_content(
            &mut d,
            page,
            &Content::Text(AddedText {
                rect: [40., 700., 500., 740.],
                text: format!("Contenu numéro {}", page + 1),
                ..Default::default()
            }),
        )
        .unwrap();
    }
    app.editor = Some(
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap(),
    );
    app.workspace = Workspace::Pages;
    // A known thumbnail makes the dragged preview observable in the actual paint output.
    app.raster = Default::default();
    let texture = ctx.load_texture(
        "dragged-page-test",
        egui::ColorImage::new([2, 2], vec![Color32::WHITE; 4]),
        Default::default(),
    );
    let texture_id = texture.id();
    app.raster.thumbs.insert(0, texture);
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let columns = ((app.canvas.width() + 20.) / 206.).floor().max(1.) as usize;
    let card_width = ((app.canvas.width() - 20. * (columns - 1) as f32) / columns as f32).min(250.);
    let first = app.canvas.min + Vec2::new(card_width / 2., 90.);
    // Drop in the right half: the insertion marker means *after* the second page.
    let second = first + Vec2::new(card_width * 1.25 + 20., 0.);
    for (stage, events) in [
        vec![
            egui::Event::PointerMoved(first),
            egui::Event::PointerButton {
                pos: first,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        vec![egui::Event::PointerMoved(second)],
        vec![egui::Event::PointerButton {
            pos: second,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    ]
    .into_iter()
    .enumerate()
    {
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
        if stage == 1 {
            assert_eq!(app.page_drag, Some(0));
            let thumbnails: Vec<_> = out
                .shapes
                .iter()
                .filter_map(|s| {
                    if let egui::epaint::Shape::Mesh(mesh) = &s.shape {
                        (mesh.texture_id == texture_id).then(|| mesh.calc_bounds())
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(
                thumbnails.len(),
                2,
                "Source thumbnail and moving preview must both be painted"
            );
            assert!(
                thumbnails
                    .iter()
                    .any(|r| r.left() > second.x && r.width() < 146.)
            );
            assert!(out.shapes.iter().any(|s| matches!(&s.shape,
                egui::epaint::Shape::LineSegment { points, stroke }
                    if stroke.width == 4. && stroke.color == icons::BLUE && points[0].x > second.x
            )), "A visible insertion marker must precede the drop");
            assert!(
                !app.editor.as_ref().unwrap().can_undo(),
                "Moving the pointer must not edit the PDF"
            );
        }
    }
    let e = app.editor.as_ref().unwrap();
    assert!(e.text(0).unwrap().plain_text().contains("numéro 2"));
    assert!(e.text(1).unwrap().plain_text().contains("numéro 1"));
    assert!(e.can_undo());
    let reopened = Editor::from_bytes((*e.bytes).clone()).unwrap();
    assert!(reopened.text(0).unwrap().plain_text().contains("numéro 2"));
    assert!(reopened.text(1).unwrap().plain_text().contains("numéro 1"));
    assert_eq!(app.page_drag, None);
}

#[test]
fn page_drag_scrolls_to_hidden_rows_and_escape_cancels_the_drop() {
    let (ctx, mut app) = setup();
    let d = pdfcraft_create::blank(595., 842., 24).unwrap();
    app.editor = Some(
        Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap(),
    );
    app.raster = Default::default();
    app.workspace = Workspace::Pages;
    let before = app.editor.as_ref().unwrap().bytes.clone();
    let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
    out.textures_delta.clear();
    let first = app.canvas.min + Vec2::new(100., 90.);
    let bottom = Pos2::new(first.x, app.canvas.bottom() - 4.);
    let initially_visible = app.thumbnail_visible.clone();
    for events in [
        vec![
            egui::Event::PointerMoved(first),
            egui::Event::PointerButton {
                pos: first,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        vec![egui::Event::PointerMoved(bottom)],
    ] {
        let mut out = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        out.textures_delta.clear();
    }
    for _ in 0..90 {
        let mut out = ctx.run_ui(Default::default(), |ui| app.frame_ui(ui));
        out.textures_delta.clear();
    }
    assert_eq!(app.page_drag, Some(0));
    assert!(
        app.thumbnail_visible
            .iter()
            .any(|p| !initially_visible.contains(p)),
        "Holding near the edge must reveal hidden pages"
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
    assert_eq!(app.page_drag, None);
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![egui::Event::PointerButton {
                pos: bottom,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
    assert!(!app.editor.as_ref().unwrap().can_undo());
}

#[test]
fn plain_wheel_scrolls_without_changing_zoom() {
    let (ctx, mut app) = setup();
    let old = app.zoom;
    let pointer = app.canvas.center();
    let mut out = ctx.run_ui(
        egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0., -100.),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    assert_eq!(app.zoom, old);
    assert!(app.scroll_offset.y > 0.);
}
#[test]
fn enter_commits_source_text_and_keeps_previous_raster_visible() {
    let (ctx, mut app) = setup();
    let selection = app.selection_for(0, Target::Block(0)).unwrap();
    app.begin_text(&ctx, selection);
    app.inline.as_mut().unwrap().selection.draft = "Texte corrigé directement.".into();
    let texture = ctx.load_texture(
        "previous-page",
        egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
        Default::default(),
    );
    let id = texture.id();
    app.raster.frames.insert(
        0,
        raster::Frame {
            texture,
            scale: 1.,
            generation: 1,
        },
    );
    let input = egui::RawInput {
        events: vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |_ui| app.shortcuts(&ctx));
    output.textures_delta.clear();
    assert_eq!(
        app.raster.frames.get(&0).unwrap().texture.id(),
        id,
        "The old page must remain visible until replacement"
    );
    let e = app.editor.as_ref().unwrap();
    let text = e.text(0).unwrap().plain_text();
    assert!(text.contains("corrigé directement"), "{text}");
    assert!(!text.contains("original"));
    assert!(app.inline.as_ref().is_some_and(|i| !i.committed));
}
#[test]
fn fit_width_keeps_scale_and_vertical_anchor_when_properties_recenter_the_page() {
    let (ctx, mut app) = setup();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Edit);
    let b = app.editor.as_ref().unwrap().blocks(0).unwrap().remove(0);
    app.editor
        .as_mut()
        .unwrap()
        .move_block(0, 0, [0., 390. - b.rect[1]])
        .unwrap();
    app.refresh(0);
    app.scroll_offset.y = 280.;
    for _ in 0..4 {
        input_frame(&ctx, &mut app, vec![]);
    }
    let before = (app.zoom, app.scroll_offset, sheet_origin(&app));
    let p = &app.editor.as_ref().unwrap().info.pages[0];
    let local = app.selection_for(0, Target::Block(0)).unwrap().view_rect(p);
    let pos = before.2 + local.center().to_vec2() * app.zoom;
    assert!(app.canvas.contains(pos), "Test target must be visible");
    for pressed in [true, false] {
        input_frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                pointer_button(pos, egui::PointerButton::Primary, pressed),
            ],
        );
    }
    for _ in 0..200 {
        input_frame(&ctx, &mut app, vec![]);
        if app
            .inline
            .as_ref()
            .is_some_and(|i| i.screen.is_positive() && !i.focus)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(app.right_rect.is_positive());
    assert!(app.inline.is_some());
    assert_eq!(
        app.zoom, before.0,
        "Opening properties changed the fit zoom"
    );
    assert!(
        (sheet_origin(&app).y - before.2.y).abs() < 0.1,
        "Opening properties changed the vertical reading position"
    );
    app.inline.as_mut().unwrap().selection.draft = "Exercice 2".into();
    assert!(app.commit_text_keep_focus(&ctx));
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    assert_eq!(app.zoom, before.0);
    let blank = Pos2::new(app.canvas.right() - 30., app.canvas.bottom() - 80.);
    for pressed in [true, false] {
        input_frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(blank),
                pointer_button(blank, egui::PointerButton::Primary, pressed),
            ],
        );
    }
    for _ in 0..4 {
        input_frame(&ctx, &mut app, vec![]);
    }
    assert!(app.selected.is_none(), "Blank click must leave editing");
    assert!(!app.right_rect.is_positive());
    assert_eq!(
        app.zoom, before.0,
        "Closing properties changed the fit zoom"
    );
    assert!((app.scroll_offset - before.1).length() < 0.1);
    assert!(
        (sheet_origin(&app) - before.2).length() < 0.1,
        "Page moved on closing"
    );
    assert!(
        app.editor
            .as_ref()
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .contains("Exercice 2")
    );
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1180., 720.))),
            ..Default::default()
        },
        |ui| app.frame_ui(ui),
    );
    out.textures_delta.clear();
    assert!(
        app.zoom > before.0,
        "Fit must still follow an actual window resize"
    );
}

#[test]
fn text_tool_drag_selects_characters_inside_the_editor_instead_of_starting_a_marquee() {
    let (ctx, mut app) = setup();
    app.set_workspace(&ctx, Workspace::Edit);
    app.set_tool(&ctx, Tool::Edit);
    app.begin_text(&ctx, app.selection_for(0, Target::Block(0)).unwrap());
    for _ in 0..200 {
        input_frame(&ctx, &mut app, vec![]);
        if app
            .inline
            .as_ref()
            .is_some_and(|i| i.screen.is_positive() && !i.focus)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let r = app.inline.as_ref().unwrap().screen;
    drag_pointer(
        &ctx,
        &mut app,
        r.left_center() + Vec2::new(16., 0.),
        r.left_center() + Vec2::new(90., 0.),
    );
    let range = egui::TextEdit::load_state(&ctx, egui::Id::new("direct-text"))
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert_ne!(
        range.primary.index, range.secondary.index,
        "Dragging must select characters"
    );
    assert!(app.marquee.selected.is_empty());
    assert!(app.inline.as_ref().is_some_and(|i| !i.committed));
    assert!(!app.editor.as_ref().unwrap().dirty());
}

#[test]
fn typing_formatting_and_enter_keep_the_caret_zoom_and_document_position() {
    use egui::text::{CCursor, CCursorRange};
    let (ctx, mut app) = setup();
    app.workspace = Workspace::Edit;
    app.tool = Tool::Edit;
    app.fit = Fit::Manual;
    app.zoom = 1.4;
    app.thumbnails = false;
    app.begin_text(&ctx, app.selection_for(0, Target::Block(0)).unwrap());
    for _ in 0..200 {
        input_frame(&ctx, &mut app, vec![]);
        if app
            .inline
            .as_ref()
            .is_some_and(|i| i.screen.is_finite() && !i.focus)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let id = egui::Id::new("direct-text");
    let mut state = egui::TextEdit::load_state(&ctx, id).unwrap();
    state
        .cursor
        .set_char_range(Some(CCursorRange::one(CCursor::new(5))));
    state.store(&ctx, id);
    ctx.memory_mut(|m| m.request_focus(id));
    input_frame(&ctx, &mut app, vec![egui::Event::Text(" nouveau".into())]);
    assert!(ctx.memory(|m| m.has_focus(id)));
    assert!(
        app.inline
            .as_ref()
            .unwrap()
            .selection
            .draft
            .starts_with("Texte nouveau")
    );
    let caret = egui::TextEdit::load_state(&ctx, id)
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    let position = app.scroll_offset;
    app.apply_format(
        &ctx,
        pdfinator::rich::Format {
            color: Some([0.1, 0.2, 0.7]),
            ..Default::default()
        },
        None,
    );
    for _ in 0..3 {
        input_frame(&ctx, &mut app, vec![]);
    }
    assert!(ctx.memory(|m| m.has_focus(id)));
    input_frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    for _ in 0..50 {
        input_frame(&ctx, &mut app, vec![]);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(app.message.is_none(), "{:?}", app.message);
    assert!(
        ctx.memory(|m| m.has_focus(id)),
        "Focus after commit: {:?}",
        ctx.memory(|m| m.focused())
    );
    assert_eq!(
        egui::TextEdit::load_state(&ctx, id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap(),
        caret
    );
    assert_eq!(app.zoom, 1.4);
    assert!((app.scroll_offset - position).length() < 0.01);
    input_frame(&ctx, &mut app, vec![egui::Event::Text(" ici".into())]);
    assert!(
        app.inline
            .as_ref()
            .unwrap()
            .selection
            .draft
            .starts_with("Texte nouveau ici")
    );
    let saved = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-output/32-focus-enregistrement.pdf");
    app.editor.as_mut().unwrap().save(&saved).unwrap();
    assert!(app.save_keep_focus(&ctx, false), "{:?}", app.message);
    for _ in 0..1000 {
        input_frame(&ctx, &mut app, vec![]);
        if app.work.saving.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(app.work.saving.is_none());
    assert!(ctx.memory(|m| m.has_focus(id)));
    assert_eq!(app.zoom, 1.4);
    assert!((app.scroll_offset - position).length() < 0.01);
    assert!(
        Editor::open(&saved)
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .contains("nouveau ici")
    );
    assert!(app.finish_text());
    assert!(
        app.editor
            .as_ref()
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .contains("nouveau ici")
    );
}
#[test]
fn outside_click_commits_and_escape_discards() {
    let (ctx, mut app) = setup();
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    app.begin_text(&ctx, s);
    let inline = app.inline.as_mut().unwrap();
    inline.selection.draft = "Correction par clic extérieur".into();
    inline.screen = Rect::from_min_size(Pos2::new(200., 100.), Vec2::new(200., 50.));
    app.canvas = Rect::from_min_max(Pos2::new(100., 40.), Pos2::new(950., 700.));
    let input = egui::RawInput {
        events: vec![
            egui::Event::PointerMoved(Pos2::new(600., 400.)),
            egui::Event::PointerButton {
                pos: Pos2::new(600., 400.),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.frame_ui(ui));
    output.textures_delta.clear();
    assert!(
        app.editor
            .as_ref()
            .unwrap()
            .text(0)
            .unwrap()
            .plain_text()
            .contains("clic extérieur")
    );
    app.inline = None;
    let s = app.selection_for(0, Target::Block(0)).unwrap();
    app.begin_text(&ctx, s);
    app.inline.as_mut().unwrap().selection.draft = "Ne pas garder".into();
    let before = app.editor.as_ref().unwrap().bytes.clone();
    let input = egui::RawInput {
        events: vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |_| app.shortcuts(&ctx));
    output.textures_delta.clear();
    assert!(app.inline.is_none());
    assert_eq!(app.editor.as_ref().unwrap().bytes, before);
}
#[test]
fn source_free_preview_preserves_page_colour_and_does_not_edit_document() {
    let (_, app) = setup();
    let e = app.editor.as_ref().unwrap();
    let before = e.bytes.clone();
    let preview = e.preview_without(0, Some(0), None).unwrap();
    let parsed = Editor::from_bytes(preview.as_ref().clone()).unwrap();
    assert!(parsed.text(0).unwrap().plain_text().trim().is_empty());
    assert_eq!(e.bytes, before);
    assert!(!e.dirty());
}

#[test]
fn properties_recenter_the_page_in_the_visible_canvas_without_changing_zoom() {
    let (ctx, mut app) = setup();
    app.workspace = Workspace::Edit;
    app.tool = Tool::Edit;
    input_frame(&ctx, &mut app, vec![]);
    let zoom = app.zoom;
    app.selected = app.selection_for(0, Target::Block(0));
    input_frame(&ctx, &mut app, vec![]);
    input_frame(&ctx, &mut app, vec![]);
    for collapsed in [false, true, false] {
        app.properties_collapsed = collapsed;
        input_frame(&ctx, &mut app, vec![]);
        input_frame(&ctx, &mut app, vec![]);
        assert_eq!(app.zoom, zoom);
        let page = app.editor.as_ref().unwrap().info.pages[0].width;
        let center = sheet_origin(&app).x + page * app.zoom / 2.;
        assert!(
            (center - app.canvas.center().x).abs() <= 10.,
            "page {center}, canvas {:?}",
            app.canvas
        );
    }
}
