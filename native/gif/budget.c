/* SPDX-License-Identifier: GPL-2.0-only OR GPL-3.0-or-later
 * 固定单线程工具的活跃分配限额；上游分配/释放调用由构建配方重定向。
 * 计入头部和realloc新旧分配同时存活的峰值；系统/CRT工作集由宿主另留余量。 */
#include "budget.h"
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#ifdef _MSC_VER
/* MSVC的C头没有max_align_t；本配方只支持64位目标，malloc对齐16字节。 */
typedef __declspec(align(16)) union { unsigned char alignment[16]; size_t bytes; } BudgetHeader;
#else
typedef union { max_align_t alignment; size_t bytes; } BudgetHeader;
#endif
static size_t maximum_bytes;
static size_t live_bytes;
void pixofold_gif_set_budget(size_t maximum) { maximum_bytes = maximum; }
size_t pixofold_gif_live_bytes(void) { return live_bytes; }
size_t pixofold_gif_alignment(void) { return _Alignof(BudgetHeader); }
static void denied(void) { _Exit(86); }
void *pixofold_gif_malloc(size_t size) {
    if (size == 0) size = 1;
    if (size > SIZE_MAX - sizeof(BudgetHeader)) denied();
    size_t total = size + sizeof(BudgetHeader);
    if (live_bytes > maximum_bytes || total > maximum_bytes - live_bytes) denied();
    BudgetHeader *header = (BudgetHeader *)malloc(total);
    if (!header) denied();
    header->bytes = total;
    live_bytes += total;
    return header + 1;
}
void pixofold_gif_free(void *pointer) {
    if (pointer) {
        /* 固定源码只释放此封装返回的指针；没有其他返回堆指针的外部API。 */
        BudgetHeader *header = (BudgetHeader *)pointer - 1;
        live_bytes -= header->bytes;
        free(header);
    }
}
void *pixofold_gif_realloc(void *pointer, size_t size) {
    if (!pointer) return pixofold_gif_malloc(size);
    if (size == 0) { pixofold_gif_free(pointer); return NULL; }
    BudgetHeader *old = (BudgetHeader *)pointer - 1;
    size_t previous = old->bytes - sizeof(BudgetHeader);
    void *replacement = pixofold_gif_malloc(size);
    memcpy(replacement, pointer, previous < size ? previous : size);
    pixofold_gif_free(pointer);
    return replacement;
}
void *pixofold_gif_calloc(size_t count, size_t size) {
    if (count && size > SIZE_MAX / count) denied();
    size_t total = count * size;
    void *pointer = pixofold_gif_malloc(total);
    memset(pointer, 0, total);
    return pointer;
}
