/* SPDX-License-Identifier: GPL-2.0-only OR GPL-3.0-or-later
 * 独立Gifsicle helper按GPLv2组合；本项目新增封装同时提供GPLv3+许可。 */
#ifndef PIXOFOLD_GIF_BUDGET_H
#define PIXOFOLD_GIF_BUDGET_H
#include <stddef.h>
void pixofold_gif_set_budget(size_t maximum);
void *pixofold_gif_malloc(size_t size);
void *pixofold_gif_calloc(size_t count, size_t size);
void *pixofold_gif_realloc(void *pointer, size_t size);
void pixofold_gif_free(void *pointer);
size_t pixofold_gif_live_bytes(void);
size_t pixofold_gif_alignment(void);
#endif
