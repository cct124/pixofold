//! 校验Exif的TIFF局部偏移和IFD图；元数据始终原字节保留，不旋转或重写MakerNote。

use super::{invalid, protected};
use crate::jpeg::JpegError;
use std::collections::HashSet;

struct Tiff<'a> {
    bytes: &'a [u8],
    little: bool,
}
impl Tiff<'_> {
    fn u16(&self, offset: usize) -> Result<u16, JpegError> {
        let b: [u8; 2] = self
            .bytes
            .get(offset..offset + 2)
            .ok_or_else(|| invalid("Exif字段截断"))?
            .try_into()
            .map_err(|_| invalid("Exif字段截断"))?;
        Ok(if self.little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    }
    fn u32(&self, offset: usize) -> Result<u32, JpegError> {
        let b: [u8; 4] = self
            .bytes
            .get(offset..offset + 4)
            .ok_or_else(|| invalid("Exif字段截断"))?
            .try_into()
            .map_err(|_| invalid("Exif字段截断"))?;
        Ok(if self.little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    }
    fn range(&self, offset: usize, length: usize) -> Result<(), JpegError> {
        if offset < 8
            || offset
                .checked_add(length)
                .is_none_or(|end| end > self.bytes.len())
        {
            return Err(invalid("Exif偏移越界"));
        }
        Ok(())
    }
}

pub(super) fn inspect(data: &[u8]) -> Result<u8, JpegError> {
    let bytes = data
        .strip_prefix(b"Exif\0\0")
        .ok_or_else(|| protected(0xe1))?;
    let little = match bytes.get(..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return Err(invalid("Exif字节序无效")),
    };
    let tiff = Tiff { bytes, little };
    if tiff.u16(2)? != 42 {
        return Err(invalid("Exif TIFF版本无效"));
    }
    let first = tiff.u32(4)? as usize;
    let mut pending = vec![(first, true)];
    let mut visited = HashSet::new();
    let mut tables: Vec<std::ops::Range<usize>> = Vec::new();
    let mut entries = 0usize;
    let mut orientation = 1;
    while let Some((offset, primary)) = pending.pop() {
        if !visited.insert(offset) {
            return Err(invalid("Exif IFD循环或重复引用"));
        }
        if visited.len() > 64 {
            return Err(JpegError::ResourceLimit("Exif IFD数"));
        }
        tiff.range(offset, 2)?;
        let count = usize::from(tiff.u16(offset)?);
        entries += count;
        if entries > 4096 {
            return Err(JpegError::ResourceLimit("Exif条目数"));
        }
        let end = offset + 2 + count * 12 + 4;
        tiff.range(offset, end - offset)?;
        if tables
            .iter()
            .any(|range| offset < range.end && range.start < end)
        {
            return Err(invalid("Exif IFD重叠"));
        }
        tables.push(offset..end);
        let mut tags = HashSet::new();
        let mut thumbnail_offset = None;
        let mut thumbnail_length = None;
        let mut strips = None;
        let mut strip_lengths = None;
        for i in 0..count {
            let entry = offset + 2 + i * 12;
            let tag = tiff.u16(entry)?;
            if !tags.insert(tag) {
                return Err(invalid("Exif标签重复"));
            }
            // TIFF的C2PA存储不是JPEG APP11，本阶段不授权剥除或保留过期声明。
            if tag == 0xcd41 {
                return Err(protected(0xe1));
            }
            let kind = tiff.u16(entry + 2)?;
            let units = tiff.u32(entry + 4)? as usize;
            let width = match kind {
                1 | 2 | 6 | 7 | 129 => 1,
                3 | 8 => 2,
                4 | 9 | 11 | 13 => 4,
                5 | 10 | 12 => 8,
                _ => return Err(protected(0xe1)),
            };
            let length = units
                .checked_mul(width)
                .ok_or_else(|| invalid("Exif长度溢出"))?;
            let value = if length <= 4 {
                entry + 8
            } else {
                tiff.u32(entry + 8)? as usize
            };
            tiff.range(value, length)?;
            if kind == 2 && units > 0 && bytes[value + length - 1] != 0 {
                return Err(invalid("Exif ASCII缺少结束符"));
            }
            if tag == 0x0112 {
                if kind != 3 || units != 1 || !(1..=8).contains(&tiff.u16(value)?) {
                    return Err(invalid("Exif方向无效"));
                }
                if primary {
                    orientation = tiff.u16(value)? as u8;
                }
            }
            if matches!(tag, 0x8769 | 0x8825 | 0xa005 | 0x014a) || kind == 13 {
                if !matches!(kind, 4 | 13)
                    || units == 0
                    || units > 64
                    || (tag != 0x014a && units != 1)
                {
                    return Err(invalid("Exif IFD指针无效"));
                }
                for index in 0..units {
                    let next = tiff.u32(value + index * 4)? as usize;
                    if next != 0 {
                        pending.push((next, false));
                    }
                }
                if pending.len() > 64 {
                    return Err(JpegError::ResourceLimit("Exif IFD数"));
                }
            }
            if matches!(tag, 0x0201 | 0x0202) {
                if kind != 4 || units != 1 {
                    return Err(invalid("Exif缩略图指针无效"));
                }
                let n = tiff.u32(value)? as usize;
                if tag == 0x0201 {
                    thumbnail_offset = Some(n);
                } else {
                    thumbnail_length = Some(n);
                }
            }
            if matches!(tag, 0x0111 | 0x0117) {
                if !matches!(kind, 3 | 4) || units == 0 || units > 4096 {
                    return Err(invalid("Exif条带数组无效"));
                }
                let values = (0..units)
                    .map(|j| {
                        if kind == 3 {
                            tiff.u16(value + j * 2).map(usize::from)
                        } else {
                            tiff.u32(value + j * 4).map(|n| n as usize)
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if tag == 0x0111 {
                    strips = Some(values);
                } else {
                    strip_lengths = Some(values);
                }
            }
        }
        match (thumbnail_offset, thumbnail_length) {
            (Some(start), Some(length)) if length > 0 => tiff.range(start, length)?,
            (None, None) => {}
            _ => return Err(invalid("Exif缩略图范围不完整")),
        }
        match (strips, strip_lengths) {
            (Some(offsets), Some(lengths)) if offsets.len() == lengths.len() => {
                for (start, length) in offsets.into_iter().zip(lengths) {
                    tiff.range(start, length)?;
                }
            }
            (None, None) => {}
            _ => return Err(invalid("Exif条带范围不完整")),
        }
        let next = tiff.u32(end - 4)? as usize;
        if next != 0 {
            pending.push((next, false));
        }
    }
    Ok(orientation)
}
