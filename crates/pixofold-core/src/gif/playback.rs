//! 通过独立gif/weezl解码索引像素，验证合成后的画面、精确厘秒时间轴及循环接缝。
//! 同时比较透明画布和逻辑背景两种解释；只存当前/恢复画布和有界时间轴摘要。

use super::Work;
use super::structure::{Code, LabError, Limits, Result, Structure, check, parse};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Cursor, num::NonZeroU64};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub delay_cs: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playback {
    pub width: u16,
    pub height: u16,
    pub frame_count: usize,
    pub decoded_bytes: usize,
    pub loop_count: Option<u16>,
    pub comment_sha256: Vec<String>,
    pub transparent: Vec<Vec<Event>>,
    pub logical: Vec<Vec<Event>>,
}
pub fn normalize(events: &[Event]) -> Result<Vec<Event>> {
    // 相同画面累加整数厘秒；超出u64时明确拒绝，保持时间轴不变量。
    let mut result: Vec<Event> = Vec::new();
    for event in events {
        if let Some(previous) = result
            .last_mut()
            .filter(|previous| previous.sha256 == event.sha256)
        {
            previous.delay_cs = previous
                .delay_cs
                .checked_add(event.delay_cs)
                .ok_or(LabError {
                    code: Code::ResourceLimit,
                    reason: "时间轴溢出",
                })?;
        } else {
            result.push(event.clone());
        }
    }
    Ok(result)
}
impl Playback {
    /// 比较完整双背景时间轴、循环语义与评论哈希；允许等价帧合并，不比较编码字节。
    pub fn equivalent(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.loop_count == other.loop_count
            && self.comment_sha256 == other.comment_sha256
            && self.transparent == other.transparent
            && self.logical == other.logical
    }
}
fn indices(frame: &gif::Frame<'_>, work: &Work<'_>) -> Result<Vec<u8>> {
    let count = usize::from(frame.width) * usize::from(frame.height);
    let (&minimum, encoded) = frame.buffer.split_first().ok_or(LabError {
        code: Code::Decode,
        reason: "缺少LZW码宽",
    })?;
    let mut decoder = weezl::decode::Decoder::new(weezl::BitOrder::Lsb, minimum);
    // 多留一个索引，既能处理跨行字典码，又能在像素满后继续确认EOI并发现超额像素。
    let mut pixels = vec![0; count + 1];
    let (mut input, mut output) = (0, 0);
    loop {
        work.check()?;
        let end = (output + 16 * 1024).min(pixels.len());
        let result = decoder.decode_bytes(&encoded[input..], &mut pixels[output..end]);
        result.status.map_err(|_| LabError {
            code: Code::Decode,
            reason: "LZW码流无效",
        })?;
        input += result.consumed_in;
        output += result.consumed_out;
        check(output <= count, Code::Decode, "LZW像素超额")?;
        if decoder.has_ended() {
            break;
        }
        check(
            result.consumed_in != 0 || result.consumed_out != 0,
            Code::Decode,
            "缺少LZW结束码",
        )?;
    }
    check(output == count, Code::Decode, "LZW像素截断")?;
    pixels.truncate(count);
    Ok(pixels)
}
fn render(
    bytes: &[u8],
    structure: &Structure,
    limits: Limits,
    background: [u8; 4],
    work: &Work<'_>,
) -> Result<Vec<Vec<Event>>> {
    let pixels = usize::from(structure.width) * usize::from(structure.height);
    let mut canvas: Vec<u8> = background.into_iter().cycle().take(pixels * 4).collect();
    let mut cycles = Vec::new();
    for _ in 0..if structure.loop_count.is_some() { 2 } else { 1 } {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::Indexed);
        options.check_frame_consistency(true);
        options.check_lzw_end_code(true);
        // 原始LZW由同一独立weezl全帧有界解码，避免逐行输出接口把NoProgress误报缺少EOI。
        options.skip_frame_decoding(true);
        options.set_memory_limit(gif::MemoryLimit::Bytes(
            NonZeroU64::new(limits.max_frame_pixels as u64 * 4).ok_or(LabError {
                code: Code::ResourceLimit,
                reason: "零解码限额",
            })?,
        ));
        let mut decoder = options
            .read_info(Cursor::new(bytes))
            .map_err(|_| LabError {
                code: Code::Decode,
                reason: "独立解码头失败",
            })?;
        let global = decoder
            .global_palette()
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        let mut events = Vec::new();
        for expected in &structure.frames {
            work.check()?;
            let frame = decoder
                .read_next_frame()
                .map_err(|_| LabError {
                    code: Code::Decode,
                    reason: "独立帧读取失败",
                })?
                .ok_or(LabError {
                    code: Code::Decode,
                    reason: "缺少帧",
                })?;
            // 未指定dispose=0与Keep=1在此子集都保留画布；解码库默认帧使用Keep。
            check(
                frame.width == expected.width
                    && frame.height == expected.height
                    && frame.left == expected.left
                    && frame.top == expected.top
                    && frame.delay == expected.control.delay
                    && frame.transparent == expected.control.transparent
                    && (frame.dispose as u8).max(1) == expected.control.dispose.max(1),
                Code::Decode,
                "独立帧头不一致",
            )?;
            let palette = frame.palette.as_deref().unwrap_or(&global);
            let count = usize::from(frame.width) * usize::from(frame.height);
            let decoded = indices(frame, work)?;
            let mut rows = Vec::with_capacity(usize::from(frame.height));
            if frame.interlaced {
                for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                    rows.extend((start..usize::from(frame.height)).step_by(step));
                }
            } else {
                rows.extend(0..usize::from(frame.height));
            }
            check(decoded.len() == count, Code::Decode, "索引像素长度")?;
            let restore = (expected.control.dispose == 3).then(|| canvas.clone());
            for (index, color_index) in decoded.iter().copied().enumerate() {
                if index % 4096 == 0 {
                    work.check()?;
                }
                check(
                    usize::from(color_index) < palette.len() / 3,
                    Code::Decode,
                    "解码像素调色板索引",
                )?;
                if frame.transparent == Some(color_index) {
                    continue;
                }
                let x = usize::from(frame.left) + index % usize::from(frame.width);
                let y = usize::from(frame.top) + rows[index / usize::from(frame.width)];
                let destination = (y * usize::from(structure.width) + x) * 4;
                let source = usize::from(color_index) * 3;
                canvas[destination..destination + 3].copy_from_slice(&palette[source..source + 3]);
                canvas[destination + 3] = 255;
            }
            let mut hash = Sha256::new();
            for chunk in canvas.chunks(64 * 1024) {
                work.check()?;
                hash.update(chunk);
            }
            events.push(Event {
                delay_cs: u64::from(frame.delay),
                sha256: format!("{:x}", hash.finalize()),
            });
            match expected.control.dispose {
                2 => {
                    for y in
                        usize::from(frame.top)..usize::from(frame.top) + usize::from(frame.height)
                    {
                        work.check()?;
                        for x in usize::from(frame.left)
                            ..usize::from(frame.left) + usize::from(frame.width)
                        {
                            let at = (y * usize::from(structure.width) + x) * 4;
                            canvas[at..at + 4].copy_from_slice(&background);
                        }
                    }
                }
                3 => {
                    canvas = restore.ok_or(LabError {
                        code: Code::Decode,
                        reason: "恢复画布缺失",
                    })?
                }
                _ => {}
            }
        }
        check(
            decoder
                .read_next_frame()
                .map_err(|_| LabError {
                    code: Code::Decode,
                    reason: "独立解码尾部失败",
                })?
                .is_none(),
            Code::Decode,
            "存在额外帧",
        )?;
        cycles.push(normalize(&events)?);
    }
    Ok(cycles)
}
pub(super) fn inspect(
    bytes: &[u8],
    limits: Limits,
    maximum_working: u64,
    maximum_dimension: u32,
    work: &Work<'_>,
) -> Result<Playback> {
    let structure = parse(bytes, limits, work)?;
    check(
        u32::from(structure.width.max(structure.height)) <= maximum_dimension,
        Code::ResourceLimit,
        "GIF单边尺寸",
    )?;
    let canvas = u64::from(structure.width) * u64::from(structure.height);
    let largest_frame = structure
        .frames
        .iter()
        .map(|frame| u64::from(frame.width) * u64::from(frame.height))
        .max()
        .unwrap_or(0);
    // 双画布、单帧索引和gif压缩Vec最多两份输入容量；字典、行表、头/摘要另留1MiB。
    check(
        canvas * 8 + largest_frame + bytes.len() as u64 * 2 + 1024 * 1024 <= maximum_working,
        Code::ResourceLimit,
        "GIF验证缓冲工作集",
    )?;
    Ok(Playback {
        width: structure.width,
        height: structure.height,
        frame_count: structure.frames.len(),
        decoded_bytes: structure.decoded_bytes,
        loop_count: structure.loop_count,
        comment_sha256: structure
            .comments
            .iter()
            .map(|comment| format!("{:x}", Sha256::digest(comment)))
            .collect(),
        transparent: render(bytes, &structure, limits, [0, 0, 0, 0], work)?,
        logical: render(bytes, &structure, limits, structure.background, work)?,
    })
}
