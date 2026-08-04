using Google.Protobuf;
using System.IO.Pipes;

namespace EvoHime.Desktop.Services;

public sealed class CoreIpcClient
{
    public const uint CurrentProtocolMajor = 1;
    public const uint CurrentProtocolMinor = 0;

    private readonly string _pipeName;
    private NamedPipeClientStream? _pipe;

    public CoreIpcClient(string pipeName)
    {
        _pipeName = pipeName;
    }

    public async Task ConnectAsync(CancellationToken cancellationToken)
    {
        var pipe = new NamedPipeClientStream(
            ".",
            _pipeName,
            PipeDirection.InOut,
            PipeOptions.Asynchronous | PipeOptions.WriteThrough);
        await pipe.ConnectAsync(cancellationToken);
        _pipe = pipe;
    }

    public bool IsConnected => _pipe?.IsConnected == true;

    public async Task SendAsync(IMessage message, CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(message);
        var pipe = GetConnectedPipe();
        var frame = FrameCodec.Encode(message.ToByteArray());
        await pipe.WriteAsync(frame, cancellationToken);
        await pipe.FlushAsync(cancellationToken);
    }

    private NamedPipeClientStream GetConnectedPipe() =>
        IsConnected ? _pipe! : throw new InvalidOperationException("Core IPC pipe is not connected.");

    public async ValueTask DisposeAsync()
    {
        if (_pipe is not null)
        {
            await _pipe.DisposeAsync();
            _pipe = null;
        }
    }
}
