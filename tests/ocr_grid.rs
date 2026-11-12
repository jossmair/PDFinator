#![cfg(windows)]
use pdfcraft_cos::{Dict, Object, Stream};

#[test]
fn isolated_coloured_grid_digits_are_recognised_as_individual_words() {
    let mut doc = pdfcraft_create::blank(400., 400., 1).unwrap();
    let mut f = Dict::new();
    f.set(b"Subtype".to_vec(), Object::name("Type1"));
    f.set(b"BaseFont".to_vec(), Object::name("Helvetica-Bold"));
    let mut fonts = Dict::new();
    fonts.set(b"F1".to_vec(), Object::Dict(f));
    let mut resources = Dict::new();
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    let mut content = String::from("0 0 0 RG 1 w ");
    for p in [40, 140, 240, 340] {
        content.push_str(&format!("{p} 40 m {p} 340 l S 40 {p} m 340 {p} l S "));
    }
    for n in 0..9 {
        let (x, y) = (80 + (n % 3) * 100, 280 - (n / 3) * 100);
        let color = if n % 2 == 0 { "0.6 0.6 1" } else { "0 1 0" };
        content.push_str(&format!(
            "BT /F1 28 Tf {color} rg 1 0 0 1 {x} {y} Tm ({}) Tj ET ",
            n + 1
        ));
    }
    let stream = doc.add(Stream::flate(Dict::new(), content.as_bytes()));
    let page = pdfcraft_model::pages(&doc)[0].obj;
    doc.update_dict(page, |d| {
        d.set(b"Resources".to_vec(), Object::Dict(resources));
        d.set(b"Contents".to_vec(), Object::Ref(stream));
    })
    .unwrap();
    let bytes = pdfcraft_cos::write_full(&doc, &Default::default()).unwrap();
    let mut renderer =
        pdfcraft_render::PageRenderer::new(std::sync::Arc::new(bytes), Default::default());
    let image = renderer.render(pdfcraft_render::RenderRequest {
        page: 0,
        scale: 2.,
        ..Default::default()
    });
    assert!(image.error.is_none());
    let result = pdfinator::ocr::recognize(&image.rgba, image.width, image.height).unwrap();
    for n in 1..=9 {
        assert!(
            result
                .lines
                .iter()
                .flat_map(|l| &l.words)
                .any(|w| w.text == n.to_string()),
            "Missing {n}: {}",
            result.text()
        );
    }
}
