/* SPDX-License-Identifier: GPL-2.0-only OR GPL-3.0-or-later
 * 可信宿主须给出原生分配额度，超限以固定86退出。 */
#include "budget.h"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
int pixofold_gifsicle_main(int argc, char **argv);
static int parse_limit(const char *text, size_t *value) {
    size_t result = 0;
    if (!*text) return 0;
    for (; *text; ++text) {
        if (*text < '0' || *text > '9' || result > (SIZE_MAX - 9) / 10) return 0;
        result = result * 10 + (size_t)(*text - '0');
    }
    if (!result || result > 256 * 1024 * 1024) return 0;
    *value = result;
    return 1;
}
int main(int argc, char **argv) {
    size_t limit;
    if (argc < 4 || strcmp(argv[1], "--memory-bytes") || !parse_limit(argv[2], &limit)) return 87;
    pixofold_gif_set_budget(limit);
    if (argc == 4 && !strcmp(argv[3], "--allocation-self-test")) {
        unsigned char *data = pixofold_gif_calloc(1, 31);
        if ((uintptr_t)data % pixofold_gif_alignment()) return 88;
        for (size_t i = 0; i < 31; ++i) if (data[i] != 0) return 88;
        memset(data, 0x5A, 31);
        data = pixofold_gif_realloc(data, 1024);
        for (size_t i = 0; i < 31; ++i) if (data[i] != 0x5A) return 88;
        pixofold_gif_free(data);
        if (pixofold_gif_live_bytes() != 0) return 88;
        puts("bounded-allocation-passed");
        return 0;
    }
    argv[2] = argv[0];
    return pixofold_gifsicle_main(argc - 2, argv + 2);
}
