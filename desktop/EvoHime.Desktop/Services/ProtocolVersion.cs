namespace EvoHime.Desktop.Services;

public readonly record struct ProtocolVersion(uint Major, uint Minor)
{
    public static bool IsCompatible(uint localMajor, uint localMinor, uint peerMajor, uint peerMinor)
        => localMajor == peerMajor;
}
