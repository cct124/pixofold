//! JPEG元数据分类、局部结构校验与C2PA范围识别。
//! 保留所有非凭据段的字节/顺序；未知APP用途仍拒绝，避免破坏全文件偏移或签名。

mod credentials;
mod exif;
#[cfg(test)]
mod tests;
mod xmp;

use super::JpegError;
use std::{collections::HashSet, ops::Range};

pub(super) struct Info {
    pub orientation: u8,
    pub credentials: Vec<Range<usize>>,
}
fn invalid(reason: &'static str) -> JpegError {
    JpegError::InvalidJpeg(reason)
}
fn protected(marker: u8) -> JpegError {
    JpegError::ProtectedMetadata(marker)
}

fn photoshop(data: &[u8]) -> Result<(), JpegError> {
    let bytes = data
        .strip_prefix(b"Photoshop 3.0\0")
        .ok_or_else(|| protected(0xed))?;
    let mut at = 0usize;
    let mut ids = HashSet::new();
    while at < bytes.len() {
        if bytes.get(at..at + 4) != Some(b"8BIM") {
            return Err(invalid("Photoshop资源签名无效"));
        }
        let id = bytes
            .get(at + 4..at + 6)
            .ok_or_else(|| invalid("Photoshop资源截断"))?;
        if !ids.insert(id) {
            return Err(invalid("Photoshop资源重复"));
        }
        let length = usize::from(
            *bytes
                .get(at + 6)
                .ok_or_else(|| invalid("Photoshop名称截断"))?,
        ) + 1;
        at += 6 + length.div_ceil(2) * 2;
        let length = u32::from_be_bytes(
            bytes
                .get(at..at + 4)
                .ok_or_else(|| invalid("Photoshop长度截断"))?
                .try_into()
                .map_err(|_| invalid("Photoshop长度截断"))?,
        ) as usize;
        let start = at + 4;
        at = at
            .checked_add(4)
            .and_then(|n| n.checked_add(length))
            .and_then(|n| n.checked_add(length % 2))
            .filter(|n| *n <= bytes.len())
            .ok_or_else(|| invalid("Photoshop资源越界"))?;
        match id {
            // Photoshop也可在IRB内携带XMP，仍应用相同保护/外链检查。
            [0x04, 0x24] => xmp::inspect(&bytes[start..start + length])?,
            // Photoshop私有Exif资源的偏移基准不作为本轮普通APP1支持范围。
            [0x04, 0x22] | [0x04, 0x23] => return Err(protected(0xed)),
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn inspect(
    metadata: &[(u8, &[u8])],
    ranges: &[Range<usize>],
) -> Result<Info, JpegError> {
    let mut seen = HashSet::new();
    let mut orientation = 1;
    for &(marker, data) in metadata {
        let class = match marker {
            0xe0 => {
                if !data.starts_with(b"JFIF\0")
                    || data.len() < 14
                    || data.len() != 14 + usize::from(data[12]) * usize::from(data[13]) * 3
                {
                    return Err(protected(marker));
                }
                0
            }
            0xe1 if data.starts_with(b"Exif\0\0") => {
                orientation = exif::inspect(data)?;
                1
            }
            0xe1 if data.starts_with(xmp::PREFIX) => {
                xmp::inspect(&data[xmp::PREFIX.len()..])?;
                2
            }
            0xe2 => {
                if !data.starts_with(b"ICC_PROFILE\0")
                    || data.len() <= 14
                    || data[12] == 0
                    || data[12] > data[13]
                {
                    return Err(protected(marker));
                }
                continue;
            }
            0xeb => continue, // 所有分片收齐后独立核对，不按APP11编号直接授权。
            0xed => {
                photoshop(data)?;
                3
            }
            0xee if data.len() == 12 && data.starts_with(b"Adobe") && data[11] <= 2 => 4,
            0xfe => continue,
            _ => return Err(protected(marker)),
        };
        if !seen.insert(class) {
            return Err(invalid("JPEG元数据类别重复"));
        }
    }
    Ok(Info {
        orientation,
        credentials: credentials::inspect(metadata, ranges)?,
    })
}

pub(super) fn remove(bytes: &[u8], ranges: &[Range<usize>]) -> Result<Vec<u8>, JpegError> {
    let mut result = Vec::with_capacity(bytes.len());
    let mut at = 0;
    for range in ranges {
        if range.start < at || range.end > bytes.len() || range.start >= range.end {
            return Err(JpegError::ValidationFailed);
        }
        result.extend_from_slice(&bytes[at..range.start]);
        at = range.end;
    }
    result.extend_from_slice(&bytes[at..]);
    Ok(result)
}
