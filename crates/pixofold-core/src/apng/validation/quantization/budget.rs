//! 多遍解码/全画布比较共用累计额度；原生直方图按显式采样上限估算，无所有帧像素缓存。
use super::*;
pub(super) struct Budget {
    pub bound: u64,
    pub pixels: u64,
    pub decoded_bytes: u64,
    pub palette_searches: u64,
    max_palette_searches: u64,
    limits: Limits,
}
impl Budget {
    #[cfg(test)]
    pub fn new(p: &Parsed<'_>, input: usize, options: Options) -> Result<Self> {
        Self::resume(p,input,options,Usage::default(),0)
    }
    pub fn resume(p:&Parsed<'_>,input:usize,options:Options,usage:Usage,resident:u64)->Result<Self> {
        usage.remaining(options.limits)?;
        require(usage.palette_searches<=options.max_palette_searches,"palette_search_budget")?;
        let canvas =
            u64::from(structure::u32_at(p.ihdr, 0)) * u64::from(structure::u32_at(p.ihdr, 4));
        let largest = p
            .frames
            .iter()
            .map(|f| u64::from(f.control.width) * u64::from(f.control.height))
            .max()
            .unwrap_or(0)
            .max(if p.default_participates { 0 } else { canvas });
        require(
            largest <= options.limits.max_frame_pixels,
            "frame_pixel_budget",
        )?;
        // 画布/previous/时域误差、双解码/量化scratch、压缩候选容量及有界直方图原生余量。
        let bound = canvas * 96
            + largest * 128
            + (input as u64 + options.limits.max_candidate_bytes as u64) * 4
            + options.max_histogram_samples as u64 * 256
            + (33 << 20) + resident; // 另含alpha索引集、直接映射缓存及存活基线/摘要。
        require(bound <= options.max_working_set_bytes, "working_set_budget")?;
        require(
            canvas * 64
                + largest * 48
                + (input as u64 + options.limits.max_candidate_bytes as u64) * 3
                + (p.frames.len() as u64) * 8192
                + (1 << 20)
                + resident
                <= options.limits.max_validation_bytes,
            "validation_budget",
        )?;
        Ok(Self {
            bound,
            pixels: usage.pixels,
            decoded_bytes: usage.decoded_bytes,
            palette_searches: usage.palette_searches,
            max_palette_searches: options.max_palette_searches,
            limits: options.limits,
        })
    }
    pub fn usage(&self)->Usage {Usage {pixels:self.pixels,decoded_bytes:self.decoded_bytes,palette_searches:self.palette_searches}}
    pub fn work(&mut self, pixels: u64) -> Result<()> {
        self.pixels = self
            .pixels
            .checked_add(pixels)
            .ok_or(Error("total_pixel_budget"))?;
        require(
            self.pixels <= self.limits.max_total_pixels,
            "total_pixel_budget",
        )
    }
    pub fn search(&mut self, count: usize) -> Result<()> {
        self.palette_searches = self
            .palette_searches
            .checked_add(count as u64)
            .ok_or(Error("palette_search_budget"))?;
        require(
            self.palette_searches <= self.max_palette_searches,
            "palette_search_budget",
        )
    }
    pub fn read(
        &mut self,
        p: &Parsed<'_>,
        f: &Frame<'_>,
        guard: &Guard<'_>,
    ) -> Result<Vec<[u16; 4]>> {
        let pixels = u64::from(f.control.width) * u64::from(f.control.height);
        self.work(pixels)?;
        // 25 bytes/像素覆盖严格zlib、原始/展开双解码及行滤波；8位输入按16位最坏收紧。
        self.decoded_bytes = self
            .decoded_bytes
            .checked_add(pixels * 25)
            .ok_or(Error("decoded_budget"))?;
        require(
            self.decoded_bytes <= self.limits.max_decoded_bytes,
            "decoded_budget",
        )?;
        Ok(decode::decode(p, f, &self.limits, guard)?.1)
    }
}
