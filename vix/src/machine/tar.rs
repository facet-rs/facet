pub(super) struct TarEntry<'a> {
    pub name: String,
    pub typeflag: u8,
    pub contents: &'a [u8],
}

pub(super) fn walk(
    bytes: &[u8],
    context: &str,
    mut visit: impl FnMut(TarEntry<'_>) -> Result<(), String>,
) -> Result<(), String> {
    let mut offset = 0usize;
    while offset + 512 <= bytes.len() {
        let header = &bytes[offset..offset + 512];
        if header.iter().all(|byte| *byte == 0) {
            return Ok(());
        }
        let name = tar_name(header)?;
        let size = tar_size(header)?;
        let typeflag = header[156];
        let data_offset = offset + 512;
        let data_end = data_offset
            .checked_add(size)
            .ok_or_else(|| "tar entry size overflow".to_string())?;
        if data_end > bytes.len() {
            return Err(format!("{context} tar entry `{name}` extends past archive"));
        }
        visit(TarEntry {
            name,
            typeflag,
            contents: &bytes[data_offset..data_end],
        })?;
        offset = data_offset
            .checked_add(padded_len(size))
            .ok_or_else(|| "tar offset overflow".to_string())?;
    }
    Ok(())
}

fn tar_name(header: &[u8]) -> Result<String, String> {
    let name = header_string(&header[0..100])?;
    let prefix = header_string(&header[345..500])?;
    if prefix.is_empty() {
        Ok(name)
    } else if name.is_empty() {
        Ok(prefix)
    } else {
        Ok(format!("{prefix}/{name}"))
    }
}

fn header_string(bytes: &[u8]) -> Result<String, String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end])
        .map(str::to_string)
        .map_err(|err| err.to_string())
}

fn tar_size(header: &[u8]) -> Result<usize, String> {
    let raw = header_string(&header[124..136])?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    usize::from_str_radix(trimmed, 8).map_err(|err| format!("tar size `{trimmed}`: {err}"))
}

fn padded_len(size: usize) -> usize {
    size.div_ceil(512) * 512
}
