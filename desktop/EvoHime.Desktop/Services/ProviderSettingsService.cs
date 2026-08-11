using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace EvoHime.Desktop.Services;

public sealed record ProviderSettings(
    string Provider,
    string BaseUrl,
    string Model,
    string ApiKey)
{
    public string CatalogMode { get; init; } = "free";

    public static ProviderSettings Default => new(
        "literouter",
        "https://api.literouter.com/v1",
        string.Empty,
        string.Empty);
}

public sealed class ProviderSettingsService
{
    private readonly string _path = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "EvoHime",
        "provider-settings.bin");

    public ProviderSettings Load()
    {
        try
        {
            if (!File.Exists(_path))
            {
                return ProviderSettings.Default;
            }

            var json = Encoding.UTF8.GetString(Unprotect(File.ReadAllBytes(_path)));
            return JsonSerializer.Deserialize<ProviderSettings>(json) ?? ProviderSettings.Default;
        }
        catch (Exception error) when (error is IOException or JsonException or CryptographicException)
        {
            return ProviderSettings.Default;
        }
    }

    public void Save(ProviderSettings settings)
    {
        var directory = Path.GetDirectoryName(_path)!;
        Directory.CreateDirectory(directory);
        var json = JsonSerializer.SerializeToUtf8Bytes(settings);
        var protectedBytes = Protect(json);
        var temporaryPath = _path + ".tmp";
        File.WriteAllBytes(temporaryPath, protectedBytes);
        File.Move(temporaryPath, _path, true);
    }

    private static byte[] Protect(byte[] input)
    {
        var inputBlob = ToBlob(input);
        try
        {
            if (!CryptProtectData(ref inputBlob, null, IntPtr.Zero, null, IntPtr.Zero, 0, out var outputBlob))
            {
                throw new CryptographicException(Marshal.GetLastWin32Error());
            }

            try
            {
                return FromBlob(outputBlob);
            }
            finally
            {
                LocalFree(outputBlob.pbData);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(inputBlob.pbData);
        }
    }

    private static byte[] Unprotect(byte[] input)
    {
        var inputBlob = ToBlob(input);
        try
        {
            if (!CryptUnprotectData(ref inputBlob, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, 0, out var outputBlob))
            {
                throw new CryptographicException(Marshal.GetLastWin32Error());
            }

            try
            {
                return FromBlob(outputBlob);
            }
            finally
            {
                LocalFree(outputBlob.pbData);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(inputBlob.pbData);
        }
    }

    private static DataBlob ToBlob(byte[] bytes)
    {
        var pointer = Marshal.AllocHGlobal(bytes.Length);
        Marshal.Copy(bytes, 0, pointer, bytes.Length);
        return new DataBlob { cbData = bytes.Length, pbData = pointer };
    }

    private static byte[] FromBlob(DataBlob blob)
    {
        var bytes = new byte[blob.cbData];
        Marshal.Copy(blob.pbData, bytes, 0, bytes.Length);
        return bytes;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct DataBlob
    {
        public int cbData;
        public IntPtr pbData;
    }

    [DllImport("crypt32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    private static extern bool CryptProtectData(
        ref DataBlob pDataIn,
        string? szDataDescr,
        IntPtr pOptionalEntropy,
        string? szDataDescrUnused,
        IntPtr pPromptStruct,
        uint dwFlags,
        out DataBlob pDataOut);

    [DllImport("crypt32.dll", SetLastError = true)]
    private static extern bool CryptUnprotectData(
        ref DataBlob pDataIn,
        IntPtr ppszDataDescr,
        IntPtr pOptionalEntropy,
        IntPtr pvReserved,
        IntPtr pPromptStruct,
        uint dwFlags,
        out DataBlob pDataOut);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr hMem);
}
