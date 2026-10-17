/* MSI基本/完整界面共用一次原生确认；静默与升级不弹窗、不删除应用数据。 */
#include "cleanup.h"
#include <commctrl.h>
#include <msi.h>
#include <msiquery.h>
#include <stdio.h>
#include <stdlib.h>
#include <wchar.h>

#ifdef PFX_INSTALLER_PROBE
#define PFX_APP_IDENTIFIER L"app.pixofold.installer-probe"
#define PFX_LOG_SUBDIRECTORY L"Temp\\PixoFoldInstallerProbe\\pixofold-logs"
#else
#define PFX_APP_IDENTIFIER L"app.pixofold.desktop"
#define PFX_LOG_SUBDIRECTORY L"Temp\\pixofold-logs"
#endif
static HMODULE module_handle;

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved) {
    (void)reserved;
    if (reason == DLL_PROCESS_ATTACH) {
        module_handle = instance;
    }
    return TRUE;
}

static BOOL property(MSIHANDLE install, const wchar_t *name, wchar_t *value, DWORD capacity) {
    DWORD length = capacity;
    value[0] = L'\0';
    return MsiGetPropertyW(install, name, value, &length) == ERROR_SUCCESS;
}

static BOOL interactive_uninstall(MSIHANDLE install) {
    wchar_t installed[16], remove[64], upgrade[128], level[16];
    if (!property(install, L"Installed", installed, 16) || !property(install, L"REMOVE", remove, 64) ||
        !property(install, L"UPGRADINGPRODUCTCODE", upgrade, 128) || !property(install, L"UILevel", level, 16)) return FALSE;
    wchar_t *end;
    unsigned long ui = wcstoul(level, &end, 10);
    return *end == L'\0' && pfx_should_prompt(installed[0] != 0, remove, upgrade[0] != 0, (unsigned)ui);
}

static BOOL chinese(void) { return PRIMARYLANGID(GetUserDefaultUILanguage()) == LANG_CHINESE; }

static void message(MSIHANDLE install, INSTALLMESSAGE kind, const wchar_t *text) {
    MSIHANDLE record = MsiCreateRecord(0);
    if (record) {
        MsiRecordSetStringW(record, 0, text);
        MsiProcessMessage(install, kind, record);
        MsiCloseHandle(record);
    }
}

typedef HRESULT (WINAPI *TaskDialogFn)(const TASKDIALOGCONFIG *, int *, int *, BOOL *);

/* msiexec宿主未必启用v6：在本DLL激活上下文中动态加载系统Common Controls。 */
static HRESULT confirmation(BOOL *remove_data, int *button) {
    ACTCTXW context = {0};
    context.cbSize = sizeof(context);
    context.dwFlags = ACTCTX_FLAG_HMODULE_VALID | ACTCTX_FLAG_RESOURCE_NAME_VALID;
    context.hModule = module_handle;
    context.lpResourceName = MAKEINTRESOURCEW(2);
    HANDLE activation = CreateActCtxW(&context);
    if (activation == INVALID_HANDLE_VALUE) return HRESULT_FROM_WIN32(GetLastError());
    ULONG_PTR cookie = 0;
    HRESULT result = E_FAIL;
    if (ActivateActCtx(activation, &cookie)) {
        HMODULE common = LoadLibraryExW(L"comctl32.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32);
        if (common) {
            FARPROC address = GetProcAddress(common, "TaskDialogIndirect");
            TaskDialogFn show = NULL;
            _Static_assert(sizeof(show) == sizeof(address), "Windows function pointer size mismatch");
            memcpy(&show, &address, sizeof(show));
            if (show) {
                TASKDIALOG_BUTTON proceed = { IDOK, chinese() ? L"继续卸载" : L"Uninstall" };
                TASKDIALOGCONFIG dialog = {0};
                dialog.cbSize = sizeof(dialog);
                dialog.dwFlags = TDF_ALLOW_DIALOG_CANCELLATION | TDF_SIZE_TO_CONTENT;
                dialog.dwCommonButtons = TDCBF_CANCEL_BUTTON;
                dialog.pszWindowTitle = chinese() ? L"卸载 PixoFold" : L"Uninstall PixoFold";
                dialog.pszMainInstruction = chinese() ? L"是否同时删除应用数据？" : L"Remove app data as well?";
                dialog.pszContent = chinese()
                    ? L"默认保留当前用户的设置、缓存和日志，以便重新安装后继续使用。\n图片、压缩结果及原图备份会保留。"
                    : L"Keep this user's settings, cache and logs for future installations by default.\nYour images, compressed results and original backups will be kept.";
                dialog.pszVerificationText = chinese() ? L"同时删除应用数据（设置、缓存和日志）"
                    : L"Also remove app data (settings, cache and logs)";
                dialog.pszFooter = chinese() ? L"仅清理当前 Windows 用户的数据，卸载成功后执行。"
                    : L"Only this Windows user's app data is removed, after successful uninstallation.";
                dialog.cButtons = 1;
                dialog.pButtons = &proceed;
                dialog.nDefaultButton = IDCANCEL;
                // 未设置TDF_VERIFICATION_FLAG_CHECKED，每次卸载均默认不勾选。
                *remove_data = FALSE;
                *button = IDCANCEL;
                result = show(&dialog, button, NULL, remove_data);
            }
            FreeLibrary(common);
        }
        DeactivateActCtx(0, cookie);
    }
    ReleaseActCtx(activation);
    return result;
}

__declspec(dllexport) UINT WINAPI ConfirmPixoFoldDataRemoval(MSIHANDLE install) {
    // 忽略外部预置选择：静默卸载传入同名属性也不能触发删除。
    if (MsiSetPropertyW(install, L"PixoFoldCleanupRequested", L"") != ERROR_SUCCESS ||
        MsiSetPropertyW(install, L"PixoFoldCleanupSid", L"") != ERROR_SUCCESS) return ERROR_INSTALL_FAILURE;
    if (!interactive_uninstall(install)) return ERROR_SUCCESS;
    HANDLE token;
    LPWSTR sid, local_data;
    if (!pfx_current_user(&token, &sid, &local_data)) {
        message(install, INSTALLMESSAGE_INFO, L"PixoFold: cleanup unavailable for this user; app data retained.");
        return ERROR_SUCCESS;
    }
    wchar_t initiating_sid[256];
    BOOL same_user = property(install, L"UserSID", initiating_sid, 256) && wcscmp(initiating_sid, sid) == 0;
    UINT result = ERROR_SUCCESS;
    if (same_user) {
        BOOL remove_data = FALSE;
        int button = IDCANCEL;
        HRESULT shown = confirmation(&remove_data, &button);
        if (FAILED(shown)) result = ERROR_INSTALL_FAILURE;
        else if (button != IDOK) result = ERROR_INSTALL_USEREXIT;
        else if (remove_data &&
            (MsiSetPropertyW(install, L"PixoFoldCleanupSid", sid) != ERROR_SUCCESS ||
             MsiSetPropertyW(install, L"PixoFoldCleanupRequested", L"1") != ERROR_SUCCESS)) result = ERROR_INSTALL_FAILURE;
    } else message(install, INSTALLMESSAGE_INFO, L"PixoFold: initiating user differs; app data retained.");
    CloseHandle(token);
    LocalFree(sid);
    CoTaskMemFree(local_data);
    return result;
}

__declspec(dllexport) UINT WINAPI RemovePixoFoldDataAfterUninstall(MSIHANDLE install) {
    wchar_t requested[8], expected[256], initiating[256];
    if (!interactive_uninstall(install) || !property(install, L"PixoFoldCleanupRequested", requested, 8) ||
        wcscmp(requested, L"1") != 0 || !property(install, L"PixoFoldCleanupSid", expected, 256) ||
        !property(install, L"UserSID", initiating, 256)) return ERROR_SUCCESS;
    HANDLE token;
    LPWSTR sid, local_data;
    if (!pfx_current_user(&token, &sid, &local_data)) return ERROR_SUCCESS;
    PfxCleanupResult result = {0};
    if (wcscmp(expected, sid) == 0 && wcscmp(initiating, sid) == 0) {
        size_t size = wcslen(local_data) + 96;
        wchar_t *app = calloc(size, sizeof(wchar_t)), *logs = calloc(size, sizeof(wchar_t));
        if (app && logs && swprintf_s(app, size, L"%s\\%s", local_data, PFX_APP_IDENTIFIER) > 0 &&
            swprintf_s(logs, size, L"%s\\%s", local_data, PFX_LOG_SUBDIRECTORY) > 0)
            result = pfx_cleanup(app, logs, sid);
        else result.failed++;
        free(app);
        free(logs);
    } else result.failed++;
    wchar_t summary[160];
    swprintf_s(summary, 160, L"PixoFold app data cleanup: removed=%u; retained_errors=%u", result.removed, result.failed);
    message(install, INSTALLMESSAGE_INFO, summary);
    if (result.failed) message(install, INSTALLMESSAGE_WARNING, chinese()
        ? L"PixoFold 已卸载，部分应用数据仍保留（文件正在使用、权限不足或存在链接）。图片及原图备份不受影响。"
        : L"PixoFold was uninstalled. Some app data was retained because files were in use, inaccessible, or linked. Images and original backups were not affected.");
    CloseHandle(token);
    LocalFree(sid);
    CoTaskMemFree(local_data);
    // 事务已成功结束，清理不完整如实提示，不能把卸载伪装成可回滚失败。
    return ERROR_SUCCESS;
}

#ifdef PFX_DIALOG_PREVIEW
int wmain(void) {
    module_handle = GetModuleHandleW(NULL);
    BOOL remove_data = FALSE;
    int button = IDCANCEL;
    HRESULT result = confirmation(&remove_data, &button);
    wprintf(L"preview_only=1 result=%ld button=%d remove_data=%d\n", (long)result, button, remove_data);
    return FAILED(result) ? 1 : 0;
}
#endif

#ifdef PFX_INSTALLER_PROBE
/* 仅独立探测DLL导出：验证真实MSI执行上下文，生产DLL不包含测试入口。 */
__declspec(dllexport) UINT WINAPI ProbePixoFoldUserContext(MSIHANDLE install) {
    HANDLE token;
    LPWSTR sid, local;
    wchar_t initiating[256];
    if (!pfx_current_user(&token, &sid, &local)) return ERROR_INSTALL_FAILURE;
    BOOL valid = property(install, L"UserSID", initiating, 256) && wcscmp(initiating, sid) == 0;
    CloseHandle(token);
    LocalFree(sid);
    CoTaskMemFree(local);
    message(install, INSTALLMESSAGE_INFO, valid ? L"PixoFold probe user context verified" : L"PixoFold probe user context mismatch");
    return valid ? ERROR_SUCCESS : ERROR_INSTALL_FAILURE;
}
#endif
