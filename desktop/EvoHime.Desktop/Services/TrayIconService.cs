using System.ComponentModel;
using System.Runtime.InteropServices;

namespace EvoHime.Desktop.Services;

public enum TrayMenuCommand : uint
{
    Show = 1,
    Exit = 2,
}

public sealed class TrayIconService : IDisposable
{
    private const uint NifMessage = 0x00000001;
    private const uint NifIcon = 0x00000002;
    private const uint NifTip = 0x00000004;
    private const uint NimAdd = 0x00000000;
    private const uint NimDelete = 0x00000002;
    private const uint WmApp = 0x8000;
    private const uint WmRButtonUp = 0x0205;
    private const uint WmLButtonDblClk = 0x0203;
    private const uint MfString = 0x00000000;
    private const uint TpmRightButton = 0x0002;
    private const uint TpmReturnCommand = 0x0100;
    private const int HwndMessage = -3;

    private readonly Action _show;
    private readonly Action _exit;
    private readonly WndProc _windowProc;
    private readonly string _className = $"EvoHime.Tray.{Guid.NewGuid():N}";
    private readonly uint _callbackMessage = WmApp + 17;
    private readonly uint _iconId = 1;
    private nint _windowHandle;
    private ushort _classAtom;
    private bool _disposed;

    public TrayIconService(Action show, Action exit)
    {
        _show = show;
        _exit = exit;
        _windowProc = WindowProc;
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var instance = GetModuleHandle(null);
        var windowClass = new WndClassEx
        {
            Size = (uint)Marshal.SizeOf<WndClassEx>(),
            WindowProc = _windowProc,
            Instance = instance,
            ClassName = _className,
        };
        _classAtom = RegisterClassEx(ref windowClass);
        if (_classAtom == 0)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Не удалось зарегистрировать tray window.");
        }

        _windowHandle = CreateWindowEx(
            0,
            _className,
            "EvoHime Tray",
            0,
            0,
            0,
            0,
            0,
            (nint)HwndMessage,
            0,
            instance,
            0);
        if (_windowHandle == 0)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Не удалось создать tray window.");
        }

        var iconData = new NotifyIconData
        {
            Size = (uint)Marshal.SizeOf<NotifyIconData>(),
            WindowHandle = _windowHandle,
            Id = _iconId,
            Flags = NifMessage | NifIcon | NifTip,
            CallbackMessage = _callbackMessage,
            IconHandle = LoadIcon(0, (nint)32512),
            Tip = "EvoHime",
        };
        if (!ShellNotifyIcon(NimAdd, ref iconData))
        {
            Dispose();
            throw new Win32Exception(Marshal.GetLastWin32Error(), "Не удалось добавить иконку EvoHime в tray.");
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        if (_windowHandle != 0)
        {
            var iconData = new NotifyIconData
            {
                Size = (uint)Marshal.SizeOf<NotifyIconData>(),
                WindowHandle = _windowHandle,
                Id = _iconId,
            };
            ShellNotifyIcon(NimDelete, ref iconData);
            DestroyWindow(_windowHandle);
            _windowHandle = 0;
        }
        if (_classAtom != 0)
        {
            UnregisterClass(_className, GetModuleHandle(null));
            _classAtom = 0;
        }
    }

    private nint WindowProc(nint window, uint message, nint wParam, nint lParam)
    {
        if (message == _callbackMessage)
        {
            var notification = unchecked((uint)lParam.ToInt64());
            if (notification == WmLButtonDblClk)
            {
                _show();
            }
            else if (notification == WmRButtonUp)
            {
                ShowContextMenu(window);
            }
        }
        return DefWindowProc(window, message, wParam, lParam);
    }

    private void ShowContextMenu(nint window)
    {
        var menu = CreatePopupMenu();
        if (menu == 0)
        {
            return;
        }

        try
        {
            AppendMenu(menu, MfString, (nuint)TrayMenuCommand.Show, "Показать");
            AppendMenu(menu, MfString, (nuint)TrayMenuCommand.Exit, "Выход");
            GetCursorPos(out var point);
            SetForegroundWindow(window);
            var command = TrackPopupMenu(
                menu,
                TpmRightButton | TpmReturnCommand,
                point.X,
                point.Y,
                0,
                window,
                0);
            if (command == (uint)TrayMenuCommand.Show)
            {
                _show();
            }
            else if (command == (uint)TrayMenuCommand.Exit)
            {
                _exit();
            }
        }
        finally
        {
            DestroyMenu(menu);
        }
    }

    public void DisposeAndSuppressErrors()
    {
        try
        {
            Dispose();
        }
        catch
        {
        }
    }

    [UnmanagedFunctionPointer(CallingConvention.Winapi)]
    private delegate nint WndProc(nint window, uint message, nint wParam, nint lParam);

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct WndClassEx
    {
        public uint Size;
        public uint Style;
        public WndProc WindowProc;
        public int ClassExtra;
        public int WindowExtra;
        public nint Instance;
        public nint Icon;
        public nint Cursor;
        public nint Background;
        public string MenuName;
        [MarshalAs(UnmanagedType.LPWStr)]
        public string ClassName;
        public nint SmallIcon;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NotifyIconData
    {
        public uint Size;
        public nint WindowHandle;
        public uint Id;
        public uint Flags;
        public uint CallbackMessage;
        public nint IconHandle;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)]
        public string Tip;
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct Point
    {
        public readonly int X;
        public readonly int Y;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern ushort RegisterClassEx(ref WndClassEx windowClass);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern nint CreateWindowEx(
        uint extendedStyle,
        string className,
        string windowName,
        uint style,
        int x,
        int y,
        int width,
        int height,
        nint parent,
        nint menu,
        nint instance,
        nint parameter);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool DestroyWindow(nint window);

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool UnregisterClass(string className, nint instance);

    [DllImport("user32.dll")]
    private static extern nint DefWindowProc(nint window, uint message, nint wParam, nint lParam);

    [DllImport("shell32.dll", SetLastError = true)]
    private static extern bool ShellNotifyIcon(uint message, ref NotifyIconData data);

    [DllImport("user32.dll")]
    private static extern nint LoadIcon(nint instance, nint resource);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern nint CreatePopupMenu();

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool AppendMenu(nint menu, uint flags, nuint id, string text);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool DestroyMenu(nint menu);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint TrackPopupMenu(
        nint menu,
        uint flags,
        int x,
        int y,
        int reserved,
        nint window,
        nint rectangle);

    [DllImport("user32.dll")]
    private static extern bool GetCursorPos(out Point point);

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(nint window);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    private static extern nint GetModuleHandle(string? moduleName);
}
