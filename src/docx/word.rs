use super::*;
use pdfcraft_export::Zip;

fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| {
            *c == '\t'
                || *c == '\n'
                || *c == '\r'
                || (*c >= ' ' && *c != '\u{fffe}' && *c != '\u{ffff}')
        })
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            _ => c.to_string(),
        })
        .collect()
}
fn twips(v: f64) -> i64 {
    (v.max(0.) * 20.).round() as i64
}
fn color(c: [f64; 3]) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        (c[0].clamp(0., 1.) * 255.).round() as u8,
        (c[1].clamp(0., 1.) * 255.).round() as u8,
        (c[2].clamp(0., 1.) * 255.).round() as u8
    )
}
fn run(r: &Run) -> String {
    let mut properties = format!(
        "<w:rFonts w:ascii=\"{}\" w:hAnsi=\"{}\"/><w:sz w:val=\"{}\"/><w:szCs w:val=\"{}\"/><w:color w:val=\"{}\"/>",
        xml(&r.font),
        xml(&r.font),
        (r.size * 2.).round() as i64,
        (r.size * 2.).round() as i64,
        color(r.color)
    );
    if r.bold {
        properties.push_str("<w:b/>");
    }
    if r.italic {
        properties.push_str("<w:i/>");
    }
    if r.underline {
        properties.push_str("<w:u w:val=\"single\"/>");
    }
    if r.strike {
        properties.push_str("<w:strike/>");
    }
    if r.script != 0 {
        properties.push_str(&format!(
            "<w:vertAlign w:val=\"{}\"/>",
            if r.script > 0 {
                "superscript"
            } else {
                "subscript"
            }
        ));
    }
    if let Some(c) = r.background {
        properties.push_str(&format!("<w:shd w:val=\"clear\" w:fill=\"{}\"/>", color(c)));
    }
    format!(
        "<w:r><w:rPr>{properties}</w:rPr>{}</w:r>",
        r.text
            .split('\n')
            .map(|s| format!("<w:t xml:space=\"preserve\">{}</w:t>", xml(s)))
            .collect::<Vec<_>>()
            .join("<w:br/>")
    )
}
fn paragraph(p: &Paragraph, left: f64, width: f64, before: f64, in_cell: bool) -> String {
    let size = paragraph_height(p);
    let spacing = p.spacing.map_or_else(
        || format!("w:line=\"{}\" w:lineRule=\"exact\"", twips(size)),
        |v| {
            format!(
                "w:line=\"{}\" w:lineRule=\"auto\"",
                (v * 240.).round() as i64
            )
        },
    );
    let indent = if in_cell || matches!(p.align, "center" | "right") {
        0
    } else {
        twips(p.rect[0] - left)
    };
    let right = 0;
    let leader = dotted(p);
    if leader {
        let mut content = leader_prefix(p)
            .unwrap()
            .iter()
            .map(run)
            .collect::<String>();
        if let Some(r) = p.runs.last() {
            let mut r = r.clone();
            r.text.clear();
            content.push_str(&run(&r).replace("<w:t xml:space=\"preserve\"></w:t>", "<w:tab/>"));
        }
        let stop = twips(
            indent as f64 / 20.
                + (p.rect[2] - p.rect[0])
                    .min(width - indent as f64 / 20. - 2.)
                    .max(4.),
        );
        return format!(
            "<w:p><w:pPr><w:spacing w:before=\"{}\" w:after=\"0\" {spacing}/><w:ind w:left=\"{indent}\"/><w:tabs><w:tab w:val=\"right\" w:leader=\"dot\" w:pos=\"{stop}\"/></w:tabs></w:pPr>{content}</w:p>",
            twips(before)
        );
    }
    let mut runs = p.runs.clone();
    let bullet = runs.first().is_some_and(|r| r.text.starts_with("• "));
    if bullet {
        runs[0].text = runs[0].text["• ".len()..].into();
    }
    let numbering = if bullet {
        "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>"
    } else {
        ""
    };
    let mut content = String::new();
    let mut tabs = String::new();
    let mut i = 0;
    while i < runs.len() {
        if tab_before(&runs, i)
            && let Some(rect) = runs[i].rect
        {
            tabs.push_str(&format!(
                "<w:tab w:val=\"left\" w:pos=\"{}\"/>",
                twips(rect[0] - left)
            ));
            content.push_str("<w:r><w:tab/></w:r>");
        }
        if let Some(f) = &runs[i].fraction {
            let mrun = |rs: &[Run]| {
                rs.iter().map(|r| format!("<m:r><m:rPr><m:sty m:val=\"p\"/></m:rPr><w:rPr><w:rFonts w:ascii=\"{}\" w:hAnsi=\"{}\"/><w:sz w:val=\"{}\"/><w:color w:val=\"{}\"/><w:position w:val=\"2\"/></w:rPr><m:t>{}</m:t></m:r>",xml(&r.font),xml(&r.font),(r.size*2.).round() as i64,color(r.color),xml(r.text.trim()))).collect::<String>()
            };
            content.push_str(&format!("<m:oMath><m:f><m:fPr><m:type m:val=\"bar\"/><m:ctrlPr><w:rPr><w:position w:val=\"2\"/></w:rPr></m:ctrlPr></m:fPr><m:num><m:argPr><m:argSz m:val=\"0\"/></m:argPr>{}</m:num><m:den><m:argPr><m:argSz m:val=\"0\"/></m:argPr>{}</m:den></m:f></m:oMath>",mrun(&f.numerator),mrun(&f.denominator)));
            i += 1;
        } else if let Some(base) = angle(&runs, i) {
            content.push_str(&format!("<m:oMath><m:acc><m:accPr><m:chr m:val=\"&#x302;\"/></m:accPr><m:e><m:r><m:rPr><m:sty m:val=\"i\"/></m:rPr><w:rPr><w:rFonts w:ascii=\"{}\" w:hAnsi=\"{}\"/><w:sz w:val=\"{}\"/></w:rPr><m:t>{}</m:t></m:r></m:e></m:acc></m:oMath>", xml(&base.font), xml(&base.font), (base.size * 2.).round() as i64, xml(base.text.trim())));
            let mut trailing = base.clone();
            trailing.text = base.text[base.text.trim_end().len()..].into();
            if !trailing.text.is_empty() {
                content.push_str(&run(&trailing));
            }
            i += 2;
        } else {
            let mut r = runs[i].clone();
            if i + 1 < runs.len() && tab_before(&runs, i + 1) {
                r.text = r.text.trim_end().into();
            }
            content.push_str(&run(&r));
            i += 1;
        }
    }
    format!(
        "<w:p><w:pPr>{numbering}<w:widowControl w:val=\"0\"/><w:jc w:val=\"{}\"/><w:spacing w:before=\"{}\" w:after=\"0\" {spacing}/><w:ind w:left=\"{indent}\" w:right=\"{right}\"/><w:tabs>{tabs}</w:tabs></w:pPr>{}</w:p>",
        p.align,
        twips(before),
        content
    )
}
fn table(t: &Table, left: f64) -> String {
    let mut out = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:tblInd w:w=\"{}\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/><w:tblBorders>",
        twips(t.rect[2] - t.rect[0]),
        twips(t.rect[0] - left + 1.5)
    );
    for edge in ["top", "left", "bottom", "right", "insideH", "insideV"] {
        out.push_str(&format!(
            "<w:{edge} w:val=\"single\" w:sz=\"{}\" w:color=\"{}\"/>",
            (t.border * 8.).round() as i64,
            color(t.color)
        ));
    }
    out.push_str("</w:tblBorders><w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"30\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"30\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>");
    for col in t.xs.windows(2) {
        out.push_str(&format!("<w:gridCol w:w=\"{}\"/>", twips(col[1] - col[0])));
    }
    out.push_str("</w:tblGrid>");
    for (i, row) in t.rows.iter().enumerate() {
        out.push_str(&format!(
            "<w:tr><w:trPr><w:cantSplit/><w:trHeight w:val=\"{}\" w:hRule=\"exact\"/></w:trPr>",
            twips(t.ys[i + 1] - t.ys[i])
        ));
        for cell in row {
            let width = t.xs[cell.column + cell.span] - t.xs[cell.column];
            out.push_str(&format!("<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/><w:gridSpan w:val=\"{}\"/><w:vAlign w:val=\"top\"/>", twips(width), cell.span));
            if let Some(merge) = cell.merge {
                out.push_str(&format!("<w:vMerge w:val=\"{merge}\"/>"));
            }
            if let Some(fill) = cell.fill {
                out.push_str(&format!(
                    "<w:shd w:val=\"clear\" w:fill=\"{}\"/>",
                    color(fill)
                ));
            }
            out.push_str("</w:tcPr>");
            if cell.paragraphs.is_empty() {
                out.push_str("<w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/></w:pPr></w:p>");
            } else {
                let mut cy = t.ys[i];
                for p in &cell.paragraphs {
                    let before = (p.rect[1] - cy).max(0.);
                    out.push_str(&paragraph(
                        p,
                        t.xs[cell.column] + 1.5,
                        width - 3.,
                        before,
                        false,
                    ));
                    cy = cy.max(p.rect[1]) + paragraph_height(p);
                }
            }
            out.push_str("</w:tc>");
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out
}
// Older Writer versions ignore OMML run sizes and force 12 pt. Keep enlarged
// fractions as grouped, editable text with a paragraph border for the bar.
fn large_fraction(p: &Paragraph, width: f64, before: f64) -> Option<String> {
    let r = p.runs.first()?;
    let f = r.fraction.as_ref()?;
    if r.size < 14. {
        return None;
    }
    let top = |runs: &[Run]| {
        runs.iter()
            .filter_map(|r| r.rect.map(|v| v[1]))
            .fold(f64::INFINITY, f64::min)
    };
    let bottom = |runs: &[Run]| {
        runs.iter()
            .filter_map(|r| r.rect.map(|v| v[3]))
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let num_top = top(&f.numerator);
    let num_bottom = bottom(&f.numerator);
    let den_top = top(&f.denominator);
    if ![num_top, num_bottom, den_top].iter().all(|v| v.is_finite()) {
        return None;
    }
    let mut out = String::from(
        "<w:sdt><w:sdtPr><w:tag w:val=\"PDFinatorFraction\"/></w:sdtPr><w:sdtContent>",
    );
    for (i, runs) in [&f.numerator, &f.denominator].into_iter().enumerate() {
        let height = bottom(runs) - top(runs);
        let gap = (den_top - num_bottom).max(0.);
        let border = if i == 0 {
            format!(
                "<w:pBdr><w:bottom w:val=\"single\" w:sz=\"4\" w:color=\"{}\" w:space=\"{}\"/></w:pBdr>",
                color(r.color),
                (gap / 2.).round() as i64
            )
        } else {
            String::new()
        };
        let offset = runs.first()?.rect?[0] - p.rect[0];
        let tab = if offset > 0.05 {
            "<w:r><w:tab/></w:r>"
        } else {
            ""
        };
        let mut content = runs.iter().map(run).collect::<String>();
        if i == 1 {
            // The paragraph border participates in Writer's baseline spacing.
            let lift = ((gap / 2.).round() + 0.25) * 2.;
            content = content.replace(
                "</w:rPr>",
                &format!("<w:position w:val=\"{}\"/></w:rPr>", lift.round() as i64),
            );
        }
        out.push_str(&format!("<w:p><w:pPr><w:spacing w:before=\"{}\" w:after=\"{}\" w:line=\"{}\" w:lineRule=\"exact\"/><w:ind w:left=\"0\" w:right=\"{}\"/><w:tabs><w:tab w:val=\"left\" w:pos=\"{}\"/></w:tabs>{border}</w:pPr>{tab}{content}</w:p>", twips(if i == 0 { before } else { 0. }), twips(if i == 0 { gap } else { 0. }), twips(height), twips(width - (p.rect[2] - p.rect[0])), twips(offset)));
    }
    out.push_str("</w:sdtContent></w:sdt>");
    Some(out)
}

// Word reduces inline fraction arguments even when an explicit size is set.
// Keep each equation in a native table cell as display math, beside ordinary
// flowing prose. This preserves the PDF size in both Word and Writer.
fn math_row(p: &Paragraph, left: f64, before: f64) -> String {
    let mut parts = math_parts(p);
    let edges: Vec<_> = parts
        .iter()
        .map(|p| p.rect[0])
        .chain(std::iter::once(p.rect[2] + 3.))
        .collect();
    let mut s = String::new();
    if before > 0. {
        s.push_str(&format!("<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"exact\"/></w:pPr></w:p>",twips(before)));
    }
    s.push_str(&format!("<w:tbl><w:tblPr><w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:tblInd w:w=\"{}\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/><w:tblBorders><w:top w:val=\"nil\"/><w:left w:val=\"nil\"/><w:bottom w:val=\"nil\"/><w:right w:val=\"nil\"/><w:insideH w:val=\"nil\"/><w:insideV w:val=\"nil\"/></w:tblBorders><w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"0\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"0\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>",twips(edges.last().unwrap()-edges[0]),twips(edges[0]-left)));
    for pair in edges.windows(2) {
        s.push_str(&format!(
            "<w:gridCol w:w=\"{}\"/>",
            twips(pair[1] - pair[0])
        ));
    }
    s.push_str("</w:tblGrid><w:tr><w:trPr><w:cantSplit/></w:trPr>");
    for (n, part) in parts.iter_mut().enumerate() {
        if let Some(r) = part.runs.last_mut() {
            r.text = r.text.trim_end().into();
        }
        let width = edges[n + 1] - edges[n];
        s.push_str(&format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/><w:vAlign w:val=\"top\"/></w:tcPr>",
            twips(width)
        ));
        let before = (part.rect[1] - p.rect[1]).max(0.);
        if let Some(contents) = large_fraction(part, width, before) {
            s.push_str(&contents);
            s.push_str("</w:tc>");
            continue;
        }
        let contents = paragraph(part, edges[n], width, before, true);
        s.push_str(
            &contents
                .replace(
                    "<m:oMath>",
                    "<m:oMathPara><m:oMathParaPr><m:jc m:val=\"left\"/></m:oMathParaPr><m:oMath>",
                )
                .replace("</m:oMath>", "</m:oMath></m:oMathPara>"),
        );
        s.push_str("</w:tc>");
    }
    s.push_str("</w:tr></w:tbl><w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"10\" w:lineRule=\"exact\"/></w:pPr></w:p>");
    s
}
fn height(item: &Item) -> f64 {
    let fraction_border = if let Item::Paragraph(p) = item {
        p.runs
            .iter()
            .filter(|r| r.size >= 14.)
            .filter_map(|r| r.fraction.as_ref())
            .filter_map(|f| {
                let num_bottom = f
                    .numerator
                    .iter()
                    .filter_map(|r| r.rect.map(|v| v[3]))
                    .reduce(f64::max)?;
                let den_top = f
                    .denominator
                    .iter()
                    .filter_map(|r| r.rect.map(|v| v[1]))
                    .reduce(f64::min)?;
                Some(((den_top - num_bottom).max(0.) / 2.).round() + 0.25)
            })
            .fold(0., f64::max)
    } else {
        0.
    };
    flow_height(item)
        + fraction_border
        + if matches!(item,Item::Paragraph(p) if p.runs.iter().any(|r|r.fraction.is_some())) {
            0.5
        } else {
            0.
        }
}
struct Writer {
    zip: Zip,
    relationships: String,
    images: usize,
}
impl Writer {
    fn picture(&mut self, p: &Picture, left: f64, before: f64) -> String {
        self.images += 1;
        let n = self.images;
        self.zip
            .add(&format!("word/media/image{n}.png"), &p.bytes, false);
        self.relationships.push_str(&format!("<Relationship Id=\"image{n}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{n}.png\"/>"));
        let width = ((p.rect[2] - p.rect[0]) * 12700.).round() as i64;
        let height = ((p.rect[3] - p.rect[1]) * 12700.).round() as i64;
        format!(
            "<w:p><w:pPr><w:spacing w:before=\"{}\" w:after=\"0\" w:line=\"20\" w:lineRule=\"atLeast\"/><w:ind w:left=\"{}\"/><w:rPr><w:sz w:val=\"2\"/></w:rPr></w:pPr><w:r><w:rPr><w:sz w:val=\"2\"/></w:rPr><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{width}\" cy=\"{height}\"/><wp:docPr id=\"{n}\" name=\"Illustration {n}\" descr=\"Illustration du PDF\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"image{n}.png\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"image{n}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
            twips(before),
            twips(p.rect[0] - left)
        )
    }
    fn item(&mut self, item: &Item, left: f64, width: f64, before: f64) -> String {
        match item {
            Item::Paragraph(p) if p.runs.iter().any(|r| r.fraction.is_some()) => {
                math_row(p, left, before)
            }
            Item::Paragraph(p) => paragraph(p, left, width, before, false),
            Item::Picture(p) => self.picture(p, left, before),
            Item::Table(t) => {
                let gap = if before > 1. {
                    format!(
                        "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"{}\" w:lineRule=\"exact\"/></w:pPr></w:p>",
                        twips(before)
                    )
                } else {
                    String::new()
                };
                gap + &table(t, left)
                    + "<w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"10\" w:lineRule=\"exact\"/></w:pPr></w:p>"
            }
        }
    }
    fn page(&mut self, page: &Page) -> String {
        let mut body = String::new();
        let mut current = page.margin[1];
        let mut i = 0;
        let left = page.margin[0];
        let width = page.width - left - page.margin[2];
        while i < page.items.len() {
            let first = &page.items[i];
            let r = first.rect();
            let (end, row) = columns(page, i);
            let before = (r[1] - current).max(0.);
            if row.len() == 1 {
                body.push_str(&self.item(first, left, width, before));
                current = current.max(r[1]) + height(first);
            } else {
                body.push_str(&format!("<w:tbl><w:tblPr><w:tblW w:w=\"{}\" w:type=\"dxa\"/><w:tblLayout w:type=\"fixed\"/><w:tblBorders><w:top w:val=\"nil\"/><w:left w:val=\"nil\"/><w:bottom w:val=\"nil\"/><w:right w:val=\"nil\"/><w:insideH w:val=\"nil\"/><w:insideV w:val=\"nil\"/></w:tblBorders><w:tblCellMar><w:top w:w=\"0\" w:type=\"dxa\"/><w:left w:w=\"0\" w:type=\"dxa\"/><w:bottom w:w=\"0\" w:type=\"dxa\"/><w:right w:w=\"0\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr><w:tblGrid>", twips(width)));
                let edges: Vec<_> = std::iter::once(left)
                    .chain(row.iter().skip(1).map(|a| a[0].rect()[0]))
                    .chain(std::iter::once(left + width))
                    .collect();
                for x in edges.windows(2) {
                    body.push_str(&format!("<w:gridCol w:w=\"{}\"/>", twips(x[1] - x[0])));
                }
                body.push_str("</w:tblGrid><w:tr>");
                let mut bottom = current;
                for (n, items) in row.iter().enumerate() {
                    let cell_width = edges[n + 1] - edges[n];
                    body.push_str(&format!("<w:tc><w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/><w:vAlign w:val=\"top\"/></w:tcPr>", twips(cell_width)));
                    let mut column_y = current;
                    for item in items {
                        let rect = item.rect();
                        body.push_str(&self.item(
                            item,
                            edges[n],
                            cell_width,
                            (rect[1] - column_y).max(0.),
                        ));
                        column_y = column_y.max(rect[1]) + height(item);
                    }
                    bottom = bottom.max(column_y);
                    body.push_str("</w:tc>");
                }
                body.push_str("</w:tr></w:tbl><w:p><w:pPr><w:spacing w:before=\"0\" w:after=\"0\" w:line=\"10\" w:lineRule=\"exact\"/></w:pPr></w:p>");
                current = bottom + 0.5;
            }
            i = end;
        }
        for decoration in &page.decorations {
            let inline = self.picture(decoration, 0., 0.);
            let start = inline.find("<w:r>").unwrap();
            let end = inline.rfind("</w:r>").unwrap() + 6;
            let drawing = inline[start..end].replace(
                "<wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">",
                "<wp:anchor distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" simplePos=\"0\" relativeHeight=\"0\" behindDoc=\"1\" locked=\"1\" layoutInCell=\"0\" allowOverlap=\"1\"><wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"page\"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom=\"page\"><wp:posOffset>0</wp:posOffset></wp:positionV>"
            ).replace("<wp:docPr", "<wp:wrapNone/><wp:docPr").replace("</wp:inline>", "</wp:anchor>");
            if let Some(at) = body.find("</w:pPr>") {
                body.insert_str(at + 8, &drawing);
            }
        }
        body
    }
}
fn section(p: &Page) -> String {
    format!(
        "<w:sectPr><w:type w:val=\"nextPage\"/><w:pgSz w:w=\"{}\" w:h=\"{}\"{}/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"0\" w:footer=\"0\" w:gutter=\"0\"/></w:sectPr>",
        twips(p.width),
        twips(p.height),
        if p.width > p.height {
            " w:orient=\"landscape\""
        } else {
            ""
        },
        twips(p.margin[1]),
        twips(p.margin[2]),
        twips(p.margin[3]),
        twips(p.margin[0])
    )
}
pub(super) fn document(pages: &[Page]) -> Result<Vec<u8>> {
    let mut w = Writer {
        zip: Zip::default(),
        relationships: String::new(),
        images: 0,
    };
    let mut body = String::new();
    let mut font_table = String::new();
    let mut font_rels = String::new();
    for (i, font) in pages.iter().flat_map(|p| &p.fonts).enumerate() {
        // GUID bytes, reversed, XOR the first 32 bytes (ECMA-376 font part).
        let key = format!("{{001B70DC-AA60-4AD5-90EC-{:012X}}}", i + 1);
        let hex: String = key.chars().filter(char::is_ascii_hexdigit).collect();
        let secret: Vec<u8> = (0..32)
            .step_by(2)
            .map(|j| u8::from_str_radix(&hex[j..j + 2], 16).unwrap())
            .rev()
            .collect();
        let mut bytes = font.bytes.clone();
        for (j, byte) in bytes.iter_mut().take(32).enumerate() {
            *byte ^= secret[j % 16];
        }
        w.zip
            .add(&format!("word/fonts/font{i}.odttf"), &bytes, false);
        let kind = match (font.bold, font.italic) {
            (true, true) => "embedBoldItalic",
            (true, false) => "embedBold",
            (false, true) => "embedItalic",
            _ => "embedRegular",
        };
        let fallback = font.name.split(" PDF ").next().unwrap_or(&font.name);
        let family = if fallback.contains("Arial") || fallback.contains("Helvetica") {
            "swiss"
        } else if fallback.contains("Courier") {
            "modern"
        } else {
            "roman"
        };
        font_table.push_str(&format!("<w:font w:name=\"{}\"><w:altName w:val=\"{}\"/><w:family w:val=\"{family}\"/><w:{kind} r:id=\"font{i}\" w:fontKey=\"{key}\" w:subsetted=\"1\"/></w:font>", xml(&font.name),xml(fallback)));
        font_rels.push_str(&format!("<Relationship Id=\"font{i}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/font\" Target=\"fonts/font{i}.odttf\"/>"));
    }
    let fonts = format!(
        "<?xml version=\"1.0\"?><w:fonts xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{font_table}</w:fonts>"
    );
    w.zip.add("word/fontTable.xml", fonts.as_bytes(), true);
    let fonts = format!(
        "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{font_rels}</Relationships>"
    );
    w.zip
        .add("word/_rels/fontTable.xml.rels", fonts.as_bytes(), true);
    w.relationships.push_str("<Relationship Id=\"fonts\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable\" Target=\"fontTable.xml\"/>");
    w.zip.add("word/settings.xml", br#"<?xml version="1.0"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:embedTrueTypeFonts/><w:embedSystemFonts/><w:saveSubsetFonts/></w:settings>"#, true);
    w.relationships.push_str("<Relationship Id=\"settings\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" Target=\"settings.xml\"/>");
    for (i, page) in pages.iter().enumerate() {
        body.push_str(&w.page(page));
        if i + 1 < pages.len() {
            body.push_str(&format!("<w:p><w:pPr><w:spacing w:after=\"0\" w:line=\"20\" w:lineRule=\"exact\"/>{}</w:pPr></w:p>", section(page)));
        } else {
            body.push_str(&section(page));
        }
    }
    if w.images > 60000 {
        return Err("Ce document contient trop d’illustrations pour être exporté en Word.".into());
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><w:body>{body}</w:body></w:document>"
    );
    w.zip.add("word/document.xml", document.as_bytes(), true);
    w.zip.add("[Content_Types].xml", br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="odttf" ContentType="application/vnd.openxmlformats-officedocument.obfuscatedFont"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#, true);
    w.zip.add("_rels/.rels", br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="document" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#, true);
    w.zip.add("word/styles.xml", br#"<?xml version="1.0" encoding="UTF-8"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:before="0" w:after="0"/><w:widowControl w:val="0"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>"#, true);
    w.zip.add("word/numbering.xml", "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:abstractNum w:abstractNumId=\"1\"><w:multiLevelType w:val=\"singleLevel\"/><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"•\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:tabs><w:tab w:val=\"num\" w:pos=\"240\"/></w:tabs><w:ind w:left=\"240\" w:hanging=\"240\"/></w:pPr><w:rPr><w:rFonts w:ascii=\"Arial\" w:hAnsi=\"Arial\"/></w:rPr></w:lvl></w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>".as_bytes(), true);
    let rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"styles\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/><Relationship Id=\"numbering\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>{}</Relationships>",
        w.relationships
    );
    w.zip
        .add("word/_rels/document.xml.rels", rels.as_bytes(), true);
    Ok(w.zip.finish())
}
