//! OpenDocument Text 1.3 writer using the same flowing layout as Word.
use super::*;
use pdfcraft_export::Zip;
fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| *c >= ' ' || matches!(c, '\n' | '\t'))
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            _ => c.to_string(),
        })
        .collect()
}
fn rgb(c: [f64; 3]) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        (c[0].clamp(0., 1.) * 255.).round() as u8,
        (c[1].clamp(0., 1.) * 255.).round() as u8,
        (c[2].clamp(0., 1.) * 255.).round() as u8
    )
}
fn text(s: &str) -> String {
    s.split('\n')
        .map(|line| {
            line.split(' ')
                .map(xml)
                .collect::<Vec<_>>()
                .join("<text:s/>")
        })
        .collect::<Vec<_>>()
        .join("<text:line-break/>")
}
struct Writer {
    zip: Zip,
    styles: String,
    count: usize,
    images: Vec<String>,
    formulas: Vec<String>,
}
impl Writer {
    fn fraction(&mut self, r: &Run, f: &Fraction) -> String {
        let name = format!("Formula{}", self.formulas.len() + 1);
        let value = |runs: &[Run]| runs.iter().map(|r| r.text.trim()).collect::<String>();
        let num = value(&f.numerator);
        let den = value(&f.denominator);
        let content = format!(
            "<?xml version=\"1.0\"?><math xmlns=\"http://www.w3.org/1998/Math/MathML\"><semantics><mstyle mathsize=\"{}pt\" mathfamily=\"{}\"><mfrac><mtext>{}</mtext><mtext>{}</mtext></mfrac></mstyle><annotation encoding=\"StarMath 5.0\">{{ {} }} over {{ {} }}</annotation></semantics></math>",
            r.size,
            xml(&r.font),
            xml(&num),
            xml(&den),
            xml(&num),
            xml(&den)
        );
        self.zip
            .add(&format!("{name}/content.xml"), content.as_bytes(), true);
        self.formulas.push(name.clone());
        let b = r.rect.unwrap();
        let graphic=self.style("graphic","<style:graphic-properties style:vertical-pos=\"middle\" style:vertical-rel=\"text\" fo:margin-left=\"0pt\" fo:margin-right=\"0pt\"/>");
        format!(
            "<draw:frame draw:style-name=\"{graphic}\" draw:name=\"{name}\" text:anchor-type=\"as-char\" svg:width=\"{}pt\" svg:height=\"{}pt\"><draw:object xlink:href=\"./{name}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
            b[2] - b[0] + 1.5,
            b[3] - b[1]
        )
    }
    fn angle(&mut self, r: &Run) -> String {
        let name = format!("Formula{}", self.formulas.len() + 1);
        let content = format!(
            "<?xml version=\"1.0\"?><math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\"><semantics><mstyle mathsize=\"{}pt\"><mover accent=\"true\"><mi mathvariant=\"italic\">{}</mi><mo stretchy=\"true\">^</mo></mover></mstyle><annotation encoding=\"StarMath 5.0\">widehat {{ {} }}</annotation></semantics></math>",
            r.size,
            xml(r.text.trim()),
            xml(r.text.trim())
        );
        self.zip
            .add(&format!("{name}/content.xml"), content.as_bytes(), true);
        self.formulas.push(name.clone());
        let width = r.rect.map_or(r.size * 2., |r| r[2] - r[0]);
        let style = self.style(
            "graphic",
            "<style:graphic-properties style:vertical-pos=\"middle\" style:vertical-rel=\"text\"/>",
        );
        format!(
            "<draw:frame draw:style-name=\"{style}\" draw:name=\"{name}\" text:anchor-type=\"as-char\" svg:width=\"{width}pt\" svg:height=\"{}pt\"><draw:object xlink:href=\"./{name}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
            r.size * 1.24
        )
    }
    fn style(&mut self, family: &str, properties: &str) -> String {
        self.count += 1;
        let name = format!("S{}", self.count);
        self.styles.push_str(&format!("<style:style style:name=\"{name}\" style:family=\"{family}\">{properties}</style:style>"));
        name
    }
    fn run(&mut self, r: &Run) -> String {
        let mut props = format!(
            "<style:text-properties style:font-name=\"{}\" fo:font-family=\"{}\" fo:font-size=\"{}pt\" fo:color=\"{}\"",
            xml(&r.font.replace(' ', "_20_")),
            xml(&r.font),
            r.size,
            rgb(r.color)
        );
        if r.bold {
            props.push_str(" fo:font-weight=\"bold\"");
        }
        if r.italic {
            props.push_str(" fo:font-style=\"italic\"");
        }
        if r.underline {
            props.push_str(
                " style:text-underline-style=\"solid\" style:text-underline-type=\"single\"",
            );
        }
        if r.strike {
            props.push_str(" style:text-line-through-style=\"solid\"");
        }
        if r.script != 0 {
            props.push_str(if r.script > 0 {
                " style:text-position=\"super 70%\""
            } else {
                " style:text-position=\"sub 70%\""
            });
        }
        if let Some(color) = r.background {
            props.push_str(&format!(" fo:background-color=\"{}\"", rgb(color)));
        }
        props.push_str("/>");
        let style = self.style("text", &props);
        format!(
            "<text:span text:style-name=\"{style}\">{}</text:span>",
            text(&r.text)
        )
    }
    fn paragraph(
        &mut self,
        p: &Paragraph,
        left: f64,
        width: f64,
        before: f64,
        in_cell: bool,
    ) -> String {
        let right = 0.;
        let indent = if in_cell || matches!(p.align, "center" | "right") {
            0.
        } else {
            (p.rect[0] - left).max(0.)
        };
        let size = paragraph_height(p);
        let line = p
            .spacing
            .map_or_else(|| format!("{size}pt"), |s| format!("{}%", s * 100.));
        let leader = dotted(p);
        if leader {
            let mut content = leader_prefix(p)
                .unwrap()
                .iter()
                .map(|r| self.run(r))
                .collect::<String>();
            if let Some(r) = p.runs.last() {
                let mut r = r.clone();
                r.text.clear();
                content.push_str(
                    &self
                        .run(&r)
                        .replace("></text:span>", "><text:tab/></text:span>"),
                );
            }
            let stop = (p.rect[2] - p.rect[0]).min(width - indent - 2.).max(4.);
            let style = self.style("paragraph", &format!("<style:paragraph-properties fo:margin-top=\"{before}pt\" fo:margin-bottom=\"0pt\" fo:margin-left=\"{indent}pt\" fo:line-height=\"{line}\"><style:tab-stops><style:tab-stop style:type=\"right\" style:position=\"{stop}pt\" style:leader-text=\".\" style:leader-style=\"solid\" style:leader-type=\"single\"/></style:tab-stops></style:paragraph-properties>"));
            return format!("<text:p text:style-name=\"{style}\">{content}</text:p>");
        }
        let mut tabs = String::new();
        {
            for i in 1..p.runs.len() {
                if tab_before(&p.runs, i)
                    && let Some(b) = p.runs[i].rect
                {
                    tabs.push_str(&format!(
                        "<style:tab-stop style:type=\"left\" style:position=\"{}pt\"/>",
                        b[0] - p.rect[0]
                    ));
                }
            }
        }
        let carrier = if p.runs.iter().all(|r| r.fraction.is_some()) {
            "<style:text-properties fo:font-size=\"0.1pt\"/>"
        } else {
            ""
        };
        let style = self.style("paragraph", &format!("<style:paragraph-properties fo:margin-top=\"{before}pt\" fo:margin-bottom=\"0pt\" fo:margin-left=\"{indent}pt\" fo:margin-right=\"{right}pt\" fo:line-height=\"{line}\" fo:text-align=\"{}\" fo:widows=\"1\" fo:orphans=\"1\"><style:tab-stops>{tabs}</style:tab-stops></style:paragraph-properties>{carrier}", if p.align == "both" { "justify" } else { p.align }));
        let mut runs = p.runs.clone();
        let bullet = runs.first().is_some_and(|r| r.text.starts_with("• "));
        if bullet {
            runs[0].text = runs[0].text["• ".len()..].into();
        }
        let mut content = String::new();
        let mut i = 0;
        while i < runs.len() {
            if tab_before(&runs, i) {
                content.push_str("<text:tab/>");
            }
            if let Some(f) = &runs[i].fraction {
                content.push_str(&self.fraction(&runs[i], f));
                i += 1;
            } else if let Some(base) = angle(&runs, i) {
                content.push_str(&self.angle(base));
                let mut trailing = base.clone();
                trailing.text = base.text[base.text.trim_end().len()..].into();
                if !trailing.text.is_empty() {
                    content.push_str(&self.run(&trailing));
                }
                i += 2;
            } else {
                let mut r = runs[i].clone();
                if i + 1 < runs.len() && tab_before(&runs, i + 1) {
                    r.text = r.text.trim_end().into();
                }
                content.push_str(&self.run(&r));
                i += 1;
            }
        }
        let p = format!("<text:p text:style-name=\"{style}\">{}</text:p>", content);
        if bullet {
            format!(
                "<text:list text:style-name=\"Bullets\"><text:list-item>{p}</text:list-item></text:list>"
            )
        } else {
            p
        }
    }
    fn picture(&mut self, p: &Picture, left: f64, before: f64) -> String {
        let name = format!("Pictures/image{}.png", self.images.len() + 1);
        self.zip.add(&name, &p.bytes, false);
        self.images.push(name.clone());
        let style = self.style("paragraph", &format!("<style:paragraph-properties fo:margin-top=\"{before}pt\" fo:margin-bottom=\"0pt\" fo:margin-left=\"{}pt\" fo:line-height=\"100%\"/><style:text-properties fo:font-size=\"1pt\"/>", (p.rect[0] - left).max(0.)));
        let graphic = self.style("graphic", "<style:graphic-properties style:vertical-pos=\"top\" style:vertical-rel=\"baseline\" style:flow-with-text=\"true\" fo:margin-top=\"0pt\" fo:margin-bottom=\"0pt\"/>");
        format!(
            "<text:p text:style-name=\"{style}\"><draw:frame draw:style-name=\"{graphic}\" draw:name=\"Illustration{}\" text:anchor-type=\"as-char\" svg:width=\"{}pt\" svg:height=\"{}pt\"><draw:image xlink:href=\"{name}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame></text:p>",
            self.images.len(),
            p.rect[2] - p.rect[0],
            p.rect[3] - p.rect[1]
        )
    }
    fn table(&mut self, t: &Table, left: f64, before: f64) -> String {
        let table = self.style("table", &format!("<style:table-properties style:width=\"{}pt\" table:align=\"left\" fo:margin-left=\"{}pt\" fo:margin-right=\"0pt\" fo:margin-top=\"{before}pt\" fo:margin-bottom=\"0pt\" table:border-model=\"collapsing\"/>", t.rect[2] - t.rect[0], (t.rect[0] - left).max(0.)));
        let mut s = format!(
            "<table:table table:name=\"Table{}\" table:style-name=\"{table}\">",
            self.count
        );
        for x in t.xs.windows(2) {
            let style = self.style(
                "table-column",
                &format!(
                    "<style:table-column-properties style:column-width=\"{}pt\"/>",
                    x[1] - x[0]
                ),
            );
            s.push_str(&format!(
                "<table:table-column table:style-name=\"{style}\"/>"
            ));
        }
        for (y, row) in t.rows.iter().enumerate() {
            let style = self.style("table-row", &format!("<style:table-row-properties style:row-height=\"{}pt\" fo:keep-together=\"always\"/>", t.ys[y + 1] - t.ys[y]));
            s.push_str(&format!("<table:table-row table:style-name=\"{style}\">"));
            for cell in row {
                if cell.merge == Some("continue") {
                    for _ in 0..cell.span {
                        s.push_str("<table:covered-table-cell/>");
                    }
                    continue;
                }
                let fill = cell.fill.map_or_else(|| "transparent".into(), rgb);
                let style = self.style("table-cell", &format!("<style:table-cell-properties fo:border=\"{}pt solid {}\" fo:padding-left=\"1.5pt\" fo:padding-right=\"1.5pt\" fo:padding-top=\"0pt\" fo:padding-bottom=\"0pt\" fo:background-color=\"{fill}\" style:vertical-align=\"top\"/>", t.border, rgb(t.color)));
                let rowspan = 1 + t.rows[y + 1..]
                    .iter()
                    .take_while(|row| {
                        row.iter().any(|c| {
                            c.column == cell.column
                                && c.span == cell.span
                                && c.merge == Some("continue")
                        })
                    })
                    .count();
                s.push_str(&format!("<table:table-cell table:style-name=\"{style}\" table:number-columns-spanned=\"{}\" table:number-rows-spanned=\"{rowspan}\" office:value-type=\"string\">", cell.span));
                if cell.paragraphs.is_empty() {
                    s.push_str("<text:p/>");
                }
                let mut cy = t.ys[y];
                for p in &cell.paragraphs {
                    s.push_str(&self.paragraph(
                        p,
                        t.xs[cell.column] + 1.5,
                        t.xs[cell.column + cell.span] - t.xs[cell.column] - 3.,
                        (p.rect[1] - cy).max(0.),
                        false,
                    ));
                    cy = cy.max(p.rect[1]) + paragraph_height(p);
                }
                s.push_str("</table:table-cell>");
                for _ in 1..cell.span {
                    s.push_str("<table:covered-table-cell/>");
                }
            }
            s.push_str("</table:table-row>");
        }
        s.push_str("</table:table>");
        s
    }
    fn math_row(&mut self, p: &Paragraph, left: f64, before: f64) -> String {
        let mut parts = math_parts(p);
        let edges: Vec<_> = parts
            .iter()
            .map(|p| p.rect[0])
            .chain(std::iter::once(p.rect[2] + 3.))
            .collect();
        let style=self.style("table",&format!("<style:table-properties style:width=\"{}pt\" table:align=\"left\" fo:margin-left=\"{}pt\" fo:margin-right=\"0pt\" fo:margin-top=\"{before}pt\" fo:margin-bottom=\"0pt\"/>",edges.last().unwrap()-edges[0],(edges[0]-left).max(0.)));
        let mut s = format!(
            "<table:table table:name=\"MathRow{}\" table:style-name=\"{style}\">",
            self.count
        );
        for pair in edges.windows(2) {
            let c = self.style(
                "table-column",
                &format!(
                    "<style:table-column-properties style:column-width=\"{}pt\"/>",
                    pair[1] - pair[0]
                ),
            );
            s.push_str(&format!("<table:table-column table:style-name=\"{c}\"/>"));
        }
        let cell=self.style("table-cell","<style:table-cell-properties fo:padding=\"0pt\" fo:border=\"none\" style:vertical-align=\"top\"/>");
        let row = self.style(
            "table-row",
            &format!(
                "<style:table-row-properties style:row-height=\"{}pt\"/>",
                p.rect[3] - p.rect[1]
            ),
        );
        s.push_str(&format!("<table:table-row table:style-name=\"{row}\">"));
        for (n, part) in parts.iter_mut().enumerate() {
            if let Some(r) = part.runs.last_mut() {
                r.text = r.text.trim_end().into();
            }
            s.push_str(&format!("<table:table-cell table:style-name=\"{cell}\">"));
            s.push_str(&self.paragraph(
                part,
                edges[n],
                edges[n + 1] - edges[n],
                (part.rect[1] - p.rect[1]).max(0.),
                true,
            ));
            s.push_str("</table:table-cell>");
        }
        s.push_str("</table:table-row></table:table>");
        s
    }
    fn item(&mut self, item: &Item, left: f64, width: f64, before: f64) -> String {
        match item {
            Item::Paragraph(p) if p.runs.iter().any(|r| r.fraction.is_some()) => {
                self.math_row(p, left, before)
            }
            Item::Paragraph(p) => self.paragraph(p, left, width, before, false),
            Item::Picture(p) => self.picture(p, left, before),
            Item::Table(t) => self.table(t, left, before),
        }
    }
    fn page(&mut self, p: &Page) -> String {
        let mut body = String::new();
        let mut i = 0;
        let mut current = p.margin[1];
        let left = p.margin[0];
        let width = p.width - left - p.margin[2];
        while i < p.items.len() {
            let (end, cols) = columns(p, i);
            if cols.len() == 1 {
                let r = p.items[i].rect();
                body.push_str(&self.item(&p.items[i], left, width, (r[1] - current).max(0.)));
                current = current.max(r[1]) + flow_height(&p.items[i]);
            } else {
                let table = self.style("table", &format!("<style:table-properties style:width=\"{width}pt\" table:align=\"left\" fo:margin-left=\"0pt\" fo:margin-right=\"0pt\" fo:margin-top=\"0pt\" fo:margin-bottom=\"0pt\"/>"));
                body.push_str(&format!(
                    "<table:table table:name=\"Columns{}\" table:style-name=\"{table}\">",
                    self.count
                ));
                let edges: Vec<_> = std::iter::once(left)
                    .chain(cols.iter().skip(1).map(|c| c[0].rect()[0]))
                    .chain(std::iter::once(left + width))
                    .collect();
                for x in edges.windows(2) {
                    let style = self.style(
                        "table-column",
                        &format!(
                            "<style:table-column-properties style:column-width=\"{}pt\"/>",
                            x[1] - x[0]
                        ),
                    );
                    body.push_str(&format!(
                        "<table:table-column table:style-name=\"{style}\"/>"
                    ));
                }
                let cell = self.style("table-cell", "<style:table-cell-properties fo:border=\"none\" fo:padding=\"0pt\" style:vertical-align=\"top\"/>");
                body.push_str("<table:table-row>");
                let mut bottom = current;
                for (n, col) in cols.iter().enumerate() {
                    body.push_str(&format!("<table:table-cell table:style-name=\"{cell}\">"));
                    let mut cy = current;
                    for item in col {
                        let r = item.rect();
                        body.push_str(&self.item(
                            item,
                            edges[n],
                            edges[n + 1] - edges[n],
                            (r[1] - cy).max(0.),
                        ));
                        cy = cy.max(r[1]) + flow_height(item);
                    }
                    bottom = bottom.max(cy);
                    body.push_str("</table:table-cell>");
                }
                body.push_str("</table:table-row></table:table>");
                current = bottom;
            }
            i = end;
        }
        for decoration in &p.decorations {
            let name = format!("Pictures/image{}.png", self.images.len() + 1);
            self.zip.add(&name, &decoration.bytes, false);
            self.images.push(name.clone());
            let graphic = self.style("graphic", "<style:graphic-properties style:wrap=\"run-through\" style:run-through=\"background\" style:horizontal-pos=\"from-left\" style:horizontal-rel=\"page\" style:vertical-pos=\"from-top\" style:vertical-rel=\"page\"/>");
            let frame = format!(
                "<draw:frame draw:style-name=\"{graphic}\" draw:name=\"Decor{}\" text:anchor-type=\"page\" svg:x=\"0pt\" svg:y=\"0pt\" svg:width=\"{}pt\" svg:height=\"{}pt\"><draw:image xlink:href=\"{name}\" xlink:type=\"simple\"/></draw:frame>",
                self.images.len(),
                p.width,
                p.height
            );
            if let Some(at) = body.find("<text:p")
                && let Some(end) = body[at..].find('>')
            {
                body.insert_str(at + end + 1, &frame);
            }
        }
        // Writer otherwise inserts its default-size trailing paragraph after a
        // final table. Near the page edge that creates a blank extra page.
        if body.ends_with("</table:table>") {
            let tail = self.style("paragraph", "<style:paragraph-properties fo:line-height=\"0.1pt\" fo:margin-top=\"0pt\" fo:margin-bottom=\"0pt\"/><style:text-properties fo:font-size=\"1pt\"/>");
            body.push_str(&format!("<text:p text:style-name=\"{tail}\"/>"));
        }
        body
    }
}
pub(super) fn document(pages: &[Page]) -> Result<Vec<u8>> {
    let mut w = Writer {
        zip: Zip::default(),
        styles: String::new(),
        count: 0,
        images: Vec::new(),
        formulas: Vec::new(),
    };
    w.zip.add(
        "mimetype",
        b"application/vnd.oasis.opendocument.text",
        false,
    );
    let mut layouts = String::new();
    let mut masters = String::new();
    let mut font_decls = String::new();
    let mut font_manifest = String::new();
    for (i, font) in pages.iter().flat_map(|p| &p.fonts).enumerate() {
        let name = format!("Fonts/font{i}.ttf");
        w.zip.add(&name, &font.bytes, false);
        font_decls.push_str(&format!("<style:font-face style:name=\"{}\" svg:font-family=\"{}\"><svg:font-face-src><svg:font-face-uri xlink:href=\"{name}\" xlink:type=\"simple\" xlink:actuate=\"onLoad\"><svg:font-face-format svg:string=\"truetype\"/></svg:font-face-uri></svg:font-face-src></style:font-face>", xml(&font.name.replace(' ', "_20_")), xml(&font.name)));
        font_manifest.push_str(&format!("<manifest:file-entry manifest:full-path=\"{name}\" manifest:media-type=\"application/x-font-ttf\"/>"));
    }
    let mut body = String::new();
    for (i, p) in pages.iter().enumerate() {
        layouts.push_str(&format!("<style:page-layout style:name=\"PL{i}\"><style:page-layout-properties fo:page-width=\"{}pt\" fo:page-height=\"{}pt\" style:print-orientation=\"{}\" fo:margin-left=\"{}pt\" fo:margin-top=\"{}pt\" fo:margin-right=\"{}pt\" fo:margin-bottom=\"{}pt\"/></style:page-layout>", p.width, p.height, if p.width > p.height { "landscape" } else { "portrait" }, p.margin[0], p.margin[1], p.margin[2], p.margin[3]));
        masters.push_str(&format!(
            "<style:master-page style:name=\"MP{i}\" style:page-layout-name=\"PL{i}\"/>"
        ));
        w.styles.push_str(&format!("<style:style style:name=\"Page{i}\" style:family=\"paragraph\" style:master-page-name=\"MP{i}\"><style:paragraph-properties fo:margin-top=\"0pt\" fo:margin-bottom=\"0pt\" fo:line-height=\"0.1pt\"{} /><style:text-properties fo:font-size=\"1pt\"/></style:style>", if i == 0 { "" } else { " fo:break-before=\"page\"" }));
        body.push_str(&format!("<text:p text:style-name=\"Page{i}\"/>"));
        body.push_str(&w.page(p));
    }
    let namespaces = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\"";
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content {namespaces} office:version=\"1.3\"><office:font-face-decls>{font_decls}</office:font-face-decls><office:automatic-styles>{}<text:list-style style:name=\"Bullets\"><text:list-level-style-bullet text:level=\"1\" text:bullet-char=\"•\"><style:list-level-properties text:space-before=\"0pt\" text:min-label-width=\"12pt\"/></text:list-level-style-bullet></text:list-style></office:automatic-styles><office:body><office:text>{body}</office:text></office:body></office:document-content>",
        w.styles
    );
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-styles {namespaces} office:version=\"1.3\"><office:font-face-decls>{font_decls}</office:font-face-decls><office:styles><style:default-style style:family=\"paragraph\"><style:paragraph-properties fo:margin-top=\"0pt\" fo:margin-bottom=\"0pt\"/></style:default-style></office:styles><office:automatic-styles>{layouts}</office:automatic-styles><office:master-styles>{masters}</office:master-styles></office:document-styles>"
    );
    w.zip.add("content.xml", content.as_bytes(), true);
    w.zip.add("styles.xml", styles.as_bytes(), true);
    let mut manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.3\"><manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/vnd.oasis.opendocument.text\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/>".to_string();
    for name in &w.images {
        manifest.push_str(&format!(
            "<manifest:file-entry manifest:full-path=\"{name}\" manifest:media-type=\"image/png\"/>"
        ));
    }
    for name in &w.formulas {
        manifest.push_str(&format!("<manifest:file-entry manifest:full-path=\"{name}/\" manifest:media-type=\"application/vnd.oasis.opendocument.formula\"/><manifest:file-entry manifest:full-path=\"{name}/content.xml\" manifest:media-type=\"text/xml\"/>"));
    }
    manifest.push_str(&font_manifest);
    manifest.push_str("</manifest:manifest>");
    w.zip
        .add("META-INF/manifest.xml", manifest.as_bytes(), true);
    Ok(w.zip.finish())
}
