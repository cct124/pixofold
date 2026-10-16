//! 只识别完整C2PA Manifest Store；不把其他APP11/JPEG XT数据变成删除许可。
//! 识别容器与可删除范围不等于验证签名真实性。

use super::{invalid, protected};
use crate::jpeg::JpegError;
use std::ops::Range;

const C2PA_UUID: &[u8; 16] = b"c2pa\0\x11\0\x10\x80\0\0\xaa\0\x38\x9b\x71";

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, JpegError> {
    Ok(u32::from_be_bytes(
        bytes
            .get(at..at + 4)
            .ok_or_else(|| invalid("JUMBF字段截断"))?
            .try_into()
            .map_err(|_| invalid("JUMBF字段截断"))?,
    ))
}

fn box_header(bytes: &[u8]) -> Result<(usize, usize), JpegError> {
    let size = u32_at(bytes, 0)?;
    if bytes.len() < 8 || size == 0 {
        return Err(invalid("JUMBF盒长度无效"));
    }
    let (size, header) = if size == 1 {
        let wide = u64::from_be_bytes(
            bytes
                .get(8..16)
                .ok_or_else(|| invalid("JUMBF长长度截断"))?
                .try_into()
                .map_err(|_| invalid("JUMBF长长度截断"))?,
        );
        (
            usize::try_from(wide).map_err(|_| invalid("JUMBF长度溢出"))?,
            16,
        )
    } else {
        (size as usize, 8)
    };
    if size < header || size > 1024 * 1024 {
        return Err(invalid("JUMBF盒长度无效"));
    }
    Ok((size, header))
}

fn validate_boxes(
    bytes: &[u8],
    depth: usize,
    count: &mut usize,
    root: bool,
) -> Result<(), JpegError> {
    if depth > 16 {
        return Err(JpegError::ResourceLimit("JUMBF嵌套深度"));
    }
    let (size, header) = box_header(bytes)?;
    if size != bytes.len() || bytes.get(4..8) != Some(b"jumb") {
        return Err(invalid("JUMBF容器无效"));
    }
    let mut at = header;
    let mut first = true;
    let mut children = 0;
    while at < bytes.len() {
        *count += 1;
        if *count > 4096 {
            return Err(JpegError::ResourceLimit("JUMBF盒数量"));
        }
        let (length, prefix) = box_header(&bytes[at..])?;
        let child = bytes
            .get(at..at + length)
            .ok_or_else(|| invalid("JUMBF子盒越界"))?;
        let kind = &child[4..8];
        if first {
            if kind != b"jumd" {
                return Err(invalid("JUMBF缺少描述盒"));
            }
            let description = &child[prefix..];
            if description.len() < 17 {
                return Err(invalid("JUMBF描述截断"));
            }
            if root {
                // C2PA根须可请求且带标签；扩展描述语义未实现时保守拒绝。
                if &description[..16] != C2PA_UUID
                    || description[16] != 3
                    || &description[17..] != b"c2pa\0"
                {
                    return Err(protected(0xeb));
                }
            }
        } else if kind == b"jumd" {
            return Err(invalid("JUMBF重复描述"));
        }
        if kind == b"jumb" {
            validate_boxes(child, depth + 1, count, false)?;
            children += 1;
        }
        at += length;
        first = false;
    }
    if first || (root && children == 0) {
        return Err(invalid("JUMBF缺少清单"));
    }
    Ok(())
}

pub(super) fn inspect(
    metadata: &[(u8, &[u8])],
    ranges: &[Range<usize>],
) -> Result<Vec<Range<usize>>, JpegError> {
    let mut parts = metadata
        .iter()
        .zip(ranges)
        .filter(|((marker, _), _)| *marker == 0xeb);
    let Some(((_, first), first_range)) = parts.next() else {
        return Ok(Vec::new());
    };
    if !first.starts_with(b"JP") || first.len() < 16 {
        return Err(protected(0xeb));
    }
    if u32_at(first, 4)? != 1 {
        return Err(invalid("JUMBF分片起点无效"));
    }
    let (length, header) = box_header(&first[8..])?;
    let instance = &first[2..4];
    let repeated_header = first
        .get(8..8 + header)
        .ok_or_else(|| invalid("JUMBF头截断"))?;
    if repeated_header.get(4..8) != Some(b"jumb") {
        return Err(protected(0xeb));
    }
    let mut store = first[8..].to_vec();
    if store.len() > length {
        return Err(invalid("JUMBF分片超长"));
    }
    let mut result = vec![first_range.clone()];
    let mut last_end = first_range.end;
    for (sequence, ((_, part), range)) in (2..).zip(parts) {
        if range.start != last_end
            || !part.starts_with(b"JP")
            || part.get(2..4) != Some(instance)
            || u32_at(part, 4)? != sequence
            || part.get(8..8 + header) != Some(repeated_header)
        {
            return Err(protected(0xeb));
        }
        let payload = part
            .get(8 + header..)
            .ok_or_else(|| invalid("JUMBF分片截断"))?;
        if payload.is_empty() || store.len() + payload.len() > length {
            return Err(invalid("JUMBF分片长度无效"));
        }
        store.extend_from_slice(payload);
        result.push(range.clone());
        last_end = range.end;
    }
    if store.len() != length {
        return Err(invalid("JUMBF分片不完整"));
    }
    validate_boxes(&store, 0, &mut 0, true)?;
    Ok(result)
}
