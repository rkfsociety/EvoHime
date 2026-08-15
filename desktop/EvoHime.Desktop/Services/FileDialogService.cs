using System.Runtime.InteropServices;

namespace EvoHime.Desktop.Services;

/// <summary>
/// Обёртка над системным диалогом выбора файлов (IFileOpenDialog).
/// В отличие от WinRT FileOpenPicker умеет открываться в заданной папке —
/// это нужно, чтобы приложение запоминало, откуда пользователь брал файлы в прошлый раз.
/// </summary>
public static class FileDialogService
{
    private const uint FosForceFilesystem = 0x00000040;
    private const uint FosAllowMultiselect = 0x00000200;
    private const uint FosPathMustExist = 0x00000800;
    private const uint FosFileMustExist = 0x00001000;
    private const uint SigdnFileSysPath = 0x80058000;
    private const int ErrorCancelled = unchecked((int)0x800704C7);

    /// <summary>
    /// Показывает модальный диалог выбора файлов. Возвращает пустой список, если отменили.
    /// </summary>
    /// <param name="ownerWindow">HWND окна-владельца.</param>
    /// <param name="initialFolder">Папка, с которой открыть диалог (игнорируется, если её нет).</param>
    public static IReadOnlyList<string> PickFiles(
        IntPtr ownerWindow,
        string? initialFolder = null,
        bool allowMultiple = true,
        string? title = null)
    {
        // Через object: приведение делает QueryInterface на живом COM-объекте.
        var dialog = (IFileOpenDialog)(object)new FileOpenDialogRcw();
        try
        {
            var options = FosForceFilesystem | FosPathMustExist | FosFileMustExist;
            if (allowMultiple)
            {
                options |= FosAllowMultiselect;
            }
            dialog.SetOptions(options);

            if (!string.IsNullOrWhiteSpace(title))
            {
                dialog.SetTitle(title);
            }

            var startFolder = TryCreateShellItem(initialFolder);
            if (startFolder is not null)
            {
                // SetFolder, а не SetDefaultFolder: нам нужна именно наша папка,
                // даже если Windows помнит собственную «последнюю».
                dialog.SetFolder(startFolder);
            }

            var hr = dialog.Show(ownerWindow);
            if (hr == ErrorCancelled)
            {
                return [];
            }
            if (hr < 0)
            {
                Marshal.ThrowExceptionForHR(hr);
            }

            dialog.GetResults(out var results);
            results.GetCount(out var count);
            var paths = new List<string>((int)count);
            for (uint index = 0; index < count; index++)
            {
                results.GetItemAt(index, out var item);
                item.GetDisplayName(SigdnFileSysPath, out var path);
                if (!string.IsNullOrWhiteSpace(path))
                {
                    paths.Add(path);
                }
            }
            return paths;
        }
        finally
        {
            Marshal.FinalReleaseComObject(dialog);
        }
    }

    private static IShellItem? TryCreateShellItem(string? folder)
    {
        if (string.IsNullOrWhiteSpace(folder) || !Directory.Exists(folder))
        {
            return null;
        }

        try
        {
            var riid = typeof(IShellItem).GUID;
            return SHCreateItemFromParsingName(folder, IntPtr.Zero, ref riid, out var item) == 0 ? item : null;
        }
        catch (Exception exception) when (exception is COMException or ArgumentException)
        {
            return null;
        }
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode, PreserveSig = true)]
    private static extern int SHCreateItemFromParsingName(
        string path,
        IntPtr bindContext,
        ref Guid riid,
        [MarshalAs(UnmanagedType.Interface)] out IShellItem item);

    [ComImport]
    [Guid("DC1C5A9C-E88A-4dde-A5A1-60F82A20AEF7")]
    [ClassInterface(ClassInterfaceType.None)]
    private sealed class FileOpenDialogRcw
    {
    }

    // Методы объявлены в порядке vtable (IModalWindow → IFileDialog → IFileOpenDialog).
    // Неиспользуемые слоты оставлены с упрощёнными сигнатурами — их нельзя удалять.
    [ComImport]
    [Guid("d57c7288-d4ad-4768-be02-9d969532d960")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IFileOpenDialog
    {
        [PreserveSig]
        int Show(IntPtr parent);

        void SetFileTypes(uint fileTypeCount, IntPtr filterSpec);
        void SetFileTypeIndex(uint fileTypeIndex);
        void GetFileTypeIndex(out uint fileTypeIndex);
        void Advise(IntPtr events, out uint cookie);
        void Unadvise(uint cookie);
        void SetOptions(uint options);
        void GetOptions(out uint options);
        void SetDefaultFolder(IShellItem folder);
        void SetFolder(IShellItem folder);
        void GetFolder(out IShellItem folder);
        void GetCurrentSelection(out IShellItem item);
        void SetFileName([MarshalAs(UnmanagedType.LPWStr)] string name);
        void GetFileName([MarshalAs(UnmanagedType.LPWStr)] out string name);
        void SetTitle([MarshalAs(UnmanagedType.LPWStr)] string title);
        void SetOkButtonLabel([MarshalAs(UnmanagedType.LPWStr)] string text);
        void SetFileNameLabel([MarshalAs(UnmanagedType.LPWStr)] string label);
        void GetResult(out IShellItem item);
        void AddPlace(IShellItem item, int placement);
        void SetDefaultExtension([MarshalAs(UnmanagedType.LPWStr)] string defaultExtension);
        void Close(int result);
        void SetClientGuid(ref Guid client);
        void ClearClientData();
        void SetFilter(IntPtr filter);
        void GetResults(out IShellItemArray items);
        void GetSelectedItems(out IShellItemArray items);
    }

    [ComImport]
    [Guid("43826d1e-e718-42ee-bc55-a1e261c37bfe")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IShellItem
    {
        void BindToHandler(IntPtr bindContext, ref Guid handler, ref Guid riid, out IntPtr result);
        void GetParent(out IShellItem parent);
        void GetDisplayName(uint sigdnName, [MarshalAs(UnmanagedType.LPWStr)] out string name);
        void GetAttributes(uint mask, out uint attributes);
        void Compare(IShellItem other, uint hint, out int order);
    }

    [ComImport]
    [Guid("b63ea76d-1f85-456f-a19c-48159efa858b")]
    [InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IShellItemArray
    {
        void BindToHandler(IntPtr bindContext, ref Guid handler, ref Guid riid, out IntPtr result);
        void GetPropertyStore(int flags, ref Guid riid, out IntPtr store);
        void GetPropertyDescriptionList(IntPtr keyType, ref Guid riid, out IntPtr list);
        void GetAttributes(int attributeFlags, uint mask, out uint attributes);
        void GetCount(out uint count);
        void GetItemAt(uint index, out IShellItem item);
        void EnumItems(out IntPtr enumerator);
    }
}
