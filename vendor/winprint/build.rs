// PDFinator uses only the native Windows.Data.Pdf backend.
// The optional upstream PDFium download/build path is deliberately unavailable.
#[cfg(feature = "pdfium")]
compile_error!("This PDFinator vendor build supports only the native Windows PDF backend");
fn main() {}
