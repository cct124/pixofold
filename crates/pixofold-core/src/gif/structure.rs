//! GIF完整结构预检：约束尺寸、帧数/累计解码和扩展，再交给独立LZW解码器。
//! 只接受明确的静态/动画子集；浏览器延时钳制和交互等待不用于掩盖语义变化。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Code {
    InvalidGif,
    Decode,
    ResourceLimit,
    UnsupportedMetadata,
    UnsupportedInteraction,
    UnsupportedTiming,
    TimedOut,
    Cancelled,
}
#[derive(Debug, Clone)]
pub struct LabError {
    pub code: Code,
    pub reason: &'static str,
}
impl std::fmt::Display for LabError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.code, self.reason)
    }
}
impl std::error::Error for LabError {}
pub type Result<T> = std::result::Result<T, LabError>;
pub fn check(condition: bool, code: Code, reason: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(LabError { code, reason })
    }
}

/// GIF验证子集；decoded按RGBA计，覆盖双背景及循环代表轮次，限额只能收紧。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_input_bytes: usize,
    pub max_canvas_pixels: usize,
    pub max_frame_pixels: usize,
    pub max_frames: usize,
    pub max_total_decoded_bytes: usize,
    pub max_metadata_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024 * 1024,
            max_canvas_pixels: 4 * 1024 * 1024,
            max_frame_pixels: 4 * 1024 * 1024,
            max_frames: 256,
            max_total_decoded_bytes: 64 * 1024 * 1024,
            max_metadata_bytes: 64 * 1024,
        }
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub struct Control {
    pub delay: u16,
    pub dispose: u8,
    pub transparent: Option<u8>,
}
#[derive(Debug, Clone)]
pub struct Frame {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub control: Control,
}
pub struct Structure {
    pub width: u16,
    pub height: u16,
    pub background: [u8; 4],
    pub loop_count: Option<u16>,
    pub comments: Vec<Vec<u8>>,
    pub frames: Vec<Frame>,
    pub decoded_bytes: usize,
}
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
    blocks: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self.at.checked_add(length).ok_or(LabError {
            code: Code::InvalidGif,
            reason: "区间溢出",
        })?;
        let result = self.bytes.get(self.at..end).ok_or(LabError {
            code: Code::InvalidGif,
            reason: "数据截断",
        })?;
        self.at = end;
        Ok(result)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u16> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }
    fn subblocks(&mut self, maximum: usize, keep: bool) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        let mut total = 0usize;
        loop {
            self.blocks += 1;
            check(self.blocks <= 65536, Code::ResourceLimit, "GIF子块数量")?;
            let length = usize::from(self.byte()?);
            if length == 0 {
                break;
            }
            total = total.checked_add(length).ok_or(LabError {
                code: Code::ResourceLimit,
                reason: "子块长度溢出",
            })?;
            check(total <= maximum, Code::ResourceLimit, "GIF子块字节")?;
            let block = self.take(length)?;
            if keep {
                bytes.extend_from_slice(block);
            }
        }
        Ok(bytes)
    }
}
pub(super) fn parse(bytes: &[u8], limits: Limits, work: &super::Work<'_>) -> Result<Structure> {
    work.check()?;
    let maximum = Limits::default();
    check(
        limits.max_input_bytes > 0
            && limits.max_input_bytes <= maximum.max_input_bytes
            && limits.max_canvas_pixels > 0
            && limits.max_canvas_pixels <= maximum.max_canvas_pixels
            && limits.max_frame_pixels > 0
            && limits.max_frame_pixels <= maximum.max_frame_pixels
            && limits.max_frames > 0
            && limits.max_frames <= maximum.max_frames
            && limits.max_total_decoded_bytes > 0
            && limits.max_total_decoded_bytes <= maximum.max_total_decoded_bytes
            && limits.max_metadata_bytes > 0
            && limits.max_metadata_bytes <= maximum.max_metadata_bytes,
        Code::ResourceLimit,
        "GIF配置只允许收紧限额",
    )?;
    check(
        bytes.len() <= limits.max_input_bytes,
        Code::ResourceLimit,
        "GIF输入字节",
    )?;
    let mut cursor = Cursor {
        bytes,
        at: 0,
        blocks: 0,
    };
    let header = cursor.take(6)?;
    check(
        header == b"GIF87a" || header == b"GIF89a",
        Code::InvalidGif,
        "GIF标识",
    )?;
    let width = cursor.word()?;
    let height = cursor.word()?;
    check(width > 0 && height > 0, Code::InvalidGif, "零画布")?;
    check(
        usize::from(width) * usize::from(height) <= limits.max_canvas_pixels,
        Code::ResourceLimit,
        "GIF画布像素",
    )?;
    let packed = cursor.byte()?;
    let background_index = cursor.byte()?;
    check(
        cursor.byte()? == 0,
        Code::UnsupportedMetadata,
        "暂不解释像素宽高比",
    )?;
    let global = if packed & 0x80 != 0 {
        cursor.take(3 * (1usize << ((packed & 7) + 1)))?
    } else {
        &[]
    };
    let mut background = [0, 0, 0, 0];
    if !global.is_empty() {
        let color = global
            .get(usize::from(background_index) * 3..usize::from(background_index) * 3 + 3)
            .ok_or(LabError {
                code: Code::InvalidGif,
                reason: "背景调色板索引",
            })?;
        background = [color[0], color[1], color[2], 255];
    }
    let mut structure = Structure {
        width,
        height,
        background,
        loop_count: None,
        comments: Vec::new(),
        frames: Vec::new(),
        decoded_bytes: 0,
    };
    let mut pending = None;
    let mut metadata = 0usize;
    loop {
        work.check()?;
        match cursor.byte()? {
            0x3b => {
                check(
                    cursor.at == bytes.len() && pending.is_none() && !structure.frames.is_empty(),
                    Code::InvalidGif,
                    "尾随数据、无帧或悬空控制块",
                )?;
                if structure.frames.len() > 1 {
                    check(
                        structure.frames.iter().all(|frame| frame.control.delay > 0),
                        Code::UnsupportedTiming,
                        "零延时动画的播放含义不确定",
                    )?;
                }
                // 验证实际解码两种背景；存在循环时每种背景另解第二轮，累计额度覆盖全部工作。
                structure.decoded_bytes = structure
                    .decoded_bytes
                    .checked_mul(if structure.loop_count.is_some() { 4 } else { 2 })
                    .ok_or(LabError {
                        code: Code::ResourceLimit,
                        reason: "重复解码字节溢出",
                    })?;
                check(
                    structure.decoded_bytes <= limits.max_total_decoded_bytes,
                    Code::ResourceLimit,
                    "背景/循环累计解码字节",
                )?;
                return Ok(structure);
            }
            0x21 => {
                let kind = cursor.byte()?;
                match kind {
                    0xf9 => {
                        check(
                            cursor.byte()? == 4 && pending.is_none(),
                            Code::InvalidGif,
                            "控制块长度或重复控制",
                        )?;
                        let flags = cursor.byte()?;
                        check(
                            flags & 0xe0 == 0 && (flags >> 2 & 7) <= 3,
                            Code::InvalidGif,
                            "保留控制位或帧处置",
                        )?;
                        check(flags & 2 == 0, Code::UnsupportedInteraction, "交互等待帧")?;
                        let delay = cursor.word()?;
                        let transparent = cursor.byte()?;
                        check(cursor.byte()? == 0, Code::InvalidGif, "控制块终止符")?;
                        pending = Some(Control {
                            delay,
                            dispose: flags >> 2 & 7,
                            transparent: (flags & 1 != 0).then_some(transparent),
                        });
                        metadata = metadata.checked_add(8).ok_or(LabError {
                            code: Code::ResourceLimit,
                            reason: "元数据计费溢出",
                        })?;
                    }
                    0xfe => {
                        let comment = cursor
                            .subblocks(limits.max_metadata_bytes.saturating_sub(metadata), true)?;
                        metadata += comment.len() + 2;
                        structure.comments.push(comment);
                    }
                    0xff => {
                        let length = usize::from(cursor.byte()?);
                        let id = cursor.take(length)?;
                        check(
                            length == 11 && (id == b"NETSCAPE2.0" || id == b"ANIMEXTS1.0"),
                            Code::UnsupportedMetadata,
                            "未知或保护应用扩展",
                        )?;
                        check(
                            structure.loop_count.is_none() && structure.frames.is_empty(),
                            Code::InvalidGif,
                            "循环声明重复或迟到",
                        )?;
                        let payload = cursor.subblocks(3, true)?;
                        check(
                            payload.len() == 3 && payload[0] == 1,
                            Code::InvalidGif,
                            "循环扩展内容",
                        )?;
                        structure.loop_count = Some(u16::from_le_bytes([payload[1], payload[2]]));
                        metadata += 19;
                    }
                    _ => {
                        return Err(LabError {
                            code: Code::UnsupportedMetadata,
                            reason: "文本绘制或未知扩展",
                        });
                    }
                }
                check(
                    metadata <= limits.max_metadata_bytes,
                    Code::ResourceLimit,
                    "GIF元数据字节",
                )?;
            }
            0x2c => {
                check(
                    structure.frames.len() < limits.max_frames,
                    Code::ResourceLimit,
                    "GIF帧数",
                )?;
                let left = cursor.word()?;
                let top = cursor.word()?;
                let frame_width = cursor.word()?;
                let frame_height = cursor.word()?;
                check(
                    frame_width > 0
                        && frame_height > 0
                        && u32::from(left) + u32::from(frame_width) <= u32::from(width)
                        && u32::from(top) + u32::from(frame_height) <= u32::from(height),
                    Code::InvalidGif,
                    "帧区域越界或零尺寸",
                )?;
                let pixels = usize::from(frame_width) * usize::from(frame_height);
                check(
                    pixels <= limits.max_frame_pixels,
                    Code::ResourceLimit,
                    "单帧像素",
                )?;
                structure.decoded_bytes = structure
                    .decoded_bytes
                    .checked_add(pixels.checked_mul(4).ok_or(LabError {
                        code: Code::ResourceLimit,
                        reason: "帧字节溢出",
                    })?)
                    .ok_or(LabError {
                        code: Code::ResourceLimit,
                        reason: "累计解码溢出",
                    })?;
                check(
                    structure.decoded_bytes <= limits.max_total_decoded_bytes,
                    Code::ResourceLimit,
                    "累计解码字节",
                )?;
                let flags = cursor.byte()?;
                check(flags & 0x18 == 0, Code::InvalidGif, "帧保留位")?;
                let palette = if flags & 0x80 != 0 {
                    cursor.take(3 * (1usize << ((flags & 7) + 1)))?
                } else {
                    global
                };
                check(!palette.is_empty(), Code::InvalidGif, "缺少调色板")?;
                let control = pending.take().unwrap_or_default();
                check(
                    control
                        .transparent
                        .is_none_or(|index| usize::from(index) < palette.len() / 3),
                    Code::InvalidGif,
                    "透明索引越界",
                )?;
                check(
                    (2..=8).contains(&cursor.byte()?),
                    Code::InvalidGif,
                    "LZW最小码宽",
                )?;
                cursor.subblocks(limits.max_input_bytes, false)?;
                structure.frames.push(Frame {
                    left,
                    top,
                    width: frame_width,
                    height: frame_height,
                    control,
                });
            }
            _ => {
                return Err(LabError {
                    code: Code::InvalidGif,
                    reason: "未知数据块",
                });
            }
        }
    }
}
