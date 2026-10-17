/* 独立临时语料验证真实删除、锁、重解析点、硬链接和用户身份边界；不依赖GUI。 */
#include "cleanup.h"
#include <stdio.h>
#include <stdlib.h>
#include <wchar.h>
#include <winioctl.h>

static unsigned checks;
#define CHECK(value) do { checks++; if (!(value)) { fprintf(stderr, "check failed line=%d error=%lu\n", __LINE__, GetLastError()); exit(1); } } while (0)
static void path(wchar_t *output, const wchar_t *root, const wchar_t *name) {
    CHECK(swprintf_s(output, 2048, L"%s\\%s", root, name) > 0);
}
static void write_bytes(const wchar_t *name, const void *data, DWORD length) {
    HANDLE file = CreateFileW(name, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    CHECK(file != INVALID_HANDLE_VALUE);
    DWORD written;
    CHECK(WriteFile(file, data, length, &written, NULL) && written == length);
    CloseHandle(file);
}
static BOOL exists(const wchar_t *name) { return GetFileAttributesW(name) != INVALID_FILE_ATTRIBUTES; }
static void make_directory(wchar_t *output, const wchar_t *root, const wchar_t *name) {
    path(output, root, name);
    CHECK(CreateDirectoryW(output, NULL));
}
static void marker(wchar_t *output, const wchar_t *root, const wchar_t *name) {
    path(output, root, name);
    write_bytes(output, "fixture", 7);
}

static void junction(const wchar_t *at, const wchar_t *to) {
    typedef struct JunctionData {
        DWORD tag;
        WORD length, reserved;
        WORD sub_offset, sub_length, print_offset, print_length;
        wchar_t buffer[4096];
    } JunctionData;
    CHECK(CreateDirectoryW(at, NULL));
    HANDLE directory = CreateFileW(at, GENERIC_WRITE, 0, NULL, OPEN_EXISTING,
        FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS, NULL);
    CHECK(directory != INVALID_HANDLE_VALUE);
    JunctionData data = {0};
    data.tag = IO_REPARSE_TAG_MOUNT_POINT;
    int count = swprintf_s(data.buffer, 4096, L"\\??\\%s", to);
    CHECK(count > 0 && count < 2048);
    data.sub_length = (WORD)(count * sizeof(wchar_t));
    data.print_offset = data.sub_length + sizeof(wchar_t);
    data.print_length = (WORD)(wcslen(to) * sizeof(wchar_t));
    memcpy((BYTE *)data.buffer + data.print_offset, to, data.print_length + sizeof(wchar_t));
    data.length = 8 + data.print_offset + data.print_length + sizeof(wchar_t);
    DWORD returned;
    CHECK(DeviceIoControl(directory, FSCTL_SET_REPARSE_POINT, &data, 8 + data.length, NULL, 0, &returned, NULL));
    CloseHandle(directory);
}

static void location(const wchar_t *root, const wchar_t *logs) {
    wchar_t file[2048];
    path(file, root, L"log-location-v1.bin");
    DWORD count = (DWORD)wcslen(logs);
    BYTE buffer[4096] = {0};
    CHECK(12 + count * 2 <= sizeof(buffer));
    memcpy(buffer, "PFXLOG1", 8);
    memcpy(buffer + 8, &count, 4);
    memcpy(buffer + 12, logs, count * 2);
    write_bytes(file, buffer, 12 + count * 2);
}

static const char header[] = "{\"event\":\"session_started\",\"schema\":\"pixofold.log.v1\"}\n";
static void log_pair(const wchar_t *root, const wchar_t *stem, wchar_t *body, wchar_t *lease) {
    wchar_t name[256];
    CHECK(swprintf_s(name, 256, L"%s.jsonl", stem) > 0);
    path(body, root, name);
    write_bytes(body, header, sizeof(header) - 1);
    CHECK(swprintf_s(name, 256, L"%s.lock", stem) > 0);
    path(lease, root, name);
    write_bytes(lease, "", 0);
}

int wmain(int argc, wchar_t **argv) {
    CHECK(argc == 2);
    const wchar_t *root = argv[1];
    HANDLE token;
    LPWSTR sid, local;
    CHECK(pfx_current_user(&token, &sid, &local));
    CloseHandle(token);
    CoTaskMemFree(local);
    for (unsigned level = 0; level <= 6; level++) {
        CHECK(pfx_should_prompt(TRUE, L"ALL", FALSE, level) == (level >= 3 && level <= 5));
        CHECK(!pfx_should_prompt(TRUE, L"ALL", TRUE, level));
        CHECK(!pfx_should_prompt(FALSE, L"ALL", FALSE, level));
        CHECK(!pfx_should_prompt(TRUE, L"MainFeature", FALSE, level));
    }
    CHECK(pfx_log_name(L"run-123-AbCdEf123456.jsonl", L".jsonl"));
    CHECK(!pfx_log_name(L"run-123-AbCdEf123456.jpg", L".jsonl"));
    CHECK(!pfx_log_name(L"run-x-AbCdEf123456.jsonl", L".jsonl"));
    CHECK(!pfx_log_name(L"run-123-abc.jsonl", L".jsonl"));
    CHECK(pfx_log_header(header, sizeof(header) - 1));
    const char nested[] = "{\"other\":{\"event\":\"session_started\",\"schema\":\"pixofold.log.v1\"}}\n";
    CHECK(!pfx_log_header(nested, sizeof(nested) - 1));
    CHECK(!pfx_log_header(header, 20));

    wchar_t app[2048], cache[2048], deep[2048], data[2048], image[2048], backup[2048], cache_file[2048];
    make_directory(app, root, L"app");
    make_directory(cache, app, L"EBWebView");
    make_directory(deep, cache, L"Default");
    marker(cache_file, deep, L"cache-entry");
    marker(data, app, L"overwrite-consent-v1.json");
    marker(image, app, L"original.jpg");
    marker(backup, app, L"original-backup-Ab12Cd.png");
    // 即使确认标记是外部原图的硬链接，也只删除此目录项。
    CHECK(DeleteFileW(data));
    CHECK(CreateHardLinkW(data, image, NULL));
    PfxCleanupResult result = pfx_cleanup(app, NULL, L"S-1-5-18");
    CHECK(result.failed > 0 && exists(data) && exists(cache_file));
    result = pfx_cleanup(app, NULL, sid);
    CHECK(result.failed == 0 && !exists(cache) && !exists(data));
    CHECK(exists(image) && exists(backup));
    HANDLE original = CreateFileW(image, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL);
    char original_bytes[8] = {0}; DWORD read;
    CHECK(original != INVALID_HANDLE_VALUE && ReadFile(original, original_bytes, 7, &read, NULL));
    CHECK(read == 7 && memcmp(original_bytes, "fixture", 7) == 0);
    CloseHandle(original);

    wchar_t outside[2048], outside_file[2048];
    make_directory(outside, root, L"photos");
    marker(outside_file, outside, L"keep.png");
    junction(cache, outside);
    result = pfx_cleanup(app, NULL, sid);
    CHECK(result.failed > 0 && exists(outside_file));
    CHECK(RemoveDirectoryW(cache));
    CHECK(CreateDirectoryW(cache, NULL));
    marker(cache_file, cache, L"locked-cache");
    HANDLE active = CreateFileW(cache_file, GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL);
    CHECK(active != INVALID_HANDLE_VALUE);
    result = pfx_cleanup(app, NULL, sid);
    CHECK(result.failed > 0 && exists(cache_file));
    CloseHandle(active);
    result = pfx_cleanup(app, NULL, sid);
    CHECK(result.failed == 0 && !exists(cache));

    wchar_t logs[2048], body[2048], lease[2048], unknown[2048];
    make_directory(logs, root, L"pixofold-logs");
    log_pair(logs, L"run-1-AbCdEf123456", body, lease);
    marker(unknown, logs, L"user-photo.jpg");
    location(app, logs);
    active = CreateFileW(lease, GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, NULL, OPEN_EXISTING, 0, NULL);
    CHECK(active != INVALID_HANDLE_VALUE);
    OVERLAPPED overlapped = {0};
    CHECK(LockFileEx(active, LOCKFILE_EXCLUSIVE_LOCK, 0, MAXDWORD, MAXDWORD, &overlapped));
    result = pfx_cleanup(app, logs, sid);
    CHECK(result.failed > 0 && exists(body) && exists(unknown));
    CHECK(UnlockFileEx(active, 0, MAXDWORD, MAXDWORD, &overlapped));
    CloseHandle(active);
    result = pfx_cleanup(app, logs, sid);
    CHECK(result.failed == 0 && !exists(body) && !exists(lease) && exists(unknown));
    CHECK(exists(image) && exists(backup) && exists(outside_file));
    LocalFree(sid);
    printf("Windows installer cleanup checks passed: %u\n", checks);
    return 0;
}
