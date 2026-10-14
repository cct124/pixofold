//! 有界、校验CRC的APNG容器解析。扩展名不参与判定；无法解释的元数据明确拒绝。
use super::*;

pub const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
pub fn u32_at(b: &[u8], n: usize) -> u32 {
    u32::from_be_bytes(b[n..n + 4].try_into().expect("checked chunk length"))
}
fn u16_at(b: &[u8], n: usize) -> u16 {
    u16::from_be_bytes([b[n], b[n + 1]])
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Control {
    pub width: u32,
    pub height: u32,
    pub x: u32,
    pub y: u32,
    pub delay_num: u16,
    pub delay_den: u16,
    pub dispose: u8,
    pub blend: u8,
}
pub struct Frame<'a> {
    pub control: Control,
    pub data: Vec<&'a [u8]>,
}
pub struct Parsed<'a> {
    pub ihdr: &'a [u8],
    pub plays: u32,
    pub default_participates: bool,
    pub default: Vec<&'a [u8]>,
    pub frames: Vec<Frame<'a>>,
    pub palette: Vec<([u8; 4], &'a [u8])>,
    pub metadata_hash: String,
}
pub fn chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    output.extend_from_slice(&crc.finalize().to_be_bytes());
}
fn seq(data: &[u8], next: &mut u32) -> Result<()> {
    require(u32_at(data, 0) == *next, "sequence")?;
    *next = next.checked_add(1).ok_or(Error("sequence"))?;
    Ok(())
}
fn complete(frame: Option<&Frame<'_>>) -> Result<()> {
    if let Some(f) = frame {
        require(f.data.iter().any(|d| !d.is_empty()), "missing_frame_data")?;
    }
    Ok(())
}
pub(super) fn parse<'a>(bytes: &'a [u8], limits: &Limits, guard: &Guard<'_>) -> Result<Parsed<'a>> {
    guard.check()?;
    require(bytes.len() <= limits.max_input_bytes, "input_budget")?;
    require(bytes.starts_with(SIGNATURE), "signature")?;
    let mut parsed = Parsed {
        ihdr: &[],
        plays: 0,
        default_participates: false,
        default: vec![],
        frames: vec![],
        palette: vec![],
        metadata_hash: String::new(),
    };
    let (mut at, mut count, mut next, mut declared) = (8usize, 0usize, 0u32, None);
    let (mut idat_seen, mut idat_closed, mut ended) = (false, false, false);
    let mut metadata = Sha256::new();
    let mut metadata_bytes = 0usize;
    let mut singleton = Vec::new();
    while at < bytes.len() {
        guard.check()?;
        count += 1;
        require(count <= limits.max_chunks, "chunk_budget")?;
        require(!ended && bytes.len() - at >= 12, "truncated_or_trailing")?;
        let length = u32_at(bytes, at) as usize;
        require(length <= 0x7fff_ffff, "chunk_length")?;
        let end = at
            .checked_add(length)
            .and_then(|v| v.checked_add(12))
            .ok_or(Error("chunk_length"))?;
        require(end <= bytes.len(), "truncated_or_trailing")?;
        let kind: [u8; 4] = bytes[at + 4..at + 8]
            .try_into()
            .map_err(|_| Error("chunk_type"))?;
        require(
            kind.iter().all(u8::is_ascii_alphabetic) && kind[2].is_ascii_uppercase(),
            "chunk_type",
        )?;
        let data = &bytes[at + 8..end - 4];
        require(
            crc32fast::hash(&bytes[at + 4..end - 4]) == u32_at(bytes, end - 4),
            "crc",
        )?;
        require(
            count == 1 && kind == *b"IHDR" || count > 1 && kind != *b"IHDR",
            "ihdr_order",
        )?;
        if idat_seen && kind != *b"IDAT" {
            idat_closed = true;
        }
        match &kind {
            b"IHDR" => {
                require(length == 13, "ihdr")?;
                let (w, h) = (u32_at(data, 0), u32_at(data, 4));
                require(
                    w > 0 && h > 0 && w <= 0x7fff_ffff && h <= 0x7fff_ffff,
                    "dimensions",
                )?;
                require(
                    w <= limits.max_dimension && h <= limits.max_dimension,
                    "dimension_budget",
                )?;
                require(
                    u64::from(w) * u64::from(h) <= limits.max_canvas_pixels,
                    "canvas_budget",
                )?;
                require(
                    matches!((data[8], data[9]), (8 | 16, 0 | 2 | 4 | 6) | (8, 3)),
                    "unsupported_representation",
                )?;
                require(data[10] == 0 && data[11] == 0 && data[12] <= 1, "ihdr")?;
                parsed.ihdr = data;
            }
            b"acTL" => {
                require(
                    length == 8 && declared.is_none() && !idat_seen,
                    "actl_order",
                )?;
                let frames = u32_at(data, 0);
                require(frames > 0, "frame_count")?;
                require(frames as usize <= limits.max_frames, "frame_budget")?;
                declared = Some(frames);
                parsed.plays = u32_at(data, 4);
            }
            b"fcTL" => {
                require(length == 26 && declared.is_some(), "fctl_order")?;
                seq(data, &mut next)?;
                complete(parsed.frames.last())?;
                require(parsed.frames.len() < limits.max_frames, "frame_budget")?;
                let control = Control {
                    width: u32_at(data, 4),
                    height: u32_at(data, 8),
                    x: u32_at(data, 12),
                    y: u32_at(data, 16),
                    delay_num: u16_at(data, 20),
                    delay_den: u16_at(data, 22),
                    dispose: data[24],
                    blend: data[25],
                };
                let (w, h) = (u32_at(parsed.ihdr, 0), u32_at(parsed.ihdr, 4));
                require(
                    control.width > 0
                        && control.height > 0
                        && control.x.checked_add(control.width).is_some_and(|v| v <= w)
                        && control
                            .y
                            .checked_add(control.height)
                            .is_some_and(|v| v <= h),
                    "frame_bounds",
                )?;
                require(
                    u64::from(control.width) * u64::from(control.height) <= limits.max_frame_pixels,
                    "frame_pixel_budget",
                )?;
                require(
                    control.dispose <= 2 && control.blend <= 1,
                    "frame_operation",
                )?;
                if !idat_seen {
                    require(
                        parsed.frames.is_empty()
                            && control.x == 0
                            && control.y == 0
                            && control.width == w
                            && control.height == h,
                        "default_frame_bounds",
                    )?;
                    parsed.default_participates = true;
                }
                parsed.frames.push(Frame {
                    control,
                    data: vec![],
                });
            }
            b"IDAT" => {
                require(declared.is_some() && !idat_closed, "idat_order")?;
                idat_seen = true;
                parsed.default.push(data);
                if parsed.default_participates {
                    parsed.frames[0].data.push(data);
                }
            }
            b"fdAT" => {
                require(
                    length >= 4
                        && idat_seen
                        && !parsed.frames.is_empty()
                        && (!parsed.default_participates || parsed.frames.len() > 1),
                    "fdat_order",
                )?;
                seq(data, &mut next)?;
                parsed
                    .frames
                    .last_mut()
                    .ok_or(Error("fdat_order"))?
                    .data
                    .push(&data[4..]);
            }
            b"IEND" => {
                require(length == 0 && idat_seen, "iend")?;
                complete(parsed.frames.last())?;
                require(declared == Some(parsed.frames.len() as u32), "frame_count")?;
                ended = true;
            }
            _ => {
                require(
                    kind[0].is_ascii_lowercase() || kind == *b"PLTE",
                    "unknown_critical",
                )?;
                require(
                    matches!(
                        &kind,
                        b"PLTE" | b"tRNS" | b"gAMA" | b"cHRM" | b"sRGB" | b"pHYs" | b"tEXt"
                    ),
                    "unsupported_metadata",
                )?;
                metadata_bytes = metadata_bytes
                    .checked_add(length + 12)
                    .ok_or(Error("metadata_budget"))?;
                require(
                    metadata_bytes <= limits.max_metadata_bytes,
                    "metadata_budget",
                )?;
                if kind != *b"tEXt" {
                    require(!idat_seen && !singleton.contains(&kind), "metadata_order")?;
                    singleton.push(kind);
                }
                match &kind {
                    b"PLTE" => require(
                        length > 0
                            && length <= 768
                            && length.is_multiple_of(3)
                            && !singleton.contains(b"tRNS")
                            && matches!(parsed.ihdr[9], 2 | 3 | 6),
                        "palette",
                    )?,
                    b"tRNS" => {
                        let palette = parsed.palette.iter().find(|(k, _)| k == b"PLTE");
                        require(
                            match parsed.ihdr[9] {
                                0 => length == 2,
                                2 => length == 6,
                                3 => palette
                                    .is_some_and(|(_, p)| length > 0 && length <= p.len() / 3),
                                _ => false,
                            },
                            "transparency",
                        )?;
                    }
                    b"gAMA" => require(
                        length == 4 && u32_at(data, 0) > 0 && !singleton.contains(b"PLTE"),
                        "metadata",
                    )?,
                    b"cHRM" => require(length == 32 && !singleton.contains(b"PLTE"), "metadata")?,
                    b"sRGB" => require(
                        length == 1 && data[0] <= 3 && !singleton.contains(b"PLTE"),
                        "metadata",
                    )?,
                    b"pHYs" => require(length == 9 && data[8] <= 1, "metadata")?,
                    b"tEXt" => require(
                        data.iter()
                            .position(|v| *v == 0)
                            .is_some_and(|n| (1..=79).contains(&n)),
                        "metadata",
                    )?,
                    _ => {}
                }
                // 保持内容、相对次序及动画锚点；序号/压缩块分片不作为元数据。
                metadata.update(kind);
                metadata.update((length as u64).to_be_bytes());
                metadata.update(data);
                metadata.update([u8::from(idat_seen)]);
                metadata.update((parsed.frames.len() as u64).to_be_bytes());
                if matches!(&kind, b"PLTE" | b"tRNS") {
                    parsed.palette.push((kind, data));
                }
            }
        }
        at = end;
    }
    require(ended && !parsed.default.is_empty(), "truncated_or_trailing")?;
    require(
        parsed.ihdr[9] != 3 || singleton.contains(b"PLTE"),
        "palette",
    )?;
    parsed.metadata_hash = format!("{:x}", metadata.finalize());
    Ok(parsed)
}
