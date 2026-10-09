//! 严格、无分配解码的JPEG结构预检。只接受能明确保留的元数据；原生验证仍不可省略。

use super::{JpegError, JpegInfo, JpegLimits, JpegLossyFallbackReason};

pub(super) struct Parsed<'a> {
    pub info: JpegInfo,
    pub coefficient_bytes: u64,
    sample_layout: Vec<[u8; 2]>,
    working_bytes: u64,
    metadata: Vec<(u8, &'a [u8])>,
}

impl Parsed<'_> {
    pub fn preview_orientation(&self) -> Result<u8, JpegError> {
        if matches!(
            self.lossy_fallback(),
            Some(
                JpegLossyFallbackReason::ColorProfile
                    | JpegLossyFallbackReason::FourComponentColor
                    | JpegLossyFallbackReason::AmbiguousColor
            )
        ) || self.ambiguous_color()
        {
            // 预览不做ICC/CMYK转换，不将不明确的颜色解释伪装成正常预览。
            return Err(JpegError::UnsupportedJpeg);
        }
        Ok(self
            .metadata
            .iter()
            .find(|(marker, _)| *marker == 0xe1)
            .and_then(|(_, data)| exif_orientation(data))
            .unwrap_or(1))
    }
    pub fn same_image_and_metadata(&self, other: &Parsed<'_>) -> bool {
        self.info == other.info
            && self.sample_layout == other.sample_layout
            && self.metadata == other.metadata
    }

    pub fn lossy_fallback(&self) -> Option<JpegLossyFallbackReason> {
        if self.metadata.iter().any(|(marker, _)| *marker == 0xe2) {
            return Some(JpegLossyFallbackReason::ColorProfile);
        }
        if self.info.components == 4 {
            return Some(JpegLossyFallbackReason::FourComponentColor);
        }
        let jfif = self
            .metadata
            .iter()
            .find(|(marker, _)| *marker == 0xe0)
            .map(|(_, data)| *data);
        if jfif.is_some_and(|data| data[12] != 0 || data[13] != 0) {
            return Some(JpegLossyFallbackReason::EmbeddedThumbnail);
        }
        self.ambiguous_color()
            .then_some(JpegLossyFallbackReason::AmbiguousColor)
    }

    fn ambiguous_color(&self) -> bool {
        let jfif = self.metadata.iter().any(|(marker, _)| *marker == 0xe0);
        let adobe = self
            .metadata
            .iter()
            .find(|(marker, _)| *marker == 0xee)
            .map(|(_, data)| data[11]);
        let ids: Vec<_> = self
            .sample_layout
            .iter()
            .map(|component| component[0])
            .collect();
        self.info.components == 3
            && (adobe == Some(2)
                || (jfif && adobe == Some(0))
                || (!jfif && adobe.is_none() && ids != [1, 2, 3] && ids != b"RGB"))
    }

    pub fn check_pixel_budget(&self, limits: JpegLimits) -> Result<(), JpegError> {
        // 原生系数上限之外保守计入两份全尺寸扫描线/颜色工作区；原生读取实际头后同样复查。
        let raw = u64::from(self.info.width)
            * u64::from(self.info.height)
            * u64::from(self.info.components);
        if self.working_bytes + raw * 2 > limits.resources.max_decoded_bytes.0 {
            return Err(JpegError::ResourceLimit("JPEG有损解码与编码工作集"));
        }
        Ok(())
    }
}

fn invalid(reason: &'static str) -> JpegError {
    JpegError::InvalidJpeg(reason)
}
fn be16(bytes: &[u8]) -> u32 {
    u32::from(u16::from_be_bytes([bytes[0], bytes[1]]))
}

pub(super) fn inspect(bytes: &[u8], limits: JpegLimits) -> Result<Parsed<'_>, JpegError> {
    if bytes.len() as u64 > limits.resources.max_input_bytes.0 {
        return Err(JpegError::ResourceLimit("输入字节"));
    }
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(invalid("缺少SOI"));
    }
    let mut offset = 2;
    let mut entropy = false;
    let mut scans = 0;
    let mut markers = 0;
    let mut frame = None;
    let mut sample_layout = Vec::new();
    let mut metadata = Vec::new();
    let mut metadata_bytes = 0u64;
    loop {
        let marker = loop {
            if entropy {
                while bytes.get(offset).is_some_and(|byte| *byte != 0xff) {
                    offset += 1;
                }
            }
            if bytes.get(offset) != Some(&0xff) {
                return Err(invalid("标记缺失或截断"));
            }
            while bytes.get(offset) == Some(&0xff) {
                offset += 1;
            }
            let marker = *bytes.get(offset).ok_or_else(|| invalid("标记截断"))?;
            offset += 1;
            if entropy && (marker == 0 || (0xd0..=0xd7).contains(&marker)) {
                continue;
            }
            break marker;
        };
        entropy = false;
        markers += 1;
        if markers > 4096 {
            return Err(JpegError::ResourceLimit("标记数"));
        }
        if marker == 0xd9 {
            if offset != bytes.len() || scans == 0 {
                return Err(invalid("EOI尾随内容或无扫描"));
            }
            validate_icc(&metadata)?;
            let (info, coefficient_bytes, working_bytes) =
                frame.ok_or_else(|| invalid("缺少图像帧"))?;
            if working_bytes + metadata_bytes * 2 > limits.resources.max_decoded_bytes.0 {
                return Err(JpegError::ResourceLimit("系数和元数据工作集"));
            }
            return Ok(Parsed {
                info,
                coefficient_bytes,
                sample_layout,
                working_bytes: working_bytes + metadata_bytes * 2,
                metadata,
            });
        }
        let length = bytes
            .get(offset..offset + 2)
            .ok_or_else(|| invalid("段长度截断"))?;
        let length = be16(length) as usize;
        if length < 2 {
            return Err(invalid("段长度小于2"));
        }
        let payload = bytes
            .get(offset + 2..offset + length)
            .ok_or_else(|| invalid("段截断"))?;
        offset += length;
        match marker {
            0xc0 | 0xc2 => {
                if frame.is_some() || scans != 0 {
                    return Err(invalid("重复图像帧"));
                }
                frame = Some(parse_frame(payload, marker == 0xc2, limits)?);
                sample_layout = payload[6..]
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|p| [p[0], p[1]])
                    .collect();
            }
            0xda => {
                if frame.is_none() || payload.len() < 6 {
                    return Err(invalid("扫描头无效"));
                }
                scans += 1;
                if scans > limits.max_scans {
                    return Err(JpegError::ResourceLimit("扫描数"));
                }
                entropy = true;
            }
            0xdb if scans == 0 => {} // 量化表在扫描中重新定义时保守拒绝。
            0xc4 | 0xdd => {}        // Huffman/重启间隔由原生解码器严格检查。
            0xe0..=0xef | 0xfe => {
                if scans != 0 {
                    return Err(JpegError::ProtectedMetadata(marker));
                }
                check_metadata(marker, payload, &metadata)?;
                metadata_bytes += payload.len() as u64 + 64;
                if metadata_bytes > 1024 * 1024 {
                    return Err(JpegError::ResourceLimit("元数据字节"));
                }
                metadata.push((marker, payload));
            }
            _ => return Err(JpegError::UnsupportedJpeg),
        }
    }
}

fn parse_frame(
    payload: &[u8],
    progressive: bool,
    limits: JpegLimits,
) -> Result<(JpegInfo, u64, u64), JpegError> {
    if payload.len() < 6 {
        return Err(invalid("帧头截断"));
    }
    let components = payload[5];
    if payload[0] != 8 || ![1, 3, 4].contains(&components) {
        return Err(JpegError::UnsupportedJpeg);
    }
    if payload.len() != 6 + 3 * usize::from(components) {
        return Err(invalid("分量表无效"));
    }
    let width = be16(&payload[3..5]);
    let height = be16(&payload[1..3]);
    if width == 0 || height == 0 {
        return Err(invalid("零尺寸"));
    }
    if width > limits.resources.max_dimension
        || height > limits.resources.max_dimension
        || u64::from(width) * u64::from(height) > limits.resources.max_pixels
    {
        return Err(JpegError::ResourceLimit("尺寸或像素数"));
    }
    let mut samples = Vec::new();
    let mut ids = Vec::new();
    for component in payload[6..].as_chunks::<3>().0 {
        let (h, v) = (u64::from(component[1] >> 4), u64::from(component[1] & 15));
        if !(1..=4).contains(&h)
            || !(1..=4).contains(&v)
            || component[2] > 3
            || ids.contains(&component[0])
        {
            return Err(invalid("采样或分量无效"));
        }
        ids.push(component[0]);
        samples.push((h, v));
    }
    let mh = samples.iter().map(|s| s.0).max().unwrap_or(1);
    let mv = samples.iter().map(|s| s.1).max().unwrap_or(1);
    let mut blocks = 0;
    let mut padded = 0;
    for (h, v) in samples {
        let w = (u64::from(width) * h).div_ceil(mh * 8);
        let rows = (u64::from(height) * v).div_ceil(mv * 8);
        blocks += w * rows;
        padded += w.div_ceil(h) * h * rows.div_ceil(v) * v;
    }
    if padded * 64 * 2 * 2 + 1024 * 1024 > limits.resources.max_decoded_bytes.0 {
        return Err(JpegError::ResourceLimit("系数工作集"));
    }
    // PFJC1 + 五个u32头字段 + 每分量五个字段/64个量化值 + 每系数有符号u32。
    let coefficient_bytes = 25 + u64::from(components) * (5 + 64) * 4 + blocks * 64 * 4;
    Ok((
        JpegInfo {
            width,
            height,
            components,
            progressive,
        },
        coefficient_bytes,
        padded * 64 * 2 * 2 + 1024 * 1024,
    ))
}

/// 调度只读取最多2MiB标记前缀，找到首个SOF即停止；只用于收紧执行上限。
/// 未知/坏头由调用方保留原预算，完整结构和像素仍由执行层验证。
pub(crate) fn probe_header(
    reader: impl std::io::Read,
    limits: JpegLimits,
) -> Option<(JpegInfo, u64)> {
    use std::io::Read;
    let mut reader = reader.take(2 * 1024 * 1024);
    let mut soi = [0u8; 2];
    reader.read_exact(&mut soi).ok()?;
    if soi != [0xff, 0xd8] {
        return None;
    }
    for _ in 0..4096 {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte).ok()?;
        if byte[0] != 0xff {
            return None;
        }
        loop {
            reader.read_exact(&mut byte).ok()?;
            if byte[0] != 0xff {
                break;
            }
        }
        let marker = byte[0];
        if marker == 0xda || marker == 0xd9 {
            return None;
        }
        let mut length = [0u8; 2];
        reader.read_exact(&mut length).ok()?;
        let length = usize::from(u16::from_be_bytes(length)).checked_sub(2)?;
        if marker == 0xc0 || marker == 0xc2 {
            let mut payload = [0u8; 18];
            let payload = payload.get_mut(..length)?;
            reader.read_exact(payload).ok()?;
            let (info, _, working) = parse_frame(payload, marker == 0xc2, limits).ok()?;
            let pixels = u64::from(info.width) * u64::from(info.height);
            // 元数据上限两份，加上最坏有损像素工作区；同时覆盖无损回退。
            let required = working + 2 * 1024 * 1024 + pixels * u64::from(info.components) * 2;
            return Some((info, required));
        }
        let mut left = length;
        let mut discard = [0u8; 8192];
        while left > 0 {
            let n = left.min(discard.len());
            reader.read_exact(&mut discard[..n]).ok()?;
            left -= n;
        }
    }
    None
}

fn check_metadata(marker: u8, data: &[u8], previous: &[(u8, &[u8])]) -> Result<(), JpegError> {
    let known = match marker {
        0xe0 => {
            data.starts_with(b"JFIF\0")
                && data.len() >= 14
                && data.len() == 14 + usize::from(data[12]) * usize::from(data[13]) * 3
        }
        0xe1 => exif_orientation(data).is_some(),
        0xe2 => {
            data.starts_with(b"ICC_PROFILE\0")
                && data.len() > 14
                && data[12] != 0
                && data[12] <= data[13]
        }
        0xee => data.len() == 12 && data.starts_with(b"Adobe") && data[11] <= 2,
        0xfe => true,
        // 包括APP11/JUMBF：即使字节能保留，也不能保证改写后的签名仍有效。
        _ => false,
    };
    if !known || (![0xe2, 0xfe].contains(&marker) && previous.iter().any(|p| p.0 == marker)) {
        return Err(JpegError::ProtectedMetadata(marker));
    }
    Ok(())
}

/// 首版只接收单IFD、单Orientation项。MakerNote/缩略图/外部偏移等不能仅靠原样复制宣称安全。
fn exif_orientation(data: &[u8]) -> Option<u8> {
    if data.len() != 32 || !data.starts_with(b"Exif\0\0") {
        return None;
    }
    let tiff = &data[6..];
    let little = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |i| {
        if little {
            u16::from_le_bytes([tiff[i], tiff[i + 1]])
        } else {
            u16::from_be_bytes([tiff[i], tiff[i + 1]])
        }
    };
    let u32_at = |i| {
        if little {
            u32::from_le_bytes([tiff[i], tiff[i + 1], tiff[i + 2], tiff[i + 3]])
        } else {
            u32::from_be_bytes([tiff[i], tiff[i + 1], tiff[i + 2], tiff[i + 3]])
        }
    };
    (u16_at(2) == 42
        && u32_at(4) == 8
        && u16_at(8) == 1
        && u16_at(10) == 0x112
        && u16_at(12) == 3
        && u32_at(14) == 1
        && (1..=8).contains(&u16_at(18))
        && u16_at(20) == 0
        && u32_at(22) == 0)
        .then_some(u16_at(18) as u8)
}

fn validate_icc(metadata: &[(u8, &[u8])]) -> Result<(), JpegError> {
    let chunks: Vec<_> = metadata
        .iter()
        .filter(|m| m.0 == 0xe2)
        .map(|m| m.1)
        .collect();
    if chunks.is_empty() {
        return Ok(());
    }
    let count = chunks[0][13];
    let size: usize = chunks.iter().map(|m| m.len() - 14).sum();
    let mut seen = [false; 256];
    for data in &chunks {
        if data[13] != count || seen[usize::from(data[12])] {
            return Err(JpegError::ProtectedMetadata(0xe2));
        }
        seen[usize::from(data[12])] = true;
    }
    let first = chunks.iter().find(|m| m[12] == 1).copied().unwrap_or(&[]);
    if chunks.len() != usize::from(count)
        || first.len() < 14 + 128
        || &first[50..54] != b"acsp"
        || u32::from_be_bytes([first[14], first[15], first[16], first[17]]) as usize != size
    {
        return Err(JpegError::ProtectedMetadata(0xe2));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_probe_caps_even_an_unterminated_marker_prefix() {
        let mut bytes = vec![0xff; 3 * 1024 * 1024];
        bytes[1] = 0xd8;
        let mut reader = std::io::Cursor::new(bytes);
        assert!(probe_header(&mut reader, JpegLimits::default()).is_none());
        assert_eq!(reader.position(), 2 * 1024 * 1024);
    }
    #[test]
    fn arbitrary_truncations_and_markers_are_errors_not_panics() {
        for len in 0..512 {
            for seed in [0, 0xff, 0x7a] {
                let mut data = vec![seed; len];
                if len >= 2 {
                    data[..2].copy_from_slice(&[0xff, 0xd8]);
                }
                assert!(inspect(&data, JpegLimits::default()).is_err());
            }
        }
    }
    #[test]
    fn embedded_thumbnail_cannot_hide_conflicting_color_markers_from_preview() {
        let mut jfif = b"JFIF".to_vec();
        jfif.extend_from_slice(&[0, 1, 1, 0, 0, 1, 0, 1, 1, 1, 0, 0, 0]);
        let adobe = &[b'A', b'd', b'o', b'b', b'e', 0, 100, 0, 0, 0, 0, 0];
        let mut parsed = Parsed {
            info: JpegInfo {
                width: 8,
                height: 8,
                components: 3,
                progressive: false,
            },
            coefficient_bytes: 0,
            working_bytes: 0,
            sample_layout: vec![[1, 0], [2, 0], [3, 0]],
            metadata: vec![(0xe0, &jfif), (0xee, adobe)],
        };
        assert_eq!(
            parsed.lossy_fallback(),
            Some(JpegLossyFallbackReason::EmbeddedThumbnail)
        );
        assert!(matches!(
            parsed.preview_orientation(),
            Err(JpegError::UnsupportedJpeg)
        ));
        parsed.metadata.pop();
        assert_eq!(parsed.preview_orientation().unwrap(), 1);
    }
    #[test]
    fn credentials_and_unknown_apps_are_never_silently_preserved() {
        for marker in [0xe3, 0xeb, 0xed] {
            assert!(
                matches!(check_metadata(marker, b"unknown", &[]), Err(JpegError::ProtectedMetadata(m)) if m == marker)
            );
        }
    }
    #[test]
    fn only_simple_orientation_exif_is_accepted_in_both_byte_orders() {
        for little in [false, true] {
            for orientation in 1u16..=8 {
                let mut data = b"Exif\0\0".to_vec();
                let mut tiff = vec![0; 26];
                tiff[..2].copy_from_slice(if little { b"II" } else { b"MM" });
                for (at, value) in [(2, 42u16), (8, 1), (10, 0x112), (12, 3), (18, orientation)] {
                    tiff[at..at + 2].copy_from_slice(&if little {
                        value.to_le_bytes()
                    } else {
                        value.to_be_bytes()
                    });
                }
                for (at, value) in [(4, 8u32), (14, 1)] {
                    tiff[at..at + 4].copy_from_slice(&if little {
                        value.to_le_bytes()
                    } else {
                        value.to_be_bytes()
                    });
                }
                data.extend_from_slice(&tiff);
                assert_eq!(exif_orientation(&data), Some(orientation as u8));
                data[31] = 1; // 非零next IFD指针不能作为安全的方向-only Exif。
                assert!(exif_orientation(&data).is_none());
            }
        }
    }
    #[test]
    fn duplicate_incomplete_icc_and_metadata_budget_are_rejected() {
        let mut profile = b"ICC_PROFILE\0".to_vec();
        profile.extend_from_slice(&[1, 1]);
        profile.extend_from_slice(&[0; 128]);
        profile[14..18].copy_from_slice(&128u32.to_be_bytes());
        profile[50..54].copy_from_slice(b"acsp");
        assert!(validate_icc(&[(0xe2, &profile)]).is_ok());
        assert!(validate_icc(&[(0xe2, &profile), (0xe2, &profile)]).is_err());
        profile[13] = 2;
        assert!(validate_icc(&[(0xe2, &profile)]).is_err());
        let mut bytes = vec![0xff, 0xd8];
        for _ in 0..17 {
            bytes.extend_from_slice(&[0xff, 0xfe, 0xff, 0xff]);
            bytes.extend_from_slice(&vec![0; 65533]);
        }
        assert!(matches!(
            inspect(&bytes, JpegLimits::default()),
            Err(JpegError::ResourceLimit("元数据字节"))
        ));
    }
    #[test]
    fn limits_charge_padded_coefficients_and_reject_precision() {
        let frame = [8, 0, 17, 0, 17, 3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1];
        assert!(parse_frame(&frame, false, JpegLimits::default()).is_ok());
        let mut limits = JpegLimits::default();
        limits.resources.max_decoded_bytes.0 = 1024 * 1024;
        assert!(matches!(
            parse_frame(&frame, false, limits),
            Err(JpegError::ResourceLimit(_))
        ));
        let mut twelve = frame;
        twelve[0] = 12;
        assert!(matches!(
            parse_frame(&twelve, false, JpegLimits::default()),
            Err(JpegError::UnsupportedJpeg)
        ));
    }
}
