using System.Buffers.Binary;

namespace EvoHime.Desktop.Services;

public static class FrameCodec
{
    public const int MaxFrameBytes = 4 * 1024 * 1024;

    public static byte[] Encode(ReadOnlySpan<byte> payload)
    {
        if (payload.Length > MaxFrameBytes)
        {
            throw new ArgumentOutOfRangeException(nameof(payload));
        }

        var frame = new byte[sizeof(uint) + payload.Length];
        BinaryPrimitives.WriteUInt32LittleEndian(frame, (uint)payload.Length);
        payload.CopyTo(frame.AsSpan(sizeof(uint)));
        return frame;
    }

    public static byte[] Decode(ReadOnlySpan<byte> frame)
    {
        if (frame.Length < sizeof(uint))
        {
            throw new InvalidDataException("IPC frame is truncated.");
        }

        var length = BinaryPrimitives.ReadUInt32LittleEndian(frame);
        if (length > MaxFrameBytes)
        {
            throw new InvalidDataException("IPC frame exceeds the size limit.");
        }

        var expectedLength = checked(sizeof(uint) + (int)length);
        if (frame.Length != expectedLength)
        {
            throw new InvalidDataException("IPC frame length does not match its payload.");
        }

        return frame[sizeof(uint)..].ToArray();
    }
}
