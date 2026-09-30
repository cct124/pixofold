/* JPEG实验的系数验证与CMYK语料生成；只处理脚本生成的隔离输入。
 * 系数按固定字节序输出供逐字节比较，不把编码文件字节当作无损不变量。
 * 错误在独立进程内终结；该工具不是产品的解码器或FFI接口。
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <jpeglib.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

#define LAB_MAX_PIXELS 8388608ULL
#define LAB_MAX_SCANS 64
#define LAB_MEMORY_BYTES (32L * 1024L * 1024L)

static void fail(const char *message) {
  fprintf(stderr, "%s\n", message);
  exit(2);
}

static void write_u32(uint32_t value) {
  unsigned char bytes[4];
  for (unsigned int i = 0; i < 4; i++) bytes[i] = (unsigned char)(value >> (i * 8));
  if (fwrite(bytes, 1, sizeof(bytes), stdout) != sizeof(bytes)) fail("coefficient write failed");
}

static void monitor(j_common_ptr common) {
  j_decompress_ptr decoder = (j_decompress_ptr)common;
  if (decoder->input_scan_number > LAB_MAX_SCANS) fail("scan limit exceeded");
}

static void coefficients(void) {
  struct jpeg_decompress_struct decoder;
  struct jpeg_error_mgr error;
  struct jpeg_progress_mgr progress;
  memset(&decoder, 0, sizeof(decoder));
  memset(&progress, 0, sizeof(progress));
  decoder.err = jpeg_std_error(&error);
  jpeg_create_decompress(&decoder);
  decoder.mem->max_memory_to_use = LAB_MEMORY_BYTES;
  progress.progress_monitor = monitor;
  decoder.progress = &progress;
  jpeg_stdio_src(&decoder, stdin);
  jpeg_read_header(&decoder, TRUE);
  if (!decoder.image_width || !decoder.image_height ||
      decoder.image_width > 16384 || decoder.image_height > 16384 ||
      (uint64_t)decoder.image_width * decoder.image_height > LAB_MAX_PIXELS)
    fail("dimension limit exceeded");
  jvirt_barray_ptr *arrays = jpeg_read_coefficients(&decoder);
  write_u32(decoder.image_width);
  write_u32(decoder.image_height);
  write_u32((uint32_t)decoder.data_precision);
  write_u32((uint32_t)decoder.jpeg_color_space);
  write_u32((uint32_t)decoder.num_components);
  for (int t = 0; t < NUM_QUANT_TBLS; t++) {
    JQUANT_TBL *table = decoder.quant_tbl_ptrs[t];
    write_u32(table != NULL);
    if (table) for (int k = 0; k < DCTSIZE2; k++) write_u32(table->quantval[k]);
  }
  for (int c = 0; c < decoder.num_components; c++) {
    jpeg_component_info *component = &decoder.comp_info[c];
    write_u32((uint32_t)component->component_id);
    write_u32((uint32_t)component->h_samp_factor);
    write_u32((uint32_t)component->v_samp_factor);
    write_u32((uint32_t)component->quant_tbl_no);
    write_u32(component->width_in_blocks);
    write_u32(component->height_in_blocks);
    for (JDIMENSION y = 0; y < component->height_in_blocks; y++) {
      JBLOCKARRAY row = (*decoder.mem->access_virt_barray)(
          (j_common_ptr)&decoder, arrays[c], y, 1, FALSE);
      for (JDIMENSION x = 0; x < component->width_in_blocks; x++)
        for (int k = 0; k < DCTSIZE2; k++) write_u32((uint32_t)(int32_t)row[0][x][k]);
    }
  }
  jpeg_finish_decompress(&decoder);
  if (error.num_warnings != 0) fail("JPEG warning rejected");
  jpeg_destroy_decompress(&decoder);
}

static void make_cmyk(int ycck) {
  struct jpeg_compress_struct encoder;
  struct jpeg_error_mgr error;
  encoder.err = jpeg_std_error(&error);
  jpeg_create_compress(&encoder);
  encoder.mem->max_memory_to_use = LAB_MEMORY_BYTES;
  jpeg_stdio_dest(&encoder, stdout);
  encoder.image_width = 192;
  encoder.image_height = 128;
  encoder.input_components = 4;
  encoder.in_color_space = JCS_CMYK;
  jpeg_set_defaults(&encoder);
  if (ycck) jpeg_set_colorspace(&encoder, JCS_YCCK);
  jpeg_set_quality(&encoder, 90, TRUE);
  jpeg_start_compress(&encoder, TRUE);
  unsigned char row[192 * 4];
  while (encoder.next_scanline < encoder.image_height) {
    for (unsigned int x = 0; x < 192; x++) {
      row[x * 4] = (unsigned char)x;
      row[x * 4 + 1] = (unsigned char)(encoder.next_scanline * 2);
      row[x * 4 + 2] = (unsigned char)((x + encoder.next_scanline) % 256);
      row[x * 4 + 3] = 32;
    }
    JSAMPROW rows[1] = { row };
    jpeg_write_scanlines(&encoder, rows, 1);
  }
  jpeg_finish_compress(&encoder);
  jpeg_destroy_compress(&encoder);
}

int main(int argc, char **argv) {
#ifdef _WIN32
  if (_setmode(_fileno(stdin), _O_BINARY) == -1 ||
      _setmode(_fileno(stdout), _O_BINARY) == -1) fail("binary mode failed");
#endif
  if (argc == 2 && strcmp(argv[1], "make-cmyk") == 0) make_cmyk(0);
  else if (argc == 2 && strcmp(argv[1], "make-ycck") == 0) make_cmyk(1);
  else if (argc == 1) coefficients();
  else fail("invalid experiment arguments");
  return fflush(stdout) == 0 ? 0 : 2;
}
