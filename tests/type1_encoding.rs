use pdfcraft_cos::{Dict, Document, Object, Stream};
use pdfcraft_fonts::pdf::Metrics;

fn font(encoding: Option<Object>, unicode: bool) -> (Document, Dict) {
    let mut doc = Document::new_empty();
    let program = doc.add(Stream::flate(Dict::new(), b"%!PS-AdobeFont-1.0: Synthetic 1.0\n/Encoding 256 array\n0 1 255 {1 index exch /.notdef put} for\ndup 0 /minus put\ndup 2 /multiply put\ndup 4 /divide put\nreadonly def\ncurrentfile eexec\n"));
    let mut descriptor = Dict::new();
    descriptor.set(b"FontFile".to_vec(), Object::Ref(program));
    let mut f = Dict::new();
    f.set(b"Subtype".to_vec(), Object::name("Type1"));
    f.set(b"BaseFont".to_vec(), Object::name("ABCDEF+SyntheticMath"));
    f.set(b"FontDescriptor".to_vec(), Object::Dict(descriptor));
    f.set(b"FirstChar".to_vec(), Object::Int(0));
    f.set(
        b"Widths".to_vec(),
        Object::Array(vec![Object::Int(600); 256]),
    );
    if let Some(encoding) = encoding {
        f.set(b"Encoding".to_vec(), encoding);
    }
    if unicode {
        let cmap = doc.add(Stream::flate(
            Dict::new(),
            b"1 beginbfchar <00> <002B> endbfchar",
        ));
        f.set(b"ToUnicode".to_vec(), Object::Ref(cmap));
    }
    (doc, f)
}

#[test]
fn embedded_type1_builtin_encoding_preserves_math_operators() {
    let (doc, f) = font(None, false);
    let metrics = Metrics::from_dict(&doc, &f);
    assert_eq!(metrics.decode(&[0, 2, 4]), "−×÷");
    assert_eq!(metrics.encode("−×÷"), Some(vec![0, 2, 4]));
    assert_eq!(
        metrics.text_of(65),
        None,
        "Undefined glyphs are not Latin letters"
    );
}

#[test]
fn explicit_pdf_encoding_and_unicode_override_type1_builtin() {
    let (doc, f) = font(Some(Object::name("WinAnsiEncoding")), false);
    assert_eq!(Metrics::from_dict(&doc, &f).decode(b"A"), "A");
    let (doc, f) = font(None, true);
    assert_eq!(Metrics::from_dict(&doc, &f).decode(&[0, 2]), "+×");
}
