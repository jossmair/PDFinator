//! Original synthetic acceptance document: real source text over coloured vector backgrounds.
use pdfcraft_cos::{Dict, Object, SaveOptions, Stream};
fn text(out: &mut Vec<u8>, font: &str, size: f64, x: f64, y: f64, value: &str) {
    out.extend(format!("BT /{font} {size} Tf 1 0 0 1 {x} {y} Tm ").as_bytes());
    out.extend(pdfcraft_fonts::literal(&pdfcraft_fonts::win_ansi(value)));
    out.extend(b" Tj ET\n");
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = pdfcraft_create::blank(595., 842., 3)?;
    let mut resources = Dict::new();
    let mut fonts = Dict::new();
    for (resource, base) in [("F1", "Helvetica"), ("F2", "Helvetica-Bold")] {
        let mut f = Dict::new();
        f.set(b"Type".to_vec(), Object::name("Font"));
        f.set(b"Subtype".to_vec(), Object::name("Type1"));
        f.set(b"BaseFont".to_vec(), Object::name(base));
        f.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
        fonts.set(resource.as_bytes().to_vec(), Object::Ref(doc.add(f)));
    }
    resources.set(b"Font".to_vec(), Object::Dict(fonts));
    for (index, page) in pdfcraft_model::pages(&doc).iter().enumerate() {
        let mut out = Vec::new();
        out.extend(b"q 0.99 0.89 0.89 rg 42 728 511 67 re f Q\n0.06 0.11 0.20 rg\n");
        text(
            &mut out,
            "F2",
            25.,
            66.,
            753.,
            if index == 0 {
                "Chapitre 3 : Proportionnalité"
            } else if index == 1 {
                "Exercices : à vous de jouer"
            } else {
                "Les bons gestes dans PDFinator"
            },
        );
        out.extend(b"q 1 0.25 0.40 rg 42 661 30 30 re f Q\n1 1 1 rg\n");
        text(&mut out, "F2", 18., 51., 669., "1");
        out.extend(b"0.06 0.11 0.20 rg\n");
        text(&mut out, "F2", 17., 84., 668., "Définition");
        out.extend(b"q 0.98 0.96 0.91 rg 42 554 511 91 re f Q\n");
        for (line, value) in [
            "Deux grandeurs sont proportionnelles lorsqu’il existe",
            "un même nombre qui permet de passer de l’une à l’autre.",
            "Ce nombre s’appelle le coefficient de proportionnalité.",
        ]
        .iter()
        .enumerate()
        {
            text(&mut out, "F1", 13., 58., 619. - line as f64 * 22., value);
        }
        out.extend(b"q 1 0.94 0.28 rg 246 500 104 28 re f Q\n");
        text(&mut out, "F2", 18., 261., 507., "y = k × x");
        out.extend(b"q 0.22 0.39 0.94 RG 1.3 w 78 272 m 268 272 l 268 464 l S 78 272 m 245 439 l S 0.90 0.93 0.98 RG 0.5 w ");
        for n in 0..6 {
            let x = 78 + n * 38;
            let y = 272 + n * 38;
            out.extend(format!("{x} 272 m {x} 462 l S 78 {y} m 268 {y} l S ").as_bytes());
        }
        out.extend(b"Q\n");
        for (n, (x, y)) in [(116, 310), (154, 348), (192, 386), (230, 424)]
            .into_iter()
            .enumerate()
        {
            out.extend(format!("q 0.20 0.38 0.95 rg {} {} 5 5 re f Q\n", x - 2, y - 2).as_bytes());
            text(
                &mut out,
                "F1",
                10.,
                x as f64 - 3.,
                254.,
                &(n + 1).to_string(),
            );
        }
        out.extend(b"q 0.90 0.96 0.98 rg 304 310 249 141 re f 0.17 0.68 0.60 rg 304 418 249 33 re f Q\n0.06 0.11 0.20 rg\n");
        text(&mut out, "F2", 16., 319., 430., "Propriété");
        for (line, value) in [
            "Dans un tableau de proportionnalité,",
            "on multiplie chaque nombre de la",
            "première ligne par le même coefficient.",
        ]
        .iter()
        .enumerate()
        {
            text(&mut out, "F1", 11., 318., 395. - line as f64 * 22., value);
        }
        out.extend(b"q 0.99 0.92 0.94 rg 42 117 511 115 re f 0.96 0.17 0.35 rg 42 204 511 28 re f Q\n1 1 1 rg\n");
        text(&mut out, "F2", 15., 58., 212., "À retenir");
        out.extend(b"0.06 0.11 0.20 rg\n");
        for (line, value) in [
            "Ouvrez Modifier pour voir les blocs de texte.",
            "Cliquez sur une phrase : modifiez-la directement, puis Entrée.",
            "Glissez un bord pour déplacer ; un coin pour redimensionner.",
        ]
        .iter()
        .enumerate()
        {
            text(&mut out, "F1", 12., 58., 182. - line as f64 * 21., value);
        }
        text(
            &mut out,
            "F1",
            9.,
            42.,
            54.,
            &format!(
                "Document d’essai original • PDFinator • page {} / 3",
                index + 1
            ),
        );
        let stream = doc.add(Stream::flate(Dict::new(), &out));
        doc.update_dict(page.obj, |d| {
            d.set(b"Resources".to_vec(), Object::Dict(resources.clone()));
            d.set(b"Contents".to_vec(), Object::Ref(stream));
        })?;
    }
    std::fs::create_dir_all("test-output")?;
    std::fs::write(
        "test-output/Atelier-PDFinator.pdf",
        pdfcraft_cos::write_full(&doc, &SaveOptions::default())?,
    )?;
    Ok(())
}
