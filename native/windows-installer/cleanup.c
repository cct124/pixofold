/* 当前用户应用数据清理；固定白名单、无重解析点遍历、持有祖先目录句柄防止替换。 */
#include "cleanup.h"
#include <aclapi.h>
#include <sddl.h>
#include <shlobj.h>
#include <stdio.h>
#include <stdlib.h>
#include <wchar.h>

#define PFX_PATH_UNITS 32768
#define PFX_MAX_ENTRIES 100000
#define PFX_MAX_DEPTH 64
#define PFX_LOCATION_FILE L"log-location-v1.bin"

BOOL pfx_should_prompt(BOOL installed, const wchar_t *remove, BOOL upgrading, unsigned ui_level) {
    return installed && remove && _wcsicmp(remove, L"ALL") == 0 && !upgrading && ui_level >= 3 && ui_level <= 5;
}

BOOL pfx_current_user(HANDLE *token, LPWSTR *sid, LPWSTR *local_data) {
    union { TOKEN_USER alignment; BYTE bytes[1024]; } buffer;
    DWORD length = 0;
    *token = NULL;
    *sid = NULL;
    *local_data = NULL;
    if (!OpenThreadToken(GetCurrentThread(), TOKEN_QUERY | TOKEN_IMPERSONATE, TRUE, token)) {
        if (GetLastError() != ERROR_NO_TOKEN ||
            !OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_IMPERSONATE, token)) return FALSE;
    }
    if (!GetTokenInformation(*token, TokenUser, buffer.bytes, sizeof(buffer.bytes), &length)) goto fail;
    PSID user = ((TOKEN_USER *)buffer.bytes)->User.Sid;
    if (IsWellKnownSid(user, WinLocalSystemSid) || IsWellKnownSid(user, WinLocalServiceSid) ||
        IsWellKnownSid(user, WinNetworkServiceSid)) goto fail;
    if (!ConvertSidToStringSidW(user, sid)) goto fail;
    if (FAILED(SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, *token, local_data))) goto fail;
    return TRUE;
fail:
    if (*sid) LocalFree(*sid);
    if (*local_data) CoTaskMemFree(*local_data);
    if (*token) CloseHandle(*token);
    *token = NULL;
    *sid = NULL;
    *local_data = NULL;
    return FALSE;
}

static wchar_t *join(const wchar_t *root, const wchar_t *name) {
    size_t a = wcslen(root), b = wcslen(name);
    if (a + b + 2 > PFX_PATH_UNITS) return NULL;
    wchar_t *path = calloc(a + b + 2, sizeof(wchar_t));
    if (path) {
        memcpy(path, root, a * sizeof(wchar_t));
        path[a] = L'\\';
        memcpy(path + a + 1, name, (b + 1) * sizeof(wchar_t));
    }
    return path;
}

static HANDLE open_ordinary(const wchar_t *path, BOOL directory, DWORD access) {
    HANDLE file = CreateFileW(path, access | FILE_READ_ATTRIBUTES | READ_CONTROL,
        FILE_SHARE_READ, NULL, OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT | (directory ? FILE_FLAG_BACKUP_SEMANTICS : 0), NULL);
    if (file == INVALID_HANDLE_VALUE) return file;
    BY_HANDLE_FILE_INFORMATION info;
    if (!GetFileInformationByHandle(file, &info) || (info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) ||
        !!(info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != !!directory) {
        CloseHandle(file);
        SetLastError(ERROR_ACCESS_DENIED);
        return INVALID_HANDLE_VALUE;
    }
    return file;
}

static BOOL same_owner(HANDLE file, const wchar_t *expected) {
    PSID owner = NULL, sid = NULL;
    PSECURITY_DESCRIPTOR descriptor = NULL;
    BOOL result = FALSE;
    if (!ConvertStringSidToSidW(expected, &sid)) return FALSE;
    if (GetSecurityInfo(file, SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION, &owner, NULL, NULL, NULL,
        &descriptor) == ERROR_SUCCESS) result = EqualSid(owner, sid);
    if (descriptor) LocalFree(descriptor);
    LocalFree(sid);
    return result;
}

static BOOL missing(void) {
    DWORD error = GetLastError();
    return error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND;
}

static BOOL dispose(HANDLE file, PfxCleanupResult *result) {
    FILE_DISPOSITION_INFO disposition = { TRUE };
    if (SetFileInformationByHandle(file, FileDispositionInfo, &disposition, sizeof(disposition))) {
        result->removed++;
        return TRUE;
    }
    result->failed++;
    return FALSE;
}

static void remove_file(const wchar_t *path, PfxCleanupResult *result) {
    HANDLE file = open_ordinary(path, FALSE, DELETE);
    if (file == INVALID_HANDLE_VALUE) {
        if (!missing()) result->failed++;
        return;
    }
    dispose(file, result);
    CloseHandle(file);
}

static void remove_tree(const wchar_t *path, unsigned depth, PfxCleanupResult *result) {
    if (depth > PFX_MAX_DEPTH || result->visited >= PFX_MAX_ENTRIES) { result->failed++; return; }
    HANDLE directory = open_ordinary(path, TRUE, FILE_LIST_DIRECTORY | DELETE);
    if (directory == INVALID_HANDLE_VALUE) {
        if (!missing()) result->failed++;
        return;
    }
    wchar_t *pattern = join(path, L"*");
    if (!pattern) { result->failed++; CloseHandle(directory); return; }
    WIN32_FIND_DATAW entry;
    HANDLE find = FindFirstFileW(pattern, &entry);
    free(pattern);
    if (find != INVALID_HANDLE_VALUE) {
        do {
            if (wcscmp(entry.cFileName, L".") == 0 || wcscmp(entry.cFileName, L"..") == 0) continue;
            if (++result->visited > PFX_MAX_ENTRIES) { result->failed++; break; }
            wchar_t *child = join(path, entry.cFileName);
            if (!child) { result->failed++; break; }
            if (entry.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) result->failed++;
            else if (entry.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) remove_tree(child, depth + 1, result);
            else remove_file(child, result);
            free(child);
        } while (FindNextFileW(find, &entry));
        if (GetLastError() != ERROR_NO_MORE_FILES) result->failed++;
        FindClose(find);
    } else if (GetLastError() != ERROR_FILE_NOT_FOUND) result->failed++;
    // 不清只读位：硬链接共享属性，修改属性可能影响用户在其他目录保留的文件。
    dispose(directory, result);
    CloseHandle(directory);
}

BOOL pfx_log_name(const wchar_t *name, const wchar_t *extension) {
    if (wcsncmp(name, L"run-", 4) != 0) return FALSE;
    const wchar_t *at = name + 4, *start = at;
    while (*at >= L'0' && *at <= L'9') at++;
    if (at == start || *at++ != L'-') return FALSE;
    for (unsigned i = 0; i < 12; i++, at++) {
        if (!((*at >= L'0' && *at <= L'9') || (*at >= L'a' && *at <= L'z') || (*at >= L'A' && *at <= L'Z'))) return FALSE;
    }
    return wcscmp(at, extension) == 0;
}

/* 只识别本应用编码器输出的顶层固定schema/event，不把嵌套字段或转义字符串当作标识。 */
BOOL pfx_log_header(const char *data, size_t length) {
    unsigned depth = 0;
    BOOL schema = FALSE, event = FALSE;
    if (!length || data[0] != '{') return FALSE;
    for (size_t i = 0; i < length; i++) {
        char c = data[i];
        if (c == '\n' || c == '\r') return depth == 0 && schema && event;
        if (c == '{' || c == '[') { if (++depth > 32) return FALSE; }
        else if (c == '}' || c == ']') { if (!depth) return FALSE; depth--; }
        else if (c == '"') {
            size_t start = i++;
            for (; i < length && data[i] != '"'; i++) {
                if ((unsigned char)data[i] < 32) return FALSE;
                if (data[i] == '\\' && ++i >= length) return FALSE;
            }
            if (i >= length) return FALSE;
            if (depth == 1 && i + 1 < length && data[i + 1] == ':') {
                const char *value = data + i + 2;
                size_t remaining = length - i - 2;
                if (i - start == 7 && memcmp(data + start, "\"schema\"", 8) == 0) {
                    if (schema || remaining < 17 || memcmp(value, "\"pixofold.log.v1\"", 17) != 0) return FALSE;
                    schema = TRUE;
                }
                if (i - start == 6 && memcmp(data + start, "\"event\"", 7) == 0) {
                    if (event || remaining < 17 || memcmp(value, "\"session_started\"", 17) != 0) return FALSE;
                    event = TRUE;
                }
            }
        }
    }
    return depth == 0 && schema && event;
}

static HANDLE lock_file(const wchar_t *path, BOOL create) {
    HANDLE file = CreateFileW(path, GENERIC_READ | GENERIC_WRITE | DELETE, FILE_SHARE_READ | FILE_SHARE_WRITE,
        NULL, create ? OPEN_ALWAYS : OPEN_EXISTING, FILE_FLAG_OPEN_REPARSE_POINT, NULL);
    if (file == INVALID_HANDLE_VALUE) return file;
    BY_HANDLE_FILE_INFORMATION info;
    LARGE_INTEGER size;
    OVERLAPPED overlapped = {0};
    if (!GetFileInformationByHandle(file, &info) || (info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)) ||
        !GetFileSizeEx(file, &size) || size.QuadPart != 0 ||
        !LockFileEx(file, LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY, 0, MAXDWORD, MAXDWORD, &overlapped)) {
        CloseHandle(file);
        SetLastError(ERROR_LOCK_VIOLATION);
        return INVALID_HANDLE_VALUE;
    }
    return file;
}

static void unlock_close(HANDLE file) {
    OVERLAPPED overlapped = {0};
    UnlockFileEx(file, 0, MAXDWORD, MAXDWORD, &overlapped);
    CloseHandle(file);
}

static void remove_logs(const wchar_t *path, const wchar_t *sid, PfxCleanupResult *result) {
    const wchar_t *name = wcsrchr(path, L'\\');
    if (!name || wcscmp(name + 1, L"pixofold-logs") != 0) { result->failed++; return; }
    HANDLE directory = open_ordinary(path, TRUE, FILE_LIST_DIRECTORY);
    if (directory == INVALID_HANDLE_VALUE) { if (!missing()) result->failed++; return; }
    if (!same_owner(directory, sid)) { result->failed++; CloseHandle(directory); return; }
    wchar_t *coord_path = join(path, L".pixofold-log.lock");
    HANDLE coordinator = coord_path ? lock_file(coord_path, TRUE) : INVALID_HANDLE_VALUE;
    free(coord_path);
    if (coordinator == INVALID_HANDLE_VALUE) { result->failed++; CloseHandle(directory); return; }
    wchar_t *pattern = join(path, L"run-*.jsonl");
    if (!pattern) { result->failed++; unlock_close(coordinator); CloseHandle(directory); return; }
    WIN32_FIND_DATAW entry;
    HANDLE find = FindFirstFileW(pattern, &entry);
    DWORD find_error = GetLastError();
    free(pattern);
    if (find != INVALID_HANDLE_VALUE) {
        do {
            if (!pfx_log_name(entry.cFileName, L".jsonl")) continue;
            if (++result->visited > PFX_MAX_ENTRIES) { result->failed++; break; }
            wchar_t *file_path = join(path, entry.cFileName);
            wchar_t lease_name[MAX_PATH];
            if (!file_path || wcslen(entry.cFileName) >= MAX_PATH) { free(file_path); result->failed++; continue; }
            wcscpy_s(lease_name, MAX_PATH, entry.cFileName);
            wcscpy_s(wcsrchr(lease_name, L'.'), 7, L".lock");
            wchar_t *lease_path = join(path, lease_name);
            HANDLE lease = lease_path ? lock_file(lease_path, TRUE) : INVALID_HANDLE_VALUE;
            free(lease_path);
            if (lease != INVALID_HANDLE_VALUE) {
                HANDLE file = open_ordinary(file_path, FALSE, GENERIC_READ | DELETE);
                if (file != INVALID_HANDLE_VALUE) {
                    char header[4096];
                    DWORD size = 0;
                    if (ReadFile(file, header, sizeof(header), &size, NULL) && pfx_log_header(header, size)) {
                        if (dispose(file, result)) dispose(lease, result);
                    }
                    CloseHandle(file);
                } else if (!missing()) result->failed++;
                unlock_close(lease);
            } else result->failed++;
            free(file_path);
        } while (FindNextFileW(find, &entry));
        if (GetLastError() != ERROR_NO_MORE_FILES) result->failed++;
        FindClose(find);
    } else if (find_error != ERROR_FILE_NOT_FOUND) result->failed++;
    // 保留零字节目录协调文件，避免其他实例仍持旧句柄时出现两把不同的目录锁。
    unlock_close(coordinator);
    CloseHandle(directory);
}

static wchar_t *log_location(const wchar_t *root) {
    wchar_t *path = join(root, PFX_LOCATION_FILE);
    HANDLE file = path ? open_ordinary(path, FALSE, GENERIC_READ) : INVALID_HANDLE_VALUE;
    free(path);
    if (file == INVALID_HANDLE_VALUE) return NULL;
    BYTE bytes[12 + PFX_PATH_UNITS * 2];
    DWORD read = 0;
    LARGE_INTEGER size;
    wchar_t *location = NULL;
    if (GetFileSizeEx(file, &size) && size.QuadPart >= 14 && size.QuadPart < sizeof(bytes) &&
        ReadFile(file, bytes, (DWORD)size.QuadPart, &read, NULL) && read == size.QuadPart &&
        memcmp(bytes, "PFXLOG1", 8) == 0) {
        DWORD count;
        memcpy(&count, bytes + 8, sizeof(count));
        if (count && count < PFX_PATH_UNITS && read == 12 + count * 2) {
            location = calloc((size_t)count + 1, sizeof(wchar_t));
            if (location) {
                memcpy(location, bytes + 12, count * 2);
                // 仅绝对DOS/UNC路径；嵌入NUL不能截短到另一个目标。
                if (wcslen(location) != count ||
                    !((count > 3 && location[1] == L':' && location[2] == L'\\') ||
                      (count > 3 && location[0] == L'\\' && location[1] == L'\\'))) {
                    free(location); location = NULL;
                }
            }
        }
    }
    CloseHandle(file);
    return location;
}

PfxCleanupResult pfx_cleanup(const wchar_t *app_data, const wchar_t *fallback_logs, const wchar_t *sid) {
    PfxCleanupResult result = {0};
    HANDLE root = open_ordinary(app_data, TRUE, FILE_LIST_DIRECTORY);
    if (root == INVALID_HANDLE_VALUE) {
        if (!missing()) result.failed++;
        if (!result.failed && fallback_logs) remove_logs(fallback_logs, sid, &result);
        return result;
    }
    if (!same_owner(root, sid)) { result.failed++; CloseHandle(root); return result; }
    wchar_t *logs = log_location(app_data);
    if (logs || fallback_logs) remove_logs(logs ? logs : fallback_logs, sid, &result);
    free(logs);
    wchar_t *webview = join(app_data, L"EBWebView");
    if (webview) { remove_tree(webview, 0, &result); free(webview); }
    else result.failed++;
    const wchar_t *files[] = {L"overwrite-consent-v1.json", PFX_LOCATION_FILE};
    for (unsigned i = 0; i < sizeof(files) / sizeof(files[0]); i++) {
        wchar_t *file = join(app_data, files[i]);
        if (file) { remove_file(file, &result); free(file); }
        else result.failed++;
    }
    CloseHandle(root);
    return result;
}
