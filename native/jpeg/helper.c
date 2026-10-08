/* PixoFold JPEG字节工具，系数PFJC1/像素PFJP1。固定静态MozJPEG，仅stdin/stdout，不接收路径。
 * 所有警告均失败；退出码不含输入内容。父进程负责管道预算、超时与kill/wait。
 * 原生库max_memory_to_use限制虚拟数组，另预检系数预算；不是RSS硬上限。
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
#include <jpeglib.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

static unsigned int scan_limit;
static void failed(j_common_ptr ignored) { (void)ignored; exit(2); }
static void message(j_common_ptr ignored, int level) {
  (void)ignored;
  if (level < 0) exit(2);
}
static void monitor(j_common_ptr common) {
  j_decompress_ptr decoder = (j_decompress_ptr)common;
  if ((unsigned int)decoder->input_scan_number > scan_limit) exit(3);
}
static uint64_t number(const char *text) {
  uint64_t result = 0;
  if (!*text) exit(4);
  for (; *text; ++text) {
    if (*text < '0' || *text > '9' || result > (UINT64_MAX - 9) / 10) exit(4);
    result = result * 10 + (unsigned int)(*text - '0');
  }
  if (!result) exit(4);
  return result;
}
static void u32(uint32_t value) {
  unsigned char bytes[4];
  for (unsigned int i = 0; i < 4; ++i) bytes[i] = (unsigned char)(value >> (8 * i));
  if (fwrite(bytes, 1, 4, stdout) != 4) exit(5);
}
static void coefficients(struct jpeg_decompress_struct *d, jvirt_barray_ptr *arrays) {
  if (fwrite("PFJC1", 1, 5, stdout) != 5) exit(5);
  u32(d->image_width); u32(d->image_height);
  u32((uint32_t)d->data_precision); u32((uint32_t)d->jpeg_color_space);
  u32((uint32_t)d->num_components);
  for (int c = 0; c < d->num_components; ++c) {
    jpeg_component_info *p = &d->comp_info[c];
    /* 使用各分量实际解码使用的量化表，而不是EOI处可能被重定义的表。 */
    if (!p->quant_table) exit(2);
    u32((uint32_t)p->component_id);
    u32((uint32_t)p->h_samp_factor); u32((uint32_t)p->v_samp_factor);
    u32(p->width_in_blocks); u32(p->height_in_blocks);
    for (int k = 0; k < DCTSIZE2; ++k) u32(p->quant_table->quantval[k]);
    for (JDIMENSION y = 0; y < p->height_in_blocks; ++y) {
      JBLOCKARRAY row = (*d->mem->access_virt_barray)((j_common_ptr)d, arrays[c], y, 1, FALSE);
      for (JDIMENSION x = 0; x < p->width_in_blocks; ++x)
        for (int k = 0; k < DCTSIZE2; ++k) u32((uint32_t)(int32_t)row[0][x][k]);
    }
  }
}

static void read_pixels(struct jpeg_decompress_struct *d) {
  d->out_color_space = d->num_components == 1 ? JCS_GRAYSCALE : JCS_RGB;
  d->dct_method = JDCT_ISLOW;
  if (!jpeg_start_decompress(d)) exit(2);
  if (fwrite("PFJP1", 1, 5, stdout) != 5) exit(5);
  u32(d->output_width); u32(d->output_height);
  u32((uint32_t)d->output_components); u32((uint32_t)d->jpeg_color_space);
  JDIMENSION width = d->output_width * (unsigned int)d->output_components;
  JSAMPARRAY row = (*d->mem->alloc_sarray)((j_common_ptr)d, JPOOL_IMAGE, width, 1);
  while (d->output_scanline < d->output_height) {
    if (jpeg_read_scanlines(d, row, 1) != 1 || fwrite(row[0], 1, width, stdout) != width) exit(5);
  }
}

static void reencode(struct jpeg_decompress_struct *d, struct jpeg_compress_struct *e,
                     unsigned int quality, uint64_t memory) {
  /* 在原颜色空间解码/编码，不把YCbCr绕经RGB，也不改变原采样或Exif方向。 */
  d->out_color_space = d->jpeg_color_space;
  d->dct_method = JDCT_ISLOW;
  if (!jpeg_start_decompress(d)) exit(2);
  jpeg_create_compress(e);
  e->mem->max_memory_to_use = (long)(memory / 2);
  e->image_width = d->output_width; e->image_height = d->output_height;
  e->input_components = d->output_components; e->in_color_space = d->out_color_space;
  jpeg_c_set_int_param(e, JINT_COMPRESS_PROFILE, JCP_FASTEST);
  jpeg_set_defaults(e);
  jpeg_set_colorspace(e, d->jpeg_color_space);
  jpeg_set_quality(e, (int)quality, TRUE);
  e->dct_method = JDCT_ISLOW;
  e->optimize_coding = TRUE;
  for (int c = 0; c < d->num_components; ++c) {
    e->comp_info[c].component_id = d->comp_info[c].component_id;
    e->comp_info[c].h_samp_factor = d->comp_info[c].h_samp_factor;
    e->comp_info[c].v_samp_factor = d->comp_info[c].v_samp_factor;
  }
  e->scan_info = NULL; e->num_scans = 0;
  if (d->progressive_mode) jpeg_simple_progression(e);
  e->write_JFIF_header = FALSE; e->write_Adobe_marker = FALSE;
  jpeg_stdio_dest(e, stdout);
  jpeg_start_compress(e, TRUE);
  for (jpeg_saved_marker_ptr m = d->marker_list; m; m = m->next)
    jpeg_write_marker(e, m->marker, m->data, m->data_length);
  JDIMENSION width = d->output_width * (unsigned int)d->output_components;
  JSAMPARRAY row = (*d->mem->alloc_sarray)((j_common_ptr)d, JPOOL_IMAGE, width, 1);
  while (d->output_scanline < d->output_height) {
    if (jpeg_read_scanlines(d, row, 1) != 1 || jpeg_write_scanlines(e, row, 1) != 1) exit(2);
  }
  jpeg_finish_compress(e);
  jpeg_destroy_compress(e);
}

int main(int argc, char **argv) {
  if (argc != 6 && argc != 7) return 4;
  int optimize = strcmp(argv[1], "optimize") == 0;
  int lossy = strcmp(argv[1], "lossy") == 0;
  int pixels_out = strcmp(argv[1], "pixels") == 0;
  if (!optimize && !lossy && !pixels_out && strcmp(argv[1], "coefficients") != 0) return 4;
  if ((lossy && argc != 7) || (!lossy && argc != 6)) return 4;
  uint64_t quality = lossy ? number(argv[6]) : 1;
  if (quality > 100) return 4;
  uint64_t dimension = number(argv[2]), pixels = number(argv[3]);
  uint64_t memory = number(argv[4]), scans = number(argv[5]);
  if (dimension > 65535 || pixels > 16777216 || memory > 536870912 ||
      memory < 1048576 || memory / 2 > LONG_MAX || scans > 64) return 4;
  scan_limit = (unsigned int)scans;
#ifdef _WIN32
  if (_setmode(_fileno(stdin), _O_BINARY) == -1 ||
      _setmode(_fileno(stdout), _O_BINARY) == -1) return 5;
#endif
  struct jpeg_decompress_struct d;
  struct jpeg_compress_struct e;
  struct jpeg_error_mgr de, ee;
  struct jpeg_progress_mgr progress;
  memset(&d, 0, sizeof(d)); memset(&e, 0, sizeof(e));
  memset(&progress, 0, sizeof(progress));
  d.err = jpeg_std_error(&de); de.error_exit = failed; de.emit_message = message;
  jpeg_create_decompress(&d);
  d.mem->max_memory_to_use = (long)(memory / 2);
  progress.progress_monitor = monitor; d.progress = &progress;
  jpeg_stdio_src(&d, stdin);
  for (int m = 0; m < 16; ++m) jpeg_save_markers(&d, JPEG_APP0 + m, 0xffff);
  jpeg_save_markers(&d, JPEG_COM, 0xffff);
  if (jpeg_read_header(&d, TRUE) != JPEG_HEADER_OK || d.data_precision != 8 ||
      !(d.num_components == 1 || d.num_components == 3 || d.num_components == 4) ||
      d.arith_code) return 2;
  if (!d.image_width || !d.image_height || d.image_width > dimension ||
      d.image_height > dimension || (uint64_t)d.image_width * d.image_height > pixels) return 3;
  uint64_t blocks = 0, mh = 1, mv = 1;
  for (int c = 0; c < d.num_components; ++c) {
    jpeg_component_info *p = &d.comp_info[c];
    if (p->h_samp_factor < 1 || p->h_samp_factor > 4 ||
        p->v_samp_factor < 1 || p->v_samp_factor > 4) return 2;
    if ((uint64_t)p->h_samp_factor > mh) mh = (uint64_t)p->h_samp_factor;
    if ((uint64_t)p->v_samp_factor > mv) mv = (uint64_t)p->v_samp_factor;
  }
  for (int c = 0; c < d.num_components; ++c) {
    jpeg_component_info *p = &d.comp_info[c];
    uint64_t w = ((uint64_t)d.image_width + mh * 8 - 1) / (mh * 8) * (unsigned int)p->h_samp_factor;
    uint64_t h = ((uint64_t)d.image_height + mv * 8 - 1) / (mv * 8) * (unsigned int)p->v_samp_factor;
    blocks += w * h;
  }
  if (blocks * DCTSIZE2 * sizeof(JCOEF) * 2 + 1048576 > memory) return 3;
  if (lossy || pixels_out) {
    if (d.jpeg_color_space != JCS_GRAYSCALE && d.jpeg_color_space != JCS_RGB &&
        d.jpeg_color_space != JCS_YCbCr) return 2;
    uint64_t raw = (uint64_t)d.image_width * d.image_height * (unsigned int)d.num_components;
    if (blocks * DCTSIZE2 * sizeof(JCOEF) * 2 + raw * 2 + 1048576 > memory) return 3;
    memory -= raw * 2;
    d.mem->max_memory_to_use = (long)(memory / 2);
    if (lossy) {
      e.err = jpeg_std_error(&ee); ee.error_exit = failed; ee.emit_message = message;
      reencode(&d, &e, (unsigned int)quality, memory);
    } else read_pixels(&d);
    if (!jpeg_finish_decompress(&d)) return 2;
    jpeg_destroy_decompress(&d);
    return fflush(stdout) == 0 ? 0 : 5;
  }
  jvirt_barray_ptr *arrays = jpeg_read_coefficients(&d);
  if (!arrays) return 2;
  if (optimize) {
    e.err = jpeg_std_error(&ee); ee.error_exit = failed; ee.emit_message = message;
    jpeg_create_compress(&e);
    e.mem->max_memory_to_use = (long)(memory / 2);
    jpeg_stdio_dest(&e, stdout);
    jpeg_copy_critical_parameters(&d, &e);
    /* Huffman优化，不量化、不旋转、不去元数据；固定单进程，不再选择原输入回退。 */
    jpeg_c_set_int_param(&e, JINT_COMPRESS_PROFILE, JCP_FASTEST);
    e.optimize_coding = TRUE;
    e.scan_info = NULL; e.num_scans = 0;
    if (d.progressive_mode) jpeg_simple_progression(&e);
    e.write_JFIF_header = FALSE; e.write_Adobe_marker = FALSE;
    jpeg_write_coefficients(&e, arrays);
    for (jpeg_saved_marker_ptr m = d.marker_list; m; m = m->next)
      jpeg_write_marker(&e, m->marker, m->data, m->data_length);
    jpeg_finish_compress(&e);
    jpeg_destroy_compress(&e);
  } else coefficients(&d, arrays);
  if (!jpeg_finish_decompress(&d)) return 2;
  jpeg_destroy_decompress(&d);
  return fflush(stdout) == 0 ? 0 : 5;
}
