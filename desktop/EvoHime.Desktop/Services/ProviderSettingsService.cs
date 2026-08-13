using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace EvoHime.Desktop.Services;

/// <summary>
/// Public settings contract kept compatible with the existing settings UI.
/// ApiKey is a runtime value only and is never serialized by this service.
/// </summary>
public sealed record ProviderSettings(
    string Provider,
    string BaseUrl,
    string Model,
    string ApiKey)
{
    public string CatalogMode { get; init; } = "free";

    /// <summary>Logical current-user credential reference, never the secret itself.</summary>
    public string? CredentialId { get; init; }

    public static ProviderSettings Default => new(
        "literouter",
        "https://api.literouter.com/v1",
        string.Empty,
        string.Empty);
}

public interface IProviderSecretStore
{
    string? Read(string credentialId);

    void Write(string credentialId, string secret);

    void Delete(string credentialId);
}

/// <summary>
/// Windows Generic Credential store. CredRead/CredWrite are scoped to the
/// identity of the calling process; this service intentionally has no
/// machine-wide or SYSTEM fallback.
/// </summary>
public sealed class WindowsProviderSecretStore : IProviderSecretStore
{
    private const uint CredTypeGeneric = 1;
    private const uint CredPersistLocalMachine = 2;
    private const int ErrorNotFound = 1168;
    private const int MaxSecretBytes = 16 * 1024;
    private const string TargetPrefix = "EvoHime.ProviderSecret.";

    public string? Read(string credentialId)
    {
        EnsureSupportedInteractiveUser();
        var targetName = TargetName(credentialId);
        if (!CredRead(targetName, CredTypeGeneric, 0, out var credentialPointer))
        {
            var error = Marshal.GetLastWin32Error();
            if (error == ErrorNotFound)
            {
                return null;
            }

            throw NativeError("прочитать credential", error);
        }

        try
        {
            var credential = Marshal.PtrToStructure<NativeCredential>(credentialPointer);
            if (credential.CredentialBlobSize > MaxSecretBytes ||
                credential.CredentialBlobSize > 0 && credential.CredentialBlob == IntPtr.Zero)
            {
                throw new CryptographicException("credential имеет недопустимый размер");
            }

            var bytes = new byte[credential.CredentialBlobSize];
            try
            {
                if (bytes.Length > 0)
                {
                    Marshal.Copy(credential.CredentialBlob, bytes, 0, bytes.Length);
                }

                return Encoding.UTF8.GetString(bytes);
            }
            finally
            {
                CryptographicOperations.ZeroMemory(bytes);
            }
        }
        finally
        {
            CredFree(credentialPointer);
        }
    }

    public void Write(string credentialId, string secret)
    {
        EnsureSupportedInteractiveUser();
        if (string.IsNullOrWhiteSpace(secret))
        {
            throw new ArgumentException("provider secret must not be empty", nameof(secret));
        }

        var secretBytes = Encoding.UTF8.GetBytes(secret);
        if (secretBytes.Length > MaxSecretBytes)
        {
            CryptographicOperations.ZeroMemory(secretBytes);
            throw new ArgumentOutOfRangeException(nameof(secret), "provider secret is too large");
        }

        var targetPointer = Marshal.StringToCoTaskMemUni(TargetName(credentialId));
        var blobPointer = Marshal.AllocCoTaskMem(secretBytes.Length);
        try
        {
            Marshal.Copy(secretBytes, 0, blobPointer, secretBytes.Length);
            var credential = new NativeCredential
            {
                Type = CredTypeGeneric,
                TargetName = targetPointer,
                CredentialBlobSize = (uint)secretBytes.Length,
                CredentialBlob = blobPointer,
                Persist = CredPersistLocalMachine,
            };

            if (!CredWrite(ref credential, 0))
            {
                throw NativeError("записать credential", Marshal.GetLastWin32Error());
            }
        }
        finally
        {
            CryptographicOperations.ZeroMemory(secretBytes);
            ZeroAndFree(blobPointer, secretBytes.Length);
            Marshal.FreeCoTaskMem(targetPointer);
        }
    }

    public void Delete(string credentialId)
    {
        EnsureSupportedInteractiveUser();
        if (CredDelete(TargetName(credentialId), CredTypeGeneric, 0))
        {
            return;
        }

        var error = Marshal.GetLastWin32Error();
        if (error != ErrorNotFound)
        {
            throw NativeError("удалить credential", error);
        }
    }

    public static string TargetName(string credentialId)
    {
        if (string.IsNullOrWhiteSpace(credentialId) || credentialId.Length > 128)
        {
            throw new ArgumentException("credential id has an invalid size", nameof(credentialId));
        }

        return TargetPrefix + credentialId;
    }

    private static void EnsureSupportedInteractiveUser()
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("provider credentials require Windows Credential Manager");
        }

        if (!Environment.UserInteractive)
        {
            throw new CryptographicException("provider credentials require an interactive Windows user");
        }
    }

    private static CryptographicException NativeError(string operation, int error)
    {
        var detail = new Win32Exception(error).Message;
        return new CryptographicException($"не удалось {operation}: {detail} (код {error})");
    }

    private static void ZeroAndFree(IntPtr pointer, int length)
    {
        if (pointer == IntPtr.Zero)
        {
            return;
        }

        if (length > 0)
        {
            var zeroes = new byte[length];
            Marshal.Copy(zeroes, 0, pointer, length);
        }

        Marshal.FreeCoTaskMem(pointer);
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct NativeCredential
    {
        public uint Flags;
        public uint Type;
        public IntPtr TargetName;
        public IntPtr Comment;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
        public uint CredentialBlobSize;
        public IntPtr CredentialBlob;
        public uint Persist;
        public uint AttributeCount;
        public IntPtr Attributes;
        public IntPtr TargetAlias;
        public IntPtr UserName;
    }

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredRead(
        string targetName,
        uint type,
        uint reservedFlag,
        out IntPtr credential);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredWrite(ref NativeCredential userCredential, uint flags);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredDelete(string targetName, uint type, uint flags);

    [DllImport("advapi32.dll")]
    private static extern void CredFree(IntPtr credential);
}

public sealed class ProviderSettingsService
{
    private readonly string _path;
    private readonly IProviderSecretStore _secretStore;

    public ProviderSettingsService(string? path = null, IProviderSecretStore? secretStore = null)
    {
        _path = path ?? Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "EvoHime",
            "provider-settings.bin");
        _secretStore = secretStore ?? new WindowsProviderSecretStore();
    }

    public ProviderSettings Load()
    {
        try
        {
            if (!File.Exists(_path))
            {
                return ProviderSettings.Default;
            }

            var persisted = ReadPersisted();
            if (!string.IsNullOrWhiteSpace(persisted.LegacyApiKey))
            {
                return MigrateLegacy(persisted);
            }

            var settings = persisted.ToRuntime(string.Empty);
            if (string.IsNullOrWhiteSpace(persisted.CredentialId))
            {
                return settings;
            }

            try
            {
                return settings with
                {
                    ApiKey = _secretStore.Read(persisted.CredentialId) ?? string.Empty,
                };
            }
            catch (CryptographicException)
            {
                // A damaged/unavailable credential must fail closed. Keep
                // non-secret metadata for reauthorization, never fall back
                // to a file, environment master password, or plaintext.
                return settings;
            }
        }
        catch (Exception error) when (error is IOException or JsonException or CryptographicException)
        {
            return ProviderSettings.Default;
        }
    }

    public void Save(ProviderSettings settings)
    {
        var currentCredentialId = TryReadPersisted()?.CredentialId;
        string? newCredentialId = null;
        var wroteNewCredential = false;

        try
        {
            if (!string.IsNullOrWhiteSpace(settings.ApiKey))
            {
                newCredentialId = NewCredentialId();
                _secretStore.Write(newCredentialId, settings.ApiKey);
                wroteNewCredential = true;
                if (!string.Equals(_secretStore.Read(newCredentialId), settings.ApiKey, StringComparison.Ordinal))
                {
                    throw new CryptographicException("проверка записанного credential не пройдена");
                }
            }

            WritePersisted(PersistedProviderSettings.FromRuntime(settings, newCredentialId));
        }
        catch
        {
            if (wroteNewCredential && newCredentialId is not null)
            {
                TryDelete(newCredentialId);
            }

            throw;
        }

        if (!string.IsNullOrWhiteSpace(currentCredentialId) &&
            !string.Equals(currentCredentialId, newCredentialId, StringComparison.Ordinal))
        {
            _secretStore.Delete(currentCredentialId);
        }
    }

    private ProviderSettings MigrateLegacy(PersistedProviderSettings legacy)
    {
        var credentialId = NewCredentialId();
        try
        {
            _secretStore.Write(credentialId, legacy.LegacyApiKey!);
            if (!string.Equals(_secretStore.Read(credentialId), legacy.LegacyApiKey, StringComparison.Ordinal))
            {
                throw new CryptographicException("проверка мигрированного credential не пройдена");
            }

            WritePersisted(legacy with { LegacyApiKey = null, CredentialId = credentialId });
            return legacy.ToRuntime(legacy.LegacyApiKey!) with { CredentialId = credentialId };
        }
        catch
        {
            TryDelete(credentialId);
            throw;
        }
    }

    private PersistedProviderSettings ReadPersisted()
    {
        var json = Encoding.UTF8.GetString(Unprotect(File.ReadAllBytes(_path)));
        return JsonSerializer.Deserialize<PersistedProviderSettings>(json)
            ?? throw new JsonException("provider settings are empty");
    }

    private PersistedProviderSettings? TryReadPersisted()
    {
        try
        {
            return File.Exists(_path) ? ReadPersisted() : null;
        }
        catch (Exception error) when (error is IOException or JsonException or CryptographicException)
        {
            return null;
        }
    }

    private void WritePersisted(PersistedProviderSettings settings)
    {
        var directory = Path.GetDirectoryName(_path)!;
        Directory.CreateDirectory(directory);
        var json = JsonSerializer.SerializeToUtf8Bytes(settings);
        var protectedBytes = Protect(json);
        var temporaryPath = _path + ".tmp";
        try
        {
            File.WriteAllBytes(temporaryPath, protectedBytes);
            File.Move(temporaryPath, _path, true);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(protectedBytes);
            if (File.Exists(temporaryPath))
            {
                File.Delete(temporaryPath);
            }
        }
    }

    private void TryDelete(string credentialId)
    {
        try
        {
            _secretStore.Delete(credentialId);
        }
        catch (CryptographicException)
        {
            // Do not replace the original rotation/migration error with a
            // cleanup error, and never include secret material in diagnostics.
        }
    }

    private static string NewCredentialId() => Guid.NewGuid().ToString("N");

    private static byte[] Protect(byte[] input)
    {
        var inputBlob = ToBlob(input);
        try
        {
            if (!CryptProtectData(ref inputBlob, "EvoHime provider settings", IntPtr.Zero, null, IntPtr.Zero, 0, out var outputBlob))
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
            ZeroAndFree(inputBlob.pbData, inputBlob.cbData);
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
            ZeroAndFree(inputBlob.pbData, inputBlob.cbData);
        }
    }

    private static DataBlob ToBlob(byte[] bytes)
    {
        var pointer = Marshal.AllocHGlobal(bytes.Length);
        Marshal.Copy(bytes, 0, pointer, bytes.Length);
        return new DataBlob { cbData = bytes.Length, pbData = pointer };
    }

    private static void ZeroAndFree(IntPtr pointer, int length)
    {
        if (pointer == IntPtr.Zero)
        {
            return;
        }

        if (length > 0)
        {
            var zeroes = new byte[length];
            Marshal.Copy(zeroes, 0, pointer, length);
        }

        Marshal.FreeHGlobal(pointer);
    }

    private static byte[] FromBlob(DataBlob blob)
    {
        if (blob.cbData < 0 || blob.cbData > 4 * 1024 * 1024 || blob.pbData == IntPtr.Zero)
        {
            throw new CryptographicException("DPAPI settings blob has an invalid size");
        }

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

    private sealed record PersistedProviderSettings(
        string Provider,
        string BaseUrl,
        string Model,
        string? CredentialId,
        string CatalogMode,
        [property: JsonPropertyName("ApiKey")] string? LegacyApiKey = null)
    {
        public ProviderSettings ToRuntime(string apiKey) => new(Provider, BaseUrl, Model, apiKey)
        {
            CatalogMode = string.IsNullOrWhiteSpace(CatalogMode) ? "free" : CatalogMode,
            CredentialId = CredentialId,
        };

        public static PersistedProviderSettings FromRuntime(ProviderSettings settings, string? credentialId) => new(
            settings.Provider,
            settings.BaseUrl,
            settings.Model,
            credentialId,
            settings.CatalogMode);
    }

}
