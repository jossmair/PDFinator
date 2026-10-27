//! Development-only conversion entry point for independent rendering checks.
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: office_probe source.pdf output-stem".into());
    }
    let source = std::fs::read(&args[0]).map_err(|e| e.to_string())?;
    for format in [pdfinator::docx::Format::Docx, pdfinator::docx::Format::Odt] {
        let output = std::path::Path::new(&args[1]).with_extension(format.extension());
        let bytes = pdfinator::docx::convert_as(source.clone(), format)?;
        std::fs::write(output, bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}
