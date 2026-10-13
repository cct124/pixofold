//! 双输入比较的流式合成器：仅保留当前/恢复画布，逐帧复用可取消LZW解码。
use super::super::{Work, playback::indices, structure::*};
use std::{io::Cursor, num::NonZeroU64};

type Reader<'a> = gif::Decoder<Cursor<&'a [u8]>>;
fn reader(bytes: &[u8], limits: Limits) -> Result<Reader<'_>> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::Indexed);
    options.check_frame_consistency(true);
    options.check_lzw_end_code(true);
    options.skip_frame_decoding(true);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        NonZeroU64::new(limits.max_frame_pixels as u64 * 4).ok_or(LabError {
            code: Code::ResourceLimit,
            reason: "实验零额度",
        })?,
    ));
    options.read_info(Cursor::new(bytes)).map_err(|_| LabError {
        code: Code::Decode,
        reason: "实验独立解码头",
    })
}

pub(super) struct Frames<'a> {
    bytes: &'a [u8],
    structure: &'a Structure,
    limits: Limits,
    decoder: Reader<'a>,
    global: Vec<u8>,
    background: [u8; 4],
    pub canvas: Vec<u8>,
    restore: Option<Vec<u8>>,
    previous: Option<usize>,
    next: usize,
}
impl<'a> Frames<'a> {
    pub fn new(
        bytes: &'a [u8],
        structure: &'a Structure,
        limits: Limits,
        logical: bool,
    ) -> Result<Self> {
        let decoder = reader(bytes, limits)?;
        let global = decoder.global_palette().unwrap_or_default().to_vec();
        let background = if logical {
            structure.background
        } else {
            [0; 4]
        };
        let pixels = usize::from(structure.width) * usize::from(structure.height);
        Ok(Self {
            bytes,
            structure,
            limits,
            decoder,
            global,
            background,
            canvas: background.into_iter().cycle().take(pixels * 4).collect(),
            restore: None,
            previous: None,
            next: 0,
        })
    }
    pub fn next(&mut self, work: &Work<'_>) -> Result<u64> {
        work.check()?;
        if let Some(index) = self.previous.take() {
            let last = &self.structure.frames[index];
            match last.control.dispose {
                2 => {
                    for y in usize::from(last.top)..usize::from(last.top + last.height) {
                        work.check()?;
                        for x in usize::from(last.left)..usize::from(last.left + last.width) {
                            let at = (y * usize::from(self.structure.width) + x) * 4;
                            self.canvas[at..at + 4].copy_from_slice(&self.background);
                        }
                    }
                }
                3 => {
                    self.canvas = self.restore.take().ok_or(LabError {
                        code: Code::Decode,
                        reason: "实验恢复画布缺失",
                    })?
                }
                _ => {}
            }
        }
        if self.next == self.structure.frames.len() {
            check(
                self.decoder
                    .read_next_frame()
                    .map_err(|_| LabError {
                        code: Code::Decode,
                        reason: "实验尾部解码",
                    })?
                    .is_none(),
                Code::Decode,
                "实验额外帧",
            )?;
            self.decoder = reader(self.bytes, self.limits)?;
            self.next = 0;
        }
        let expected = &self.structure.frames[self.next];
        let frame = self
            .decoder
            .read_next_frame()
            .map_err(|_| LabError {
                code: Code::Decode,
                reason: "实验帧读取",
            })?
            .ok_or(LabError {
                code: Code::Decode,
                reason: "实验帧缺失",
            })?;
        check(
            frame.width == expected.width
                && frame.height == expected.height
                && frame.left == expected.left
                && frame.top == expected.top
                && frame.delay == expected.control.delay
                && frame.transparent == expected.control.transparent
                && (frame.dispose as u8).max(1) == expected.control.dispose.max(1),
            Code::Decode,
            "实验独立帧头不符",
        )?;
        let palette = frame.palette.as_deref().unwrap_or(&self.global);
        let decoded = indices(frame, work)?;
        let mut rows = Vec::with_capacity(usize::from(frame.height));
        if frame.interlaced {
            for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                rows.extend((start..usize::from(frame.height)).step_by(step));
            }
        } else {
            rows.extend(0..usize::from(frame.height));
        }
        if expected.control.dispose == 3 {
            self.restore = Some(self.canvas.clone());
        }
        for (index, color) in decoded.into_iter().enumerate() {
            if index % 4096 == 0 {
                work.check()?;
            }
            check(
                usize::from(color) < palette.len() / 3,
                Code::Decode,
                "实验调色板索引",
            )?;
            if frame.transparent == Some(color) {
                continue;
            }
            let x = usize::from(frame.left) + index % usize::from(frame.width);
            let y = usize::from(frame.top) + rows[index / usize::from(frame.width)];
            let at = (y * usize::from(self.structure.width) + x) * 4;
            let rgb = usize::from(color) * 3;
            self.canvas[at..at + 3].copy_from_slice(&palette[rgb..rgb + 3]);
            self.canvas[at + 3] = 255;
        }
        self.previous = Some(self.next);
        self.next += 1;
        Ok(u64::from(expected.control.delay).max(1))
    }
}
