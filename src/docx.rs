//! Structured Word export. Paragraphs and table cells flow in the document;
//! artwork is inline. No positioned text boxes or whole-page screenshots.
mod fonts;
mod layout;
mod odt;
mod word;
use crate::document::{Editor, Result, guarded};

#[derive(Clone, Debug)]
struct Run {
    text: String,
    font: String,
    size: f64,
    color: [f64; 3],
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    script: i8,
    background: Option<[f64; 3]>,
    rect: Option<[f64; 4]>,
    fraction: Option<Box<Fraction>>,
}
#[derive(Clone, Debug)]
struct Fraction {
    numerator: Vec<Run>,
    denominator: Vec<Run>,
}
#[derive(Clone, Debug)]
struct Paragraph {
    rect: [f64; 4], // displayed page, top down, points
    runs: Vec<Run>,
    align: &'static str,
    spacing: Option<f64>,
}
#[derive(Clone, Debug)]
struct Picture {
    rect: [f64; 4],
    bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
struct Cell {
    column: usize,
    span: usize,
    merge: Option<&'static str>,
    fill: Option<[f64; 3]>,
    paragraphs: Vec<Paragraph>,
}
#[derive(Clone, Debug)]
struct Table {
    rect: [f64; 4],
    xs: Vec<f64>,
    ys: Vec<f64>,
    rows: Vec<Vec<Cell>>,
    border: f64,
    color: [f64; 3],
}
#[derive(Debug)]
enum Item {
    Paragraph(Paragraph),
    Picture(Picture),
    Table(Table),
}
impl Item {
    fn rect(&self) -> [f64; 4] {
        match self {
            Self::Paragraph(p) => p.rect,
            Self::Picture(p) => p.rect,
            Self::Table(t) => t.rect,
        }
    }
}
#[derive(Debug)]
struct Page {
    width: f64,
    height: f64,
    margin: [f64; 4],
    items: Vec<Item>,
    decorations: Vec<Picture>,
    fonts: Vec<SourceFont>,
}
#[derive(Debug)]
struct SourceFont {
    name: String,
    bytes: Vec<u8>,
    bold: bool,
    italic: bool,
}

fn dotted(p: &Paragraph) -> bool {
    leader_prefix(p).is_some()
}
fn leader_prefix(p: &Paragraph) -> Option<Vec<Run>> {
    let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
    let prefix = text.trim_end_matches(|c: char| matches!(c, '.' | '…' | '·') || c.is_whitespace());
    if text[prefix.len()..]
        .chars()
        .filter(|c| !c.is_whitespace())
        .count()
        <= 8
    {
        return None;
    }
    let mut end = prefix.len();
    Some(
        p.runs
            .iter()
            .filter_map(|r| {
                if end == 0 {
                    return None;
                }
                let mut r = r.clone();
                r.text.truncate(end.min(r.text.len()));
                end -= r.text.len();
                Some(r)
            })
            .collect(),
    )
}
fn angle(runs: &[Run], i: usize) -> Option<&Run> {
    let marker = runs.get(i)?;
    let base = runs.get(i + 1)?;
    let value = base.text.trim();
    (marker.text.trim() == "^"
        && (2..=4).contains(&value.len())
        && value.chars().all(|c| c.is_ascii_uppercase())
        && marker
            .rect
            .zip(base.rect)
            .is_some_and(|(a, b)| a[0] < b[2] && b[0] < a[2]))
    .then_some(base)
}

// Track the height actually emitted by the word processor, rather than the
// source font's em box. Mixing these two caused cumulative vertical drift.
fn flow_height(item: &Item) -> f64 {
    match item {
        Item::Paragraph(p) => paragraph_height(p),
        Item::Picture(p) => p.rect[3] - p.rect[1] + 0.2,
        Item::Table(t) => t.rect[3] - t.rect[1] + 0.5,
    }
}
fn paragraph_height(p: &Paragraph) -> f64 {
    if p.runs.iter().any(|r| r.fraction.is_some()) {
        p.rect[3] - p.rect[1]
    } else {
        p.runs.iter().map(|r| r.size).fold(1., f64::max)
    }
}
fn math_parts(p: &Paragraph) -> Vec<Paragraph> {
    let mut parts: Vec<Paragraph> = Vec::new();
    for r in &p.runs {
        let rect = r.rect.unwrap_or(p.rect);
        if r.fraction.is_none()
            && let Some(last) = parts.last_mut()
            && last.runs.last().is_some_and(|r| r.fraction.is_none())
            && rect[0] - last.rect[2] < r.size * 1.5
        {
            last.rect = [
                last.rect[0].min(rect[0]),
                last.rect[1].min(rect[1]),
                last.rect[2].max(rect[2]),
                last.rect[3].max(rect[3]),
            ];
            last.runs.push(r.clone());
        } else {
            parts.push(Paragraph {
                rect,
                runs: vec![r.clone()],
                align: "left",
                spacing: None,
            });
        }
    }
    parts
}
fn tab_before(runs: &[Run], i: usize) -> bool {
    i > 0
        && runs[i].rect.zip(runs[i - 1].rect).is_some_and(|(b, a)| {
            let math_token = |r: &Run| {
                !r.font.starts_with("Arial")
                    && r.text
                        .chars()
                        .any(|c| c.is_ascii_digit() || "=×÷+−".contains(c))
                    && r.text.chars().all(|c| {
                        c.is_ascii_digit() || c.is_whitespace() || ".,/=×÷+−-…%".contains(c)
                    })
            };
            if runs.first().is_some_and(math_token)
                // Compact equations already fit their source advances. Tabs
                // between their tightly spaced operators can skip a stop.
                && runs.iter().any(|r| r.font.starts_with("Arial") || r.size >= 14.)
                && math_token(&runs[i])
                && math_token(&runs[i - 1])
                && runs[i].script == 0
                && runs[i - 1].script == 0
                && b[0] >= a[2] - 0.5
            {
                return true;
            }
            let math_to_prose = runs[i].font.starts_with("Arial")
                && !runs[i - 1].font.starts_with("Arial")
                && runs[i].script == 0
                && runs[i - 1].script == 0
                && runs[i]
                    .text
                    .trim_start()
                    .chars()
                    .next()
                    .is_some_and(char::is_alphabetic)
                && runs[i - 1]
                    .text
                    .trim_end()
                    .chars()
                    .last()
                    .is_some_and(|c| c.is_ascii_digit());
            if math_to_prose && b[0] >= a[2] - 0.5 {
                return true;
            }
            b[0] - a[2]
                > if runs[i].fraction.is_some() || runs[i - 1].fraction.is_some() {
                    2.
                } else {
                    runs[i].size * 0.45
                }
        })
}

/// One band of parallel columns, including following paragraphs beside a tall
/// illustration. Both formats share this reading order and column structure.
fn columns(page: &Page, start: usize) -> (usize, Vec<Vec<&Item>>) {
    let first = &page.items[start];
    let r = first.rect();
    let mut end = start + 1;
    while end < page.items.len() && end - start < 8 {
        let next = page.items[end].rect();
        if ((next[1] - r[1]).abs() > 14. && !(matches!(first, Item::Picture(_)) && next[1] < r[3]))
            || page.items[start..end].iter().any(|a| {
                let a = a.rect();
                a[0] + 1. < next[2] && next[0] + 1. < a[2]
            })
        {
            break;
        }
        end += 1;
    }
    let mut cols: Vec<Vec<&Item>> = page.items[start..end].iter().map(|i| vec![i]).collect();
    cols.sort_by(|a, b| a[0].rect()[0].total_cmp(&b[0].rect()[0]));
    if cols.len() > 1 {
        let mut bottom = cols.iter().map(|c| c[0].rect()[3]).fold(r[3], f64::max);
        while end < page.items.len() && page.items[end].rect()[1] < bottom + 15. {
            let item = &page.items[end];
            let rect = item.rect();
            let col = cols.iter().enumerate().position(|(n, c)| {
                rect[0]
                    >= if n == 0 {
                        page.margin[0] - 2.
                    } else {
                        c[0].rect()[0] - 3.
                    }
                    && rect[2]
                        <= cols
                            .get(n + 1)
                            .map_or(page.width - page.margin[2], |c| c[0].rect()[0])
                            + 2.
            });
            if let Some(col) = col {
                bottom = bottom.max(rect[3]);
                cols[col].push(item);
                end += 1;
            } else {
                break;
            }
        }
    }
    (end, cols)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Format {
    Docx,
    Odt,
}
impl Format {
    pub fn extension(self) -> &'static str {
        if self == Self::Docx { "docx" } else { "odt" }
    }
    pub fn label(self) -> &'static str {
        if self == Self::Docx {
            "Word"
        } else {
            "OpenDocument"
        }
    }
}

/// Converts a snapshot of the working PDF, including unsaved edits. Fails
/// without writing a partial file if a page cannot be read completely.
pub fn convert(bytes: Vec<u8>) -> Result<Vec<u8>> {
    convert_as(bytes, Format::Docx)
}
pub fn convert_as(bytes: Vec<u8>, format: Format) -> Result<Vec<u8>> {
    convert_with_progress(
        bytes,
        format,
        &std::sync::atomic::AtomicBool::new(false),
        |_, _| {},
    )
}
pub fn convert_with_progress(
    bytes: Vec<u8>,
    format: Format,
    cancelled: &std::sync::atomic::AtomicBool,
    progress: impl Fn(usize, usize),
) -> Result<Vec<u8>> {
    guarded(|| {
        let editor = Editor::from_bytes(bytes)?;
        if !editor.can_copy() {
            return Err("Ce PDF n’autorise pas l’exportation de son contenu.".into());
        }
        let mut pages = Vec::new();
        let check = || {
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                Err("Export annulé.".to_string())
            } else {
                Ok(())
            }
        };
        progress(0, editor.info.pages.len());
        for index in 0..editor.info.pages.len() {
            check()?;
            pages.push(layout::page(&editor, index)?);
            progress(index + 1, editor.info.pages.len());
        }
        check()?;
        if format == Format::Docx {
            word::document(&pages)
        } else {
            odt::document(&pages)
        }
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn percentages_export_regression() {
        let Some(path) = std::env::var_os("PDFINATOR_PERCENTAGES_PDF") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let editor = super::Editor::from_bytes(bytes).unwrap();
        std::fs::write(
            output("47-drawings.txt"),
            format!("{:#?}", editor.export_drawings(0).unwrap()),
        )
        .unwrap();
        let pages: Vec<_> = (0..editor.info.pages.len())
            .map(|i| super::layout::page(&editor, i).unwrap())
            .collect();
        let fractions: Vec<_> = pages
            .iter()
            .flat_map(|p| &p.items)
            .filter_map(|i| {
                if let super::Item::Paragraph(p) = i {
                    Some(&p.runs)
                } else {
                    None
                }
            })
            .flatten()
            .filter_map(|r| r.fraction.as_ref())
            .collect();
        assert_eq!(
            fractions.len(),
            5,
            "All five fractions must be editable, including the two grey filled bars"
        );
        std::fs::write(output("47-percentages-layout.txt"), format!("{pages:#?}")).unwrap();
        std::fs::write(
            output("47-percentages.docx"),
            super::word::document(&pages).unwrap(),
        )
        .unwrap();
        std::fs::write(
            output("47-percentages.odt"),
            super::odt::document(&pages).unwrap(),
        )
        .unwrap();
    }
    use super::*;
    use crate::drawing::{Drawing, Kind, StrokeStyle};
    fn output(name: &str) -> std::path::PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-output");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }
    #[test]
    fn word_export_builds_flowing_text_real_table_cells_and_inline_pictures() {
        let d =
            pdfcraft_create::from_text("", "Compte rendu de la classe", (595., 842.), 18.).unwrap();
        let mut e =
            Editor::from_bytes(pdfcraft_cos::write_full(&d, &Default::default()).unwrap()).unwrap();
        let mut drawing = Drawing {
            kind: Kind::FilledRectangle,
            a: [60., 640.],
            u: [480., 0.],
            v: [0., 30.],
            width: 0.7,
            color: [0.75, 0.85, 0.6],
            style: StrokeStyle::Solid,
            rows: 1,
            columns: 1,
        };
        e.add_drawing(0, drawing.clone()).unwrap();
        drawing.kind = Kind::Table;
        drawing.a = [60., 550.];
        drawing.v = [0., 120.];
        drawing.rows = 4;
        drawing.columns = 3;
        drawing.color = [0.; 3];
        e.add_drawing(0, drawing).unwrap();
        for (row, cells) in [
            ["Élève", "Résultat", "Appréciation"],
            ["Alice", "18 / 20", "Très bien"],
            ["Benoît", "14 / 20", "Bon travail"],
            ["Chloé", "16 / 20", "Bravo"],
        ]
        .into_iter()
        .enumerate()
        {
            for (col, text) in cells.into_iter().enumerate() {
                e.add_text(
                    0,
                    pdfcraft_edit::AddedText {
                        rect: [
                            65. + col as f64 * 160.,
                            640. - row as f64 * 30.,
                            210. + col as f64 * 160.,
                            665. - row as f64 * 30.,
                        ],
                        text: text.into(),
                        size: 11.,
                        bold: row == 0,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
        }
        e.add_text(
            0,
            pdfcraft_edit::AddedText {
                rect: [60., 450., 520., 500.],
                text: "Les résultats sont encourageants. Chaque élève progresse à son rythme."
                    .into(),
                size: 12.,
                ..Default::default()
            },
        )
        .unwrap();
        let png = crate::ocr::encode_png(&[30, 140, 230, 255].repeat(100 * 50), 100, 50).unwrap();
        e.image_bytes(0, [60., 310., 260., 410.], "illustration.png", &png, None)
            .unwrap();
        let page = layout::page(&e, 0).unwrap();
        let tables: Vec<_> = page
            .items
            .iter()
            .filter_map(|i| {
                if let Item::Table(t) = i {
                    Some(t)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows.len(), 4);
        assert_eq!(tables[0].rows[0].len(), 3);
        assert!(tables[0].rows[0].iter().all(|c| c.fill.is_some()));
        assert!(page.items.iter().any(|i| matches!(i, Item::Picture(_))));
        let before = e.bytes.clone();
        std::fs::write(
            output("38-structure.docx"),
            convert(e.bytes.as_ref().clone()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            output("38-structure.odt"),
            convert_as(e.bytes.as_ref().clone(), Format::Odt).unwrap(),
        )
        .unwrap();
        assert_eq!(before, e.bytes);
        e.save(&output("38-structure-source.pdf")).unwrap();
    }

    #[test]
    fn prisms_lesson_keeps_figures_on_the_source_page() {
        let Some(path) = std::env::var_os("PDFINATOR_PRISMS_PDF") else {
            return;
        };
        let before = crate::files::read(std::path::Path::new(&path)).unwrap();
        let e = Editor::from_bytes(before.clone()).unwrap();
        let pages: Vec<_> = (0..e.info.pages.len())
            .map(|i| layout::page(&e, i).unwrap())
            .collect();
        assert_eq!(pages.len(), 1);
        let p = &pages[0];
        assert_eq!(
            p.items
                .iter()
                .filter(|i| matches!(i, Item::Picture(_)))
                .count(),
            3
        );
        assert_eq!(
            p.items
                .iter()
                .filter(|i| matches!(i,Item::Paragraph(p) if dotted(p)))
                .count(),
            7
        );
        assert!(!p.decorations.is_empty());
        assert!(p.items.iter().any(|i| matches!(i,Item::Paragraph(p) if p.runs.iter().map(|r|r.text.as_str()).collect::<String>().starts_with("Chapitre 2"))));
        assert!(
            p.items
                .iter()
                .filter(|i| matches!(i, Item::Picture(_)))
                .all(|i| i.rect()[3] < p.height - 1.)
        );

        let mut report = String::new();
        for p in &pages {
            report.push_str(&format!("Page {}x{} {:?}\n", p.width, p.height, p.margin));
            for (i, item) in p.items.iter().enumerate() {
                let label = match item {
                    Item::Paragraph(p) => p.runs.iter().map(|r| r.text.as_str()).collect(),
                    Item::Table(t) => {
                        format!("Table {}x{} {:?}", t.xs.len() - 1, t.ys.len() - 1, t.rows)
                    }
                    Item::Picture(_) => "Image".into(),
                };
                report.push_str(&format!("{i}: {:?} {label}\n", item.rect()));
            }
            let mut i = 0;
            while i < p.items.len() {
                let (end, cols) = columns(p, i);
                report.push_str(&format!(
                    "Band {i}..{end} {:?}\n",
                    cols.iter()
                        .map(|c| c.iter().map(|a| a.rect()).collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                ));
                i = end;
            }
        }
        std::fs::write(output("40-prisms-layout.txt"), report).unwrap();
        std::fs::write(output("40-prisms.docx"), word::document(&pages).unwrap()).unwrap();
        std::fs::write(output("40-prisms.odt"), odt::document(&pages).unwrap()).unwrap();
        assert_eq!(
            crate::files::read(std::path::Path::new(&path)).unwrap(),
            before
        );
    }

    #[test]
    fn worksheet_columns_and_fractions() {
        let Some(path) = std::env::var_os("PDFINATOR_LAYOUT_PDF") else {
            return;
        };
        let before = crate::files::read(std::path::Path::new(&path)).unwrap();
        let e = Editor::from_bytes(before.clone()).unwrap();
        let pages: Vec<_> = (0..e.info.pages.len())
            .map(|i| layout::page(&e, i).unwrap())
            .collect();
        assert_eq!(pages.len(), 1);
        let p = &pages[0];
        let symbols = p
            .fonts
            .iter()
            .find(|f| f.name.starts_with("OpenSymbol"))
            .expect("Keep the source Macintosh symbol subset");
        let symbols = ttf_parser::Face::parse(&symbols.bytes, 0).unwrap();
        assert!(symbols.glyph_index('=').is_some() && symbols.glyph_index('×').is_some());
        let table = p
            .items
            .iter()
            .find_map(|i| {
                if let Item::Table(t) = i {
                    Some(t)
                } else {
                    None
                }
            })
            .expect("Native source table");
        assert_eq!((table.xs.len() - 1, table.ys.len() - 1), (7, 9));
        let cell_text = |c: &Cell| {
            c.paragraphs
                .iter()
                .flat_map(|p| &p.runs)
                .map(|r| r.text.as_str())
                .collect::<String>()
        };
        assert_eq!(cell_text(&table.rows[0][0]).trim(), "Nombre");
        assert_eq!(cell_text(&table.rows[8][0]).trim(), "4 123");
        let fraction_count = p
            .items
            .iter()
            .filter_map(|i| {
                if let Item::Paragraph(p) = i {
                    Some(p)
                } else {
                    None
                }
            })
            .flat_map(|p| &p.runs)
            .filter(|r| r.fraction.is_some())
            .count();
        assert_eq!(
            fraction_count,
            14,
            "All numerator/denominator pairs, including the ellipsis, remain editable: {:?}",
            p.items
                .iter()
                .filter_map(|i| if let Item::Paragraph(p) = i {
                    Some(&p.runs)
                } else {
                    None
                })
                .flatten()
                .filter(|r| r.fraction.is_some())
                .map(|r| &r.text)
                .collect::<Vec<_>>()
        );
        let mut i = 0;
        let mut table_band = false;
        while i < p.items.len() {
            let (end, cols) = columns(p, i);
            if cols.iter().flatten().any(|i| matches!(i, Item::Table(_))) {
                assert_eq!(cols.len(), 2);
                assert!(cols[1].iter().any(|i|matches!(i,Item::Paragraph(p) if p.runs.iter().map(|r|r.text.as_str()).collect::<String>().contains("est leur nombre"))));
                table_band = true;
            }
            i = end;
        }
        assert!(table_band);
        let mut report = String::new();
        for p in &pages {
            report.push_str(&format!("Page {}x{} {:?}\n", p.width, p.height, p.margin));
            for (i, item) in p.items.iter().enumerate() {
                let label = match item {
                    Item::Paragraph(p) => format!(
                        "{} {:?}",
                        p.runs.iter().map(|r| r.text.as_str()).collect::<String>(),
                        p.runs
                            .iter()
                            .map(|r| (&r.font, r.size, r.rect))
                            .collect::<Vec<_>>()
                    ),
                    Item::Table(t) => format!("Table {}x{}", t.xs.len() - 1, t.ys.len() - 1),
                    Item::Picture(_) => "Image".into(),
                };
                report.push_str(&format!("{i}: {:?} {label}\n", item.rect()));
            }
            let mut i = 0;
            while i < p.items.len() {
                let (end, cols) = columns(p, i);
                report.push_str(&format!(
                    "Band {i}..{end} {:?}\n",
                    cols.iter()
                        .map(|c| c.iter().map(|a| a.rect()).collect::<Vec<_>>())
                        .collect::<Vec<_>>()
                ));
                i = end;
            }
        }
        std::fs::write(output("39-layout.txt"), report).unwrap();
        std::fs::write(output("39-multiples.docx"), word::document(&pages).unwrap()).unwrap();
        std::fs::write(output("39-multiples.odt"), odt::document(&pages).unwrap()).unwrap();
        assert_eq!(
            crate::files::read(std::path::Path::new(&path)).unwrap(),
            before
        );
    }
    #[test]
    fn user_pdf_exports_all_pages_without_mutating_the_original() {
        let Some(path) = std::env::var_os("PDFINATOR_REGRESSION_PDF") else {
            return;
        };
        let path = std::path::Path::new(&path);
        let before = crate::files::read(path).unwrap();
        let e = Editor::from_bytes(before.clone()).unwrap();
        let pages: Vec<_> = (0..e.info.pages.len())
            .map(|i| layout::page(&e, i).unwrap())
            .collect();
        std::fs::write(output("38-disposition.txt"), format!("{pages:#?}")).unwrap();
        assert_eq!(pages.len(), 2);
        assert!(
            !pages[0].decorations.is_empty(),
            "Keep the ornamental heading frame without rasterizing its words"
        );
        assert!(pages[0].items.iter().any(|i| matches!(i, Item::Paragraph(p) if (0..p.runs.len()).any(|n| angle(&p.runs, n).is_some()))), "Recognize the angle accents as editable equations");
        assert!(
            pages[0]
                .items
                .iter()
                .any(|i| matches!(i, Item::Paragraph(p) if dotted(p))),
            "PDF ellipses must become native dot leaders"
        );
        assert!(
            pages[0]
                .items
                .iter()
                .any(|i| matches!(i, Item::Paragraph(p) if p.runs.iter().any(|r| r.underline))),
            "Source vector underlines remain text formatting"
        );
        for region in [[80., 280., 200., 340.], [293.5, 14.8, 581., 164.2]] {
            assert!(pages[1].items.iter().any(|item| matches!(item, Item::Picture(p) if p.rect[0] <= region[0] && p.rect[1] <= region[1] && p.rect[2] >= region[2] && p.rect[3] >= region[3])), "A complete curved figure and its labels must travel together: {region:?}");
        }
        assert!(
            pages
                .iter()
                .any(|p| p.items.iter().any(|i| matches!(i, Item::Table(_)))),
            "The grading table should remain a real Word table"
        );
        std::fs::write(
            output("38-document-reel.docx"),
            word::document(&pages).unwrap(),
        )
        .unwrap();
        std::fs::write(
            output("38-document-reel.odt"),
            odt::document(&pages).unwrap(),
        )
        .unwrap();
        assert_eq!(crate::files::read(path).unwrap(), before);
    }
}
