#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <objbase.h>

/* 路径只由当前用户Known Folder及受限日志定位记录导出；不接受MSI路径属性。 */
typedef struct PfxCleanupResult {
    unsigned removed;
    unsigned failed;
    unsigned visited;
} PfxCleanupResult;

BOOL pfx_current_user(HANDLE *token, LPWSTR *sid, LPWSTR *local_data);
BOOL pfx_should_prompt(BOOL installed, const wchar_t *remove, BOOL upgrading, unsigned ui_level);
PfxCleanupResult pfx_cleanup(const wchar_t *app_data, const wchar_t *fallback_logs, const wchar_t *sid);
BOOL pfx_log_name(const wchar_t *name, const wchar_t *extension);
BOOL pfx_log_header(const char *data, size_t length);
