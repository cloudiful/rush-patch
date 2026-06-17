use encoding_rs::Encoding;
use std::fs;
use std::io;
use std::path::Path;

pub fn read_text(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;

    if let Some((encoding, _)) = Encoding::for_bom(&bytes) {
        let (text, had_errors) = encoding.decode_with_bom_removal(&bytes);
        if had_errors {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to decode BOM text file {}", path.display()),
            ));
        }
        return Ok(text.into_owned());
    }

    String::from_utf8(bytes).map_err(|source| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "failed to decode UTF-8 text file {}: {source}",
                path.display()
            ),
        )
    })
}
