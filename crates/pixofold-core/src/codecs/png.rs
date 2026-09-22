//! 严格 PNG 无损基线：只优化压缩表示，不降低位深、变更色型或隐藏 RGB。

use std::time::Duration;

use crate::{
    model::{ProcessingError, ResourceLimits},
    probe::{self, DecodedPng},
};

pub(crate) fn optimize(data: &[u8], limits: ResourceLimits) -> Result<Vec<u8>, ProcessingError> {
    let original = probe::chunks(data)?;
    for chunk in &original {
        // 未知 unsafe-to-copy chunk 可能引用文件偏移/签名，不能在改写 IDAT 后原样搬运。
        if chunk.name[0] & 32 != 0
            && chunk.name[3] & 32 == 0
            && !matches!(
                &chunk.name,
                b"cHRM"
                    | b"gAMA"
                    | b"iCCP"
                    | b"sBIT"
                    | b"sRGB"
                    | b"cICP"
                    | b"mDCV"
                    | b"cLLI"
                    | b"tRNS"
                    | b"bKGD"
                    | b"hIST"
                    | b"pHYs"
                    | b"sPLT"
                    | b"tIME"
            )
        {
            return Err(ProcessingError::ValidationFailed(
                "含不支持改写的元数据 chunk",
            ));
        }
    }
    let options = oxipng::Options {
        fix_errors: false,
        force: false,
        interlace: None,
        optimize_alpha: false,
        bit_depth_reduction: false,
        color_type_reduction: false,
        palette_reduction: false,
        grayscale_reduction: false,
        scale_16: false,
        strip: oxipng::StripChunks::None,
        // 这是停止尝试更多过滤策略的软期限，不承诺即时中断 libdeflate。
        timeout: Some(Duration::from_secs(30)),
        // Adam7 的扫描行/过滤字节可能超过 packed 像素大小，仍受独立上限约束。
        max_decompressed_size: Some(limits.max_decoded_bytes.0 as usize),
        ..oxipng::Options::from_preset(1)
    };
    let optimized = oxipng::optimize_from_memory(data, &options)
        .map_err(|error| ProcessingError::Encode(Box::new(error)))?;
    let encoded = probe::chunks(&optimized)?;
    if encoded[0] != original[0] {
        return Err(ProcessingError::ValidationFailed("编码器改变了 IHDR"));
    }
    // oxipng 即使禁用 reduction 也会规范化 tRNS 长度和 chunk 顺序。
    // 仅采用其 IDAT，保留原始 IHDR/调色板/元数据；随后对实际产物完整解码验证。
    let mut output = Vec::with_capacity(data.len());
    output.extend_from_slice(&data[..8]);
    let mut wrote_idat = false;
    for chunk in original {
        if chunk.name == *b"IDAT" {
            if !wrote_idat {
                for idat in encoded.iter().filter(|chunk| chunk.name == *b"IDAT") {
                    output.extend_from_slice(idat.encoded);
                }
                wrote_idat = true;
            }
        } else {
            output.extend_from_slice(chunk.encoded);
        }
    }
    // 保留原始元数据后的收益由输出层统一裁决，不能沿用编码器自己的比较结果。
    Ok(output)
}

pub(crate) fn validate(
    original: &[u8],
    decoded: &DecodedPng,
    candidate: &[u8],
    limits: ResourceLimits,
) -> Result<(), ProcessingError> {
    let output = probe::decode(candidate, limits)?;
    if output.info != decoded.info || output.pixels != decoded.pixels {
        return Err(ProcessingError::ValidationFailed(
            "像素、位深或色型发生变化",
        ));
    }
    let metadata = |data| {
        probe::chunks(data).map(|chunks| {
            chunks
                .into_iter()
                .filter(|chunk| chunk.name != *b"IDAT")
                .collect::<Vec<_>>()
        })
    };
    // 保守地要求全部非 IDAT chunk 顺序和字节不变（含 PLTE/tRNS/ICC/EXIF）。
    // 遇到编码器会改写的特殊元数据，拒绝产物而非静默丢弃。
    if metadata(original)? != metadata(candidate)? {
        return Err(ProcessingError::ValidationFailed("非 IDAT chunk 发生变化"));
    }
    Ok(())
}
